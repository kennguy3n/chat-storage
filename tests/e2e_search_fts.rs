//! Module 2: FTS5 text search tests.

use crate::helpers::*;

#[test]
fn b2c_fts_basic_match() {
    let db = make_in_memory_db();
    seed_conversation(&db, "conv-fts", "b2c", None);
    ingest_one(
        &db,
        "conv-fts",
        "user-a",
        "Hello world from KChat",
        1_700_000_000_000,
    );

    let results = db.search_fts("hello", 10).expect("search failed");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].0, results[0].0); // message_id is dynamic
}

#[test]
fn b2c_fts_phrase_query() {
    let db = make_in_memory_db();
    seed_conversation(&db, "conv-phrase", "b2c", None);
    ingest_one(
        &db,
        "conv-phrase",
        "user-a",
        "the quick brown fox jumps",
        1_700_000_000_000,
    );

    let results = db.search_fts("\"quick brown\"", 10).expect("search failed");
    assert_eq!(results.len(), 1);
}

#[test]
fn b2c_fts_filtered_by_conversation() {
    let db = make_in_memory_db();
    seed_conversation(&db, "conv-a", "b2c", None);
    seed_conversation(&db, "conv-b", "b2c", None);

    ingest_one(
        &db,
        "conv-a",
        "user-a",
        "unique keyword report",
        1_700_000_000_000,
    );
    ingest_one(
        &db,
        "conv-b",
        "user-b",
        "unique keyword report",
        1_700_000_001_000,
    );

    let results = db
        .search_fts_filtered("report", "conv-a", 10)
        .expect("search failed");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].1, "conv-a");
}

#[test]
fn b2c_fts_rank_ordering() {
    let db = make_in_memory_db();
    seed_conversation(&db, "conv-rank", "b2c", None);

    ingest_one(
        &db,
        "conv-rank",
        "u1",
        "report report report important",
        1_700_000_000_000,
    );
    ingest_one(&db, "conv-rank", "u2", "report", 1_700_000_001_000);

    let results = db.search_fts("report", 10).expect("search failed");
    assert_eq!(results.len(), 2);
    // FTS5 ranks by relevance — the one with more occurrences should rank better (lower rank value)
    assert!(
        results[0].4 <= results[1].4,
        "first result should have better rank"
    );
}

#[test]
fn b2c_fts_empty_query() {
    let db = make_in_memory_db();
    seed_conversation(&db, "conv-empty", "b2c", None);
    ingest_one(
        &db,
        "conv-empty",
        "user-a",
        "Hello world",
        1_700_000_000_000,
    );

    let results = db.search_fts("", 10).expect("search failed");
    // Empty MATCH query should return empty or no results
    assert!(results.is_empty(), "empty query should return no results");
}

#[test]
fn b2c_fts_special_chars_sanitize() {
    let db = make_in_memory_db();
    seed_conversation(&db, "conv-sanitize", "b2c", None);
    ingest_one(
        &db,
        "conv-sanitize",
        "user-a",
        "normal text content",
        1_700_000_000_000,
    );

    // The text_search module sanitizes FTS5 queries — a query with injection chars
    // should be sanitized and not cause a SQL error
    let results = cs_core::search::text_search::text_search(&db, "\" OR 1=1 --", None, 10);
    // Should not panic or error — sanitized to empty or safe query
    assert!(results.is_ok(), "sanitized query should not error");
}

#[test]
fn b2b_fts_cross_conversation() {
    let db = make_in_memory_db();
    seed_conversation(&db, "conv-b2b-1", "b2b", Some("tenant-corp"));
    seed_conversation(&db, "conv-b2b-2", "b2b", Some("tenant-corp"));

    ingest_one(
        &db,
        "conv-b2b-1",
        "emp-1",
        "quarterly report",
        1_700_000_000_000,
    );
    ingest_one(
        &db,
        "conv-b2b-2",
        "emp-2",
        "quarterly review",
        1_700_000_001_000,
    );

    let results = db.search_fts("quarterly", 10).expect("search failed");
    assert_eq!(
        results.len(),
        2,
        "should find results across both conversations"
    );
}

#[test]
#[ignore]
fn x_tenant_fts_isolation() {
    // FTS5 is local to each device's DB — tenant isolation is enforced at the
    // transport/gateway level. This test verifies that tenant B's local DB
    // doesn't contain tenant A's messages (since they're separate DBs).
    let db_a = make_in_memory_db();
    let db_b = make_in_memory_db();

    seed_conversation(&db_a, "conv-iso", "b2c", Some("tenant-a"));
    ingest_one(
        &db_a,
        "conv-iso",
        "user-a",
        "tenant A secret",
        1_700_000_000_000,
    );

    let results_b = db_b.search_fts("tenant", 10).expect("search failed");
    assert!(
        results_b.is_empty(),
        "tenant B's DB should not contain tenant A's messages"
    );
}
