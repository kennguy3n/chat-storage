//! BLAKE3 content hashing for archive/backup integrity.
//!
//! BLAKE3 matches the content hashing used by `chat-storage-search`
//! and `zk-object-fabric` for convergent dedup interop.

/// Compute the BLAKE3 hash of `data`.
pub fn content_hash(data: &[u8]) -> [u8; 32] {
    blake3::hash(data).into()
}

/// Compute the BLAKE3 hash and return as hex string.
pub fn content_hash_hex(data: &[u8]) -> String {
    hex::encode(content_hash(data))
}

/// Verify that `data` hashes to `expected` using constant-time comparison.
pub fn verify_content_hash(data: &[u8], expected: &[u8; 32]) -> bool {
    let actual = content_hash(data);
    crate::security::constant_time_eq(&actual, expected)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_content_hash_deterministic() {
        let h1 = content_hash(b"hello");
        let h2 = content_hash(b"hello");
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_content_hash_different_inputs() {
        let h1 = content_hash(b"hello");
        let h2 = content_hash(b"world");
        assert_ne!(h1, h2);
    }

    #[test]
    fn test_verify_content_hash() {
        let data = b"hello world";
        let hash = content_hash(data);
        assert!(verify_content_hash(data, &hash));
        assert!(!verify_content_hash(b"wrong", &hash));
    }
}
