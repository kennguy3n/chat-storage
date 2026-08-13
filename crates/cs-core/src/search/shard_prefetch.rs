//! Search shard prefetch — batch-by-bucket prefetch to coarsen access patterns.

use crate::formats::search_shard::ShardId;

/// Plan a batch prefetch for a given conversation and time bucket.
/// When any shard in a `(conversation_hash, bucket)` pair is needed,
/// all shards for that pair are fetched to coarsen the access signal.
pub fn plan_batch_prefetch(conversation_hash: &str, bucket: &str) -> Vec<ShardId> {
    vec![
        ShardId {
            conversation_hash: conversation_hash.to_string(),
            bucket: bucket.to_string(),
            shard_type: crate::formats::search_shard::ShardType::Text,
        },
        ShardId {
            conversation_hash: conversation_hash.to_string(),
            bucket: bucket.to_string(),
            shard_type: crate::formats::search_shard::ShardType::Fuzzy,
        },
        ShardId {
            conversation_hash: conversation_hash.to_string(),
            bucket: bucket.to_string(),
            shard_type: crate::formats::search_shard::ShardType::Vector,
        },
        ShardId {
            conversation_hash: conversation_hash.to_string(),
            bucket: bucket.to_string(),
            shard_type: crate::formats::search_shard::ShardType::Media,
        },
        ShardId {
            conversation_hash: conversation_hash.to_string(),
            bucket: bucket.to_string(),
            shard_type: crate::formats::search_shard::ShardType::Bloom,
        },
    ]
}
