//! Backup segment builder — builds encrypted backup segments.

use crate::archive::types::{compress, decompress};
use crate::crypto::{self, Key32};

/// Nonce size used by the AEAD.
const NONCE_SIZE: usize = 24;

/// Build an encrypted backup segment from plaintext data.
///
/// The returned bytes are a self-contained frame: `[nonce (24 bytes) | ciphertext]`.
/// The plaintext hash is also returned for manifest inclusion.
#[allow(clippy::type_complexity)]
pub fn build_segment(
    plaintext: &[u8],
    segment_key: &Key32,
) -> Result<(Vec<u8>, [u8; 24], [u8; 32]), crate::Error> {
    let compressed = compress(plaintext)?;
    let nonce = crypto::aead::random_nonce_24();
    let ciphertext = crypto::seal(
        segment_key,
        &nonce,
        &compressed,
        b"chat-storage/backup-segment/v1",
    )?;
    let hash = crypto::content_hash(plaintext);

    // Prepend nonce to ciphertext for a self-contained frame
    let mut frame = Vec::with_capacity(NONCE_SIZE + ciphertext.len());
    frame.extend_from_slice(&nonce);
    frame.extend_from_slice(&ciphertext);

    Ok((frame, nonce, hash))
}

/// Decrypt and decompress a self-contained backup segment frame.
///
/// The input `frame` must be `[nonce (24 bytes) | ciphertext]`.
pub fn open_segment(frame: &[u8], segment_key: &Key32) -> Result<Vec<u8>, crate::Error> {
    if frame.len() < NONCE_SIZE {
        return Err(crate::Error::Storage(
            "backup segment too short to contain nonce".into(),
        ));
    }
    let (nonce_bytes, ciphertext) = frame.split_at(NONCE_SIZE);
    let mut nonce = [0u8; NONCE_SIZE];
    nonce.copy_from_slice(nonce_bytes);

    let compressed = crypto::open(
        segment_key,
        &nonce,
        ciphertext,
        b"chat-storage/backup-segment/v1",
    )?;
    decompress(&compressed)
}
