//! Key derivation bridge — derives purpose-specific keys from the
//! KDRV1 `DomainKey` (or `ShareGrantKey` in Max mode) using HKDF-SHA256.
//!
//! This bridges the KDRV1 file-centric key model with the purpose-
//! specific key hierarchy from `chat-storage-search` §2.1:
//!
//! ```text
//! DomainKey (KDRV1)
//!   ├── K_archive_root    = HKDF(DomainKey, "chat-storage/archive-root/v1")
//!   │     └── K_archive_epoch(epoch_id)
//!   │           ├── K_archive_segment(segment_id)
//!   │           └── K_archive_manifest(manifest_id)
//!   ├── K_backup_root     = HKDF(DomainKey, "chat-storage/backup-root/v1")
//!   │     ├── K_backup_segment(segment_id)
//!   │     └── K_backup_manifest(manifest_id)
//!   ├── K_search_root     = HKDF(DomainKey, "chat-storage/search-root/v1")
//!   │     ├── K_text_index_shard(shard_id)
//!   │     ├── K_vector_index_shard(shard_id)
//!   │     └── K_media_index_shard(shard_id)
//!   └── K_local_db        = HKDF(DomainKey, "chat-storage/local-db/v1")
//! ```

use hkdf::Hkdf;
use sha2::Sha256;

use super::Key32;

/// HKDF-SHA256 extract+expand to produce a 32-byte derived key.
fn hkdf_derive(ikm: &[u8], info: &[u8]) -> Key32 {
    let hk = Hkdf::<Sha256>::new(None, ikm);
    let mut okm = [0u8; 32];
    hk.expand(info, &mut okm)
        .expect("HKDF-SHA256 expand to 32 bytes cannot fail");
    okm
}

/// HKDF-SHA256 expand from an existing PRK to produce a 32-byte derived key.
fn hkdf_expand(prk: &[u8], info: &[u8]) -> Key32 {
    let hk = Hkdf::<Sha256>::from_prk(prk).expect("PRK must be valid");
    let mut okm = [0u8; 32];
    hk.expand(info, &mut okm)
        .expect("HKDF-SHA256 expand to 32 bytes cannot fail");
    okm
}

// ---------------------------------------------------------------------------
// Root-level derivations (from DomainKey or ShareGrantKey)
// ---------------------------------------------------------------------------

const ARCHIVE_ROOT_INFO: &[u8] = b"chat-storage/archive-root/v1";
const BACKUP_ROOT_INFO: &[u8] = b"chat-storage/backup-root/v1";
const SEARCH_ROOT_INFO: &[u8] = b"chat-storage/search-root/v1";
const LOCAL_DB_INFO: &[u8] = b"chat-storage/local-db/v1";

/// Derive `K_archive_root` from the Drive's wrapping key.
pub fn derive_archive_root(wrapping_key: &Key32) -> Key32 {
    hkdf_derive(wrapping_key, ARCHIVE_ROOT_INFO)
}

/// Derive `K_backup_root` from the Drive's wrapping key.
pub fn derive_backup_root(wrapping_key: &Key32) -> Key32 {
    hkdf_derive(wrapping_key, BACKUP_ROOT_INFO)
}

/// Derive `K_search_root` from the Drive's wrapping key.
pub fn derive_search_root(wrapping_key: &Key32) -> Key32 {
    hkdf_derive(wrapping_key, SEARCH_ROOT_INFO)
}

/// Derive `K_local_db` (SQLCipher key) from the Drive's wrapping key.
pub fn derive_local_db_key(wrapping_key: &Key32) -> Key32 {
    hkdf_derive(wrapping_key, LOCAL_DB_INFO)
}

// ---------------------------------------------------------------------------
// Archive key derivations
// ---------------------------------------------------------------------------

/// Derive `K_archive_epoch(epoch_id)` from `K_archive_root`.
///
/// `epoch_id` is a u64 representing the epoch number (e.g. months since
/// Unix epoch for monthly rotation).
pub fn derive_archive_epoch(archive_root: &Key32, epoch_id: u64) -> Key32 {
    let mut info = b"chat-storage/archive-epoch/v1".to_vec();
    info.extend_from_slice(&epoch_id.to_be_bytes());
    hkdf_expand(archive_root, &info)
}

/// Derive `K_archive_segment(segment_id)` from `K_archive_epoch`.
///
/// `segment_id` is a unique 32-byte identifier for the segment.
pub fn derive_archive_segment(epoch_key: &Key32, segment_id: &[u8]) -> Key32 {
    let mut info = b"chat-storage/archive-segment/v1".to_vec();
    info.extend_from_slice(segment_id);
    hkdf_expand(epoch_key, &info)
}

/// Derive `K_archive_manifest(manifest_id)` from `K_archive_epoch`.
pub fn derive_archive_manifest(epoch_key: &Key32, manifest_id: &[u8]) -> Key32 {
    let mut info = b"chat-storage/archive-manifest/v1".to_vec();
    info.extend_from_slice(manifest_id);
    hkdf_expand(epoch_key, &info)
}

// ---------------------------------------------------------------------------
// Backup key derivations
// ---------------------------------------------------------------------------

/// Derive `K_backup_segment(segment_id)` from `K_backup_root`.
pub fn derive_backup_segment(backup_root: &Key32, segment_id: &[u8]) -> Key32 {
    let mut info = b"chat-storage/backup-segment/v1".to_vec();
    info.extend_from_slice(segment_id);
    hkdf_expand(backup_root, &info)
}

/// Derive `K_backup_manifest(manifest_id)` from `K_backup_root`.
pub fn derive_backup_manifest(backup_root: &Key32, manifest_id: &[u8]) -> Key32 {
    let mut info = b"chat-storage/backup-manifest/v1".to_vec();
    info.extend_from_slice(manifest_id);
    hkdf_expand(backup_root, &info)
}

// ---------------------------------------------------------------------------
// Search key derivations
// ---------------------------------------------------------------------------

/// Derive `K_text_index_shard(shard_id)` from `K_search_root`.
pub fn derive_text_index_shard(search_root: &Key32, shard_id: &[u8]) -> Key32 {
    let mut info = b"chat-storage/text-index-shard/v1".to_vec();
    info.extend_from_slice(shard_id);
    hkdf_expand(search_root, &info)
}

/// Derive `K_vector_index_shard(shard_id)` from `K_search_root`.
pub fn derive_vector_index_shard(search_root: &Key32, shard_id: &[u8]) -> Key32 {
    let mut info = b"chat-storage/vector-index-shard/v1".to_vec();
    info.extend_from_slice(shard_id);
    hkdf_expand(search_root, &info)
}

/// Derive `K_media_index_shard(shard_id)` from `K_search_root`.
pub fn derive_media_index_shard(search_root: &Key32, shard_id: &[u8]) -> Key32 {
    let mut info = b"chat-storage/media-index-shard/v1".to_vec();
    info.extend_from_slice(shard_id);
    hkdf_expand(search_root, &info)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_root_derivations_are_distinct() {
        let domain_key = [0x42u8; 32];
        let archive = derive_archive_root(&domain_key);
        let backup = derive_backup_root(&domain_key);
        let search = derive_search_root(&domain_key);
        let local = derive_local_db_key(&domain_key);

        assert_ne!(archive, backup);
        assert_ne!(archive, search);
        assert_ne!(archive, local);
        assert_ne!(backup, search);
        assert_ne!(backup, local);
        assert_ne!(search, local);
    }

    #[test]
    fn test_epoch_derivation_is_deterministic() {
        let archive_root = derive_archive_root(&[0x42u8; 32]);
        let epoch1 = derive_archive_epoch(&archive_root, 1);
        let epoch1_again = derive_archive_epoch(&archive_root, 1);
        let epoch2 = derive_archive_epoch(&archive_root, 2);

        assert_eq!(epoch1, epoch1_again);
        assert_ne!(epoch1, epoch2);
    }

    #[test]
    fn test_segment_derivation_is_deterministic() {
        let archive_root = derive_archive_root(&[0x42u8; 32]);
        let epoch = derive_archive_epoch(&archive_root, 1);
        let seg1 = derive_archive_segment(&epoch, &[1, 2, 3, 4]);
        let seg1_again = derive_archive_segment(&epoch, &[1, 2, 3, 4]);
        let seg2 = derive_archive_segment(&epoch, &[5, 6, 7, 8]);

        assert_eq!(seg1, seg1_again);
        assert_ne!(seg1, seg2);
    }

    #[test]
    fn test_search_shard_derivation() {
        let search_root = derive_search_root(&[0x42u8; 32]);
        let text = derive_text_index_shard(&search_root, &[0xAA]);
        let vector = derive_vector_index_shard(&search_root, &[0xAA]);
        let media = derive_media_index_shard(&search_root, &[0xAA]);

        assert_ne!(text, vector);
        assert_ne!(text, media);
        assert_ne!(vector, media);
    }
}
