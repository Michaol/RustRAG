/// Mock embedder for testing purposes.
///
/// Generates deterministic embeddings based on text hash,
/// mirroring the Go version's `MockEmbedder`.
use std::hash::{DefaultHasher, Hash, Hasher};

use super::{Embedder, EmbedderError};

/// A mock embedder that produces deterministic vectors from text hashes.
///
/// Useful for testing without loading a real ONNX model.
pub struct MockEmbedder {
    pub dimensions: usize,
}

impl MockEmbedder {
    /// Create a new `MockEmbedder` with the given dimensionality.
    #[must_use]
    pub fn new(dimensions: usize) -> Self {
        Self { dimensions }
    }
}

impl Default for MockEmbedder {
    fn default() -> Self {
        Self { dimensions: 384 }
    }
}

impl Embedder for MockEmbedder {
    fn embed(&self, text: &str) -> Result<Vec<f32>, EmbedderError> {
        // Generate a deterministic embedding based on text hash
        let mut hasher = DefaultHasher::new();
        text.hash(&mut hasher);
        let hash = hasher.finish();

        // Use the hash bytes to seed deterministic float values
        let bytes = hash.to_le_bytes();
        let mut embedding = Vec::with_capacity(self.dimensions);
        for i in 0..self.dimensions {
            embedding.push(bytes[i % 8] as f32 / 255.0);
        }

        // L2 normalize
        let norm_sq: f32 = embedding.iter().map(|v| v * v).sum();
        if norm_sq > 0.0 {
            let inv = 1.0 / norm_sq.sqrt();
            for v in &mut embedding {
                *v *= inv;
            }
        }

        Ok(embedding)
    }

    fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, EmbedderError> {
        texts.iter().map(|t| self.embed(t)).collect()
    }

    fn dimensions(&self) -> usize {
        self.dimensions
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mock_embed_dimensions() {
        let embedder = MockEmbedder::new(384);
        let result = embedder.embed("hello world").unwrap();
        assert_eq!(result.len(), 384);
    }

    #[test]
    fn test_mock_embed_deterministic() {
        let embedder = MockEmbedder::new(384);
        let a = embedder.embed("hello").unwrap();
        let b = embedder.embed("hello").unwrap();
        assert_eq!(a, b, "same input should produce same output");
    }

    #[test]
    fn test_mock_embed_different_inputs() {
        let embedder = MockEmbedder::new(384);
        let a = embedder.embed("hello").unwrap();
        let b = embedder.embed("world").unwrap();
        assert_ne!(a, b, "different inputs should produce different outputs");
    }

    #[test]
    fn test_mock_embed_normalized() {
        let embedder = MockEmbedder::new(384);
        let vec = embedder.embed("test normalization").unwrap();
        let norm: f32 = vec.iter().map(|v| v * v).sum::<f32>().sqrt();
        assert!(
            (norm - 1.0).abs() < 0.01,
            "vector should be approximately unit length, got {norm}"
        );
    }

    #[test]
    fn test_mock_embed_batch() {
        let embedder = MockEmbedder::new(128);
        let results = embedder.embed_batch(&["a", "b", "c"]).unwrap();
        assert_eq!(results.len(), 3);
        for vec in &results {
            assert_eq!(vec.len(), 128);
        }
    }

    #[test]
    fn test_mock_default_dimensions() {
        let embedder = MockEmbedder::default();
        assert_eq!(embedder.dimensions(), 384);
    }
}
