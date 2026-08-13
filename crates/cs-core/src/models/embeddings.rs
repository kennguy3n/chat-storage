//! Text embeddings (XLM-R) — trait + stub implementations.

use crate::models::ModelError;

/// Text embedder trait.
pub trait TextEmbedder: Send + Sync {
    /// Embed text into a float vector.
    fn embed(&self, text: &str) -> Result<Vec<f32>, ModelError>;

    /// Model version string.
    fn model_version(&self) -> &str;
}

/// Stub embedder (returns zeros — used when ONNX is not available).
#[derive(Debug)]
pub struct StubEmbedder;

impl TextEmbedder for StubEmbedder {
    fn embed(&self, _text: &str) -> Result<Vec<f32>, ModelError> {
        Ok(vec![0.0; 384])
    }

    fn model_version(&self) -> &str {
        "stub@v0"
    }
}
