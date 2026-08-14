//! Video keyframe extraction + embedding — via kchat-safety vision when `ml-vision` is enabled.
//!
//! # Architecture
//!
//! Keyframe extraction from video byte buffers requires a video
//! decoder (FFmpeg, AVFoundation, etc.) which is NOT provided by
//! `kchat-ai-runtime`. The [`extract_keyframes`] free function
//! returns `NotAvailable` because there is no video decoder in the
//! runtime. Callers that have their own video decoder should:
//!
//! 1. Decode the video into individual frame images (JPEG/PNG bytes).
//! 2. Pass each frame to [`kchat::preprocess_image`] and
//!    [`kchat::VisionImageClassifier`] for MobileCLIP-S2 embedding
//!    and classification.
//! 3. Use [`kchat::aggregate_frame_verdicts`] to combine per-frame
//!    verdicts into a single video-level safety descriptor.

use crate::models::ModelError;

/// Extract keyframes from a video byte buffer.
///
/// Always returns `NotAvailable` — `kchat-ai-runtime` does not
/// include a video decoder. Callers with their own decoder should
/// pass individual frames to the `kchat::VisionImageClassifier`.
pub fn extract_keyframes(_video: &[u8]) -> Result<Vec<Vec<u8>>, ModelError> {
    Err(ModelError::NotAvailable(
        "video keyframe extraction requires a video decoder not present \
         in kchat-ai-runtime; decode frames externally and pass them \
         to kchat::VisionImageClassifier instead"
            .into(),
    ))
}

/// kchat-safety vision re-exports (available with `ml-vision`).
///
/// These types let callers with their own video decoder run
/// MobileCLIP-S2 image classification on individual frames and
/// aggregate the results.
#[cfg(feature = "ml-vision")]
pub mod kchat {
    pub use kchat_safety::vision::{
        aggregate_frame_verdicts, aggregate_frame_verdicts_smoothed, preprocess_image,
        FrameVerdict, MobileClipSession, MobileClipSessionError, TemporalSmoothingConfig,
        VisionEncoderAdapter, VisionEncoderAdapterBuilder, VisionEncoderError,
        VisionEncoderVerdict, VisionImageClassifier, VisionImagePreprocessError,
        MOBILECLIP_EMBED_DIM, MOBILECLIP_IMAGE_SIZE,
    };
}
