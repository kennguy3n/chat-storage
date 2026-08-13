//! Backup segment builder — builds encrypted backup segments.

use crate::crypto::{self, Key32};
use crate::formats::archive_segment::{compress, decompress};

/// Build an encrypted backup segment from plaintext data.
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
    Ok((ciphertext, nonce, hash))
}

/// Decrypt and decompress a backup segment.
pub fn open_segment(
    ciphertext: &[u8],
    nonce: &[u8; 24],
    segment_key: &Key32,
) -> Result<Vec<u8>, crate::Error> {
    let compressed = crypto::open(
        segment_key,
        nonce,
        ciphertext,
        b"chat-storage/backup-segment/v1",
    )?;
    decompress(&compressed)
}
