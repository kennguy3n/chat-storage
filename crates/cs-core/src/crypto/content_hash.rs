//! BLAKE3 content hashing for archive/backup integrity.
//!
//! BLAKE3 matches the content hashing used by `chat-storage-search`
//! and `zk-object-fabric` for convergent dedup interop. One-shot and
//! streaming variants produce the same digest on equivalent inputs.

use std::io::{self, Read};

/// Length of a BLAKE3 digest in bytes.
pub const HASH_LEN: usize = 32;

/// Compute the BLAKE3 hash of `data`.
pub fn content_hash(data: &[u8]) -> [u8; HASH_LEN] {
    blake3::hash(data).into()
}

/// Compute the BLAKE3 hash of an `io::Read` source by streaming
/// 64 KiB at a time. Equivalent to [`content_hash`] for the
/// concatenated bytes the reader yields.
pub fn content_hash_streaming<R: Read>(mut reader: R) -> io::Result<[u8; HASH_LEN]> {
    let mut hasher = blake3::Hasher::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hasher.finalize().into())
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

    #[test]
    fn test_streaming_matches_one_shot() {
        let data = b"the quick brown fox jumps over the lazy dog";
        let one_shot = content_hash(data);
        let streamed = content_hash_streaming(&data[..]).unwrap();
        assert_eq!(one_shot, streamed);
    }

    #[test]
    fn test_streaming_multi_buffer() {
        let data = vec![0xABu8; 64 * 1024 * 3 + 17];
        let one_shot = content_hash(&data);
        let streamed = content_hash_streaming(&data[..]).unwrap();
        assert_eq!(one_shot, streamed);
    }
}
