//! kdrive gateway backup sink.

use crate::transport::ChatStorageTransport;

/// Upload a backup segment to the kdrive gateway.
/// Reuses the archive segment upload endpoint (segments are generic encrypted blobs).
pub fn upload_segment(
    transport: &dyn ChatStorageTransport,
    segment_id: &str,
    ciphertext: &[u8],
) -> Result<String, crate::Error> {
    transport
        .upload_archive_segment(segment_id, ciphertext)
        .map_err(|e| crate::Error::Storage(e.to_string().into()))
}

/// Upload a backup manifest to the kdrive gateway.
pub fn upload_manifest(
    transport: &dyn ChatStorageTransport,
    manifest: &[u8],
) -> Result<(), crate::Error> {
    transport
        .upload_backup_manifest(manifest)
        .map_err(|e| crate::Error::Storage(e.to_string().into()))
}

/// Download backup manifests after a given generation.
pub fn download_manifests(
    transport: &dyn ChatStorageTransport,
    after_generation: u64,
) -> Result<Vec<Vec<u8>>, crate::Error> {
    transport
        .fetch_backup_manifests(after_generation)
        .map_err(|e| crate::Error::Storage(e.to_string().into()))
}
