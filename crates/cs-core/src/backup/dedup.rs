//! Backup dedup — intra-tenant backup deduplication.

pub fn compute_dedup_key(data: &[u8]) -> [u8; 32] {
    crate::crypto::content_hash(data)
}
