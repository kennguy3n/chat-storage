//! Search shard cache — local LRU cache for cold search index shards.

use std::collections::HashMap;
use std::sync::Mutex;

use crate::formats::search_shard::ShardId;
use crate::search::SearchShardFrame;

/// LRU cache for encrypted search shards.
#[derive(Debug)]
pub struct ShardCache {
    entries: Mutex<HashMap<ShardId, CacheEntry>>,
    max_entries: usize,
    counter: Mutex<u64>,
}

#[derive(Debug)]
struct CacheEntry {
    frame: SearchShardFrame,
    last_access: u64,
}

impl ShardCache {
    pub fn new(max_entries: usize) -> Self {
        Self {
            entries: Mutex::new(HashMap::new()),
            max_entries,
            counter: Mutex::new(0),
        }
    }

    pub fn get(&self, shard_id: &ShardId) -> Option<SearchShardFrame> {
        let Ok(mut counter) = self.counter.lock() else {
            return None;
        };
        *counter += 1;
        let ts = *counter;
        drop(counter);

        let Ok(mut entries) = self.entries.lock() else {
            return None;
        };
        if let Some(entry) = entries.get_mut(shard_id) {
            entry.last_access = ts;
            return Some(entry.frame.clone());
        }
        None
    }

    pub fn insert(&self, shard_id: ShardId, frame: SearchShardFrame) {
        let Ok(mut counter) = self.counter.lock() else {
            return;
        };
        *counter += 1;
        let ts = *counter;
        drop(counter);

        let Ok(mut entries) = self.entries.lock() else {
            return;
        };
        if entries.len() >= self.max_entries && !entries.contains_key(&shard_id) {
            // Evict least-recently-used
            if let Some(lru_key) = entries
                .iter()
                .min_by_key(|(_, v)| v.last_access)
                .map(|(k, _)| k.clone())
            {
                entries.remove(&lru_key);
            }
        }
        entries.insert(
            shard_id,
            CacheEntry {
                frame,
                last_access: ts,
            },
        );
    }

    pub fn len(&self) -> usize {
        self.entries.lock().map(|e| e.len()).unwrap_or(0)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn clear(&self) {
        if let Ok(mut e) = self.entries.lock() {
            e.clear();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formats::search_shard::ShardType;

    fn make_shard_id(n: &str) -> ShardId {
        ShardId {
            conversation_hash: n.to_string(),
            bucket: "2024-01".to_string(),
            shard_type: ShardType::Text,
        }
    }

    fn make_frame(n: &str) -> SearchShardFrame {
        SearchShardFrame {
            nonce: [0u8; 24],
            ciphertext: n.as_bytes().to_vec(),
            plaintext_hash: [0u8; 32],
            plaintext_size: n.len() as u64,
        }
    }

    #[test]
    fn test_lru_eviction() {
        let cache = ShardCache::new(2);
        let k1 = make_shard_id("a");
        let k2 = make_shard_id("b");
        let k3 = make_shard_id("c");

        cache.insert(k1.clone(), make_frame("a"));
        cache.insert(k2.clone(), make_frame("b"));

        // Access k1 so k2 becomes LRU
        let _ = cache.get(&k1);

        cache.insert(k3, make_frame("c"));

        assert!(cache.get(&k1).is_some());
        assert!(cache.get(&k2).is_none()); // evicted
        assert!(cache.get(&make_shard_id("c")).is_some());
    }
}
