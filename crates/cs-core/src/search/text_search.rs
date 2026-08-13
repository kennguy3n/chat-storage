//! Text search via FTS5.

use crate::search::SearchError;
use crate::SearchResult;
use uuid::Uuid;

/// Execute an FTS5 text search against the local store.
pub fn text_search(
    db: &crate::local_store::LocalStoreDb,
    query: &str,
    conversation_id: Option<Uuid>,
    limit: usize,
) -> Result<Vec<SearchResult>, SearchError> {
    let fts_query = sanitize_fts_query(query);
    if fts_query.is_empty() {
        return Ok(Vec::new());
    }

    let rows = if let Some(cid) = conversation_id {
        let cid_str = cid.to_string();
        db.search_fts_filtered(&fts_query, &cid_str, limit)
            .map_err(|e| SearchError::Fts5(e.to_string()))?
    } else {
        db.search_fts(&fts_query, limit)
            .map_err(|e| SearchError::Fts5(e.to_string()))?
    };

    let results = rows
        .into_iter()
        .map(
            |(message_id, conversation_id, sender_id, created_at_ms, rank)| SearchResult {
                message_id: Uuid::parse_str(&message_id).unwrap_or_default(),
                conversation_id: Uuid::parse_str(&conversation_id).unwrap_or_default(),
                sender_id,
                created_at_ms,
                snippet: String::new(),
                score: 1.0 / (1.0 + rank.abs()),
                from_cold: false,
            },
        )
        .collect();

    Ok(results)
}

/// Sanitize a user query string for FTS5 MATCH syntax.
fn sanitize_fts_query(query: &str) -> String {
    // Escape FTS5 special characters by wrapping each token in double quotes
    query
        .split_whitespace()
        .map(|tok| {
            let cleaned: String = tok
                .chars()
                .filter(|c| !matches!(c, '"' | '*' | '(' | ')' | ':'))
                .collect();
            if cleaned.is_empty() {
                String::new()
            } else {
                format!("\"{}\"", cleaned)
            }
        })
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}
