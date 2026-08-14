//! Log-mel spectrogram computation for Whisper — via kchat-asr when `ml` is enabled.
//!
//! # Architecture
//!
//! The free function [`log_mel_spectrogram`] computes a log-mel
//! spectrogram from raw f32 PCM samples when `ml` is enabled,
//! delegating to [`kchat_asr::audio`]. The samples are first
//! resampled to 16 kHz mono and padded/truncated to exactly
//! 480 000 samples (30 s) as Whisper's encoder expects.

/// Compute log-mel spectrogram from raw f32 PCM samples.
///
/// Returns a flat `Vec<f32>` of length `WHISPER_N_MELS * WHISPER_N_FRAMES`
/// (240 000) in row-major layout (`out[mel_bin * 3000 + frame]`).
///
/// **Without `ml`:** returns an empty vector.
/// **With `ml`:** delegates to [`kchat_asr::audio`] for the full
/// Whisper-compatible preprocessing pipeline (resample → pad/truncate →
/// Hann window → STFT → Slaney mel filterbank → log10).
pub fn log_mel_spectrogram(samples: &[f32], sample_rate: u32) -> Vec<f32> {
    #[cfg(feature = "ml")]
    {
        use kchat_asr::audio::{
            whisper_pad_or_truncate, whisper_to_mono_16k, DecodedAudio, WhisperMelKernel,
            WHISPER_N_MELS, WHISPER_N_FRAMES,
        };

        // Treat the input as a single-channel `DecodedAudio` so we
        // can reuse the resample + pad/truncate + log-mel pipeline.
        let audio = DecodedAudio {
            samples: samples.to_vec(),
            sample_rate,
            channels: 1,
        };
        let mono_16k = whisper_to_mono_16k(&audio);
        let padded = whisper_pad_or_truncate(mono_16k);
        let kernel = WhisperMelKernel::new();
        match kernel.log_mel(&padded) {
            Ok(mel) => {
                debug_assert_eq!(
                    mel.len(),
                    WHISPER_N_MELS * WHISPER_N_FRAMES,
                    "log_mel output length must be WHISPER_N_MELS * WHISPER_N_FRAMES"
                );
                mel
            }
            Err(e) => {
                tracing::warn!("log_mel_spectrogram failed: {e}");
                Vec::new()
            }
        }
    }
    #[cfg(not(feature = "ml"))]
    {
        let _ = (samples, sample_rate);
        Vec::new()
    }
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

// Compile-time sanity: when `ml` is off, the function still
// compiles without the kchat_asr import.
#[cfg(not(feature = "ml"))]
const _: u32 = 16000; // WHISPER_SAMPLE_RATE
