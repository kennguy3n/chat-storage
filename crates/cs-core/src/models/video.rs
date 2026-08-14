//! Video keyframe extraction + embedding — via kchat-safety vision when `ml` is enabled.

use crate::models::ModelError;

/// Extract keyframes from a video byte buffer.
///
/// Without the `ml` feature, returns `NotAvailable`.
/// With `ml-vision`, this would delegate to kchat-safety's vision
/// module for MobileCLIP-S2 image embedding of extracted frames.
pub fn extract_keyframes(_video: &[u8]) -> Result<Vec<Vec<u8>>, ModelError> {
    Err(ModelError::NotAvailable(
        "video keyframe extraction not available without ml-vision feature".into(),
    ))
}

/// kchat-safety vision re-exports (available with `ml-vision`).
#[cfg(feature = "ml-vision")]
pub mod kchat {
    pub use kchat_safety::vision::{
        aggregate_frame_verdicts, preprocess_image, MobileClipSession,
        VisionEncoderAdapter, VisionImageClassifier,
        MOBILECLIP_EMBED_DIM, MOBILECLIP_IMAGE_SIZE,
    };
}
