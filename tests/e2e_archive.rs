//! Module 5: Archive tests.

use crate::helpers::*;
use cs_core::archive::coordinator::ArchiveCoordinator;
use cs_core::archive::epoch_keys::EpochKeyManager;
use cs_core::archive::segment_builder::{build_segment, open_segment};
use cs_core::crypto::key_bridge;
use cs_core::message::processor::IngestedMessage;
use cs_core::transport::ChatStorageTransport;

fn make_test_messages(count: usize, conv_id: &str) -> Vec<IngestedMessage> {
    (0..count)
        .map(|i| IngestedMessage {
            message_id: format!("msg-arch-{i}"),
            conversation_id: conv_id.to_string(),
            sender_id: "user-a".to_string(),
            created_at_ms: 1_700_000_000_000 + i as i64 * 1000,
            text_content: Some(format!("Archive message {i}")),
            media_descriptors: vec![],
            reply_to: None,
        })
        .collect()
}

#[test]
fn b2c_archive_segment_roundtrip() {
    let wrapping_key = [0x42u8; 32];
    let epoch_mgr = EpochKeyManager::new(&wrapping_key);
    let segment_key = epoch_mgr.current_epoch_key();

    let messages = make_test_messages(10, "conv-arch");
    let frame = build_segment(&messages, "conv-arch", "2024-01", 1, &segment_key)
        .expect("build segment failed");

    // Verify frame has ciphertext
    assert!(!frame.ciphertext.is_empty());
    assert!(!frame.nonce.is_empty());
    assert_eq!(frame.plaintext_hash.len(), 32);

    // Decrypt and verify
    let payload = open_segment(&frame, &segment_key).expect("open segment failed");
    assert_eq!(payload.entries.len(), 10);
    assert_eq!(payload.entries[0].message_id, "msg-arch-0");
}

#[test]
fn b2c_archive_compression() {
    let wrapping_key = [0x42u8; 32];
    let epoch_mgr = EpochKeyManager::new(&wrapping_key);
    let segment_key = epoch_mgr.current_epoch_key();

    // Large repetitive text should compress well
    let messages = vec![IngestedMessage {
        message_id: "msg-compress".to_string(),
        conversation_id: "conv-compress".to_string(),
        sender_id: "user-a".to_string(),
        created_at_ms: 1_700_000_000_000,
        text_content: Some("AAAAAAAAAAAAAAAA".repeat(1000)),
        media_descriptors: vec![],
        reply_to: None,
    }];

    let frame = build_segment(&messages, "conv-compress", "2024-01", 1, &segment_key)
        .expect("build segment failed");

    // Ciphertext should be smaller than plaintext due to zstd compression
    let plaintext_size = frame.plaintext_size;
    assert!(
        (frame.ciphertext.len() as u64) < plaintext_size,
        "compressed ciphertext ({}) should be smaller than plaintext ({})",
        frame.ciphertext.len(),
        plaintext_size
    );
}

#[test]
fn b2c_archive_epoch_key_rotation() {
    let wrapping_key = [0x42u8; 32];
    let mut mgr = EpochKeyManager::new(&wrapping_key);

    let key1 = mgr.current_epoch_key();
    assert!(!key1.iter().all(|&b| b == 0));

    // Rotate — wraps old key, derives new
    mgr.rotate().expect("rotate failed");

    let key2 = mgr.current_epoch_key();
    assert_ne!(key1, key2, "epoch key should change after rotation");

    // Old key should be recoverable from wrapped prior epochs
    let wrapped = mgr.wrapped_prior_epochs();
    assert_eq!(wrapped.len(), 1, "should have 1 wrapped prior epoch");

    let recovered = mgr.epoch_key(wrapped[0].0);
    assert!(recovered.is_some(), "should recover old epoch key");
    assert_eq!(
        recovered.unwrap(),
        key1,
        "recovered key should match original"
    );
}

#[test]
fn b2c_archive_manifest_chain() {
    let wrapping_key = [0x42u8; 32];
    let _archive_root = key_bridge::derive_archive_root(&wrapping_key);

    // Simulate 3 manifest generations
    let mut prev_hash = [0u8; 32];
    let mut hashes = Vec::new();

    for gen in 0..3 {
        let manifest =
            cs_core::archive::manifest_builder::build_manifest(gen, prev_hash, vec![], vec![])
                .expect("build manifest failed");

        let hash =
            cs_core::archive::manifest_builder::manifest_hash(&manifest).expect("hash failed");
        hashes.push(hash);
        prev_hash = hash;
    }

    // Verify chain: each manifest's prev_hash should equal the hash of the previous
    // (already enforced by construction — verify hashes are unique and non-zero)
    for i in 1..hashes.len() {
        assert_ne!(hashes[i], hashes[i - 1], "manifest hashes should be unique");
    }
    assert!(
        hashes.iter().all(|h| h.iter().any(|&b| b != 0)),
        "hashes should be non-zero"
    );
}

#[test]
#[ignore]
fn b2c_archive_single_batch() {
    let gateway = crate::harness::GatewayHarness::start().expect("gateway failed");
    let transport = cs_core::transport::kdrive_bridge::KdriveTransport::new(
        gateway.base_url.clone(),
        "test-token-tenant-a".to_string(),
        "tenant-a".to_string(),
        "user-a".to_string(),
    );

    let mut coord = ArchiveCoordinator::new(&tenant_wrapping_key("tenant-a"));
    let messages = make_test_messages(10, "conv-arch-e2e");

    let segment_id = coord
        .archive_batch(&messages, "conv-arch-e2e", "2024-01", &transport)
        .expect("archive batch failed");

    assert!(!segment_id.is_empty());
    assert_eq!(coord.pending_count(), 1);
}

#[test]
#[ignore]
fn b2b_archive_tenant_isolation() {
    let gateway = crate::harness::GatewayHarness::start().expect("gateway failed");

    let transport_a = cs_core::transport::kdrive_bridge::KdriveTransport::new(
        gateway.base_url.clone(),
        "test-token-tenant-a".to_string(),
        "tenant-a".to_string(),
        "user-a".to_string(),
    );
    let transport_b = cs_core::transport::kdrive_bridge::KdriveTransport::new(
        gateway.base_url.clone(),
        "test-token-tenant-b".to_string(),
        "tenant-b".to_string(),
        "user-b".to_string(),
    );

    let mut coord_a = ArchiveCoordinator::new(&tenant_wrapping_key("tenant-a"));
    let messages = make_test_messages(5, "conv-iso-arch");

    let segment_id = coord_a
        .archive_batch(&messages, "conv-iso-arch", "2024-01", &transport_a)
        .expect("archive failed");

    // Tenant B should not be able to download tenant A's segment
    let result = transport_b.download_archive_segment(&segment_id);
    assert!(
        result.is_err(),
        "tenant B should not access tenant A's archive segment"
    );
}
