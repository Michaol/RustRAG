/// BERT tokenizer wrapper around HuggingFace `tokenizers` crate.
///
/// Provides tokenization with attention masks for the ONNX embedder.
/// Mirrors Go version's `tokenizer.go`.
use std::path::Path;

use anyhow::Result;
use tokenizers::Tokenizer;

/// Wrapper around the HuggingFace tokenizer for BERT-style models.
pub struct BertTokenizer {
    inner: Tokenizer,
    max_length: usize,
}

/// Output of a tokenization operation.
#[derive(Debug, Clone)]
pub struct TokenizerOutput {
    /// Token IDs (input_ids for the model).
    pub input_ids: Vec<i64>,
    /// Attention mask (1 for real tokens, 0 for padding).
    pub attention_mask: Vec<i64>,
}

impl BertTokenizer {
    /// Load a tokenizer from a `tokenizer.json` file in the model directory.
    pub fn from_model_dir(model_dir: &Path) -> Result<Self> {
        let tokenizer_path = model_dir.join("tokenizer.json");

        anyhow::ensure!(
            tokenizer_path.exists(),
            "tokenizer.json not found in {}",
            model_dir.display()
        );

        let mut inner = Tokenizer::from_file(&tokenizer_path)
            .map_err(|e| anyhow::anyhow!("failed to load tokenizer: {e}"))?;

        // Configure truncation to max 512 tokens (XLM-RoBERTa default)
        let _ = inner.with_truncation(Some(tokenizers::TruncationParams {
            max_length: 512,
            ..Default::default()
        }));

        // Configure padding
        inner.with_padding(Some(tokenizers::PaddingParams {
            ..Default::default()
        }));

        Ok(Self {
            inner,
            max_length: 512,
        })
    }

    /// Tokenize a single text, returning input IDs and attention mask.
    pub fn tokenize(&self, text: &str) -> Result<TokenizerOutput> {
        let encoding = self
            .inner
            .encode(text, true)
            .map_err(|e| anyhow::anyhow!("failed to encode text: {e}"))?;

        let input_ids: Vec<i64> = encoding.get_ids().iter().map(|&id| id as i64).collect();
        let attention_mask: Vec<i64> = encoding
            .get_attention_mask()
            .iter()
            .map(|&m| m as i64)
            .collect();

        Ok(TokenizerOutput {
            input_ids,
            attention_mask,
        })
    }

    /// Tokenize multiple texts in a batch.
    pub fn tokenize_batch(&self, texts: &[&str]) -> Result<Vec<TokenizerOutput>> {
        let encodings = self
            .inner
            .encode_batch(texts.to_vec(), true)
            .map_err(|e| anyhow::anyhow!("failed to encode batch: {e}"))?;

        let results = encodings
            .iter()
            .map(|enc| {
                let input_ids: Vec<i64> = enc.get_ids().iter().map(|&id| id as i64).collect();
                let attention_mask: Vec<i64> =
                    enc.get_attention_mask().iter().map(|&m| m as i64).collect();
                TokenizerOutput {
                    input_ids,
                    attention_mask,
                }
            })
            .collect();

        Ok(results)
    }

    /// Get the vocabulary size.
    #[must_use]
    pub fn vocab_size(&self) -> usize {
        self.inner.get_vocab_size(false)
    }

    /// Get the configured maximum sequence length.
    #[must_use]
    pub fn max_length(&self) -> usize {
        self.max_length
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// This test requires the actual tokenizer.json file.
    /// Run with: cargo test tokenizer -- --ignored
    #[test]
    #[ignore]
    fn test_tokenize_with_real_model() {
        let model_dir = Path::new("models/multilingual-e5-small");
        if !model_dir.join("tokenizer.json").exists() {
            eprintln!("Skipping: model files not downloaded");
            return;
        }

        let tokenizer = BertTokenizer::from_model_dir(model_dir).unwrap();
        let output = tokenizer.tokenize("Hello, world!").unwrap();

        assert!(!output.input_ids.is_empty());
        assert_eq!(output.input_ids.len(), output.attention_mask.len());
        // Should have CLS and SEP tokens
        assert!(output.input_ids.len() >= 3);
    }

    #[test]
    #[ignore]
    fn test_tokenize_batch_with_real_model() {
        let model_dir = Path::new("models/multilingual-e5-small");
        if !model_dir.join("tokenizer.json").exists() {
            return;
        }

        let tokenizer = BertTokenizer::from_model_dir(model_dir).unwrap();
        let outputs = tokenizer
            .tokenize_batch(&["Hello", "World", "Test"])
            .unwrap();

        assert_eq!(outputs.len(), 3);
        for output in &outputs {
            assert!(!output.input_ids.is_empty());
        }
    }

    #[test]
    fn test_tokenizer_missing_file() {
        let result = BertTokenizer::from_model_dir(Path::new("/nonexistent/path"));
        assert!(result.is_err());
    }
}
