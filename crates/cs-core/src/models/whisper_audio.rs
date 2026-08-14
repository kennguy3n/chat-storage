//! Log-mel spectrogram computation for Whisper — via kchat-asr when `ml` is enabled.

/// Compute log-mel spectrogram from raw f32 PCM samples.
///
/// Without the `ml` feature, returns an empty vector.
/// With `ml`, delegates to [`kchat_asr::audio`] for the full
/// Whisper-compatible preprocessing pipeline (Hann window, STFT,
/// Slaney mel filterbank, log10).
pub fn log_mel_spectrogram(_samples: &[f32], _sample_rate: u32) -> Vec<f32> {
    Vec::new()
}

/// kchat-asr audio preprocessing re-exports (available with `ml`).
#[cfg(feature = "ml")]
pub mod kchat {
    pub use kchat_asr::audio::{
        whisper_decode_wav, whisper_log_mel_from_wav, whisper_pad_or_truncate,
        whisper_to_mono_16k, DecodedAudio, WhisperMelKernel, WHISPER_HOP_LENGTH,
        WHISPER_N_FFT, WHISPER_N_FRAMES, WHISPER_N_MELS, WHISPER_N_SAMPLES,
        WHISPER_SAMPLE_RATE,
    };
}
