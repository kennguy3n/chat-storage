//! Module 3: Fuzzy search tests.

use crate::helpers::*;

#[test]
fn b2c_fuzzy_typo_tolerance() {
    let db = make_in_memory_db();
    seed_conversation(&db, "conv-typo", "b2c", None);
    ingest_one(
        &db,
        "conv-typo",
        "user-a",
        "meeting tomorrow at noon",
        1_700_000_000_000,
    );

    let results = cs_core::search::fuzzy_search::fuzzy_search(&db, "metting", 10)
        .expect("fuzzy search failed");
    assert!(
        !results.is_empty(),
        "fuzzy search should find 'meeting' with typo 'metting'"
    );
}

#[test]
fn b2c_fuzzy_trigram_english() {
    let db = make_in_memory_db();
    seed_conversation(&db, "conv-tri", "b2c", None);
    ingest_one(
        &db,
        "conv-tri",
        "user-a",
        "keyboard typing test",
        1_700_000_000_000,
    );

    let results = cs_core::search::fuzzy_search::fuzzy_search(&db, "kyeboard", 10)
        .expect("fuzzy search failed");
    assert!(
        !results.is_empty(),
        "trigram search should find 'keyboard' with 'kyeboard'"
    );
}

#[test]
fn b2c_fuzzy_bigram_cjk() {
    let db = make_in_memory_db();
    seed_conversation(&db, "conv-cjk", "b2c", None);
    ingest_one(
        &db,
        "conv-cjk",
        "user-a",
        "会議の議事録を共有します",
        1_700_000_000_000,
    );

    let results =
        cs_core::search::fuzzy_search::fuzzy_search(&db, "会議", 10).expect("fuzzy search failed");
    assert!(!results.is_empty(), "bigram search should find CJK text");
}

#[test]
fn b2c_fuzzy_no_match() {
    let db = make_in_memory_db();
    seed_conversation(&db, "conv-nomatch", "b2c", None);
    ingest_one(
        &db,
        "conv-nomatch",
        "user-a",
        "hello world",
        1_700_000_000_000,
    );

    let results = cs_core::search::fuzzy_search::fuzzy_search(&db, "zzzzzzzzz", 10)
        .expect("fuzzy search failed");
    assert!(
        results.is_empty(),
        "unrelated query should return no results"
    );
}

#[test]
fn b2c_fuzzy_multilingual() {
    let db = make_in_memory_db();
    seed_conversation(&db, "conv-multi", "b2c", None);
    seed_multilingual(&db, "conv-multi");

    // English fuzzy
    let en = cs_core::search::fuzzy_search::fuzzy_search(&db, "KChat", 10).expect("failed");
    assert!(!en.is_empty(), "should find English text");

    // Japanese fuzzy
    let jp = cs_core::search::fuzzy_search::fuzzy_search(&db, "会議", 10).expect("failed");
    assert!(!jp.is_empty(), "should find Japanese text");

    // Arabic fuzzy
    let ar = cs_core::search::fuzzy_search::fuzzy_search(&db, "مرحبا", 10).expect("failed");
    assert!(!ar.is_empty(), "should find Arabic text");
}

#[test]
fn b2b_fuzzy_large_corpus() {
    let db = make_in_memory_db();
    seed_conversation(&db, "conv-large", "b2b", Some("tenant-corp"));
    seed_messages(&db, "conv-large", 500);

    // Search for a common word that appears in many messages
    let results = cs_core::search::fuzzy_search::fuzzy_search(&db, "Message", 20)
        .expect("fuzzy search failed");
    assert!(!results.is_empty(), "should find results in large corpus");
    assert!(results.len() <= 20, "should respect limit");
}
