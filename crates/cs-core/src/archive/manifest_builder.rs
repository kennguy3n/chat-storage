//! Archive manifest builder — chained, signed manifests.

use crate::crypto::content_hash;
use crate::formats::backup_manifest::*;

/// Build a new archive manifest generation.
pub fn build_manifest(
    generation: u64,
    previous_manifest_hash: [u8; 32],
    segments: Vec<SegmentRef>,
    wrapped_epoch_keys: Vec<WrappedEpochKey>,
) -> Result<BackupManifestPayload, crate::Error> {
    Ok(BackupManifestPayload {
        generation,
        previous_manifest_hash,
        segments,
        wrapped_epoch_keys,
        created_at_ms: now_ms(),
    })
}

/// Compute the hash of a manifest payload (for chaining).
pub fn manifest_hash(payload: &BackupManifestPayload) -> Result<[u8; 32], crate::Error> {
    let data = encode_payload(payload)?;
    Ok(content_hash(&data))
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}
