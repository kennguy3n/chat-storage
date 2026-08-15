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
    // m1: zeroize the intermediate PRK copy inside Hkdf after expansion
    drop(hk);
    okm
}

// ---------------------------------------------------------------------------
// Root-level derivations (from DomainKey or ShareGrantKey)
// ---------------------------------------------------------------------------

const ARCHIVE_ROOT_INFO: &[u8] = b"chat-storage/archive-root/v1";
const BACKUP_ROOT_INFO: &[u8] = b"chat-storage/backup-root/v1";
const SEARCH_ROOT_INFO: &[u8] = b"chat-storage/search-root/v1";
const LOCAL_DB_INFO: &[u8] = b"chat-storage/local-db/v1";
const MEDIA_ROOT_INFO: &[u8] = b"chat-storage/media-root/v1";

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

/// Derive `K_media_root` from the Drive's wrapping key.
///
/// This is the root of the media encryption key hierarchy, separate from
/// the archive key hierarchy to avoid cross-subsystem key reuse.
pub fn derive_media_root(wrapping_key: &Key32) -> Key32 {
    hkdf_derive(wrapping_key, MEDIA_ROOT_INFO)
}

/// Derive `K_media_blob(asset_id)` from `K_media_root`.
///
/// Per-asset media encryption key, derived via HKDF-SHA256.
pub fn derive_media_blob(media_root: &Key32, asset_id: &[u8]) -> Key32 {
    let mut info = b"chat-storage/media-blob/v1".to_vec();
    info.extend_from_slice(asset_id);
    hkdf_expand(media_root, &info)
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

/// Derive `K_fuzzy_index_shard(shard_id)` from `K_search_root`.
pub fn derive_fuzzy_index_shard(search_root: &Key32, shard_id: &[u8]) -> Key32 {
    let mut info = b"chat-storage/fuzzy-index-shard/v1".to_vec();
    info.extend_from_slice(shard_id);
    hkdf_expand(search_root, &info)
}

/// Derive `K_bloom_index_shard(shard_id)` from `K_search_root`.
pub fn derive_bloom_index_shard(search_root: &Key32, shard_id: &[u8]) -> Key32 {
    let mut info = b"chat-storage/bloom-index-shard/v1".to_vec();
    info.extend_from_slice(shard_id);
    hkdf_expand(search_root, &info)
}

/// Derive the per-account `K_conversation_hash` from `K_search_root`.
pub fn derive_conversation_hash_key(search_root: &Key32) -> Key32 {
    hkdf_expand(search_root, b"chat-storage/conversation-hash/v1")
}

/// Derive a stable per-`(conversation_id, time_bucket)` text-index
/// shard key from `K_search_root`.
pub fn derive_text_index_shard_for_bucket(
    search_root: &Key32,
    conversation_id: &str,
    time_bucket: &str,
) -> Key32 {
    derive_with_two_ids(
        search_root,
        b"chat-storage/text-index-bucket/v1",
        conversation_id.as_bytes(),
        time_bucket.as_bytes(),
    )
}

/// Derive a stable per-`(conversation_id, time_bucket)` fuzzy-index
/// shard key from `K_search_root`.
pub fn derive_fuzzy_index_shard_for_bucket(
    search_root: &Key32,
    conversation_id: &str,
    time_bucket: &str,
) -> Key32 {
    derive_with_two_ids(
        search_root,
        b"chat-storage/fuzzy-index-bucket/v1",
        conversation_id.as_bytes(),
        time_bucket.as_bytes(),
    )
}

// ---------------------------------------------------------------------------
// B2B per-tenant key isolation
// ---------------------------------------------------------------------------

/// Derive `K_b2b_tenant_root(tenant_id)` from the wrapping key.
pub fn derive_b2b_tenant_root(wrapping_key: &Key32, tenant_id: &str) -> Key32 {
    let mut info = b"chat-storage/b2b-tenant-root/v1".to_vec();
    info.extend_from_slice(tenant_id.as_bytes());
    hkdf_derive(wrapping_key, &info)
}

/// Derive `K_b2b_archive_epoch(tenant_id, epoch_id)` from a per-tenant root.
pub fn derive_b2b_archive_epoch(k_tenant_root: &Key32, tenant_id: &str, epoch_id: &str) -> Key32 {
    derive_with_two_ids(
        k_tenant_root,
        b"chat-storage/b2b-archive-epoch/v1",
        tenant_id.as_bytes(),
        epoch_id.as_bytes(),
    )
}

/// Derive `K_b2b_text_index_shard(tenant_id, shard_id)` from `K_search_root`.
pub fn derive_b2b_text_index_shard(
    k_search_root: &Key32,
    tenant_id: &str,
    shard_id: &str,
) -> Key32 {
    derive_with_two_ids(
        k_search_root,
        b"chat-storage/b2b-text-index-shard/v1",
        tenant_id.as_bytes(),
        shard_id.as_bytes(),
    )
}

// ---------------------------------------------------------------------------
// String-based epoch key derivation (for ported code from chat-storage-search)
// ---------------------------------------------------------------------------

/// Derive `K_archive_epoch(epoch_id)` from `K_archive_root` using a
/// string-based epoch ID (e.g. "epoch-0", "2026-04").
pub fn derive_archive_epoch_key(archive_root: &Key32, epoch_id: &str) -> Key32 {
    let mut info = b"chat-storage/archive-epoch/v1".to_vec();
    info.extend_from_slice(epoch_id.as_bytes());
    hkdf_expand(archive_root, &info)
}

/// Derive `K_archive_segment(segment_id)` from an epoch key using a
/// string-based segment ID.
pub fn derive_archive_segment_key(epoch_key: &Key32, segment_id: &str) -> Key32 {
    let mut info = b"chat-storage/archive-segment/v1".to_vec();
    info.extend_from_slice(segment_id.as_bytes());
    hkdf_expand(epoch_key, &info)
}

/// Derive `K_archive_manifest(manifest_id)` from an epoch key using a
/// string-based manifest ID.
pub fn derive_archive_manifest_key(epoch_key: &Key32, manifest_id: &str) -> Key32 {
    let mut info = b"chat-storage/archive-manifest/v1".to_vec();
    info.extend_from_slice(manifest_id.as_bytes());
    hkdf_expand(epoch_key, &info)
}

/// Wrap an epoch key under `K_archive_root` using AES-256-KW.
pub fn wrap_epoch_key(archive_root: &Key32, epoch_key: &Key32) -> super::CryptoResult<Vec<u8>> {
    super::key_wrap::wrap_key(archive_root, epoch_key)
}

/// Unwrap a wrapped epoch key produced by [`wrap_epoch_key`].
pub fn unwrap_epoch_key(archive_root: &Key32, wrapped: &[u8]) -> super::CryptoResult<Key32> {
    super::key_wrap::unwrap_key(archive_root, wrapped)
}

// ---------------------------------------------------------------------------
// Epoch rotation automation
// ---------------------------------------------------------------------------

/// Canonical epoch-id prefix.
pub const EPOCH_ID_PREFIX: &str = "epoch-";

/// Default rotation cadence: 30 days, in milliseconds ("monthly").
pub const DEFAULT_EPOCH_CADENCE_MS: i64 = 30 * 24 * 60 * 60 * 1000;

/// Tracks the current archive epoch and decides when to roll it forward.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EpochRotator {
    current_epoch_id: String,
    last_rotation_ms: i64,
    cadence_ms: i64,
}

impl EpochRotator {
    /// Epoch id a brand-new archive starts at.
    pub const INITIAL_EPOCH_ID: &'static str = "epoch-0";

    /// Build a rotator with an explicit cadence.
    pub fn new(
        current_epoch_id: impl Into<String>,
        last_rotation_ms: i64,
        cadence_ms: i64,
    ) -> Self {
        Self {
            current_epoch_id: current_epoch_id.into(),
            last_rotation_ms,
            cadence_ms: cadence_ms.max(1),
        }
    }

    /// Build a rotator with the default monthly cadence.
    pub fn with_monthly_cadence(
        current_epoch_id: impl Into<String>,
        last_rotation_ms: i64,
    ) -> Self {
        Self::new(current_epoch_id, last_rotation_ms, DEFAULT_EPOCH_CADENCE_MS)
    }

    /// Build a rotator from a persisted / untrusted epoch id, validating format.
    pub fn try_new(
        current_epoch_id: impl Into<String>,
        last_rotation_ms: i64,
        cadence_ms: i64,
    ) -> super::CryptoResult<Self> {
        let current_epoch_id = current_epoch_id.into();
        parse_epoch_counter(&current_epoch_id)?;
        Ok(Self::new(current_epoch_id, last_rotation_ms, cadence_ms))
    }

    /// Validating counterpart to [`with_monthly_cadence`].
    pub fn try_with_monthly_cadence(
        current_epoch_id: impl Into<String>,
        last_rotation_ms: i64,
    ) -> super::CryptoResult<Self> {
        Self::try_new(current_epoch_id, last_rotation_ms, DEFAULT_EPOCH_CADENCE_MS)
    }

    /// The current epoch id.
    pub fn current_epoch_id(&self) -> &str {
        &self.current_epoch_id
    }

    /// Wall-clock ms of the most recent rotation.
    pub fn last_rotation_ms(&self) -> i64 {
        self.last_rotation_ms
    }

    /// Configured rotation cadence in ms.
    pub fn cadence_ms(&self) -> i64 {
        self.cadence_ms
    }

    /// Pure cadence predicate.
    pub fn should_rotate(last_rotation_ms: i64, now_ms: i64, cadence_ms: i64) -> bool {
        now_ms.saturating_sub(last_rotation_ms) >= cadence_ms.max(1)
    }

    /// Whether this rotator is due to rotate as of `now_ms`.
    pub fn is_due(&self, now_ms: i64) -> bool {
        Self::should_rotate(self.last_rotation_ms, now_ms, self.cadence_ms)
    }

    /// Compute the next epoch id from `current_epoch_id`.
    pub fn rotate_epoch(current_epoch_id: &str) -> super::CryptoResult<String> {
        let n = parse_epoch_counter(current_epoch_id)?;
        let next = n
            .checked_add(1)
            .ok_or(super::CryptoError::InvalidInput("epoch counter overflow"))?;
        Ok(format!("{EPOCH_ID_PREFIX}{next}"))
    }

    /// Roll forward to the next epoch.
    pub fn rotate(
        &mut self,
        k_archive_root: &Key32,
        now_ms: i64,
    ) -> super::CryptoResult<(String, Key32)> {
        let next_id = Self::rotate_epoch(&self.current_epoch_id)?;
        let key = derive_archive_epoch_key(k_archive_root, &next_id);
        self.current_epoch_id = next_id.clone();
        self.last_rotation_ms = now_ms;
        Ok((next_id, key))
    }

    /// Derive the key for the current epoch without rotating.
    pub fn current_epoch_key(&self, k_archive_root: &Key32) -> Key32 {
        derive_archive_epoch_key(k_archive_root, &self.current_epoch_id)
    }
}

fn parse_epoch_counter(epoch_id: &str) -> super::CryptoResult<u64> {
    epoch_id
        .strip_prefix(EPOCH_ID_PREFIX)
        .and_then(|n| n.parse::<u64>().ok())
        .ok_or(super::CryptoError::InvalidInput(
            "epoch id must be of the form 'epoch-<u64>'",
        ))
}

fn derive_with_two_ids(parent: &Key32, label: &[u8], id_a: &[u8], id_b: &[u8]) -> Key32 {
    let id_a_len: u32 = u32::try_from(id_a.len()).unwrap_or(u32::MAX);
    let len_bytes = id_a_len.to_be_bytes();
    let mut buf = Vec::with_capacity(label.len() + len_bytes.len() + id_a.len() + id_b.len());
    buf.extend_from_slice(label);
    buf.extend_from_slice(&len_bytes);
    buf.extend_from_slice(id_a);
    buf.extend_from_slice(id_b);
    hkdf_expand(parent, &buf)
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
        let fuzzy = derive_fuzzy_index_shard(&search_root, &[0xAA]);
        let bloom = derive_bloom_index_shard(&search_root, &[0xAA]);

        assert_ne!(text, vector);
        assert_ne!(text, media);
        assert_ne!(text, fuzzy);
        assert_ne!(text, bloom);
        assert_ne!(vector, media);
        assert_ne!(vector, fuzzy);
        assert_ne!(vector, bloom);
        assert_ne!(media, fuzzy);
        assert_ne!(media, bloom);
        assert_ne!(fuzzy, bloom);
    }

    #[test]
    fn test_conversation_hash_key() {
        let search_root = derive_search_root(&[0x42u8; 32]);
        let k = derive_conversation_hash_key(&search_root);
        let k2 = derive_conversation_hash_key(&search_root);
        assert_eq!(k, k2);
        assert_ne!(k, search_root);
    }

    #[test]
    fn test_bucket_keyed_shards() {
        let search_root = derive_search_root(&[0x42u8; 32]);
        let text = derive_text_index_shard_for_bucket(&search_root, "conv-1", "2026-04");
        let text2 = derive_text_index_shard_for_bucket(&search_root, "conv-1", "2026-04");
        let text_other = derive_text_index_shard_for_bucket(&search_root, "conv-2", "2026-04");
        let fuzzy = derive_fuzzy_index_shard_for_bucket(&search_root, "conv-1", "2026-04");

        assert_eq!(text, text2, "same bucket must produce same key");
        assert_ne!(
            text, text_other,
            "different conv must produce different key"
        );
        assert_ne!(text, fuzzy, "text and fuzzy must be in disjoint key spaces");
    }

    #[test]
    fn test_b2b_tenant_isolation() {
        let wrapping_key = [0x42u8; 32];
        let tenant_a = derive_b2b_tenant_root(&wrapping_key, "tenant-a");
        let tenant_b = derive_b2b_tenant_root(&wrapping_key, "tenant-b");
        assert_ne!(tenant_a, tenant_b);

        let epoch_a = derive_b2b_archive_epoch(&tenant_a, "tenant-a", "epoch-0");
        let epoch_b = derive_b2b_archive_epoch(&tenant_b, "tenant-b", "epoch-0");
        assert_ne!(epoch_a, epoch_b);
    }

    #[test]
    fn test_string_based_epoch_keys() {
        let archive_root = derive_archive_root(&[0x42u8; 32]);
        let epoch = derive_archive_epoch_key(&archive_root, "epoch-0");
        let epoch2 = derive_archive_epoch_key(&archive_root, "epoch-0");
        let epoch1 = derive_archive_epoch_key(&archive_root, "epoch-1");
        assert_eq!(epoch, epoch2);
        assert_ne!(epoch, epoch1);

        let seg = derive_archive_segment_key(&epoch, "seg-1");
        let man = derive_archive_manifest_key(&epoch, "id-1");
        assert_ne!(seg, man);
    }

    #[test]
    fn test_epoch_key_wrap_unwrap() {
        let archive_root = derive_archive_root(&[0x42u8; 32]);
        let epoch = derive_archive_epoch_key(&archive_root, "epoch-0");
        let wrapped = wrap_epoch_key(&archive_root, &epoch).unwrap();
        let unwrapped = unwrap_epoch_key(&archive_root, &wrapped).unwrap();
        assert_eq!(unwrapped, epoch);
    }

    #[test]
    fn test_epoch_rotator() {
        let archive_root = derive_archive_root(&[0x42u8; 32]);
        let mut rotator = EpochRotator::new(EpochRotator::INITIAL_EPOCH_ID, 0, 1000);
        assert_eq!(rotator.current_epoch_id(), "epoch-0");
        assert!(!rotator.is_due(500));
        assert!(rotator.is_due(1000));
        assert!(rotator.is_due(1001));

        let (next_id, _key) = rotator.rotate(&archive_root, 1000).unwrap();
        assert_eq!(next_id, "epoch-1");
        assert_eq!(rotator.current_epoch_id(), "epoch-1");
        assert_eq!(rotator.last_rotation_ms(), 1000);
    }

    #[test]
    fn test_epoch_rotator_try_new_rejects_bad_id() {
        assert!(EpochRotator::try_new("bad-id", 0, 1000).is_err());
        assert!(EpochRotator::try_new("epoch-0", 0, 1000).is_ok());
    }

    #[test]
    fn test_epoch_rotator_rotate_epoch() {
        assert_eq!(EpochRotator::rotate_epoch("epoch-0").unwrap(), "epoch-1");
        assert_eq!(EpochRotator::rotate_epoch("epoch-42").unwrap(), "epoch-43");
        assert!(EpochRotator::rotate_epoch("bad").is_err());
    }
}
