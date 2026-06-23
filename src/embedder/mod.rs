/// Embedder trait and shared types for text embedding.
pub mod api;
pub mod mock;

use thiserror::Error;

/// Errors that can occur during embedding operations.
#[derive(Error, Debug)]
pub enum EmbedderError {
    #[error("inference failed: {0}")]
    InferenceFailed(String),

    #[error("model load failed: {0}")]
    ModelLoadFailed(String),
}

/// Trait for text embedding implementations.
///
/// All implementations must be `Send + Sync` to allow concurrent use
/// behind `Arc`.
pub trait Embedder: Send + Sync {
    /// Embed a single text string into a vector.
    fn embed(&self, text: &str) -> Result<Vec<f32>, EmbedderError>;

    /// Embed multiple text strings into vectors.
    fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, EmbedderError>;

    /// Return the dimensionality of the embedding vectors.
    fn dimensions(&self) -> usize;
}
