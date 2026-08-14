//! Cold shard source — fetch and decrypt encrypted search index shards from the gateway.

use crate::formats::search_shard::ShardId;
use crate::search::SearchError;
use crate::search::SearchShardFrame;
use crate::transport::ChatStorageTransport;

/// Source for cold (offloaded) search index shards.
pub trait ColdShardSource: Send + Sync {
    /// Fetch an encrypted shard from the backend.
    fn fetch_shard(&self, shard_id: &ShardId) -> Result<SearchShardFrame, SearchError>;
}

/// HTTP-based cold shard source that fetches shards via `ChatStorageTransport`.
pub struct HttpColdShardSource {
    transport: Box<dyn ChatStorageTransport>,
}

impl std::fmt::Debug for HttpColdShardSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HttpColdShardSource")
            .finish_non_exhaustive()
    }
}

impl HttpColdShardSource {
    pub fn new(transport: Box<dyn ChatStorageTransport>) -> Self {
        Self { transport }
    }
}

impl ColdShardSource for HttpColdShardSource {
    fn fetch_shard(&self, shard_id: &ShardId) -> Result<SearchShardFrame, SearchError> {
        let shard_key = shard_id.to_key();
        let ciphertext = self
            .transport
            .download_search_shard(&shard_key)
            .map_err(|e| SearchError::ColdSource(e.to_string()))?;

        // Deserialize the frame from CBOR/JSON
        let frame: SearchShardFrame = serde_json::from_slice(&ciphertext)
            .map_err(|e| SearchError::ColdSource(format!("decode: {}", e)))?;

        Ok(frame)
    }
}
