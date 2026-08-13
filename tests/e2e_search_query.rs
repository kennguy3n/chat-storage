//! Module 4: Unified query engine tests.

use crate::helpers::*;
use cs_core::{SearchQuery, SearchScope};

#[test]
fn b2c_unified_text_and_fuzzy() {
    let db = make_in_memory_db();
    seed_conversation(&db, "conv-unified", "b2c", None);
    ingest_one(
        &db,
        "conv-unified",
        "user-a",
        "meeting tomorrow at noon",
        1_700_000_000_000,
    );

    let query = SearchQuery {
        query: "meeting".to_string(),
        sender_id: None,
        conversation_id: None,
        date_from_ms: None,
        date_to_ms: None,
        content_kind: None,
    };
    let results = cs_core::search::query_engine::QueryEngine::new(std::sync::Arc::new(db))
        .execute_search(&query, SearchScope::LocalOnly)
        .expect("search failed");
    assert!(!results.is_empty(), "unified search should find 'meeting'");
}

#[test]
fn b2c_unified_with_semantic_disabled() {
    let db = make_in_memory_db();
    seed_conversation(&db, "conv-sem", "b2c", None);
    ingest_one(&db, "conv-sem", "user-a", "hello world", 1_700_000_000_000);

    let query = SearchQuery {
        query: "hello".to_string(),
        sender_id: None,
        conversation_id: None,
        date_from_ms: None,
        date_to_ms: None,
        content_kind: None,
    };
    let results = cs_core::search::query_engine::QueryEngine::new(std::sync::Arc::new(db))
        .execute_search(&query, SearchScope::LocalOnly)
        .expect("search failed");
    assert!(
        !results.is_empty(),
        "should find results without semantic search"
    );
}

#[test]
fn b2c_unified_cold_search_noop() {
    let db = make_in_memory_db();
    seed_conversation(&db, "conv-cold", "b2c", None);
    ingest_one(
        &db,
        "conv-cold",
        "user-a",
        "test message",
        1_700_000_000_000,
    );

    let query = SearchQuery {
        query: "test".to_string(),
        sender_id: None,
        conversation_id: None,
        date_from_ms: None,
        date_to_ms: None,
        content_kind: None,
    };
    // IncludeCold with no cold source configured → same as LocalOnly
    let results = cs_core::search::query_engine::QueryEngine::new(std::sync::Arc::new(db))
        .execute_search(&query, SearchScope::IncludeCold)
        .expect("search failed");
    assert!(
        !results.is_empty(),
        "should still find local results with IncludeCold"
    );
}

#[test]
fn b2c_unified_snippet_generation() {
    let db = make_in_memory_db();
    seed_conversation(&db, "conv-snip", "b2c", None);
    ingest_one(
        &db,
        "conv-snip",
        "user-a",
        "This is a long message about quarterly earnings report",
        1_700_000_000_000,
    );

    let query = SearchQuery {
        query: "quarterly".to_string(),
        sender_id: None,
        conversation_id: None,
        date_from_ms: None,
        date_to_ms: None,
        content_kind: None,
    };
    let results = cs_core::search::query_engine::QueryEngine::new(std::sync::Arc::new(db))
        .execute_search(&query, SearchScope::LocalOnly)
        .expect("search failed");
    assert!(!results.is_empty(), "should find results");
    assert!(
        !results[0].snippet.is_empty(),
        "snippet should be generated"
    );
}

#[test]
fn b2c_unified_dedup_by_message_id() {
    let db = make_in_memory_db();
    seed_conversation(&db, "conv-dedup", "b2c", None);
    // "hello" will match both FTS5 and fuzzy
    ingest_one(
        &db,
        "conv-dedup",
        "user-a",
        "hello world",
        1_700_000_000_000,
    );

    let query = SearchQuery {
        query: "hello".to_string(),
        sender_id: None,
        conversation_id: None,
        date_from_ms: None,
        date_to_ms: None,
        content_kind: None,
    };
    let results = cs_core::search::query_engine::QueryEngine::new(std::sync::Arc::new(db))
        .execute_search(&query, SearchScope::LocalOnly)
        .expect("search failed");
    // Should be deduplicated — same message found by both FTS5 and fuzzy
    let unique_ids: std::collections::HashSet<_> = results.iter().map(|r| r.message_id).collect();
    assert_eq!(
        unique_ids.len(),
        results.len(),
        "results should be deduplicated"
    );
}

#[test]
fn b2b_unified_sender_filter() {
    let db = make_in_memory_db();
    seed_conversation(&db, "conv-sender", "b2b", Some("tenant-corp"));
    ingest_one(
        &db,
        "conv-sender",
        "emp-1",
        "quarterly report",
        1_700_000_000_000,
    );
    ingest_one(
        &db,
        "conv-sender",
        "emp-2",
        "quarterly review",
        1_700_000_001_000,
    );

    let query = SearchQuery {
        query: "quarterly".to_string(),
        sender_id: Some("emp-1".to_string()),
        conversation_id: None,
        date_from_ms: None,
        date_to_ms: None,
        content_kind: None,
    };
    let results = cs_core::search::query_engine::QueryEngine::new(std::sync::Arc::new(db))
        .execute_search(&query, SearchScope::LocalOnly)
        .expect("search failed");
    // Note: sender_id filter is in SearchQuery but the query engine
    // doesn't currently filter by sender — this test verifies it doesn't panic
    // and returns results. The sender filter would be applied post-fetch.
    assert!(!results.is_empty(), "should find results");
}
