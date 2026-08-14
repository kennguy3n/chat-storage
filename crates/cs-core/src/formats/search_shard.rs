//! Encrypted search index shard wire format.
//!
//! Mirrors the JSON sketch in `docs/DESIGN.md §7.8` but written as
//! CBOR for on-disk / on-wire use.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::serde_bytes_array;

/// Magic string for [`SearchIndexShard`].
pub const SHARD_MAGIC: &str = "KCHAT_INDEX_SHARD_V1";

/// On-wire `version` field for [`SearchIndexShard`].
pub const SHARD_VERSION: u32 = 1;

/// Compression scheme applied inside the AEAD (`zstd`).
pub const SHARD_COMPRESSION: &str = "zstd";

/// AEAD construction used to seal the shard payload.
pub const SHARD_ENCRYPTION: &str = "xchacha20-poly1305";

/// Discriminant for the kinds of search index a shard can carry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum IndexType {
    Bloom,
    Text,
    Fuzzy,
    Vector,
    Media,
}

impl IndexType {
    pub fn all() -> &'static [IndexType] {
        &[
            IndexType::Bloom,
            IndexType::Text,
            IndexType::Fuzzy,
            IndexType::Vector,
            IndexType::Media,
        ]
    }
}

// --- Compatibility aliases for existing callers ---

/// Compatibility alias for `IndexType`.
pub type ShardType = IndexType;

impl ShardType {
    pub fn as_str(&self) -> &'static str {
        match self {
            ShardType::Text => "text",
            ShardType::Fuzzy => "fuzzy",
            ShardType::Vector => "vector",
            ShardType::Media => "media",
            ShardType::Bloom => "bloom",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "text" => Some(ShardType::Text),
            "fuzzy" => Some(ShardType::Fuzzy),
            "vector" => Some(ShardType::Vector),
            "media" => Some(ShardType::Media),
            "bloom" => Some(ShardType::Bloom),
            _ => None,
        }
    }
}

/// Search index shard identifier.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ShardId {
    pub conversation_hash: String,
    pub bucket: String,
    pub shard_type: ShardType,
}

impl ShardId {
    pub fn to_key(&self) -> String {
        format!(
            "{}__{}__{}",
            self.conversation_hash,
            self.bucket,
            self.shard_type.as_str()
        )
    }
}

/// Encrypted search index shard frame.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SearchIndexShard {
    pub magic: String,
    pub version: u32,
    pub shard_id: Uuid,
    pub index_type: IndexType,
    #[serde(with = "serde_bytes")]
    pub conversation_id_hash: Vec<u8>,
    pub time_bucket: String,
    pub doc_count: u64,
    pub compression: String,
    pub encryption: String,
    #[serde(with = "serde_bytes_array")]
    pub nonce: [u8; 24],
    #[serde(with = "serde_bytes_array")]
    pub aad_hash: [u8; 32],
    #[serde(with = "serde_bytes")]
    pub ciphertext: Vec<u8>,
    #[serde(with = "serde_bytes_array")]
    pub ciphertext_sha256: [u8; 32],
}

impl SearchIndexShard {
    pub fn has_valid_header(&self) -> bool {
        self.magic == SHARD_MAGIC
            && self.version == SHARD_VERSION
            && self.compression == SHARD_COMPRESSION
            && self.encryption == SHARD_ENCRYPTION
    }
}

// Note: SearchShardFrame is an internal type in the search module,
// not the wire-format SearchIndexShard.

// --- Payload types (used by shard builders) ---

/// Text shard payload — FTS5 index data for a (conversation, bucket).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextShardPayload {
    pub entries: Vec<TextShardEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextShardEntry {
    pub message_id: String,
    pub conversation_id: String,
    pub sender_id: String,
    pub created_at_ms: i64,
    pub text_content: String,
}

/// Fuzzy shard payload — trigram/bigram tokens.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FuzzyShardPayload {
    pub entries: Vec<FuzzyShardEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FuzzyShardEntry {
    pub token: String,
    pub script: String,
    pub message_id: String,
}

/// Vector shard payload — embedding rows.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorShardPayload {
    pub model_version: String,
    pub entries: Vec<VectorShardEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorShardEntry {
    pub message_id: String,
    pub embedding: Vec<u8>,
}

/// Media shard payload — OCR/caption/transcript data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaShardPayload {
    pub entries: Vec<MediaShardEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaShardEntry {
    pub asset_id: String,
    pub kind: String,
    pub text: String,
    pub language: Option<String>,
    pub confidence: Option<f32>,
}

/// Bloom shard payload — bloom filter for cold-shard pruning.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BloomShardPayload {
    pub bits: Vec<u8>,
    pub bit_count: u64,
    pub hash_count: u8,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_shard(index_type: IndexType) -> SearchIndexShard {
        let seed = match index_type {
            IndexType::Bloom => 0x05,
            IndexType::Text => 0x10,
            IndexType::Fuzzy => 0x20,
            IndexType::Vector => 0x30,
            IndexType::Media => 0x40,
        };
        SearchIndexShard {
            magic: SHARD_MAGIC.to_string(),
            version: SHARD_VERSION,
            shard_id: Uuid::now_v7(),
            index_type,
            conversation_id_hash: vec![seed ^ 0xA5; 32],
            time_bucket: "2026-04".to_string(),
            doc_count: 12_000,
            compression: SHARD_COMPRESSION.to_string(),
            encryption: SHARD_ENCRYPTION.to_string(),
            nonce: [seed; 24],
            aad_hash: [seed.wrapping_add(1); 32],
            ciphertext: vec![seed.wrapping_add(2); 256],
            ciphertext_sha256: [seed.wrapping_add(3); 32],
        }
    }

    #[test]
    fn shard_round_trips_for_every_index_type() {
        for &it in IndexType::all() {
            let shard = sample_shard(it);
            let bytes = crate::cbor::to_vec(&shard).expect("encode");
            let decoded: SearchIndexShard = crate::cbor::from_slice(&bytes).expect("decode");
            assert_eq!(decoded, shard, "round-trip failed for {it:?}");
        }
    }

    #[test]
    fn shard_magic_and_version_are_v1() {
        let shard = sample_shard(IndexType::Text);
        assert_eq!(shard.magic, "KCHAT_INDEX_SHARD_V1");
        assert_eq!(shard.version, 1);
        assert!(shard.has_valid_header());
    }

    #[test]
    fn shard_rejects_wrong_magic() {
        let mut shard = sample_shard(IndexType::Vector);
        shard.magic = "NOT_KCHAT".to_string();
        assert!(!shard.has_valid_header());
    }

    #[test]
    fn shard_rejects_wrong_compression() {
        let mut shard = sample_shard(IndexType::Vector);
        shard.compression = "gzip".to_string();
        assert!(!shard.has_valid_header());
    }

    #[test]
    fn distinct_index_types_produce_distinct_cbor() {
        let bloom = crate::cbor::to_vec(&sample_shard(IndexType::Bloom)).unwrap();
        let text = crate::cbor::to_vec(&sample_shard(IndexType::Text)).unwrap();
        let fuzzy = crate::cbor::to_vec(&sample_shard(IndexType::Fuzzy)).unwrap();
        let vector = crate::cbor::to_vec(&sample_shard(IndexType::Vector)).unwrap();
        let media = crate::cbor::to_vec(&sample_shard(IndexType::Media)).unwrap();
        let all = [&bloom, &text, &fuzzy, &vector, &media];
        for i in 0..all.len() {
            for j in (i + 1)..all.len() {
                assert_ne!(all[i], all[j], "shards {i} and {j} encode to the same CBOR");
            }
        }
    }

    #[test]
    fn index_type_round_trips_via_lowercase_string() {
        for &it in IndexType::all() {
            let bytes = crate::cbor::to_vec(&it).expect("encode");
            let decoded: IndexType = crate::cbor::from_slice(&bytes).expect("decode");
            assert_eq!(decoded, it);
        }
    }
}
