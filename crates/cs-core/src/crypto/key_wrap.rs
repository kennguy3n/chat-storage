//! AES-256-KW (Key Wrap) for wrapping epoch keys under root keys.
//!
//! Uses AES-256-GCM with a zero-plaintext-length AAD for the key-wrap
//! construction. This is a simplified key-wrap that provides
//! confidentiality and integrity for stored key material.

use aes_gcm::{
    aead::{Aead, KeyInit, Payload},
    Aes256Gcm, Nonce as AesNonce,
};

use super::{CryptoError, Key32, Nonce12};

/// Wrap a 32-byte key using AES-256-GCM.
///
/// Returns `nonce(12) || ciphertext(32) || tag(16)` = 60 bytes.
pub fn wrap_key(wrapping_key: &Key32, key_to_wrap: &Key32) -> Result<Vec<u8>, CryptoError> {
    use rand::RngCore;
    let mut nonce = [0u8; 12];
    rand::rngs::OsRng.fill_bytes(&mut nonce);

    let cipher = Aes256Gcm::new(wrapping_key.into());
    let ct = cipher
        .encrypt(
            AesNonce::from_slice(&nonce),
            Payload {
                msg: key_to_wrap.as_slice(),
                aad: b"chat-storage/key-wrap/v1",
            },
        )
        .map_err(|e| CryptoError::KeyWrap(e.to_string()))?;

    let mut out = Vec::with_capacity(12 + ct.len());
    out.extend_from_slice(&nonce);
    out.extend_from_slice(&ct);
    Ok(out)
}

/// Unwrap a wrapped key.
///
/// Input: `nonce(12) || ciphertext(32) || tag(16)` = 60 bytes.
pub fn unwrap_key(wrapping_key: &Key32, wrapped: &[u8]) -> Result<Key32, CryptoError> {
    if wrapped.len() < 12 + 16 + 32 {
        return Err(CryptoError::KeyWrap(format!(
            "wrapped key too short: {} bytes (need at least 60)",
            wrapped.len()
        )));
    }

    let nonce: Nonce12 = wrapped[..12].try_into().expect("checked length");
    let ct = &wrapped[12..];

    let cipher = Aes256Gcm::new(wrapping_key.into());
    let pt = cipher
        .decrypt(
            AesNonce::from_slice(&nonce),
            Payload {
                msg: ct,
                aad: b"chat-storage/key-wrap/v1",
            },
        )
        .map_err(|e| CryptoError::KeyWrap(e.to_string()))?;

    if pt.len() != 32 {
        return Err(CryptoError::KeyWrap(format!(
            "unwrapped key has wrong length: {} (expected 32)",
            pt.len()
        )));
    }

    let mut key = [0u8; 32];
    key.copy_from_slice(&pt);
    Ok(key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_key_wrap_roundtrip() {
        let wrapping_key = [0xABu8; 32];
        let key_to_wrap = [0xCDu8; 32];

        let wrapped = wrap_key(&wrapping_key, &key_to_wrap).unwrap();
        assert_eq!(wrapped.len(), 60);

        let unwrapped = unwrap_key(&wrapping_key, &wrapped).unwrap();
        assert_eq!(unwrapped, key_to_wrap);
    }

    #[test]
    fn test_key_wrap_wrong_key_fails() {
        let wrapping_key = [0xABu8; 32];
        let wrong_key = [0xEFu8; 32];
        let key_to_wrap = [0xCDu8; 32];

        let wrapped = wrap_key(&wrapping_key, &key_to_wrap).unwrap();
        let result = unwrap_key(&wrong_key, &wrapped);

        assert!(result.is_err());
    }
}
