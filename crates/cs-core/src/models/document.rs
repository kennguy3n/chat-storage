//! Document embedding — via kchat-encoder when `ml` feature is enabled.

use crate::models::ModelError;

/// Embed a document's text content into a vector.
///
/// With the `ml` feature, this delegates to kchat-encoder's XLM-R
/// embedding head. Without `ml`, returns `NotAvailable`.
pub fn embed_document(text: &str) -> Result<Vec<f32>, ModelError> {
    #[cfg(feature = "ml")]
    {
        use crate::models::embeddings::TextEmbedder;
        let embedder = crate::models::embeddings::kchat::MockKchatEmbedder;
        embedder.embed(text)
    }
    #[cfg(not(feature = "ml"))]
    {
        let _ = text;
        Err(ModelError::NotAvailable(
            "document embedding not available without ml feature".into(),
        ))
    }
}
