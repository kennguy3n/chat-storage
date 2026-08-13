//! Crypto module — key derivation bridge from KDRV1 DomainKey to
//! purpose-specific keys (archive, backup, search, local DB).
//!
//! The KDRV1 protocol (`kchat-drive-crypto`) provides file-level
//! encryption with `VersionDEK`, `DomainKey`, and `ShareGrantKey`.
//! This module derives the purpose-specific key hierarchy from the
//! Drive's `DomainKey` (or `ShareGrantKey` in Max mode) using
//! HKDF-SHA256, matching the logical hierarchy from
//! `chat-storage-search` §2.1.

pub mod aead;
pub mod content_hash;
pub mod key_bridge;
pub mod key_wrap;

pub use aead::{open, open_in_place, seal, seal_in_place};
pub use content_hash::content_hash;
pub use key_bridge::{
    derive_archive_epoch, derive_archive_manifest, derive_archive_root, derive_archive_segment,
    derive_backup_manifest, derive_backup_root, derive_backup_segment, derive_local_db_key,
    derive_media_index_shard, derive_search_root, derive_text_index_shard,
    derive_vector_index_shard,
};
pub use key_wrap::{unwrap_key, wrap_key};

/// Crypto errors.
#[derive(Debug, thiserror::Error)]
pub enum CryptoError {
    #[error("AEAD: {0}")]
    Aead(String),

    #[error("key derivation: {0}")]
    Kdf(String),

    #[error("key wrap: {0}")]
    KeyWrap(String),

    #[error("invalid key length: expected {expected}, got {actual}")]
    InvalidKeyLength { expected: usize, actual: usize },

    #[error("hash mismatch: expected {expected}, got {actual}")]
    HashMismatch { expected: String, actual: String },

    #[error("kdrive crypto: {0}")]
    Kdrive(#[from] kchat_drive_types::DriveError),
}

/// 32-byte key material.
pub type Key32 = [u8; 32];

/// 24-byte nonce for XChaCha20-Poly1305.
pub type Nonce24 = [u8; 24];

/// 12-byte nonce for AES-256-GCM.
pub type Nonce12 = [u8; 12];
