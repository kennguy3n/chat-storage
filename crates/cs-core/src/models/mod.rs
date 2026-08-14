//! On-device ML models — text embeddings, image/video, audio, OCR.
//!
//! When the `ml` cargo feature is enabled, these modules delegate to
//! the `kchat-ai-runtime` crates (`kchat-encoder`, `kchat-safety`,
//! `kchat-asr`, `kchat-core`) which provide real ONNX Runtime
//! inference. Without the feature, stub implementations return
//! `ModelError::NotAvailable` so the rest of the pipeline compiles
//! and runs without ML.

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

    #[error("ORT ({op}): {detail}")]
    Ort { op: &'static str, detail: String },

    #[error("tokenizer ({op}): {detail}")]
    Tokenizer { op: &'static str, detail: String },

    #[error("audio decode ({op}): {detail}")]
    MediaDecode { op: &'static str, detail: String },

    #[error("model `{0}` not cached")]
    NotCached(&'static str),

    #[error("`{0}` lock poisoned")]
    LockPoisoned(&'static str),

    #[error("{0}")]
    Custom(String),
}

impl From<String> for ModelError {
    fn from(s: String) -> Self {
        ModelError::Custom(s)
    }
}

// Bridge: convert kchat-ai-runtime errors into ModelError.

#[cfg(feature = "ml")]
impl From<kchat_encoder::EncoderError> for ModelError {
    fn from(e: kchat_encoder::EncoderError) -> Self {
        match e {
            kchat_encoder::EncoderError::InferenceFailed(d) => ModelError::Inference(d),
            kchat_encoder::EncoderError::TokenizerError(d) => ModelError::Tokenizer {
                op: "encoder",
                detail: d,
            },
            kchat_encoder::EncoderError::SessionError(d) => ModelError::Load(d),
            kchat_encoder::EncoderError::DimensionMismatch { expected, actual } => {
                ModelError::Inference(format!("dim mismatch: expected {expected}, got {actual}"))
            }
        }
    }
}

#[cfg(feature = "ml")]
impl From<kchat_asr::AsrError> for ModelError {
    fn from(e: kchat_asr::AsrError) -> Self {
        match e {
            kchat_asr::AsrError::AudioDecode { op, detail } => ModelError::MediaDecode { op, detail },
            kchat_asr::AsrError::Ort { op, detail } => ModelError::Ort { op, detail },
            kchat_asr::AsrError::Tokenizer { op, detail } => ModelError::Tokenizer { op, detail },
            kchat_asr::AsrError::NotCached(s) => ModelError::NotCached(s),
            kchat_asr::AsrError::LockPoisoned(s) => ModelError::LockPoisoned(s),
            kchat_asr::AsrError::Custom(s) => ModelError::Custom(s),
        }
    }
}

#[cfg(feature = "ml")]
impl From<kchat_core::error::CoreError> for ModelError {
    fn from(e: kchat_core::error::CoreError) -> Self {
        ModelError::Custom(e.to_string())
    }
}

#[cfg(feature = "ml-vision")]
impl From<kchat_safety::vision::MobileClipSessionError> for ModelError {
    fn from(e: kchat_safety::vision::MobileClipSessionError) -> Self {
        match e {
            kchat_safety::vision::MobileClipSessionError::LoadFailed { reason } => {
                ModelError::Load(reason)
            }
            kchat_safety::vision::MobileClipSessionError::InvalidGraph { reason } => {
                ModelError::Load(reason)
            }
            kchat_safety::vision::MobileClipSessionError::TensorBuildFailed { reason } => {
                ModelError::Ort {
                    op: "mobileclip_tensor_build",
                    detail: reason,
                }
            }
            kchat_safety::vision::MobileClipSessionError::InferenceFailed { reason } => {
                ModelError::Inference(reason)
            }
            kchat_safety::vision::MobileClipSessionError::UnexpectedOutputShape { got } => {
                ModelError::Inference(format!("unexpected output shape: {got:?}"))
            }
        }
    }
}

#[cfg(feature = "ml-vision")]
impl From<kchat_safety::vision::VisionEncoderError> for ModelError {
    fn from(e: kchat_safety::vision::VisionEncoderError) -> Self {
        ModelError::Inference(e.to_string())
    }
}

#[cfg(feature = "ml-vision")]
impl From<kchat_safety::vision::VisionImagePreprocessError> for ModelError {
    fn from(e: kchat_safety::vision::VisionImagePreprocessError) -> Self {
        ModelError::MediaDecode {
            op: "vision_preprocess",
            detail: e.to_string(),
        }
    }
}

#[cfg(feature = "ml-vision")]
impl From<kchat_safety::vision::FrameAggregationError> for ModelError {
    fn from(e: kchat_safety::vision::FrameAggregationError) -> Self {
        ModelError::Inference(e.to_string())
    }
}
