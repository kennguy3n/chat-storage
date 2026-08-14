//! Media descriptor wire format.
//!
//! The descriptor binds the high-level message-layer view of a media
//! object (`asset_id`, `mime_type`) to the storage-layer view
//! (`blob_id`, chunk count, Merkle root) and to the encrypted key
//! material (`wrapped_k_asset`). KDRV1-specific fields (`node_id`,
//! `version_id`) link the asset to its kdrive object.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::serde_bytes_array;

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

/// Encrypted-media descriptor.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MediaDescriptor {
    /// Stable identifier for the media asset.
    pub asset_id: Uuid,

    /// IANA media type (e.g. `image/jpeg`, `video/mp4`).
    pub mime_type: String,

    /// Plaintext byte length of the asset.
    pub bytes_total: u64,

    /// Number of encrypted chunks the asset was split into.
    pub chunk_count: u32,

    /// 32-byte BLAKE3 Merkle root over the per-chunk SHA-256 hashes
    /// of the ciphertext chunks.
    #[serde(with = "serde_bytes_array")]
    pub merkle_root: [u8; 32],

    /// Backend blob identifier.
    pub blob_id: Uuid,

    /// `K_asset` wrapped by the appropriate root (one of `K_local_db`,
    /// `K_archive_root`, `K_backup_root`). The wrap algorithm is
    /// AES-256-KW (RFC 3394).
    #[serde(with = "serde_bytes")]
    pub wrapped_k_asset: Vec<u8>,

    /// Storage sink tag for the media blob (`"kchat_backend"`,
    /// `"zk_object_fabric"`). `None` means the default sink.
    #[serde(default)]
    pub storage_sink: Option<String>,

    // --- KDRV1-specific fields ---
    /// Width in pixels (images/video), 0 for audio.
    #[serde(default)]
    pub width: u32,

    /// Height in pixels (images/video), 0 for audio.
    #[serde(default)]
    pub height: u32,

    /// Duration in milliseconds (audio/video), 0 for images.
    #[serde(default)]
    pub duration_ms: u64,

    /// Thumbnail reference (blob key on gateway or local cache path).
    #[serde(default)]
    pub thumbnail_ref: Option<String>,

    /// KDRV1 node ID (hex).
    #[serde(default)]
    pub node_id: String,

    /// KDRV1 version ID (hex).
    #[serde(default)]
    pub version_id: String,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(seed: u8) -> MediaDescriptor {
        MediaDescriptor {
            asset_id: Uuid::now_v7(),
            mime_type: format!(
                "image/{}",
                if seed.is_multiple_of(2) {
                    "jpeg"
                } else {
                    "png"
                }
            ),
            bytes_total: 1_048_576 + u64::from(seed),
            chunk_count: 4,
            merkle_root: [seed; 32],
            blob_id: Uuid::now_v7(),
            wrapped_k_asset: vec![seed; 40],
            storage_sink: None,
            width: 1920,
            height: 1080,
            duration_ms: 0,
            thumbnail_ref: Some("thumb-001".to_string()),
            node_id: "abc123".to_string(),
            version_id: "v1".to_string(),
        }
    }

    #[test]
    fn media_descriptor_round_trips_through_cbor() {
        let desc = sample(0x07);
        let bytes = crate::cbor::to_vec(&desc).expect("encode");
        let decoded: MediaDescriptor = crate::cbor::from_slice(&bytes).expect("decode");
        assert_eq!(decoded, desc);
    }

    #[test]
    fn distinct_descriptors_produce_distinct_cbor() {
        let a = sample(0x01);
        let b = sample(0x02);
        let bytes_a = crate::cbor::to_vec(&a).unwrap();
        let bytes_b = crate::cbor::to_vec(&b).unwrap();
        assert_ne!(bytes_a, bytes_b);
    }

    #[test]
    fn all_fields_survive_round_trip() {
        let desc = MediaDescriptor {
            asset_id: Uuid::now_v7(),
            mime_type: "video/mp4".to_string(),
            bytes_total: 2 * 1024 * 1024 * 1024 + 17,
            chunk_count: 137,
            merkle_root: {
                let mut m = [0u8; 32];
                m.iter_mut()
                    .enumerate()
                    .for_each(|(i, b)| *b = i as u8 ^ 0xA5);
                m
            },
            blob_id: Uuid::now_v7(),
            wrapped_k_asset: (0..40u8).collect(),
            storage_sink: Some("zk_object_fabric".to_string()),
            width: 3840,
            height: 2160,
            duration_ms: 65_000,
            thumbnail_ref: None,
            node_id: "node-xyz".to_string(),
            version_id: "v2".to_string(),
        };
        let bytes = crate::cbor::to_vec(&desc).unwrap();
        let decoded: MediaDescriptor = crate::cbor::from_slice(&bytes).unwrap();
        assert_eq!(decoded, desc);
    }

    #[test]
    fn legacy_payload_without_kdrv_fields_decodes_with_defaults() {
        #[derive(Serialize)]
        struct LegacyMediaDescriptor {
            asset_id: Uuid,
            mime_type: String,
            bytes_total: u64,
            chunk_count: u32,
            #[serde(with = "serde_bytes_array")]
            merkle_root: [u8; 32],
            blob_id: Uuid,
            #[serde(with = "serde_bytes")]
            wrapped_k_asset: Vec<u8>,
        }

        let legacy = LegacyMediaDescriptor {
            asset_id: Uuid::now_v7(),
            mime_type: "image/jpeg".to_string(),
            bytes_total: 4096,
            chunk_count: 1,
            merkle_root: [0x42; 32],
            blob_id: Uuid::now_v7(),
            wrapped_k_asset: vec![0x99; 40],
        };
        let bytes = crate::cbor::to_vec(&legacy).unwrap();
        let decoded: MediaDescriptor = crate::cbor::from_slice(&bytes).unwrap();
        assert_eq!(decoded.asset_id, legacy.asset_id);
        assert_eq!(decoded.bytes_total, legacy.bytes_total);
        assert_eq!(decoded.width, 0);
        assert_eq!(decoded.height, 0);
        assert_eq!(decoded.node_id, "");
        assert_eq!(decoded.version_id, "");
    }

    #[test]
    fn merkle_root_serialised_as_cbor_byte_string() {
        let desc = sample(0xAA);
        let bytes = crate::cbor::to_vec(&desc).unwrap();
        assert!(
            bytes.windows(2).any(|w| w == [0x58, 0x20]),
            "expected CBOR byte-string header for the 32-byte Merkle root, got {:02x?}",
            bytes,
        );
    }
}
