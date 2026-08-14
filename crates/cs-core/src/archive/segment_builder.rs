//! Archive segment builder — constructs encrypted archive segments from messages.

use crate::archive::types::*;
use crate::crypto::{self, Key32};
use crate::message::processor::IngestedMessage;

/// Build an encrypted archive segment from a set of messages.
pub fn build_segment(
    messages: &[IngestedMessage],
    conversation_id: &str,
    time_bucket: &str,
    epoch_id: u64,
    segment_key: &Key32,
) -> Result<ArchiveSegmentFrame, crate::Error> {
    let segment_id = uuid::Uuid::now_v7().to_string();

    let mut entries: Vec<ArchiveEntry> = Vec::with_capacity(messages.len());
    for msg in messages {
        let body_nonce = crypto::aead::random_nonce_24();
        let body_plaintext = msg.text_content.as_deref().unwrap_or("").as_bytes();
        let body_ct = crypto::seal(
            segment_key,
            &body_nonce,
            body_plaintext,
            b"chat-storage/archive-body/v1",
        )?;
        entries.push(ArchiveEntry {
            message_id: msg.message_id.clone(),
            created_at_ms: msg.created_at_ms,
            kind: if msg.media_descriptors.is_empty() {
                EntryKind::Text
            } else {
                EntryKind::Media
            },
            body_ciphertext: body_ct,
            body_nonce,
            media_refs: msg
                .media_descriptors
                .iter()
                .map(|d| MediaRef {
                    asset_id: d.asset_id.to_string(),
                    mime_type: d.mime_type.clone(),
                    node_id: d.node_id.clone(),
                    version_id: d.version_id.clone(),
                    bytes_total: d.bytes_total,
                })
                .collect(),
        });
    }

    let payload = ArchiveSegmentPayload {
        segment_id,
        conversation_id: conversation_id.to_string(),
        time_bucket: time_bucket.to_string(),
        epoch_id,
        entries,
    };

    let plaintext = encode_payload(&payload)?;
    let plaintext_hash = crypto::content_hash(&plaintext);
    let plaintext_size = plaintext.len() as u64;

    let compressed = compress(&plaintext)?;
    let nonce = crypto::aead::random_nonce_24();
    let ciphertext = crypto::seal(
        segment_key,
        &nonce,
        &compressed,
        b"chat-storage/archive-segment/v1",
    )?;

    Ok(ArchiveSegmentFrame {
        nonce,
        ciphertext,
        plaintext_hash,
        plaintext_size,
    })
}

/// Decrypt and decode an archive segment.
pub fn open_segment(
    frame: &ArchiveSegmentFrame,
    segment_key: &Key32,
) -> Result<ArchiveSegmentPayload, crate::Error> {
    let compressed = crypto::open(
        segment_key,
        &frame.nonce,
        &frame.ciphertext,
        b"chat-storage/archive-segment/v1",
    )?;
    let plaintext = decompress(&compressed)?;
    let payload = decode_payload(&plaintext)?;
    Ok(payload)
}
