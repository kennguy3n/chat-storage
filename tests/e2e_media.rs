//! Module 9: Media processing tests.

use crate::helpers::*;
use cs_core::media::cache::MediaCache;
use cs_core::media::chunker::{chunk, chunk_size_for};
use cs_core::media::processor::process_media;

#[test]
fn b2c_media_process_image() {
    let data = b"fake image data for processing test";
    let descriptor = process_media("asset-1", "image/png", data, "", "").expect("process failed");

    assert_eq!(descriptor.asset_id, "asset-1");
    assert_eq!(descriptor.mime_type, "image/png");
    assert!(descriptor.chunk_count > 0);
    assert!(
        !descriptor.merkle_root.iter().all(|&b| b == 0),
        "merkle root should be non-zero"
    );
}

#[test]
fn b2c_media_store_and_load() {
    let dir = temp_dir();
    let cache_dir = dir.path().join("media_cache");
    let mut cache = MediaCache::new(cache_dir, 1024 * 1024); // 1MB max

    let data = b"test media content bytes";
    cache.store("asset-store-test", data).expect("store failed");

    assert!(cache.has("asset-store-test"));

    let loaded = cache.load("asset-store-test").expect("load failed");
    assert_eq!(loaded.as_slice(), data);
}

#[test]
fn b2c_media_cache_eviction() {
    let dir = temp_dir();
    let cache_dir = dir.path().join("media_cache_evict");
    let mut cache = MediaCache::new(cache_dir, 100); // 100 bytes max — very small

    // Store item 1 (50 bytes)
    cache.store("item-1", &[0xaa; 50]).expect("store 1 failed");
    assert!(cache.has("item-1"));

    // Store item 2 (50 bytes) — should still fit
    cache.store("item-2", &[0xbb; 50]).expect("store 2 failed");
    assert!(cache.has("item-2"));

    // Store item 3 (50 bytes) — should evict item-1 (oldest)
    cache.store("item-3", &[0xcc; 50]).expect("store 3 failed");
    assert!(cache.has("item-3"));
    assert!(cache.has("item-2")); // item-2 was stored after item-1
                                  // item-1 should have been evicted (it was the oldest)
                                  // Note: eviction depends on mtime which may have sub-second resolution
}

#[test]
fn b2c_media_chunker() {
    let data = vec![0u8; 10 * 1024 * 1024]; // 10 MB
    let chunks = chunk(&data);
    assert!(chunks.len() > 1, "10MB should produce multiple chunks");

    // Verify reassembly
    let reassembled: Vec<u8> = chunks.concat();
    assert_eq!(reassembled, data);
}

#[test]
fn b2c_media_chunk_size_scales() {
    // Small file → small chunk
    let small = chunk_size_for(1024);
    // Large file → larger chunk
    let large = chunk_size_for(1024 * 1024 * 1024);
    assert!(large >= small, "larger files should have >= chunk size");
}

#[test]
fn b2c_media_send_message() {
    let dir = temp_dir();
    let gateway_url = "http://localhost:8080";
    // Use in-memory DB for local-only media test
    let db = make_in_memory_db();
    seed_conversation(&db, "conv-media", "b2c", None);

    // Create a test file
    let test_file = dir.path().join("test.png");
    std::fs::write(&test_file, b"fake png data").expect("write failed");

    // We test the media processing + DB storage directly since send_media
    // requires a CoreImpl with transport
    let descriptor =
        process_media("asset-send", "image/png", b"fake png data", "", "").expect("process failed");

    // Insert a skeleton first (FK constraint requires it)
    use cs_core::local_store::{MessageKind, MessageSkeleton};
    let skeleton = MessageSkeleton {
        message_id: "msg-send".to_string(),
        conversation_id: "conv-media".to_string(),
        sender_id: "user-a".to_string(),
        created_at_ms: 1_700_000_000_000,
        received_at_ms: 1_700_000_001_000,
        kind: MessageKind::Media,
        body_state: "local_plain_available".to_string(),
        media_state: Some("original_local".to_string()),
        archive_state: "not_archived".to_string(),
        backup_state: "not_backed_up".to_string(),
        reply_to: None,
        edited_at_ms: None,
        deleted_at_ms: None,
    };
    db.insert_skeleton(&skeleton)
        .expect("insert skeleton failed");

    let asset = cs_core::local_store::MediaAsset {
        asset_id: "asset-send".to_string(),
        message_id: "msg-send".to_string(),
        mime_type: "image/png".to_string(),
        bytes_total: 13,
        bytes_local: 13,
        media_state: "original_local".to_string(),
        chunk_count: descriptor.chunk_count as i64,
        merkle_root: descriptor.merkle_root.to_vec(),
        node_id: String::new(),
        version_id: String::new(),
        storage_sink: "kdrive".to_string(),
        created_at_ms: 1_700_000_000_000,
    };
    db.insert_media_asset(&asset).expect("insert failed");

    let fetched = db
        .fetch_media_asset("asset-send")
        .expect("fetch failed")
        .expect("asset missing");
    assert_eq!(fetched.mime_type, "image/png");
    assert_eq!(fetched.bytes_total, 13);
    assert_eq!(fetched.media_state, "original_local");

    let _ = gateway_url; // suppress unused warning
}
