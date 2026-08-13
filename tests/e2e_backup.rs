//! Module 6: Backup tests.

use crate::helpers::*;
use cs_core::backup::coordinator::BackupCoordinator;
use cs_core::backup::manifest_builder::{build_manifest, manifest_hash};
use cs_core::backup::segment_builder::{build_segment, open_segment};
use cs_core::crypto::key_bridge;
use cs_core::formats::backup_manifest::{decode_payload, encode_payload, SegmentRef};
use cs_core::transport::ChatStorageTransport;

#[test]
fn b2c_backup_incremental() {
    let mut coord = BackupCoordinator::new();
    let _backup_key = key_bridge::derive_backup_root(&[0x42u8; 32]);

    // We can't test with real transport here (no gateway), but we can verify
    // the coordinator state transitions
    assert_eq!(coord.current_generation, 0);

    // Simulate generation increment
    coord.next_generation();
    assert_eq!(coord.current_generation, 1);

    coord.next_generation();
    assert_eq!(coord.current_generation, 2);
}

#[test]
fn b2c_backup_manifest_chain_integrity() {
    let mut prev_hash = [0u8; 32];
    let mut hashes = Vec::new();

    let mut manifests = Vec::new();
    for gen in 0..3u64 {
        let manifest = build_manifest(gen, prev_hash, vec![], vec![]).expect("build failed");
        let hash = manifest_hash(&manifest).expect("hash failed");
        hashes.push(hash);
        prev_hash = hash;
        manifests.push(manifest);
    }

    // Verify chain integrity
    cs_core::restore::manifest_verifier::verify_chain(&manifests).expect("chain should be valid");
}

#[test]
fn b2c_backup_segment_encrypted() {
    let backup_key = key_bridge::derive_backup_root(&[0x42u8; 32]);
    let plaintext = b"sensitive backup data with secrets";

    let (ciphertext, nonce, hash) = build_segment(plaintext, &backup_key).expect("build failed");

    // Ciphertext should not contain plaintext
    assert!(!ciphertext.windows(plaintext.len()).any(|w| w == plaintext));
    assert!(!hash.iter().all(|&b| b == 0));

    // Decrypt and verify
    let recovered = open_segment(&ciphertext, &nonce, &backup_key).expect("open failed");
    assert_eq!(recovered.as_slice(), plaintext);
}

#[test]
fn b2c_backup_manifest_hash_nonzero() {
    let manifest = build_manifest(0, [0u8; 32], vec![], vec![]).expect("build failed");
    let hash = manifest_hash(&manifest).expect("hash failed");
    assert!(
        hash.iter().any(|&b| b != 0),
        "manifest hash should be non-zero"
    );
}

#[test]
fn b2c_backup_manifest_encode_decode_roundtrip() {
    let manifest = build_manifest(
        5,
        [0xaa; 32],
        vec![SegmentRef {
            segment_id: "seg-1".to_string(),
            storage_key: "key-1".to_string(),
            size: 1024,
            merkle_root: [0xbb; 32],
        }],
        vec![],
    )
    .expect("build failed");

    let encoded = encode_payload(&manifest).expect("encode failed");
    let decoded = decode_payload(&encoded).expect("decode failed");

    assert_eq!(decoded.generation, 5);
    assert_eq!(decoded.segments.len(), 1);
    assert_eq!(decoded.segments[0].segment_id, "seg-1");
}

#[test]
#[ignore]
fn b2b_backup_tenant_isolation() {
    let gateway = crate::harness::GatewayHarness::start().expect("gateway failed");

    let transport_a = cs_core::transport::kdrive_bridge::KdriveTransport::new(
        gateway.base_url.clone(),
        "tenant-a".to_string(),
        "user-a".to_string(),
    );
    let transport_b = cs_core::transport::kdrive_bridge::KdriveTransport::new(
        gateway.base_url.clone(),
        "tenant-b".to_string(),
        "user-b".to_string(),
    );

    let backup_key = key_bridge::derive_backup_root(&tenant_wrapping_key("tenant-a"));
    let mut coord = BackupCoordinator::new();

    let data = b"tenant A backup data";
    coord
        .run_backup(data, &backup_key, &transport_a)
        .expect("backup failed");

    // Tenant B should not see tenant A's backup manifests
    let manifests = transport_b.fetch_backup_manifests(0).expect("fetch failed");
    assert!(
        manifests.is_empty(),
        "tenant B should not see tenant A's backup manifests"
    );
}
