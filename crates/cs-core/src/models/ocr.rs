//! OCR (stub — platform-specific bridges needed).

use crate::models::ModelError;

pub fn extract_text(_image: &[u8]) -> Result<String, ModelError> {
    Err(ModelError::NotAvailable("ocr not available".into()))
}
