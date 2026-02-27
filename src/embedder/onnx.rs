/// ONNX Runtime embedder using the `ort` crate.
///
/// Loads a multilingual-e5-small ONNX model, runs inference, applies mean
/// pooling with attention mask, and L2-normalizes the result. Mirrors the
/// Go version's `onnx.go`.
use std::path::Path;
use std::sync::Mutex;

use ort::session::Session;
use ort::value::Tensor;
use tracing::info;

use super::tokenizer::BertTokenizer;
use super::{Embedder, EmbedderError};

/// ONNX-backed embedder implementing the `Embedder` trait.
pub struct OnnxEmbedder {
    session: Mutex<Session>,
    tokenizer: BertTokenizer,
    dimensions: usize,
}

impl OnnxEmbedder {
    /// Create a new `OnnxEmbedder` by loading a model from the given directory.
    ///
    /// Expects `model.onnx` and `tokenizer.json` in `model_dir`.
    pub fn new(model_dir: &Path) -> Result<Self, EmbedderError> {
        let model_path = model_dir.join("model.onnx");

        if !model_path.exists() {
            return Err(EmbedderError::ModelLoadFailed(format!(
                "model.onnx not found in {}",
                model_dir.display()
            )));
        }

        info!("Initializing ONNX Runtime...");

        let session = Session::builder()
            .map_err(|e| EmbedderError::ModelLoadFailed(format!("session builder error: {e}")))?
            .with_intra_threads(4)
            .map_err(|e| EmbedderError::ModelLoadFailed(format!("thread config error: {e}")))?
            .with_inter_threads(4)
            .map_err(|e| EmbedderError::ModelLoadFailed(format!("thread config error: {e}")))?
            .commit_from_file(&model_path)
            .map_err(|e| EmbedderError::ModelLoadFailed(format!("model load error: {e}")))?;

        info!("ONNX model loaded successfully");

        let tokenizer = BertTokenizer::from_model_dir(model_dir)
            .map_err(|e| EmbedderError::ModelLoadFailed(format!("tokenizer error: {e}")))?;

        info!("Tokenizer loaded (vocab size: {})", tokenizer.vocab_size());

        Ok(Self {
            session: Mutex::new(session),
            tokenizer,
            dimensions: 384, // multilingual-e5-small output dimension
        })
    }
}

impl Embedder for OnnxEmbedder {
    fn embed(&self, text: &str) -> Result<Vec<f32>, EmbedderError> {
        // Tokenize
        let tokens = self
            .tokenizer
            .tokenize(text)
            .map_err(|e| EmbedderError::InferenceFailed(format!("tokenization failed: {e}")))?;

        let seq_len = tokens.input_ids.len();

        // Create input tensors using (shape, data) tuple form
        // This avoids ndarray version coupling with ort
        let input_ids_val = Tensor::from_array(([1usize, seq_len], tokens.input_ids.clone()))
            .map_err(|e| EmbedderError::InferenceFailed(format!("input_ids error: {e}")))?;
        let attention_mask_val =
            Tensor::from_array(([1usize, seq_len], tokens.attention_mask.clone())).map_err(
                |e| EmbedderError::InferenceFailed(format!("attention_mask error: {e}")),
            )?;
        let token_type_ids_val = Tensor::from_array(([1usize, seq_len], vec![0i64; seq_len]))
            .map_err(|e| EmbedderError::InferenceFailed(format!("token_type_ids error: {e}")))?;

        // Run inference with named inputs
        let mut session = self
            .session
            .lock()
            .map_err(|e| EmbedderError::InferenceFailed(format!("lock poisoned: {e}")))?;
        let outputs = session
            .run(ort::inputs![
                "input_ids" => input_ids_val,
                "attention_mask" => attention_mask_val,
                "token_type_ids" => token_type_ids_val,
            ])
            .map_err(|e| EmbedderError::InferenceFailed(format!("inference failed: {e}")))?;

        // Extract output: shape [batch_size=1, seq_length, hidden_size]
        // try_extract_tensor returns Result<(&Shape, &[T])>
        let (_shape, hidden_data) = outputs[0]
            .try_extract_tensor::<f32>()
            .map_err(|e| EmbedderError::InferenceFailed(format!("output extraction: {e}")))?;

        // Mean pooling with attention mask
        let embedding = mean_pooling(
            hidden_data,
            &tokens.attention_mask,
            seq_len,
            self.dimensions,
        );

        // L2 normalize
        Ok(l2_normalize(&embedding))
    }

    fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, EmbedderError> {
        // Process one by one (same as Go version)
        texts.iter().map(|t| self.embed(t)).collect()
    }

    fn dimensions(&self) -> usize {
        self.dimensions
    }
}

/// Mean pooling over hidden states weighted by attention mask.
///
/// `hidden_data` is a flat array with shape `[1, seq_len, hidden_size]`.
fn mean_pooling(
    hidden_data: &[f32],
    attention_mask: &[i64],
    seq_len: usize,
    hidden_size: usize,
) -> Vec<f32> {
    let mut result = vec![0.0f32; hidden_size];
    let mut mask_sum: f32 = 0.0;

    for t in 0..seq_len {
        let mask = attention_mask[t] as f32;
        mask_sum += mask;

        for h in 0..hidden_size {
            let idx = t * hidden_size + h;
            result[h] += hidden_data[idx] * mask;
        }
    }

    // Average by number of real tokens
    if mask_sum > 0.0 {
        for v in &mut result {
            *v /= mask_sum;
        }
    }

    result
}

/// L2-normalize a vector, returning the normalized copy.
fn l2_normalize(vec: &[f32]) -> Vec<f32> {
    let norm_sq: f32 = vec.iter().map(|v| v * v).sum();
    if norm_sq == 0.0 {
        return vec.to_vec();
    }

    let inv_norm = 1.0 / norm_sq.sqrt();
    vec.iter().map(|v| v * inv_norm).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_l2_normalize() {
        let v = vec![3.0, 4.0];
        let normed = l2_normalize(&v);
        let norm: f32 = normed.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 1e-6);
        assert!((normed[0] - 0.6).abs() < 1e-6);
        assert!((normed[1] - 0.8).abs() < 1e-6);
    }

    #[test]
    fn test_l2_normalize_zero() {
        let v = vec![0.0, 0.0, 0.0];
        let normed = l2_normalize(&v);
        assert_eq!(normed, vec![0.0, 0.0, 0.0]);
    }

    #[test]
    fn test_mean_pooling_simple() {
        // 1 token, hidden_size=3, all attention=1
        let hidden = vec![1.0, 2.0, 3.0];
        let mask = vec![1i64];
        let result = mean_pooling(&hidden, &mask, 1, 3);
        assert_eq!(result, vec![1.0, 2.0, 3.0]);
    }

    #[test]
    fn test_mean_pooling_with_padding() {
        // 2 tokens, hidden_size=2, second token is padding (mask=0)
        let hidden = vec![1.0, 2.0, 10.0, 20.0];
        let mask = vec![1i64, 0i64];
        let result = mean_pooling(&hidden, &mask, 2, 2);
        // Only first token contributes
        assert_eq!(result, vec![1.0, 2.0]);
    }

    /// Integration test requiring actual model files.
    #[test]
    #[ignore]
    fn test_onnx_embed() {
        let model_dir = Path::new("models/multilingual-e5-small");
        if !model_dir.join("model.onnx").exists() {
            eprintln!("Skipping: model files not downloaded");
            return;
        }

        let embedder = OnnxEmbedder::new(model_dir).unwrap();
        let vec = embedder.embed("Hello, world!").unwrap();

        assert_eq!(vec.len(), 384);
        let norm: f32 = vec.iter().map(|v| v * v).sum::<f32>().sqrt();
        assert!(
            (norm - 1.0).abs() < 0.01,
            "expected unit vector, got norm={norm}"
        );
    }

    #[test]
    #[ignore]
    fn test_onnx_embed_batch() {
        let model_dir = Path::new("models/multilingual-e5-small");
        if !model_dir.join("model.onnx").exists() {
            return;
        }

        let embedder = OnnxEmbedder::new(model_dir).unwrap();
        let results = embedder.embed_batch(&["hello", "world"]).unwrap();
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].len(), 384);
    }
}
