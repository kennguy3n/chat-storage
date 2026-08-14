//! Document embedding — via kchat-encoder when `ml` feature is enabled.
//!
//! # Architecture
//!
//! The free function [`embed_document`] returns `NotAvailable` when
//! `ml` is enabled because ONNX embedding requires model artifacts
//! that must be loaded from disk. Callers that need real embeddings
//! should construct a [`crate::models::embeddings::kchat::KchatEmbedder`]
//! directly with the model paths and reuse it across calls.

use crate::models::ModelError;

/// Embed a document's text content into a vector.
///
/// **Without `ml`:** returns `NotAvailable`.
///
/// **With `ml`:** returns `NotAvailable` with a message directing
/// the caller to [`crate::models::embeddings::kchat::KchatEmbedder`],
/// which requires model artifact paths to construct an ONNX session.
/// The free function API cannot load model files from an unknown
/// location.
pub fn embed_document(_text: &str) -> Result<Vec<f32>, ModelError> {
    #[cfg(feature = "ml")]
    {
        Err(ModelError::NotAvailable(
            "embed_document cannot run ONNX inference without model paths; \
             construct a crate::models::embeddings::kchat::KchatEmbedder with model/tokenizer paths instead"
                .into(),
        ))
    }
    #[cfg(not(feature = "ml"))]
    {
        Err(ModelError::NotAvailable(
            "document embedding not available without ml feature".into(),
        ))
    }
}
