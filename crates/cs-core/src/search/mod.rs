//! Search engine — FTS5, fuzzy, semantic, cold shards, multilingual.

pub mod cold_shard_source;
pub mod coordinator;
pub mod fuzzy_search;
pub mod query_engine;
pub mod search_target;
pub mod semantic_search;
pub mod shard_builder;
pub mod shard_cache;
pub mod shard_prefetch;
pub mod text_search;
pub mod tokenizer;

/// Search errors.
#[derive(Debug, thiserror::Error)]
pub enum SearchError {
    #[error("query parse: {0}")]
    QueryParse(String),

    #[error("FTS5: {0}")]
    Fts5(String),

    #[error("cold source: {0}")]
    ColdSource(String),

    #[error("semantic: {0}")]
    Semantic(String),

    #[error("{0}")]
    Custom(String),
}

impl From<String> for SearchError {
    fn from(s: String) -> Self {
        SearchError::Custom(s)
    }
}
