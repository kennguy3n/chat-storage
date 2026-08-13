//! On-device ML models — text embeddings, image/video, audio, OCR.

pub mod document;
pub mod embeddings;
pub mod ocr;
pub mod video;
pub mod whisper;
pub mod whisper_audio;

/// Model errors.
#[derive(Debug, thiserror::Error)]
pub enum ModelError {
    #[error("model not available: {0}")]
    NotAvailable(String),

    #[error("inference: {0}")]
    Inference(String),

    #[error("model load: {0}")]
    Load(String),

    #[error("{0}")]
    Custom(String),
}

impl From<String> for ModelError {
    fn from(s: String) -> Self {
        ModelError::Custom(s)
    }
}
