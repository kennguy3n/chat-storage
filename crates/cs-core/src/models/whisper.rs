//! Whisper audio transcription (stub).

use crate::models::ModelError;

pub fn transcribe(_audio: &[f32], _sample_rate: u32) -> Result<String, ModelError> {
    Err(ModelError::NotAvailable(
        "whisper not available without onnx feature".into(),
    ))
}
