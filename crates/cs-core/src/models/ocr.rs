//! OCR — via kchat-core's OcrBridge when `ml` feature is enabled.
//!
//! # Architecture
//!
//! The free function [`extract_text`] uses [`kchat_core::ocr::SkipOcrBridge`]
//! when `ml` is enabled, which returns empty results (no text found)
//! rather than an error. This lets the media-indexing pipeline keep
//! processing the batch. Callers that have a real platform OCR backend
//! (iOS Vision, Android ML Kit, etc.) should use
//! [`kchat::extract_text_with_bridge`] with their bridge implementation.

use crate::models::ModelError;

/// Extract text from an image.
///
/// **Without `ml`:** returns `NotAvailable`.
/// **With `ml`:** uses [`kchat_core::ocr::SkipOcrBridge`] which
/// returns an empty string (no text found). To get real OCR results,
/// construct a platform-specific [`kchat::OcrBridge`] implementation
/// and call [`kchat::extract_text_with_bridge`] instead.
pub fn extract_text(image: &[u8]) -> Result<String, ModelError> {
    #[cfg(feature = "ml")]
    {
        use kchat_core::ocr::{OcrBridge, SkipOcrBridge};
        let bridge = SkipOcrBridge;
        let results = bridge
            .recognize_text(image, "image/png")
            .map_err(|e| ModelError::Custom(e.to_string()))?;
        Ok(results
            .into_iter()
            .map(|r| r.text)
            .collect::<Vec<_>>()
            .join("\n"))
    }
    #[cfg(not(feature = "ml"))]
    {
        let _ = image;
        Err(ModelError::NotAvailable("ocr not available".into()))
    }
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
