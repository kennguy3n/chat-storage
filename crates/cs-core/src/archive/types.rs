//! Internal archive segment types — payload, entry, and frame used
//! for building and opening encrypted archive segments.
//!
//! These are internal to the archive engine; the CBOR wire-format
//! frames live in [`crate::formats`].

use serde::{Deserialize, Serialize};

/// One message entry inside an archive segment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchiveEntry {
    pub message_id: String,
    pub created_at_ms: i64,
    pub kind: EntryKind,
    pub body_ciphertext: Vec<u8>,
    pub body_nonce: [u8; 24],
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
    pub time_bucket: String,
    pub epoch_id: u64,
    pub entries: Vec<ArchiveEntry>,
}

/// Internal archive segment frame (after compression + encryption).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchiveSegmentFrame {
    pub nonce: [u8; 24],
    pub ciphertext: Vec<u8>,
    pub plaintext_hash: [u8; 32],
    pub plaintext_size: u64,
}

/// Encode an archive segment payload to JSON.
pub fn encode_payload(payload: &ArchiveSegmentPayload) -> Result<Vec<u8>, crate::Error> {
    serde_json::to_vec(payload).map_err(|e| crate::Error::Storage(e.to_string().into()))
}

/// Decode an archive segment payload from JSON.
pub fn decode_payload(data: &[u8]) -> Result<ArchiveSegmentPayload, crate::Error> {
    serde_json::from_slice(data).map_err(|e| crate::Error::Storage(e.to_string().into()))
}

/// Compress payload with zstd.
pub fn compress(data: &[u8]) -> Result<Vec<u8>, crate::Error> {
    zstd::encode_all(data, 3).map_err(|e| crate::Error::Storage(e.to_string().into()))
}

/// Decompress zstd data.
pub fn decompress(data: &[u8]) -> Result<Vec<u8>, crate::Error> {
    zstd::decode_all(data).map_err(|e| crate::Error::Storage(e.to_string().into()))
}
