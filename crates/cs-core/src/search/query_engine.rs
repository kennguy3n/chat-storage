//! Unified query engine — combines FTS5, fuzzy, and semantic search
//! with cold-shard search when `SearchScope::IncludeCold` is selected.

use std::sync::Arc;

use crate::local_store::LocalStoreDb;
use crate::search::{
    cold_shard_source::ColdShardSource, fuzzy_search::fuzzy_search,
    semantic_search::semantic_search, shard_cache::ShardCache, text_search::text_search,
    SearchError,
};
use crate::{ContentKind, SearchQuery, SearchResult, SearchScope};

/// The query engine unifies local FTS5, fuzzy, and semantic search
/// with cold-shard search when `SearchScope::IncludeCold` is selected.
pub struct QueryEngine {
    db: Arc<LocalStoreDb>,
    shard_cache: ShardCache,
    cold_source: Option<Box<dyn ColdShardSource>>,
}

impl std::fmt::Debug for QueryEngine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("QueryEngine")
            .field("db", &self.db)
            .field("shard_cache", &self.shard_cache)
            .field("has_cold_source", &self.cold_source.is_some())
            .finish()
    }
}

impl QueryEngine {
    pub fn new(db: Arc<LocalStoreDb>) -> Self {
        Self {
            db,
            shard_cache: ShardCache::new(64),
            cold_source: None,
        }
    }

    pub fn with_cold_source(mut self, source: Box<dyn ColdShardSource>) -> Self {
        self.cold_source = Some(source);
        self
    }

    /// Execute a search query.
    pub fn execute_search(
        &self,
        query: &SearchQuery,
        scope: SearchScope,
    ) -> Result<Vec<SearchResult>, SearchError> {
        let limit = 50;
        let mut results = Vec::new();

        // 1. FTS5 text search
        let text_results = text_search(&self.db, &query.query, query.conversation_id, limit)?;
        results.extend(text_results);

        // 2. Fuzzy search (if FTS5 returned few results)
        if results.len() < limit / 2 {
            let fuzzy_results = fuzzy_search(&self.db, &query.query, limit)?;
            results.extend(fuzzy_results);
        }

        // 3. Semantic search (if enabled)
        let semantic_results = semantic_search(&self.db, &query.query, limit)?;
        results.extend(semantic_results);

        // 4. Cold shard search (if scope includes cold and source is configured)
        if scope == SearchScope::IncludeCold {
            if let Some(ref cold) = self.cold_source {
                let cold_results = self.search_cold_shards(&**cold, &query.query, limit)?;
                results.extend(cold_results);
            }
        }

        // 5. Generate snippets for results that don't have them
        for result in &mut results {
            if result.snippet.is_empty() {
                if let Ok(Some(body)) = self.db.fetch_body(&result.message_id.to_string()) {
                    if let Some(text) = &body.text_content {
                        result.snippet = generate_snippet(text, &query.query, 80);
                    }
                }
            }
        }

        // 6. Apply post-filters from SearchQuery (sender_id, date range, content_kind)
        results.retain(|r| {
            // Filter by sender_id
            if let Some(ref sender) = query.sender_id {
                if &r.sender_id != sender {
                    return false;
                }
            }
            // Filter by date range
            if let Some(from) = query.date_from_ms {
                if r.created_at_ms < from {
                    return false;
                }
            }
            if let Some(to) = query.date_to_ms {
                if r.created_at_ms > to {
                    return false;
                }
            }
            // Filter by content_kind
            if let Some(kind) = query.content_kind {
                if !matches_content_kind(r, kind, &self.db) {
                    return false;
                }
            }
            true
        });

        // Deduplicate by message_id and sort by score descending, recency tiebreaker
        let mut seen = std::collections::HashSet::new();
        results.retain(|r| seen.insert(r.message_id));
        results.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| b.created_at_ms.cmp(&a.created_at_ms))
        });
        results.truncate(limit);

        Ok(results)
    }

    /// Search cold shards by fetching and decrypting them.
    fn search_cold_shards(
        &self,
        source: &dyn ColdShardSource,
        query: &str,
        limit: usize,
    ) -> Result<Vec<SearchResult>, SearchError> {
        // For now, cold shard search is a no-op since we don't have
        // a shard directory/listing endpoint. In production, the gateway
        // would provide a list of shard IDs for the tenant, and we'd
        // fetch+decrypt+search each one.
        //
        // The shard_cache is checked first to avoid re-fetching.
        let _ = (source, query, limit);
        Ok(Vec::new())
    }
}

/// Check if a search result matches the requested content kind filter.
///
/// Looks up the message skeleton to determine if the message is text, media,
/// etc. Link filtering checks the snippet for URLs.
fn matches_content_kind(result: &SearchResult, kind: ContentKind, db: &LocalStoreDb) -> bool {
    match kind {
        ContentKind::Text => {
            // Text messages: skeleton kind == "text"
            if let Ok(Some(skeleton)) = db.fetch_skeleton(&result.message_id.to_string()) {
                skeleton.kind == crate::local_store::MessageKind::Text
            } else {
                false
            }
        }
        ContentKind::Media => {
            // Media messages: skeleton kind == "media"
            if let Ok(Some(skeleton)) = db.fetch_skeleton(&result.message_id.to_string()) {
                skeleton.kind == crate::local_store::MessageKind::Media
            } else {
                false
            }
        }
        ContentKind::Document => {
            // Document: media with document-like mime type (pdf, etc.)
            // Check if there's a media asset with a document mime type
            if let Ok(Some(skeleton)) = db.fetch_skeleton(&result.message_id.to_string()) {
                if skeleton.kind != crate::local_store::MessageKind::Media {
                    return false;
                }
                // Could check media_asset mime_type here; for now treat all media as potential documents
                true
            } else {
                false
            }
        }
        ContentKind::Link => {
            // Link: snippet contains a URL pattern
            result.snippet.contains("http://") || result.snippet.contains("https://")
        }
    }
}

/// Generate a snippet around the first match of `query` in `text`.
fn generate_snippet(text: &str, query: &str, max_len: usize) -> String {
    let query_lower = query.to_lowercase();
    let text_lower = text.to_lowercase();

    if let Some(pos) = text_lower.find(&query_lower) {
        let start = pos.saturating_sub(max_len / 3);
        let end = (pos + query.len() + max_len / 2).min(text.len());
        let snippet = &text[start..end];
        let prefix = if start > 0 { "..." } else { "" };
        let suffix = if end < text.len() { "..." } else { "" };
        format!("{}{}{}", prefix, snippet, suffix)
    } else if text.len() <= max_len {
        text.to_string()
    } else {
        format!("{}...", &text[..max_len])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_snippet_match() {
        let text = "This is a long text that contains the word hello somewhere in the middle";
        let snippet = generate_snippet(text, "hello", 30);
        assert!(snippet.contains("hello"));
    }

    #[test]
    fn test_generate_snippet_no_match() {
        let text = "Short text";
        let snippet = generate_snippet(text, "missing", 20);
        assert_eq!(snippet, "Short text");
    }

    #[test]
    fn test_generate_snippet_truncates() {
        let text = "A very long text that exceeds the max length limit for the snippet generation function";
        let snippet = generate_snippet(text, "missing", 20);
        assert!(snippet.ends_with("..."));
        assert!(snippet.len() <= 23); // 20 + "..."
    }
}
