/// OpenAI-compatible Embedding API client.
///
/// Works with any provider that implements the standard `/v1/embeddings`
/// endpoint format: DashScope, Ollama, OpenAI, Azure OpenAI, etc.
///
/// Features:
/// - Smart batching (adapts batch size to text length)
/// - Exponential backoff retry (up to 3 attempts for retryable errors)
use std::time::Duration;

use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use tracing::{debug, warn};

use crate::config::EmbeddingConfig;
use super::EmbedderError;

/// Maximum number of retry attempts for retryable API errors.
const MAX_RETRIES: u32 = 3;

/// Base delay in milliseconds for exponential backoff.
const BASE_RETRY_DELAY_MS: u64 = 100;

/// Maximum estimated tokens per batch to avoid API payload size limits.
const MAX_TOKENS_PER_BATCH: usize = 8000;

/// OpenAI-compatible embedding API client.
pub struct ApiEmbedder {
    client: Client,
    api_url: String,
    api_key: String,
    model: String,
    dimensions: usize,
    batch_size: usize,
}

#[derive(Serialize)]
struct EmbeddingRequest {
    model: String,
    input: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    dimensions: Option<usize>,
}

#[derive(Deserialize)]
struct EmbeddingResponse {
    data: Vec<EmbeddingData>,
    #[allow(dead_code)]
    usage: Option<UsageInfo>,
}

#[derive(Deserialize)]
struct EmbeddingData {
    embedding: Vec<f32>,
    index: usize,
}

#[derive(Deserialize)]
struct UsageInfo {
    #[allow(dead_code)]
    prompt_tokens: u64,
    #[allow(dead_code)]
    total_tokens: u64,
}

/// Errors returned by the API, with retryability classification.
#[derive(Debug)]
struct ApiError {
    message: String,
    retryable: bool,
}

impl ApiEmbedder {
    /// Create a new API embedder from configuration.
    ///
    /// # Errors
    /// Returns an error if the API key is not configured or the HTTP client
    /// cannot be built.
    pub fn new(config: &EmbeddingConfig) -> Result<Self, EmbedderError> {
        let api_key = config.resolve_api_key();
        if api_key.is_empty() {
            return Err(EmbedderError::ModelLoadFailed(
                "API key not configured. Set 'embedding.api_key' in config.json \
                 or RAG_API_KEY / DASHSCOPE_API_KEY / OPENAI_API_KEY environment variable"
                    .to_string(),
            ));
        }

        let client = Client::builder()
            .timeout(Duration::from_secs(config.timeout_secs))
            .build()
            .map_err(|e| EmbedderError::ModelLoadFailed(format!("Failed to build HTTP client: {e}")))?;

        Ok(Self {
            client,
            api_url: config.api_url.clone(),
            api_key,
            model: config.api_model.clone(),
            dimensions: config.dimensions,
            batch_size: config.batch_size,
        })
    }

    /// Send an HTTP request to the embedding API.
    fn send_request(&self, request: &EmbeddingRequest) -> Result<EmbeddingResponse, ApiError> {
        let response = self
            .client
            .post(&self.api_url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(request)
            .send()
            .map_err(|e| {
                let retryable = e.is_timeout() || e.is_connect() || e.is_request();
                ApiError {
                    message: format!("Network error: {e}"),
                    retryable,
                }
            })?;

        let status = response.status();

        if !status.is_success() {
            let error_text = response
                .text()
                .unwrap_or_else(|_| "<failed to read response body>".to_string());
            let retryable = status.is_server_error()
                || status.as_u16() == 429  // Too Many Requests
                || status.as_u16() == 408; // Request Timeout

            return Err(ApiError {
                message: format!("HTTP {status}: {error_text}"),
                retryable,
            });
        }

        response
            .json::<EmbeddingResponse>()
            .map_err(|e| ApiError {
                message: format!("Failed to parse response: {e}"),
                retryable: false,
            })
    }

    /// Create smart batches based on text length to stay within API token limits.
    ///
    /// Groups texts so that each batch contains at most `batch_size` texts AND
    /// at most ~8000 estimated tokens. This prevents 413 (payload too large)
    /// errors when processing long texts.
    fn create_smart_batches<'a>(&self, texts: &[&'a str]) -> Vec<Vec<&'a str>> {
        let mut batches: Vec<Vec<&str>> = Vec::new();
        let mut current_batch: Vec<&str> = Vec::new();
        let mut current_tokens: usize = 0;

        for text in texts {
            let tokens = estimate_tokens(text);

            // Single text exceeds limit → standalone batch
            if tokens > MAX_TOKENS_PER_BATCH {
                if !current_batch.is_empty() {
                    batches.push(std::mem::take(&mut current_batch));
                    current_tokens = 0;
                }
                batches.push(vec![text]);
                continue;
            }

            let would_exceed_tokens = current_tokens + tokens > MAX_TOKENS_PER_BATCH;
            let would_exceed_count = current_batch.len() >= self.batch_size;

            if (would_exceed_tokens || would_exceed_count) && !current_batch.is_empty() {
                batches.push(std::mem::take(&mut current_batch));
                current_tokens = 0;
            }

            current_batch.push(text);
            current_tokens += tokens;
        }

        if !current_batch.is_empty() {
            batches.push(current_batch);
        }

        batches
    }

    /// Process a single batch with retry logic.
    fn process_batch(&self, batch: &[&str]) -> Result<Vec<Vec<f32>>, EmbedderError> {
        let request = EmbeddingRequest {
            model: self.model.clone(),
            input: batch.iter().map(|s| s.to_string()).collect(),
            dimensions: Some(self.dimensions),
        };

        // Exponential backoff retry
        for attempt in 0..MAX_RETRIES {
            match self.send_request(&request) {
                Ok(response) => {
                    return self.validate_response(response, batch.len());
                }
                Err(e) => {
                    let is_last_attempt = attempt + 1 >= MAX_RETRIES;
                    if !e.retryable || is_last_attempt {
                        return Err(EmbedderError::InferenceFailed(e.message));
                    }

                    let delay =
                        Duration::from_millis(BASE_RETRY_DELAY_MS * 2u64.pow(attempt));
                    warn!(
                        attempt = attempt + 1,
                        max_retries = MAX_RETRIES,
                        delay_ms = delay.as_millis() as u64,
                        "API request failed, retrying: {}",
                        e.message
                    );
                    std::thread::sleep(delay);
                }
            }
        }

        // Unreachable: loop always returns via Ok or Err above
        unreachable!("retry loop must return within MAX_RETRIES iterations")
    }

    /// Validate the API response: correct count, correct dimensions, proper ordering.
    fn validate_response(
        &self,
        response: EmbeddingResponse,
        expected_count: usize,
    ) -> Result<Vec<Vec<f32>>, EmbedderError> {
        if response.data.len() != expected_count {
            return Err(EmbedderError::InferenceFailed(format!(
                "API returned {} embeddings but expected {}",
                response.data.len(),
                expected_count
            )));
        }

        // Sort by index (API may return out of order)
        let mut sorted: Vec<(usize, Vec<f32>)> = response
            .data
            .into_iter()
            .map(|d| (d.index, d.embedding))
            .collect();
        sorted.sort_by_key(|(idx, _)| *idx);

        let result: Vec<Vec<f32>> = sorted.into_iter().map(|(_, emb)| emb).collect();

        // Validate dimensions
        for (i, emb) in result.iter().enumerate() {
            if emb.len() != self.dimensions {
                return Err(EmbedderError::InferenceFailed(format!(
                    "Embedding {} has {} dimensions but expected {}",
                    i,
                    emb.len(),
                    self.dimensions
                )));
            }
        }

        if let Some(usage) = response.usage {
            debug!(
                prompt_tokens = usage.prompt_tokens,
                total_tokens = usage.total_tokens,
                "API usage"
            );
        }

        Ok(result)
    }
}

impl super::Embedder for ApiEmbedder {
    fn embed(&self, text: &str) -> Result<Vec<f32>, EmbedderError> {
        let results = self.embed_batch(&[text])?;
        results
            .into_iter()
            .next()
            .ok_or_else(|| EmbedderError::InferenceFailed("Empty embedding response".to_string()))
    }

    fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, EmbedderError> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }

        let batches = self.create_smart_batches(texts);
        debug!(
            total_texts = texts.len(),
            batch_count = batches.len(),
            "Processing embedding batches"
        );

        let mut all_embeddings = Vec::with_capacity(texts.len());
        for batch in &batches {
            let embeddings = self.process_batch(batch)?;
            all_embeddings.extend(embeddings);
        }

        Ok(all_embeddings)
    }

    fn dimensions(&self) -> usize {
        self.dimensions
    }
}

/// Estimate token count from text length (~4 chars per token for English/mixed text).
fn estimate_tokens(text: &str) -> usize {
    // Use a conservative estimate: 3 chars per token for CJK, 4 for others.
    // For simplicity, use 3 as a safe upper bound.
    text.len().div_ceil(3)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::embedder::Embedder;

    fn test_embedder() -> ApiEmbedder {
        ApiEmbedder {
            client: Client::new(),
            api_url: "http://localhost:0/v1/embeddings".to_string(),
            api_key: "test".to_string(),
            model: "test-model".to_string(),
            dimensions: 1024,
            batch_size: 32,
        }
    }

    fn test_embedder_with_batch_size(batch_size: usize) -> ApiEmbedder {
        ApiEmbedder {
            client: Client::new(),
            api_url: "http://localhost:0/v1/embeddings".to_string(),
            api_key: "test".to_string(),
            model: "test-model".to_string(),
            dimensions: 1024,
            batch_size,
        }
    }

    #[test]
    fn test_estimate_tokens() {
        assert_eq!(estimate_tokens(""), 0);
        assert_eq!(estimate_tokens("a"), 1);
        assert_eq!(estimate_tokens("abc"), 1);
        assert_eq!(estimate_tokens("abcd"), 2);
        assert_eq!(estimate_tokens("hello world"), 4);
    }

    #[test]
    fn test_create_smart_batches_empty() {
        let embedder = test_embedder();
        let texts: Vec<&str> = vec![];
        let batches = embedder.create_smart_batches(&texts);
        assert!(batches.is_empty());
    }

    #[test]
    fn test_create_smart_batches_short_texts() {
        let embedder = test_embedder_with_batch_size(3);
        let texts: Vec<&str> = vec!["a", "b", "c", "d", "e"];
        let batches = embedder.create_smart_batches(&texts);
        // With batch_size=3 and short texts: [a,b,c], [d,e]
        assert_eq!(batches.len(), 2);
        assert_eq!(batches[0].len(), 3);
        assert_eq!(batches[1].len(), 2);
    }

    #[test]
    fn test_create_smart_batches_respects_count_limit() {
        let embedder = test_embedder_with_batch_size(2);
        let texts: Vec<&str> = vec!["hello", "world", "foo", "bar"];
        let batches = embedder.create_smart_batches(&texts);
        assert_eq!(batches.len(), 2);
        assert_eq!(batches[0].len(), 2);
        assert_eq!(batches[1].len(), 2);
    }

    #[test]
    fn test_create_smart_batches_long_text_standalone() {
        let embedder = test_embedder_with_batch_size(32);
        // Create a text that exceeds 8000 tokens (~24000 chars at 3 chars/token)
        let long_text = "x".repeat(25000);
        let short_text = "hello";
        let texts: Vec<&str> = vec![&long_text, short_text];
        let batches = embedder.create_smart_batches(&texts);
        // Long text should be in its own batch
        assert_eq!(batches.len(), 2);
        assert_eq!(batches[0].len(), 1);
        assert_eq!(batches[1].len(), 1);
    }

    #[test]
    fn test_embed_batch_empty() {
        let embedder = test_embedder();
        let result = embedder.embed_batch(&[]).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_api_embedder_missing_key() {
        let config = EmbeddingConfig {
            api_key: String::new(),
            ..Default::default()
        };
        // SAFETY: single-threaded test
        unsafe {
            std::env::remove_var("RAG_API_KEY");
            std::env::remove_var("DASHSCOPE_API_KEY");
            std::env::remove_var("OPENAI_API_KEY");
        }
        let result = ApiEmbedder::new(&config);
        assert!(result.is_err());
    }

    #[test]
    fn test_api_embedder_dimensions() {
        let config = EmbeddingConfig {
            api_key: "test-key".to_string(),
            dimensions: 1024,
            ..Default::default()
        };
        let embedder = ApiEmbedder::new(&config).unwrap();
        assert_eq!(embedder.dimensions(), 1024);
    }
}
