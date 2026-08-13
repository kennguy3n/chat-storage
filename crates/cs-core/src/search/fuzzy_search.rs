//! Fuzzy search using trigram/bigram indexing with script-aware Levenshtein.

use crate::search::tokenizer::{bigrams, tokenize, trigrams, Script};
use crate::search::SearchError;
use crate::SearchResult;

/// Execute a fuzzy search against the local fuzzy index.
pub fn fuzzy_search(
    db: &crate::local_store::LocalStoreDb,
    query: &str,
    limit: usize,
) -> Result<Vec<SearchResult>, SearchError> {
    let tokens = tokenize(query);
    let mut candidate_ids = std::collections::HashSet::new();

    let conn = db.read().map_err(|e| SearchError::Custom(e.to_string()))?;
    let mut stmt = conn
        .prepare("SELECT DISTINCT message_id FROM search_fuzzy WHERE token = ?1 LIMIT 50")
        .map_err(|e| SearchError::Custom(e.to_string()))?;

    for (token, script) in &tokens {
        let grams = match script {
            Script::Hani | Script::Hira | Script::Kana | Script::Hang => bigrams(token),
            _ => trigrams(token),
        };

        for gram in &grams {
            let rows = stmt
                .query_map(rusqlite::params![gram], |row| row.get::<_, String>(0))
                .map_err(|e| SearchError::Custom(e.to_string()))?;

            for id in rows.flatten() {
                candidate_ids.insert(id);
            }
        }
    }

    Ok(candidate_ids
        .into_iter()
        .take(limit)
        .map(|id| SearchResult {
            message_id: uuid::Uuid::parse_str(&id).unwrap_or_default(),
            conversation_id: uuid::Uuid::nil(),
            sender_id: String::new(),
            created_at_ms: 0,
            snippet: String::new(),
            score: 0.5,
            from_cold: false,
        })
        .collect())
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
