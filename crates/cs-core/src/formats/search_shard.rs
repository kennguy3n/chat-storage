//! Search index shard — encrypted FTS/fuzzy/vector/media index data
//! stored on the gateway and fetched on-device for cold search.

use serde::{Deserialize, Serialize};

/// Type of search index shard.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ShardType {
    /// FTS5 text index data.
    Text,
    /// Fuzzy trigram/bigram index data.
    Fuzzy,
    /// Vector embedding index data (HNSW).
    Vector,
    /// Media search index (OCR text, captions, transcripts).
    Media,
    /// Bloom filter for cold-shard pruning.
    Bloom,
}

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
    /// Hash of the conversation ID (coarse, server-safe).
    pub conversation_hash: String,
    /// Time bucket (e.g. "2024-01").
    pub bucket: String,
    /// Shard type.
    pub shard_type: ShardType,
}

impl ShardId {
    /// Convert to a unique string key for storage.
    pub fn to_key(&self) -> String {
        format!(
            "{}__{}__{}",
            self.conversation_hash,
            self.bucket,
            self.shard_type.as_str()
        )
    }
}

/// Search index shard frame (encrypted).
///
/// Layout: `nonce(24) || XChaCha20-Poly1305(payload)`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchShardFrame {
    pub nonce: [u8; 24],
    pub ciphertext: Vec<u8>,
    pub plaintext_hash: [u8; 32],
    pub plaintext_size: u64,
}

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
    /// INT8-quantized embedding bytes.
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
    /// Bit-packed bloom filter.
    pub bits: Vec<u8>,
    /// Number of bits in the filter.
    pub bit_count: u64,
    /// Number of hash functions.
    pub hash_count: u8,
}
