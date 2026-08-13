//! Semantic search using XLM-R embeddings and HNSW vector index.
//!
//! Disabled when the `onnx` feature is not enabled.

use crate::search::SearchError;
use crate::SearchResult;

/// Execute a semantic search. Returns empty results when ONNX is not available.
pub fn semantic_search(
    _db: &crate::local_store::LocalStoreDb,
    _query: &str,
    _limit: usize,
) -> Result<Vec<SearchResult>, SearchError> {
    #[cfg(not(feature = "onnx"))]
    {
        Ok(Vec::new())
    }
    #[cfg(feature = "onnx")]
    {
        // TODO: implement with ONNX Runtime + HNSW
        Ok(Vec::new())
    }
}
