//! Archive upload — upload segments and manifests via kdrive transport.

use crate::formats::archive_segment::ArchiveSegmentFrame;
use crate::transport::ChatStorageTransport;

/// Upload an archive segment to the gateway.
pub fn upload_segment(
    transport: &dyn ChatStorageTransport,
    segment_id: &str,
    frame: &ArchiveSegmentFrame,
) -> Result<String, crate::Error> {
    let payload =
        serde_json::to_vec(frame).map_err(|e| crate::Error::Storage(e.to_string().into()))?;
    transport
        .upload_archive_segment(segment_id, &payload)
        .map_err(|e| crate::Error::Storage(e.to_string().into()))
}

/// Upload an archive manifest to the gateway.
pub fn upload_manifest(
    transport: &dyn ChatStorageTransport,
    manifest: &[u8],
) -> Result<(), crate::Error> {
    transport
        .upload_archive_manifest(manifest)
        .map_err(|e| crate::Error::Storage(e.to_string().into()))
}
