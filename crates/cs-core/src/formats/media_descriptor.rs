//! Media descriptor — describes a media asset attached to a message.

use serde::{Deserialize, Serialize};

/// Media descriptor (analogous to chat-storage-search §3.2).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaDescriptor {
    pub asset_id: String,
    pub mime_type: String,
    /// Width in pixels (images/video), 0 for audio.
    pub width: u32,
    /// Height in pixels (images/video), 0 for audio.
    pub height: u32,
    /// Duration in milliseconds (audio/video), 0 for images.
    pub duration_ms: u64,
    /// Number of KDRV1 chunks.
    pub chunk_count: u64,
    /// Merkle root of the chunk plan (from KDRV1).
    pub merkle_root: [u8; 32],
    /// Wrapped K_asset (media encryption key, wrapped by K_local_db).
    pub wrapped_k_asset: Vec<u8>,
    /// Thumbnail reference (blob key on gateway or local cache path).
    pub thumbnail_ref: Option<String>,
    /// KDRV1 node ID (hex).
    pub node_id: String,
    /// KDRV1 version ID (hex).
    pub version_id: String,
}

/// Media kind classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MediaKind {
    Image,
    Video,
    Audio,
    Document,
    Sticker,
    Gif,
}

impl MediaDescriptor {
    /// Classify the media kind from the MIME type.
    pub fn kind(&self) -> MediaKind {
        match self.mime_type.split('/').next().unwrap_or("") {
            "image" => {
                if self.mime_type.contains("gif") {
                    MediaKind::Gif
                } else {
                    MediaKind::Image
                }
            }
            "video" => MediaKind::Video,
            "audio" => MediaKind::Audio,
            _ => MediaKind::Document,
        }
    }
}
