//! Backup manifest frame — chained, signed manifest for backup integrity.
//!
//! Each manifest generation chains to the previous one via
//! `previous_manifest_hash`. Manifests are signed with the device's
//! hybrid Ed25519 + ML-DSA-65 signing key.

use serde::{Deserialize, Serialize};

/// Reference to a backup segment within a manifest.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SegmentRef {
    pub segment_id: String,
    /// Storage key (S3 key or blob key on the gateway).
    pub storage_key: String,
    pub size: u64,
    /// BLAKE3 Merkle root of the segment ciphertext.
    pub merkle_root: [u8; 32],
}

/// A wrapped epoch key stored in the manifest for key recovery.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WrappedEpochKey {
    pub epoch_id: u64,
    /// AES-256-KW wrapped `K_archive_epoch` under `K_archive_root`.
    pub wrapped_key: Vec<u8>,
}

/// Backup manifest payload (before encryption).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupManifestPayload {
    /// Monotonic generation number.
    pub generation: u64,
    /// BLAKE3 hash of the previous manifest payload (chained).
    /// Zero for the first manifest.
    pub previous_manifest_hash: [u8; 32],
    /// Segments included in this backup generation.
    pub segments: Vec<SegmentRef>,
    /// Wrapped epoch keys for archive key recovery.
    pub wrapped_epoch_keys: Vec<WrappedEpochKey>,
    /// Timestamp (ms since Unix epoch).
    pub created_at_ms: i64,
}

/// Backup manifest frame (encrypted + signed).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupManifestFrame {
    /// 24-byte XChaCha20-Poly1305 nonce.
    pub nonce: [u8; 24],
    /// Encrypted manifest payload.
    pub ciphertext: Vec<u8>,
    /// Ed25519 signature over the manifest payload.
    pub signature_ed25519: Vec<u8>,
    /// BLAKE3 hash of the plaintext payload.
    pub plaintext_hash: [u8; 32],
}

/// Encode a backup manifest payload.
pub fn encode_payload(payload: &BackupManifestPayload) -> Result<Vec<u8>, crate::Error> {
    serde_json::to_vec(payload).map_err(|e| crate::Error::Storage(e.to_string().into()))
}

/// Decode a backup manifest payload.
pub fn decode_payload(data: &[u8]) -> Result<BackupManifestPayload, crate::Error> {
    serde_json::from_slice(data).map_err(|e| crate::Error::Storage(e.to_string().into()))
}
