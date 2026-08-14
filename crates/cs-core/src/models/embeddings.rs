//! Text embeddings — XLM-R via kchat-encoder when `ml` feature is enabled.
//!
//! Without `ml`, returns a zero-vector stub so search indexing still
//! compiles and runs (with degenerate embeddings).

use crate::models::ModelError;

/// Text embedder trait.
pub trait TextEmbedder: Send + Sync {
    /// Embed text into a float vector.
    fn embed(&self, text: &str) -> Result<Vec<f32>, ModelError>;

    /// Model version string.
    fn model_version(&self) -> &str;
}

/// Stub embedder (returns zeros — used when ML is not available).
///
/// Returns 768-dim zero vectors to match `kchat-encoder::EMBEDDING_DIM`
/// so that vectors stored with the stub are dimension-compatible with
/// real kchat-encoder embeddings (though cosine similarity will still
/// be 0.0 for zero vectors, so semantic search effectively returns no
/// results — which is the intended degraded behaviour).
#[derive(Debug, Default, Clone, Copy)]
pub struct StubEmbedder;

/// Embedding dimension — matches `kchat_encoder::EMBEDDING_DIM`.
pub const EMBEDDING_DIM: usize = 768;

impl TextEmbedder for StubEmbedder {
    fn embed(&self, _text: &str) -> Result<Vec<f32>, ModelError> {
        Ok(vec![0.0; EMBEDDING_DIM])
    }

    fn model_version(&self) -> &str {
        "stub@v0"
    }
}

/// kchat-encoder-backed text embedder (available with `ml` feature).
#[cfg(feature = "ml")]
pub mod kchat {
    use super::{ModelError, TextEmbedder};

    /// kchat-encoder-backed embedder.
    pub struct KchatEmbedder {
        session: std::sync::Arc<kchat_encoder::EncoderSession>,
    }

    impl std::fmt::Debug for KchatEmbedder {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("KchatEmbedder")
                .field("model_version", &self.model_version())
                .finish_non_exhaustive()
        }
    }

    impl KchatEmbedder {
        /// Create a new embedder from an ONNX model file and tokenizer.
        pub fn new(
            model_path: &str,
            tokenizer_path: &str,
            quantization: kchat_encoder::Quantization,
            intra_threads: usize,
        ) -> Result<Self, ModelError> {
            let session = kchat_encoder::EncoderSession::new(
                model_path,
                tokenizer_path,
                quantization,
                intra_threads,
            )?;
            Ok(Self {
                session: std::sync::Arc::new(session),
            })
        }

        /// Access the underlying encoder session (for shared use
        /// with safety classification and reranking).
        pub fn session(&self) -> &std::sync::Arc<kchat_encoder::EncoderSession> {
            &self.session
        }
    }

    impl TextEmbedder for KchatEmbedder {
        fn embed(&self, text: &str) -> Result<Vec<f32>, ModelError> {
            let head = kchat_encoder::EmbedHead::new(&self.session);
            let embedding = head.embed(text)?;
            Ok(embedding)
        }

        fn model_version(&self) -> &str {
            "kchat-encoder-xlmr@v1"
        }
    }

    /// Mock embedder backed by kchat-encoder's mock implementation
    /// (deterministic, no ONNX required — for tests).
    #[derive(Debug, Default)]
    pub struct MockKchatEmbedder;

    impl TextEmbedder for MockKchatEmbedder {
        fn embed(&self, text: &str) -> Result<Vec<f32>, ModelError> {
            let head = kchat_encoder::MockEmbedHead::new(kchat_encoder::EMBEDDING_DIM);
            Ok(head.embed(text)?)
        }

        fn model_version(&self) -> &str {
            "kchat-encoder-mock@v0"
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stub_embedder_returns_correct_dimension() {
        // CRITICAL: StubEmbedder must return the same dimension as
        // kchat-encoder (768), otherwise vectors stored with the stub
        // would be incompatible with real embeddings.
        let embedder = StubEmbedder;
        let vec = embedder.embed("test").unwrap();
        assert_eq!(vec.len(), EMBEDDING_DIM, "StubEmbedder dimension mismatch");
        assert_eq!(vec.len(), 768, "EMBEDDING_DIM must be 768 to match kchat-encoder");
        assert!(vec.iter().all(|&v| v == 0.0), "stub must return zeros");
    }

    #[test]
    fn stub_embedder_model_version_is_stable() {
        let embedder = StubEmbedder;
        assert_eq!(embedder.model_version(), "stub@v0");
    }

    #[cfg(feature = "ml")]
    #[test]
    fn mock_kchat_embedder_matches_encoder_dim() {
        let embedder = crate::models::embeddings::kchat::MockKchatEmbedder;
        let vec = embedder.embed("test").unwrap();
        assert_eq!(
            vec.len(),
            kchat_encoder::EMBEDDING_DIM,
            "MockKchatEmbedder dimension must match kchat-encoder"
        );
    }

    #[cfg(feature = "ml")]
    #[test]
    fn mock_kchat_embedder_is_deterministic() {
        let embedder = crate::models::embeddings::kchat::MockKchatEmbedder;
        let a = embedder.embed("hello").unwrap();
        let b = embedder.embed("hello").unwrap();
        assert_eq!(a, b, "MockKchatEmbedder must be deterministic for same input");
    }
}
