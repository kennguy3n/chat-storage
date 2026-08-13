//! Video keyframe extraction + embedding (stub).

use crate::models::ModelError;

pub fn extract_keyframes(_video: &[u8]) -> Result<Vec<Vec<u8>>, ModelError> {
    Err(ModelError::NotAvailable(
        "video keyframe extraction not available".into(),
    ))
}
