//! Whisper audio transcription — via kchat-asr when `ml` feature is enabled.
//!
//! Without `ml`, returns `NotAvailable`.
//!
//! # Architecture
//!
//! The free function [`transcribe_bytes`] returns `NotAvailable` when
//! `ml` is enabled because Whisper inference requires model artifacts
//! (encoder/decoder ONNX files + tokenizer) that must be loaded from
//! disk. Callers that need real transcription should construct a
//! [`kchat::KchatWhisperTranscriber`] directly with the model paths
//! and reuse it across calls to amortize the session-load cost.

use crate::models::ModelError;

/// Transcribe raw f32 PCM samples to text.
///
/// `samples` is interleaved f32 PCM at `sample_rate` Hz.
/// Without the `ml` feature, this always returns `NotAvailable`.
/// With `ml`, use [`kchat::KchatWhisperTranscriber`] instead —
/// this free function cannot run inference without a loaded model.
pub fn transcribe(_samples: &[f32], _sample_rate: u32) -> Result<String, ModelError> {
    Err(ModelError::NotAvailable(
        "whisper not available without ml feature; \
         with ml, use KchatWhisperTranscriber instead"
            .into(),
    ))
}

/// Transcribe raw audio bytes (e.g. WAV) to text.
///
/// **Without `ml`:** returns `NotAvailable`.
///
/// **With `ml`:** returns `NotAvailable` with a message directing
/// the caller to [`kchat::KchatWhisperTranscriber`], which requires
/// model artifact paths to construct an ONNX session. The free
/// function API cannot load model files from an unknown location.
pub fn transcribe_bytes(_audio_data: &[u8], _mime_type: &str) -> Result<String, ModelError> {
    #[cfg(feature = "ml")]
    {
        Err(ModelError::NotAvailable(
            "transcribe_bytes cannot run Whisper inference without model paths; \
             construct a kchat::KchatWhisperTranscriber with encoder/decoder/tokenizer paths instead"
                .into(),
        ))
    }
    #[cfg(not(feature = "ml"))]
    {
        Err(ModelError::NotAvailable(
            "whisper not available without ml feature".into(),
        ))
    }
}

/// kchat-asr-backed transcriber (available with `ml` feature).
#[cfg(feature = "ml")]
pub mod kchat {
    use crate::models::ModelError;
    use std::path::Path;

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
        /// Create a new transcriber from a directory containing
        /// `encoder_model.onnx`, `decoder_model.onnx`, and
        /// `tokenizer.json` (canonical HuggingFace layout).
        ///
        /// `intra_threads` controls ONNX Runtime intra-op
        /// parallelism. Use 2 for low-tier devices, 3 for medium,
        /// 4+ for high-tier.
        pub fn new_from_dir(
            encoder_dir: &Path,
            intra_threads: usize,
        ) -> Result<Self, ModelError> {
            let inner = kchat_asr::onnx_session::OnnxWhisperTranscriber::new(
                encoder_dir,
                intra_threads,
            )?;
            Ok(Self { inner })
        }

        /// Create a new transcriber from explicit paths to the
        /// encoder ONNX, decoder ONNX, and tokenizer JSON files.
        ///
        /// `intra_threads` controls ONNX Runtime intra-op
        /// parallelism.
        pub fn new(
            encoder_path: &Path,
            decoder_path: &Path,
            tokenizer_path: &Path,
            intra_threads: usize,
        ) -> Result<Self, ModelError> {
            let inner = kchat_asr::onnx_session::OnnxWhisperTranscriber::new_with_paths(
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

    impl WhisperTranscriber for KchatWhisperTranscriber {
        fn transcribe(
            &self,
            audio_data: &[u8],
            mime_type: &str,
        ) -> Result<TranscriptionResult, kchat_asr::AsrError> {
            self.inner.transcribe(audio_data, mime_type)
        }
    }
}
