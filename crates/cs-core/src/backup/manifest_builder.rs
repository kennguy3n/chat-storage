//! Backup manifest builder — chained manifests with hybrid signatures.

use crate::crypto::content_hash;
use crate::formats::manifest::*;
use uuid::Uuid;

/// Build a new backup manifest generation.
pub fn build_manifest(
    generation: u64,
    previous_manifest_hash: [u8; 32],
    segments: Vec<ManifestSegmentRef>,
    _wrapped_epoch_keys: Vec<WrappedEpochKeyRef>,
) -> Result<BackupManifest, crate::Error> {
    Ok(BackupManifest {
        magic: BACKUP_MANIFEST_MAGIC.to_string(),
        version: MANIFEST_VERSION,
        manifest_id: Uuid::now_v7(),
        generation,
        previous_manifest_hash,
        segments,
        search_index_shards: Vec::new(),
        media_references: Vec::new(),
        tombstones: Vec::new(),
        merkle_root: [0u8; 32],
        manifest_signature: Vec::new(),
        pqc_signature: Vec::new(),
    })
}

/// Compute the hash of a manifest (for chaining).
pub fn manifest_hash(manifest: &BackupManifest) -> Result<[u8; 32], crate::Error> {
    let data =
        crate::cbor::to_vec(manifest).map_err(|e| crate::Error::Storage(e.to_string().into()))?;
    Ok(content_hash(&data))
}
