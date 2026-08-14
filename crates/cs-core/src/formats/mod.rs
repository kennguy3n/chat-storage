//! Shared on-wire / on-disk binary formats.
//!
//! Locks the CBOR-encoded frames and manifests that travel
//! between the device, the KChat backend, and the ZK Object Fabric
//! backup sink. Every type in this module:
//!
//! * derives `Serialize` / `Deserialize` and round-trips through
//!   [`crate::cbor::to_vec`] / [`crate::cbor::from_slice`] (which sit
//!   on top of `ciborium`),
//! * carries a literal `magic` field that the deserializer can use to
//!   reject the wrong frame type,
//! * uses `#[serde(with = "serde_bytes")]` on byte arrays so CBOR
//!   emits a compact byte-string instead of an array of integers.

pub mod manifest;
pub mod media_descriptor;
pub mod search_shard;

// Compatibility re-exports for existing callers.
pub mod archive_segment {
    pub use super::manifest::*;
}
pub mod backup_manifest {
    pub use super::manifest::*;
}

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Magic bytes for [`BackupSegmentFrame`]. Exactly 12 ASCII bytes.
pub const BACKUP_SEGMENT_MAGIC: [u8; 12] = *b"KCHAT_BAK_V1";

/// Magic bytes for [`ArchiveSegmentFrame`]. Exactly 12 ASCII bytes.
pub const ARCHIVE_SEGMENT_MAGIC: [u8; 12] = *b"KCHAT_ARC_V1";

/// On-wire `version` field carried by every frame in this module.
pub const FRAME_VERSION: u16 = 1;

/// Segment-type discriminant covering both backup and archive segments.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SegmentType {
    Events,
    MessageDelta,
    TimelineSkeleton,
    MediaKeyDelta,
    SearchTextIndex,
    SearchVectorIndex,
    MediaIndex,
    Checkpoint,
}

impl SegmentType {
    pub fn is_backup_segment(self) -> bool {
        matches!(self, SegmentType::Events)
    }

    pub fn is_archive_segment(self) -> bool {
        !self.is_backup_segment()
    }
}

/// Backup segment frame.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BackupSegmentFrame {
    #[serde(with = "serde_bytes_array")]
    pub magic: [u8; 12],
    pub version: u16,
    pub segment_id: Uuid,
    pub segment_type: SegmentType,
    pub event_seq_from: u64,
    pub event_seq_to: u64,
    #[serde(with = "serde_bytes_array")]
    pub nonce: [u8; 24],
    #[serde(with = "serde_bytes_array")]
    pub aad_hash: [u8; 32],
    #[serde(with = "serde_bytes")]
    pub ciphertext: Vec<u8>,
    #[serde(with = "serde_bytes_array")]
    pub ciphertext_sha256: [u8; 32],
}

impl BackupSegmentFrame {
    pub fn has_valid_header(&self) -> bool {
        self.magic == BACKUP_SEGMENT_MAGIC && self.version == FRAME_VERSION
    }
}

/// Archive segment frame.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ArchiveSegmentFrame {
    #[serde(with = "serde_bytes_array")]
    pub magic: [u8; 12],
    pub version: u16,
    pub segment_id: Uuid,
    pub segment_type: SegmentType,
    pub event_seq_from: u64,
    pub event_seq_to: u64,
    #[serde(with = "serde_bytes_array")]
    pub nonce: [u8; 24],
    #[serde(with = "serde_bytes_array")]
    pub aad_hash: [u8; 32],
    #[serde(with = "serde_bytes")]
    pub ciphertext: Vec<u8>,
    #[serde(with = "serde_bytes_array")]
    pub ciphertext_sha256: [u8; 32],
}

impl ArchiveSegmentFrame {
    pub fn has_valid_header(&self) -> bool {
        self.magic == ARCHIVE_SEGMENT_MAGIC && self.version == FRAME_VERSION
    }
}

/// `serde_bytes` for fixed-size byte arrays.
pub(crate) mod serde_bytes_array {
    use serde::de::Error as _;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S, const N: usize>(bytes: &[u8; N], ser: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serde_bytes::Bytes::new(bytes).serialize(ser)
    }

    pub fn deserialize<'de, D, const N: usize>(de: D) -> Result<[u8; N], D::Error>
    where
        D: Deserializer<'de>,
    {
        let bytes = <serde_bytes::ByteBuf>::deserialize(de)?;
        let bytes = bytes.into_vec();
        if bytes.len() != N {
            return Err(D::Error::custom(format!(
                "expected {} bytes, got {}",
                N,
                bytes.len()
            )));
        }
        let mut out = [0u8; N];
        out.copy_from_slice(&bytes);
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_backup_segment() -> BackupSegmentFrame {
        BackupSegmentFrame {
            magic: BACKUP_SEGMENT_MAGIC,
            version: FRAME_VERSION,
            segment_id: Uuid::now_v7(),
            segment_type: SegmentType::Events,
            event_seq_from: 0,
            event_seq_to: 1023,
            nonce: [0x11; 24],
            aad_hash: [0x22; 32],
            ciphertext: b"sealed-zstd-cbor-events".to_vec(),
            ciphertext_sha256: [0x33; 32],
        }
    }

    fn sample_archive_segment(segment_type: SegmentType) -> ArchiveSegmentFrame {
        ArchiveSegmentFrame {
            magic: ARCHIVE_SEGMENT_MAGIC,
            version: FRAME_VERSION,
            segment_id: Uuid::now_v7(),
            segment_type,
            event_seq_from: 1024,
            event_seq_to: 2047,
            nonce: [0x44; 24],
            aad_hash: [0x55; 32],
            ciphertext: b"sealed-zstd-cbor-archive-payload".to_vec(),
            ciphertext_sha256: [0x66; 32],
        }
    }

    #[test]
    fn backup_segment_frame_round_trips_through_cbor() {
        let frame = sample_backup_segment();
        let bytes = crate::cbor::to_vec(&frame).expect("encode");
        let decoded: BackupSegmentFrame = crate::cbor::from_slice(&bytes).expect("decode");
        assert_eq!(decoded, frame);
    }

    #[test]
    fn backup_segment_frame_magic_is_kchat_bak_v1() {
        let frame = sample_backup_segment();
        assert_eq!(&frame.magic, b"KCHAT_BAK_V1");
        assert!(frame.has_valid_header());
    }

    #[test]
    fn backup_segment_frame_rejects_wrong_magic() {
        let mut frame = sample_backup_segment();
        frame.magic = *b"NOT_KCHAT_AA";
        assert!(!frame.has_valid_header());
    }

    #[test]
    fn archive_segment_frame_round_trips_for_every_variant() {
        for st in [
            SegmentType::MessageDelta,
            SegmentType::TimelineSkeleton,
            SegmentType::MediaKeyDelta,
            SegmentType::SearchTextIndex,
            SegmentType::SearchVectorIndex,
            SegmentType::MediaIndex,
            SegmentType::Checkpoint,
        ] {
            let frame = sample_archive_segment(st);
            let bytes = crate::cbor::to_vec(&frame).expect("encode");
            let decoded: ArchiveSegmentFrame = crate::cbor::from_slice(&bytes).expect("decode");
            assert_eq!(decoded, frame, "round-trip failed for {st:?}");
        }
    }

    #[test]
    fn archive_segment_frame_magic_is_kchat_arc_v1() {
        let frame = sample_archive_segment(SegmentType::MessageDelta);
        assert_eq!(&frame.magic, b"KCHAT_ARC_V1");
        assert!(frame.has_valid_header());
    }

    #[test]
    fn segment_type_split_matches_proposal() {
        assert!(SegmentType::Events.is_backup_segment());
        assert!(!SegmentType::Events.is_archive_segment());
        for st in [
            SegmentType::MessageDelta,
            SegmentType::TimelineSkeleton,
            SegmentType::MediaKeyDelta,
            SegmentType::SearchTextIndex,
            SegmentType::SearchVectorIndex,
            SegmentType::MediaIndex,
            SegmentType::Checkpoint,
        ] {
            assert!(st.is_archive_segment(), "{st:?}");
            assert!(!st.is_backup_segment(), "{st:?}");
        }
    }

    #[test]
    fn distinct_segments_produce_distinct_cbor() {
        let backup = sample_backup_segment();
        let archive = sample_archive_segment(SegmentType::MessageDelta);
        let backup_bytes = crate::cbor::to_vec(&backup).unwrap();
        let archive_bytes = crate::cbor::to_vec(&archive).unwrap();
        assert_ne!(backup_bytes, archive_bytes);
    }

    #[test]
    fn cbor_encodes_byte_arrays_as_byte_strings() {
        let frame = sample_backup_segment();
        let bytes = crate::cbor::to_vec(&frame).unwrap();
        assert!(
            bytes.windows(2).any(|w| w == [0x58, 0x18]),
            "expected CBOR byte-string header for the 24-byte nonce, got {:02x?}",
            bytes,
        );
    }
}
