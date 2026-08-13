//! AEAD helpers — XChaCha20-Poly1305 for archive/backup/search,
//! AES-256-GCM for hot paths.

use aes_gcm::{Aes256Gcm, Nonce as AesNonce};
use chacha20poly1305::{
    aead::{Aead, KeyInit, Payload},
    XChaCha20Poly1305, XNonce,
};

use super::{CryptoError, Key32, Nonce12, Nonce24};

/// Seal plaintext with XChaCha20-Poly1305.
///
/// Returns `ciphertext || tag` (the combined output from the AEAD).
pub fn seal(
    key: &Key32,
    nonce: &Nonce24,
    plaintext: &[u8],
    aad: &[u8],
) -> Result<Vec<u8>, CryptoError> {
    let cipher = XChaCha20Poly1305::new(key.into());
    cipher
        .encrypt(
            XNonce::from_slice(nonce),
            Payload {
                msg: plaintext,
                aad,
            },
        )
        .map_err(|e| CryptoError::Aead(e.to_string()))
}

/// Open (decrypt) XChaCha20-Poly1305 ciphertext.
pub fn open(
    key: &Key32,
    nonce: &Nonce24,
    ciphertext: &[u8],
    aad: &[u8],
) -> Result<Vec<u8>, CryptoError> {
    let cipher = XChaCha20Poly1305::new(key.into());
    cipher
        .decrypt(
            XNonce::from_slice(nonce),
            Payload {
                msg: ciphertext,
                aad,
            },
        )
        .map_err(|e| CryptoError::Aead(e.to_string()))
}

/// Seal plaintext with AES-256-GCM (hot path, platform-accelerated).
pub fn seal_aes(
    key: &Key32,
    nonce: &Nonce12,
    plaintext: &[u8],
    aad: &[u8],
) -> Result<Vec<u8>, CryptoError> {
    let cipher = Aes256Gcm::new(key.into());
    cipher
        .encrypt(
            AesNonce::from_slice(nonce),
            Payload {
                msg: plaintext,
                aad,
            },
        )
        .map_err(|e| CryptoError::Aead(e.to_string()))
}

/// Open (decrypt) AES-256-GCM ciphertext.
pub fn open_aes(
    key: &Key32,
    nonce: &Nonce12,
    ciphertext: &[u8],
    aad: &[u8],
) -> Result<Vec<u8>, CryptoError> {
    let cipher = Aes256Gcm::new(key.into());
    cipher
        .decrypt(
            AesNonce::from_slice(nonce),
            Payload {
                msg: ciphertext,
                aad,
            },
        )
        .map_err(|e| CryptoError::Aead(e.to_string()))
}

/// Seal in-place (allocates a new buffer for the ciphertext).
pub fn seal_in_place(
    key: &Key32,
    nonce: &Nonce24,
    plaintext: Vec<u8>,
    aad: &[u8],
) -> Result<Vec<u8>, CryptoError> {
    seal(key, nonce, &plaintext, aad)
}

/// Open in-place (allocates a new buffer for the plaintext).
pub fn open_in_place(
    key: &Key32,
    nonce: &Nonce24,
    ciphertext: Vec<u8>,
    aad: &[u8],
) -> Result<Vec<u8>, CryptoError> {
    open(key, nonce, &ciphertext, aad)
}

/// Generate a random 24-byte nonce for XChaCha20-Poly1305.
pub fn random_nonce_24() -> Nonce24 {
    use rand::RngCore;
    let mut nonce = [0u8; 24];
    rand::rngs::OsRng.fill_bytes(&mut nonce);
    nonce
}

/// Generate a random 12-byte nonce for AES-256-GCM.
pub fn random_nonce_12() -> Nonce12 {
    use rand::RngCore;
    let mut nonce = [0u8; 12];
    rand::rngs::OsRng.fill_bytes(&mut nonce);
    nonce
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_xchacha20_roundtrip() {
        let key = [0x42u8; 32];
        let nonce = random_nonce_24();
        let plaintext = b"hello world";
        let aad = b"associated data";

        let ct = seal(&key, &nonce, plaintext, aad).unwrap();
        let pt = open(&key, &nonce, &ct, aad).unwrap();

        assert_eq!(pt, plaintext);
    }

    #[test]
    fn test_aes256gcm_roundtrip() {
        let key = [0x42u8; 32];
        let nonce = random_nonce_12();
        let plaintext = b"hello world";
        let aad = b"associated data";

        let ct = seal_aes(&key, &nonce, plaintext, aad).unwrap();
        let pt = open_aes(&key, &nonce, &ct, aad).unwrap();

        assert_eq!(pt, plaintext);
    }

    #[test]
    fn test_aad_mismatch_fails() {
        let key = [0x42u8; 32];
        let nonce = random_nonce_24();
        let plaintext = b"hello world";
        let aad = b"correct aad";

        let ct = seal(&key, &nonce, plaintext, aad).unwrap();
        let result = open(&key, &nonce, &ct, b"wrong aad");

        assert!(result.is_err());
    }
}
