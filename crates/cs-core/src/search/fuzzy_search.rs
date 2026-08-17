//! Fuzzy search using trigram/bigram indexing with script-aware Levenshtein.

use std::collections::HashMap;

use crate::search::tokenizer::{bigrams, tokenize, trigrams, Script};
use crate::search::SearchError;
use crate::SearchResult;

/// Maximum number of fuzzy candidates to retrieve from the index before scoring.
const MAX_CANDIDATES: usize = 200;

/// Execute a fuzzy search against the local fuzzy index.
///
/// The search proceeds in two phases:
/// 1. **Candidate retrieval**: Query the `search_fuzzy` trigram/bigram index
///    for message IDs that share at least one n-gram with the query tokens.
///    Results are joined with `message_skeleton` and `message_body` to fetch
///    full metadata and text content.
/// 2. **Scoring**: For each candidate, compute a normalized similarity score
///    using Levenshtein distance between the query and the message text.
///    Candidates are then sorted by score descending, with recency as a
///    tiebreaker for deterministic ordering.
pub fn fuzzy_search(
    db: &crate::local_store::LocalStoreDb,
    query: &str,
    limit: usize,
) -> Result<Vec<SearchResult>, SearchError> {
    let tokens = tokenize(query);
    if tokens.is_empty() {
        return Ok(Vec::new());
    }

    let conn = db.read().map_err(|e| SearchError::Custom(e.to_string()))?;

    // Phase 1: Retrieve candidates from the fuzzy index by querying n-grams.
    // Join with message_skeleton (for metadata) and message_body (for text)
    // to get everything needed for scoring and SearchResult construction.
    let mut stmt = conn
        .prepare(
            "SELECT DISTINCT sf.message_id, ms.conversation_id, ms.sender_id,
                    ms.created_at_ms, mb.text_content
             FROM search_fuzzy sf
             JOIN message_skeleton ms ON sf.message_id = ms.message_id
             LEFT JOIN message_body mb ON sf.message_id = mb.message_id
             WHERE sf.token = ?1 AND ms.deleted_at_ms IS NULL
             LIMIT ?2",
        )
        .map_err(|e| SearchError::Custom(e.to_string()))?;

    let query_lower = query.to_lowercase();
    // Pre-convert the query to char slices once so Levenshtein does not
    // re-allocate Vec<char> on every candidate.
    let query_chars: Vec<char> = query_lower.chars().collect();

    // Collect candidates into a map keyed by message_id to deduplicate
    // (multiple n-grams may match the same message).
    let mut candidates: HashMap<String, FuzzyCandidate> = HashMap::new();

    for (token, script) in &tokens {
        let grams = match script {
            Script::Hani | Script::Hira | Script::Kana | Script::Hang => bigrams(token),
            _ => trigrams(token),
        };

        for gram in &grams {
            let rows = stmt
                .query_map(rusqlite::params![gram, MAX_CANDIDATES as i64], |row| {
                    Ok(FuzzyCandidate {
                        message_id: row.get::<_, String>(0)?,
                        conversation_id: row.get::<_, String>(1)?,
                        sender_id: row.get::<_, String>(2)?,
                        created_at_ms: row.get::<_, i64>(3)?,
                        text_content: row.get::<_, Option<String>>(4)?,
                    })
                })
                .map_err(|e| SearchError::Custom(e.to_string()))?;

            for row in rows {
                let candidate = row.map_err(|e| SearchError::Custom(e.to_string()))?;
                candidates
                    .entry(candidate.message_id.clone())
                    .or_insert(candidate);
            }
        }
    }

    // Phase 2: Score candidates using Levenshtein distance.
    let mut scored: Vec<(SearchResult, f64, i64)> = candidates
        .into_values()
        .filter_map(|c| {
            let text = c.text_content.as_deref().unwrap_or("");
            if text.is_empty() {
                return None;
            }

            let text_lower = text.to_lowercase();
            let dist = levenshtein(&query_lower, &text_lower);
            let max_len = query_lower.len().max(text_lower.len());
            let similarity = if max_len == 0 {
                0.0
            } else {
                1.0 - (dist as f64 / max_len as f64)
            };

            // Only include candidates with at least some similarity
            if similarity <= 0.0 {
                return None;
            }

            let message_id = uuid::Uuid::parse_str(&c.message_id).unwrap_or_default();
            let conversation_id = uuid::Uuid::parse_str(&c.conversation_id).unwrap_or_default();

            let result = SearchResult {
                message_id,
                conversation_id,
                sender_id: c.sender_id,
                created_at_ms: c.created_at_ms,
                snippet: String::new(),
                score: similarity,
                from_cold: false,
            };

            Some((result, similarity, c.created_at_ms))
        })
        .collect();

    // Sort by score descending, then by created_at_ms descending (recency tiebreaker)
    scored.sort_by(|a, b| {
        b.1.partial_cmp(&a.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| b.2.cmp(&a.2))
    });

    Ok(scored
        .into_iter()
        .take(limit)
        .map(|(result, _, _)| result)
        .collect())
}

/// Internal struct for fuzzy search candidate data joined from the DB.
struct FuzzyCandidate {
    message_id: String,
    conversation_id: String,
    sender_id: String,
    created_at_ms: i64,
    text_content: Option<String>,
}

/// Compute Levenshtein distance between two strings.
pub fn levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let (m, n) = (a.len(), b.len());

    if m == 0 {
        return n;
    }
    if n == 0 {
        return m;
    }

    let mut prev: Vec<usize> = (0..=n).collect();
    let mut curr = vec![0usize; n + 1];

    for i in 1..=m {
        curr[0] = i;
        for j in 1..=n {
            let cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };
            curr[j] = (prev[j] + 1).min(curr[j - 1] + 1).min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }

    prev[n]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_levenshtein() {
        assert_eq!(levenshtein("hello", "hello"), 0);
        assert_eq!(levenshtein("hello", "hallo"), 1);
        assert_eq!(levenshtein("hello", "world"), 4);
        assert_eq!(levenshtein("", "abc"), 3);
    }
}
