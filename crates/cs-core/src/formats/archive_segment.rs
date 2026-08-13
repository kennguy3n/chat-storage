//! Archive segment frame — JSON-encoded, zstd-compressed, AEAD-sealed.
//!
//! A segment covers a `(conversation_id, time_bucket)` pair and
//! contains one or more message entries. Segments are encrypted
//! with `K_archive_segment(segment_id)` derived from `K_archive_epoch`.

use serde::{Deserialize, Serialize};

/// One message entry inside an archive segment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchiveEntry {
    pub message_id: String,
    pub created_at_ms: i64,
    pub kind: EntryKind,
    /// JSON-encoded message body (text content + rich meta).
    pub body_ciphertext: Vec<u8>,
    /// Nonce for the body ciphertext (sealed with K_archive_segment).
    pub body_nonce: [u8; 24],
    /// Media asset references (asset_id, node_id, version_id).
    pub media_refs: Vec<MediaRef>,
}

/// Kind of message entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EntryKind {
    Text,
    Media,
    System,
    Deleted,
}

/// Reference to a media asset within an archive entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaRef {
    pub asset_id: String,
    pub mime_type: String,
    pub node_id: String,
    pub version_id: String,
    pub bytes_total: u64,
}

/// Archive segment payload (before compression + encryption).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchiveSegmentPayload {
    pub segment_id: String,
    pub conversation_id: String,
    /// Time bucket identifier (e.g. "2024-01" for monthly).
    pub time_bucket: String,
    /// Epoch ID this segment belongs to.
    pub epoch_id: u64,
    /// Message entries in this segment.
    pub entries: Vec<ArchiveEntry>,
}

/// Archive segment frame (after compression + encryption).
///
/// Layout: `nonce(24) || XChaCha20-Poly1305(zstd(json(payload)))`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchiveSegmentFrame {
    /// 24-byte XChaCha20-Poly1305 nonce.
    pub nonce: [u8; 24],
    /// Ciphertext (compressed-then-encrypted payload).
    pub ciphertext: Vec<u8>,
    /// BLAKE3 hash of the plaintext payload (for integrity verification).
    pub plaintext_hash: [u8; 32],
    /// Size of the original plaintext payload (before compression).
    pub plaintext_size: u64,
}

/// Encode an archive segment payload to JSON.
pub fn encode_payload(payload: &ArchiveSegmentPayload) -> Result<Vec<u8>, crate::Error> {
    serde_encode(payload)
}

/// Decode an archive segment payload from JSON.
pub fn decode_payload(data: &[u8]) -> Result<ArchiveSegmentPayload, crate::Error> {
    serde_decode(data)
}

/// Compress payload with zstd.
pub fn compress(data: &[u8]) -> Result<Vec<u8>, crate::Error> {
    zstd::encode_all(data, 3).map_err(|e| crate::Error::Storage(e.to_string().into()))
}

/// Decompress zstd data.
pub fn decompress(data: &[u8]) -> Result<Vec<u8>, crate::Error> {
    zstd::decode_all(data).map_err(|e| crate::Error::Storage(e.to_string().into()))
}

fn serde_encode<T: Serialize>(value: &T) -> Result<Vec<u8>, crate::Error> {
    serde_json::to_vec(value).map_err(|e| crate::Error::Storage(e.to_string().into()))
}

fn serde_decode<T: for<'de> Deserialize<'de>>(data: &[u8]) -> Result<T, crate::Error> {
    serde_json::from_slice(data).map_err(|e| crate::Error::Storage(e.to_string().into()))
}
