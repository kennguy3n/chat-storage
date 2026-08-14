//! Module 1: Message ingest + timeline tests.

use crate::helpers::*;
use uuid::Uuid;

#[test]
fn b2c_ingest_single_message() {
    let db = make_in_memory_db();
    seed_conversation(&db, "conv-1", "b2c", None);

    let id = ingest_one(&db, "conv-1", "user-a", "Hello world", 1_700_000_000_000);

    let skeleton = db
        .fetch_skeleton(&id)
        .expect("fetch failed")
        .expect("skeleton missing");
    assert_eq!(skeleton.conversation_id, "conv-1");
    assert_eq!(skeleton.sender_id, "user-a");

    let body = db
        .fetch_body(&id)
        .expect("fetch failed")
        .expect("body missing");
    assert_eq!(body.text_content, Some("Hello world".to_string()));
}

#[test]
fn b2c_ingest_batch_messages() {
    let db = make_in_memory_db();
    seed_conversation(&db, "conv-batch", "b2c", None);

    let ids = seed_messages(&db, "conv-batch", 100);

    let count = db.count_messages("conv-batch").expect("count failed");
    assert_eq!(count, 100);
    assert_eq!(ids.len(), 100);
}

#[test]
fn b2c_ingest_duplicate_idempotent() {
    let db = make_in_memory_db();
    seed_conversation(&db, "conv-dup", "b2c", None);

    let id = ingest_one(&db, "conv-dup", "user-a", "Hello", 1_700_000_000_000);

    // Ingest the same message again
    let msg = cs_core::message::processor::IngestedMessage {
        message_id: id.clone(),
        conversation_id: "conv-dup".to_string(),
        sender_id: "user-a".to_string(),
        created_at_ms: 1_700_000_000_000,
        text_content: Some("Hello".to_string()),
        media_descriptors: vec![],
        reply_to: None,
    };
    let result =
        cs_core::message::processor::MessagePersister::new(&db).persist_ingested_message(&msg);
    assert!(
        matches!(
            result,
            Err(cs_core::message::ProcessorError::DuplicateMessage)
        ),
        "duplicate should return DuplicateMessage error"
    );

    let count = db.count_messages("conv-dup").expect("count failed");
    assert_eq!(count, 1);
}

#[test]
fn b2c_timeline_pagination() {
    let db = make_in_memory_db();
    seed_conversation(&db, "conv-page", "b2c", None);

    let _ids = seed_messages(&db, "conv-page", 50);

    // Page 1: first 10
    let page1 = db
        .fetch_timeline("conv-page", 10, None)
        .expect("fetch failed");
    assert_eq!(page1.len(), 10);

    // Page 2: next 10 before the oldest of page1
    let before = page1.last().unwrap().created_at_ms;
    let page2 = db
        .fetch_timeline("conv-page", 10, Some(before))
        .expect("fetch failed");
    assert_eq!(page2.len(), 10);

    // Verify no overlap
    let page1_ids: std::collections::HashSet<_> = page1.iter().map(|r| &r.message_id).collect();
    let page2_ids: std::collections::HashSet<_> = page2.iter().map(|r| &r.message_id).collect();
    assert!(
        page1_ids.is_disjoint(&page2_ids),
        "pages should not overlap"
    );
}

#[test]
fn b2c_timeline_excludes_deleted() {
    let db = make_in_memory_db();
    seed_conversation(&db, "conv-del", "b2c", None);

    let id1 = ingest_one(&db, "conv-del", "user-a", "Keep me", 1_700_000_000_000);
    let _id2 = ingest_one(&db, "conv-del", "user-a", "Delete me", 1_700_000_001_000);

    mark_deleted(&db, &_id2);

    let timeline = db
        .fetch_timeline("conv-del", 10, None)
        .expect("fetch failed");
    assert_eq!(timeline.len(), 1);
    assert_eq!(timeline[0].message_id, id1);
}

#[test]
fn b2b_ingest_tenant_scoped() {
    let db = make_in_memory_db();
    seed_conversation(&db, "conv-b2b", "b2b", Some("tenant-corp"));

    let id = ingest_one(
        &db,
        "conv-b2b",
        "employee-1",
        "Q3 report",
        1_700_000_000_000,
    );

    let skeleton = db
        .fetch_skeleton(&id)
        .expect("fetch failed")
        .expect("skeleton missing");
    assert_eq!(skeleton.conversation_id, "conv-b2b");

    // Verify conversation has tenant_id
    let conn = db.read().expect("read lock failed");
    let tenant: String = conn
        .query_row(
            "SELECT tenant_id FROM conversation WHERE conversation_id = ?1",
            rusqlite::params!["conv-b2b"],
            |row| row.get(0),
        )
        .expect("query failed");
    assert_eq!(tenant, "tenant-corp");
}

#[test]
fn b2b_timeline_ordering() {
    let db = make_in_memory_db();
    seed_conversation(&db, "conv-order", "b2b", Some("tenant-corp"));

    // Insert out of order
    ingest_one(&db, "conv-order", "u1", "Oldest", 1_700_000_003_000);
    ingest_one(&db, "conv-order", "u2", "Newest", 1_700_000_001_000);
    ingest_one(&db, "conv-order", "u3", "Middle", 1_700_000_002_000);

    let timeline = db
        .fetch_timeline("conv-order", 10, None)
        .expect("fetch failed");
    assert_eq!(timeline.len(), 3);
    // DESC ordering: newest first
    assert!(timeline[0].created_at_ms >= timeline[1].created_at_ms);
    assert!(timeline[1].created_at_ms >= timeline[2].created_at_ms);
}

#[test]
#[ignore]
fn x_tenant_isolation_ingest() {
    let dir = temp_dir();
    let gateway = crate::harness::GatewayHarness::start().expect("gateway failed");
    let core_a = make_core(&gateway.base_url, "tenant-a", "user-a", dir.path());
    let dir_b = temp_dir();
    let core_b = make_core(&gateway.base_url, "tenant-b", "user-b", dir_b.path());

    let conv_id = Uuid::now_v7();
    let _client_id = core_a
        .send_text(conv_id, "tenant A secret message", None)
        .expect("send failed");

    // Tenant B should not be able to fetch tenant A's messages
    let result = core_b.ingest_remote_messages(conv_id, None);
    // The gateway should return empty or error since tenant B has no data for this conv
    if let Ok(ingest) = result {
        assert_eq!(
            ingest.new_count, 0,
            "tenant B should not see tenant A's messages"
        );
    }
}
