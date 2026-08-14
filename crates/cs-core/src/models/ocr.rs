//! OCR — via kchat-core's OcrBridge when `ml` feature is enabled.
//!
//! Without `ml`, returns `NotAvailable`.

use crate::models::ModelError;

/// Extract text from an image.
///
/// Without the `ml` feature, this always returns `NotAvailable`.
pub fn extract_text(_image: &[u8]) -> Result<String, ModelError> {
    Err(ModelError::NotAvailable("ocr not available".into()))
}

/// kchat-core-backed OCR (available with `ml` feature).
#[cfg(feature = "ml")]
pub mod kchat {
    use crate::models::ModelError;

    pub use kchat_core::ocr::{
        BoundingBox, MockOcrBridge, OcrBridge, OcrResult, SkipOcrBridge,
    };

    /// Extract text from an image using a platform OCR bridge.
    ///
    /// Returns the concatenated text from all recognized regions.
    /// Returns `Ok(String::new())` when the image has no text.
    pub fn extract_text_with_bridge(
        bridge: &dyn OcrBridge,
        image_data: &[u8],
        mime_type: &str,
    ) -> Result<String, ModelError> {
        let results = bridge.recognize_text(image_data, mime_type)?;
        Ok(results
            .into_iter()
            .map(|r| r.text)
            .collect::<Vec<_>>()
            .join("\n"))
    }
}
