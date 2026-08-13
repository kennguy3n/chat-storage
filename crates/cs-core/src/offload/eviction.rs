//! Eviction — tiered eviction across media, search shards, message bodies.

use crate::local_store::{LocalStoreDb, StorageError};
use crate::offload::scoring;

/// Eviction tier (ordered by eviction priority).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvictionTier {
    MediaOriginals,
    MediaThumbnails,
    ColdSearchShards,
    MessageBodies,
}

/// An eviction candidate identified by the planner.
#[derive(Debug, Clone)]
pub struct EvictionCandidate {
    pub kind: EvictionTier,
    pub id: String,
    pub size_bytes: u64,
    pub score: f64,
}

/// Plan tiered eviction to free `target_bytes`.
/// Queries the local DB for evictable items and scores them.
pub fn plan_eviction(
    db: &LocalStoreDb,
    target_bytes: u64,
) -> Result<Vec<EvictionCandidate>, StorageError> {
    let mut candidates = Vec::new();
    let now_ms = now_ms();

    // 1. Media originals (highest eviction priority)
    let media_items = db.fetch_evictable_media(200)?;
    for (asset_id, bytes_local, created_at_ms) in media_items {
        let age_ms = now_ms - created_at_ms;
        let score = scoring::eviction_score(age_ms, 0, false);
        candidates.push(EvictionCandidate {
            kind: EvictionTier::MediaOriginals,
            id: asset_id,
            size_bytes: bytes_local as u64,
            score,
        });
    }

    // 2. Message bodies (lowest priority — only evict archived messages)
    let body_items = db.fetch_evictable_bodies(200)?;
    for (message_id, text_len, created_at_ms) in body_items {
        let age_ms = now_ms - created_at_ms;
        let score = scoring::eviction_score(age_ms, 0, false);
        candidates.push(EvictionCandidate {
            kind: EvictionTier::MessageBodies,
            id: message_id,
            size_bytes: text_len as u64,
            score,
        });
    }

    // Sort by score (highest score = evict first)
    candidates.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    // Truncate to only what's needed to meet target_bytes
    let mut accumulated = 0u64;
    let mut result = Vec::new();
    for candidate in candidates {
        if accumulated >= target_bytes {
            break;
        }
        accumulated += candidate.size_bytes;
        result.push(candidate);
    }

    Ok(result)
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

/// Plan a simple tiered eviction order (no DB query, just tier ordering).
pub fn plan_eviction_tiers(target_bytes: u64) -> Vec<EvictionTier> {
    let _ = target_bytes;
    vec![
        EvictionTier::MediaOriginals,
        EvictionTier::ColdSearchShards,
        EvictionTier::MediaThumbnails,
        EvictionTier::MessageBodies,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plan_eviction_tiers_order() {
        let tiers = plan_eviction_tiers(1024);
        assert_eq!(tiers[0], EvictionTier::MediaOriginals);
        assert_eq!(tiers[1], EvictionTier::ColdSearchShards);
        assert_eq!(tiers[2], EvictionTier::MediaThumbnails);
        assert_eq!(tiers[3], EvictionTier::MessageBodies);
    }
}
