//! Search index shard builder — builds encrypted FTS/fuzzy/vector/media/bloom shards.

use crate::crypto::{self, Key32};
use crate::formats::search_shard::*;

/// Build an encrypted text shard from a set of text entries.
pub fn build_text_shard(
    entries: Vec<TextShardEntry>,
    shard_key: &Key32,
) -> Result<SearchShardFrame, crate::Error> {
    let payload = TextShardPayload { entries };
    let plaintext =
        serde_json::to_vec(&payload).map_err(|e| crate::Error::Storage(e.to_string().into()))?;
    let plaintext_hash = crypto::content_hash(&plaintext);
    let plaintext_size = plaintext.len() as u64;

    let nonce = crypto::aead::random_nonce_24();
    let ciphertext = crypto::seal(shard_key, &nonce, &plaintext, b"chat-storage/text-shard/v1")?;

    Ok(SearchShardFrame {
        nonce,
        ciphertext,
        plaintext_hash,
        plaintext_size,
    })
}

/// Decrypt a text shard.
pub fn open_text_shard(
    frame: &SearchShardFrame,
    shard_key: &Key32,
) -> Result<TextShardPayload, crate::Error> {
    let plaintext = crypto::open(
        shard_key,
        &frame.nonce,
        &frame.ciphertext,
        b"chat-storage/text-shard/v1",
    )?;
    let payload: TextShardPayload = serde_json::from_slice(&plaintext)
        .map_err(|e| crate::Error::Storage(e.to_string().into()))?;
    Ok(payload)
}

/// Build an encrypted fuzzy shard from a set of fuzzy token entries.
pub fn build_fuzzy_shard(
    entries: Vec<FuzzyShardEntry>,
    shard_key: &Key32,
) -> Result<SearchShardFrame, crate::Error> {
    let payload = FuzzyShardPayload { entries };
    let plaintext =
        serde_json::to_vec(&payload).map_err(|e| crate::Error::Storage(e.to_string().into()))?;
    let plaintext_hash = crypto::content_hash(&plaintext);
    let plaintext_size = plaintext.len() as u64;

    let nonce = crypto::aead::random_nonce_24();
    let ciphertext = crypto::seal(
        shard_key,
        &nonce,
        &plaintext,
        b"chat-storage/fuzzy-shard/v1",
    )?;

    Ok(SearchShardFrame {
        nonce,
        ciphertext,
        plaintext_hash,
        plaintext_size,
    })
}

/// Decrypt a fuzzy shard.
pub fn open_fuzzy_shard(
    frame: &SearchShardFrame,
    shard_key: &Key32,
) -> Result<FuzzyShardPayload, crate::Error> {
    let plaintext = crypto::open(
        shard_key,
        &frame.nonce,
        &frame.ciphertext,
        b"chat-storage/fuzzy-shard/v1",
    )?;
    let payload: FuzzyShardPayload = serde_json::from_slice(&plaintext)
        .map_err(|e| crate::Error::Storage(e.to_string().into()))?;
    Ok(payload)
}

/// Build an encrypted bloom shard from a bloom filter payload.
pub fn build_bloom_shard(
    payload: BloomShardPayload,
    shard_key: &Key32,
) -> Result<SearchShardFrame, crate::Error> {
    let plaintext =
        serde_json::to_vec(&payload).map_err(|e| crate::Error::Storage(e.to_string().into()))?;
    let plaintext_hash = crypto::content_hash(&plaintext);
    let plaintext_size = plaintext.len() as u64;

    let nonce = crypto::aead::random_nonce_24();
    let ciphertext = crypto::seal(
        shard_key,
        &nonce,
        &plaintext,
        b"chat-storage/bloom-shard/v1",
    )?;

    Ok(SearchShardFrame {
        nonce,
        ciphertext,
        plaintext_hash,
        plaintext_size,
    })
}

/// Decrypt a bloom shard.
pub fn open_bloom_shard(
    frame: &SearchShardFrame,
    shard_key: &Key32,
) -> Result<BloomShardPayload, crate::Error> {
    let plaintext = crypto::open(
        shard_key,
        &frame.nonce,
        &frame.ciphertext,
        b"chat-storage/bloom-shard/v1",
    )?;
    let payload: BloomShardPayload = serde_json::from_slice(&plaintext)
        .map_err(|e| crate::Error::Storage(e.to_string().into()))?;
    Ok(payload)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_key() -> Key32 {
        [0x42u8; 32]
    }

    #[test]
    fn test_text_shard_roundtrip() {
        let entries = vec![
            TextShardEntry {
                message_id: "msg-1".to_string(),
                conversation_id: "conv-1".to_string(),
                sender_id: "user-1".to_string(),
                created_at_ms: 1_700_000_000_000,
                text_content: "Hello world".to_string(),
            },
            TextShardEntry {
                message_id: "msg-2".to_string(),
                conversation_id: "conv-1".to_string(),
                sender_id: "user-2".to_string(),
                created_at_ms: 1_700_000_001_000,
                text_content: "Goodbye world".to_string(),
            },
        ];

        let key = test_key();
        let frame = build_text_shard(entries.clone(), &key).unwrap();
        let payload = open_text_shard(&frame, &key).unwrap();
        assert_eq!(payload.entries.len(), 2);
        assert_eq!(payload.entries[0].message_id, "msg-1");
        assert_eq!(payload.entries[1].text_content, "Goodbye world");
    }

    #[test]
    fn test_fuzzy_shard_roundtrip() {
        let entries = vec![
            FuzzyShardEntry {
                token: "hel".to_string(),
                script: "Latn".to_string(),
                message_id: "msg-1".to_string(),
            },
            FuzzyShardEntry {
                token: "hel".to_string(),
                script: "Latn".to_string(),
                message_id: "msg-2".to_string(),
            },
        ];

        let key = test_key();
        let frame = build_fuzzy_shard(entries.clone(), &key).unwrap();
        let payload = open_fuzzy_shard(&frame, &key).unwrap();
        assert_eq!(payload.entries.len(), 2);
        assert_eq!(payload.entries[0].token, "hel");
        assert_eq!(payload.entries[0].message_id, "msg-1");
    }

    #[test]
    fn test_bloom_shard_roundtrip() {
        let payload_data = BloomShardPayload {
            bits: vec![0x01, 0x04, 0x00, 0x20],
            bit_count: 32,
            hash_count: 3,
        };

        let key = test_key();
        let plaintext = serde_json::to_vec(&payload_data).unwrap();
        let nonce = crypto::aead::random_nonce_24();
        let ciphertext =
            crypto::seal(&key, &nonce, &plaintext, b"chat-storage/bloom-shard/v1").unwrap();
        let frame = SearchShardFrame {
            nonce,
            ciphertext,
            plaintext_hash: crypto::content_hash(&plaintext),
            plaintext_size: plaintext.len() as u64,
        };
        let payload = open_bloom_shard(&frame, &key).unwrap();
        assert_eq!(payload.bit_count, 32);
        assert_eq!(payload.hash_count, 3);
        assert_eq!(payload.bits, vec![0x01, 0x04, 0x00, 0x20]);
    }
}
