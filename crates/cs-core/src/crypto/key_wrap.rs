//! AES-256-KW key wrapping (NIST 800-38F / RFC 3394).
//!
//! Implements the construction the higher layers depend on:
//! a `K_asset` (32 bytes) wrapped under one of `K_local_db`,
//! `K_archive_root`, or `K_backup_root` (also 32 bytes) so that the
//! ciphertext alone never reveals the asset key. The wrapped output
//! is exactly **40 bytes** — the input key plus an 8-byte integrity
//! check value.

use aes_kw::Kek;

use super::{CryptoError, CryptoResult, Key32, KEY_LEN};

/// AES-256-KW wrap of a 32-byte key produces 40 bytes (32 + 8-byte
/// integrity check value).
pub const WRAPPED_KEY_LEN: usize = KEY_LEN + 8;

/// Wrap `key_to_wrap` under `wrapping_key` using AES-256-KW
/// (RFC 3394). Output is exactly [`WRAPPED_KEY_LEN`] bytes.
pub fn wrap_key(wrapping_key: &Key32, key_to_wrap: &Key32) -> CryptoResult<Vec<u8>> {
    let kek = Kek::from(*wrapping_key);
    let mut out = vec![0u8; WRAPPED_KEY_LEN];
    kek.wrap(key_to_wrap, &mut out)
        .map_err(|_| CryptoError::KeyWrap("aes-kw wrap failed".into()))?;
    Ok(out)
}

/// Unwrap a 32-byte key from `wrapped_key` using AES-256-KW. The
/// wrapped input must be exactly [`WRAPPED_KEY_LEN`] bytes.
pub fn unwrap_key(wrapping_key: &Key32, wrapped_key: &[u8]) -> CryptoResult<Key32> {
    if wrapped_key.len() != WRAPPED_KEY_LEN {
        return Err(CryptoError::InvalidInput(
            "aes-kw unwrap: wrapped key must be 40 bytes",
        ));
    }
    let kek = Kek::from(*wrapping_key);
    let mut out = [0u8; KEY_LEN];
    kek.unwrap(wrapped_key, &mut out)
        .map_err(|_| CryptoError::KeyWrap("aes-kw unwrap failed".into()))?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh_kek() -> Key32 {
        let mut k = [0u8; KEY_LEN];
        for (i, b) in k.iter_mut().enumerate() {
            *b = (i as u8).wrapping_mul(7).wrapping_add(31);
        }
        k
    }

    fn fresh_k_asset() -> Key32 {
        let mut k = [0u8; KEY_LEN];
        for (i, b) in k.iter_mut().enumerate() {
            *b = (i as u8) ^ 0xA5;
        }
        k
    }

    #[test]
    fn wrap_unwrap_round_trip() {
        let kek = fresh_kek();
        let k_asset = fresh_k_asset();
        let wrapped = wrap_key(&kek, &k_asset).unwrap();
        assert_eq!(wrapped.len(), WRAPPED_KEY_LEN);
        let unwrapped = unwrap_key(&kek, &wrapped).unwrap();
        assert_eq!(unwrapped, k_asset);
    }

    #[test]
    fn wrong_wrapping_key_is_rejected() {
        let kek = fresh_kek();
        let k_asset = fresh_k_asset();
        let wrapped = wrap_key(&kek, &k_asset).unwrap();

        let mut wrong_kek = kek;
        wrong_kek[0] ^= 0x01;
        let res = unwrap_key(&wrong_kek, &wrapped);
        assert!(res.is_err(), "wrong-KEK unwrap accepted: {res:?}");
    }

    #[test]
    fn tampered_wrapped_key_is_rejected() {
        let kek = fresh_kek();
        let k_asset = fresh_k_asset();
        let mut wrapped = wrap_key(&kek, &k_asset).unwrap();
        let last = wrapped.len() - 1;
        wrapped[last] ^= 0x01;
        let res = unwrap_key(&kek, &wrapped);
        assert!(res.is_err(), "tampered wrap accepted: {res:?}");
    }

    #[test]
    fn wrong_length_wrapped_input_is_rejected() {
        let kek = fresh_kek();
        let too_short = vec![0u8; WRAPPED_KEY_LEN - 1];
        assert!(unwrap_key(&kek, &too_short).is_err());
        let too_long = vec![0u8; WRAPPED_KEY_LEN + 1];
        assert!(unwrap_key(&kek, &too_long).is_err());
    }

    #[test]
    fn wrap_is_deterministic() {
        let kek = fresh_kek();
        let k_asset = fresh_k_asset();
        let a = wrap_key(&kek, &k_asset).unwrap();
        let b = wrap_key(&kek, &k_asset).unwrap();
        assert_eq!(a, b);
    }
}
