//! Media processor — generates thumbnails, extracts metadata, queues ML tasks.

use crate::formats::media_descriptor::MediaDescriptor;
use uuid::Uuid;

/// Process a media file and produce a descriptor.
pub fn process_media(
    asset_id: &str,
    mime_type: &str,
    plaintext: &[u8],
    blob_id: &str,
    node_id: &str,
    version_id: &str,
) -> Result<MediaDescriptor, crate::Error> {
    let chunk_count = crate::media::chunker::chunk(plaintext).len() as u32;
    let merkle_root = crate::crypto::content_hash(plaintext);

    Ok(MediaDescriptor {
        asset_id: Uuid::parse_str(asset_id).unwrap_or_else(|_| Uuid::now_v7()),
        mime_type: mime_type.to_string(),
        bytes_total: plaintext.len() as u64,
        chunk_count,
        merkle_root,
        blob_id: Uuid::parse_str(blob_id).unwrap_or_else(|_| Uuid::now_v7()),
        wrapped_k_asset: Vec::new(),
        storage_sink: None,
        width: 0,
        height: 0,
        duration_ms: 0,
        thumbnail_ref: None,
        node_id: node_id.to_string(),
        version_id: version_id.to_string(),
    })
}
