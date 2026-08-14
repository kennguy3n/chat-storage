//! Whisper audio transcription — via kchat-asr when `ml` feature is enabled.
//!
//! Without `ml`, returns `NotAvailable`.

use crate::models::ModelError;

/// Transcribe raw f32 PCM samples to text.
///
/// `samples` is interleaved f32 PCM at `sample_rate` Hz.
/// Without the `ml` feature, this always returns `NotAvailable`.
pub fn transcribe(_samples: &[f32], _sample_rate: u32) -> Result<String, ModelError> {
    Err(ModelError::NotAvailable(
        "whisper not available without ml feature".into(),
    ))
}

/// Transcribe raw audio bytes (e.g. WAV) to text.
///
/// With the `ml` feature, this delegates to [`kchat_asr`] which
/// handles WAV decoding, mel-spectrogram extraction, and ONNX
/// Whisper encoder/decoder inference.
pub fn transcribe_bytes(audio_data: &[u8], mime_type: &str) -> Result<String, ModelError> {
    #[cfg(feature = "ml")]
    {
        let result = kchat_asr::transcribe::transcribe(audio_data, mime_type)?;
        Ok(result.text)
    }
    #[cfg(not(feature = "ml"))]
    {
        let _ = (audio_data, mime_type);
        Err(ModelError::NotAvailable(
            "whisper not available without ml feature".into(),
        ))
    }
}

/// kchat-asr-backed transcriber (available with `ml` feature).
#[cfg(feature = "ml")]
pub mod kchat {
    use crate::models::ModelError;

    pub use kchat_asr::backend::{
        AudioTranscriber, MockWhisperTranscriber, SkipWhisperTranscriber, TranscriptionResult,
        TranscriptionSegment, WhisperBackend, WhisperTranscriber,
    };

    /// ONNX Whisper transcriber.
    ///
    /// Wraps [`kchat_asr::onnx_session::OnnxWhisperTranscriber`] and
    /// adapts it to the [`WhisperTranscriber`] trait (converting
    /// [`kchat_asr::AsrError`] into [`ModelError`]).
    pub struct KchatWhisperTranscriber {
        inner: kchat_asr::onnx_session::OnnxWhisperTranscriber,
    }

    impl std::fmt::Debug for KchatWhisperTranscriber {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("KchatWhisperTranscriber")
                .finish_non_exhaustive()
        }
    }

    impl KchatWhisperTranscriber {
        /// Create a new transcriber from ONNX model files.
        pub fn new(
            encoder_path: &str,
            decoder_path: &str,
            tokenizer_path: &str,
            intra_threads: usize,
        ) -> Result<Self, ModelError> {
            let inner = kchat_asr::onnx_session::OnnxWhisperTranscriber::new(
                encoder_path,
                decoder_path,
                tokenizer_path,
                intra_threads,
            )?;
            Ok(Self { inner })
        }

        /// Transcribe audio bytes, converting [`kchat_asr::AsrError`] into
        /// [`ModelError`].
        pub fn transcribe(
            &self,
            audio_data: &[u8],
            mime_type: &str,
        ) -> Result<TranscriptionResult, ModelError> {
            Ok(self.inner.transcribe(audio_data, mime_type)?)
        }
    }
}
