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
/// `mime_type` should be the image's MIME type (e.g. `"image/png"`,
/// `"image/jpeg"`). It is passed through to the OCR bridge for
/// format-aware implementations.
///
/// **Without `ml`:** returns `NotAvailable`.
/// **With `ml`:** uses [`kchat_core::ocr::SkipOcrBridge`] which
/// returns an empty string (no text found). To get real OCR results,
/// construct a platform-specific [`kchat::OcrBridge`] implementation
/// and call [`kchat::extract_text_with_bridge`] instead.
pub fn extract_text(image: &[u8], mime_type: &str) -> Result<String, ModelError> {
    #[cfg(feature = "ml")]
    {
        use kchat_core::ocr::{OcrBridge, SkipOcrBridge};
        let bridge = SkipOcrBridge;
        let results = bridge
            .recognize_text(image, mime_type)
            .map_err(|e| ModelError::Custom(e.to_string()))?;
        Ok(results
            .into_iter()
            .map(|r| r.text)
            .collect::<Vec<_>>()
            .join("\n"))
    }
    #[cfg(not(feature = "ml"))]
    {
        let _ = (image, mime_type);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(not(feature = "ml"))]
    #[test]
    fn extract_text_returns_not_available_without_ml() {
        let result = extract_text(b"image", "image/png");
        assert!(matches!(result, Err(ModelError::NotAvailable(_))));
    }

    #[cfg(feature = "ml")]
    #[test]
    fn extract_text_returns_empty_with_skip_bridge() {
        // With ml, extract_text uses SkipOcrBridge which returns empty.
        let result = extract_text(b"image", "image/png").unwrap();
        assert!(result.is_empty(), "SkipOcrBridge must return empty text");
    }

    #[cfg(feature = "ml")]
    #[test]
    fn extract_text_with_bridge_returns_concatenated_text() {
        use kchat_core::ocr::{OcrBridge, OcrResult};
        #[derive(Debug)]
        struct FakeBridge;
        impl OcrBridge for FakeBridge {
            fn recognize_text(
                &self,
                _image_data: &[u8],
                _mime_type: &str,
            ) -> kchat_core::error::Result<Vec<OcrResult>> {
                Ok(vec![
                    OcrResult {
                        text: "hello".into(),
                        confidence: 0.9,
                        language: Some("en".into()),
                        bounding_box: None,
                    },
                    OcrResult {
                        text: "world".into(),
                        confidence: 0.8,
                        language: None,
                        bounding_box: None,
                    },
                ])
            }
        }
        let result = kchat::extract_text_with_bridge(&FakeBridge, b"img", "image/jpeg").unwrap();
        assert_eq!(result, "hello\nworld");
    }
}
