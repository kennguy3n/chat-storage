//! Archive download — fetch segments and manifests from the gateway.

use crate::formats::archive_segment::ArchiveSegmentFrame;
use crate::transport::ChatStorageTransport;

/// Download an archive segment from the gateway.
pub fn download_segment(
    transport: &dyn ChatStorageTransport,
    segment_id: &str,
) -> Result<ArchiveSegmentFrame, crate::Error> {
    let ciphertext = transport
        .download_archive_segment(segment_id)
        .map_err(|e| crate::Error::Storage(e.to_string().into()))?;
    let frame: ArchiveSegmentFrame = serde_json::from_slice(&ciphertext)
        .map_err(|e| crate::Error::Storage(e.to_string().into()))?;
    Ok(frame)
}

/// Download archive manifests after a given generation.
pub fn download_manifests(
    transport: &dyn ChatStorageTransport,
    after_generation: u64,
) -> Result<Vec<Vec<u8>>, crate::Error> {
    transport
        .fetch_archive_manifests(after_generation)
        .map_err(|e| crate::Error::Storage(e.to_string().into()))
}
