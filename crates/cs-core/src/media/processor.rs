//! Media processor — generates thumbnails, extracts metadata, queues ML tasks.

use crate::formats::media_descriptor::MediaDescriptor;

/// Process a media file and produce a descriptor.
pub fn process_media(
    asset_id: &str,
    mime_type: &str,
    plaintext: &[u8],
    node_id: &str,
    version_id: &str,
) -> Result<MediaDescriptor, crate::Error> {
    let chunk_count = crate::media::chunker::chunk(plaintext).len() as u64;
    let merkle_root = crate::crypto::content_hash(plaintext);

    Ok(MediaDescriptor {
        asset_id: asset_id.to_string(),
        mime_type: mime_type.to_string(),
        width: 0,
        height: 0,
        duration_ms: 0,
        chunk_count,
        merkle_root,
        wrapped_k_asset: Vec::new(),
        thumbnail_ref: None,
        node_id: node_id.to_string(),
        version_id: version_id.to_string(),
    })
}
