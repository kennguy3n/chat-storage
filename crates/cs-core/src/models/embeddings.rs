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
#[derive(Debug, Default, Clone, Copy)]
pub struct StubEmbedder;

impl TextEmbedder for StubEmbedder {
    fn embed(&self, _text: &str) -> Result<Vec<f32>, ModelError> {
        Ok(vec![0.0; 384])
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
