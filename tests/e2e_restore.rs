//! Module 7: Restore tests.

use crate::helpers::*;
use cs_core::backup::manifest_builder::{build_manifest, manifest_hash};
use cs_core::formats::backup_manifest::{
    decode_payload, encode_payload, BackupManifestPayload, SegmentRef,
};
use cs_core::restore::manifest_verifier::verify_chain;
use cs_core::restore::state_machine::RestoreState;

#[test]
fn b2c_restore_state_machine_transitions() {
    let states = [
        RestoreState::NotStarted,
        RestoreState::FetchingManifests,
        RestoreState::FetchingSkeletons,
        RestoreState::FetchingBodies,
        RestoreState::FetchingMedia,
        RestoreState::BuildingIndexes,
        RestoreState::Complete,
    ];

    // Verify all states are distinct
    let unique: std::collections::HashSet<_> = states.iter().collect();
    assert_eq!(
        unique.len(),
        states.len(),
        "all restore states should be distinct"
    );
}

#[test]
fn b2c_restore_manifest_verification_valid_chain() {
    let mut prev_hash = [0u8; 32];
    let mut manifests = Vec::new();

    for gen in 0..3 {
        let manifest = build_manifest(gen, prev_hash, vec![], vec![]).expect("build failed");
        let hash = manifest_hash(&manifest).expect("hash failed");
        prev_hash = hash;
        manifests.push(manifest);
    }

    verify_chain(&manifests).expect("valid chain should verify");
}

#[test]
fn b2c_restore_manifest_verification_tampered() {
    let mut prev_hash = [0u8; 32];
    let mut manifests = Vec::new();

    for gen in 0..3 {
        let manifest = build_manifest(gen, prev_hash, vec![], vec![]).expect("build failed");
        let hash = manifest_hash(&manifest).expect("hash failed");
        prev_hash = hash;
        manifests.push(manifest);
    }

    // Tamper with the second manifest's previous_manifest_hash
    manifests[1].previous_manifest_hash = [0xff; 32];

    let result = verify_chain(&manifests);
    assert!(result.is_err(), "tampered chain should fail verification");
}

#[test]
fn b2c_restore_manifest_decode_roundtrip() {
    let manifest = BackupManifestPayload {
        generation: 42,
        previous_manifest_hash: [0xaa; 32],
        segments: vec![SegmentRef {
            segment_id: "seg-restore".to_string(),
            storage_key: "key-restore".to_string(),
            size: 2048,
            merkle_root: [0xbb; 32],
        }],
        wrapped_epoch_keys: vec![],
        created_at_ms: 1_700_000_000_000,
    };

    let encoded = encode_payload(&manifest).expect("encode failed");
    let decoded = decode_payload(&encoded).expect("decode failed");

    assert_eq!(decoded.generation, 42);
    assert_eq!(decoded.segments.len(), 1);
    assert_eq!(decoded.segments[0].segment_id, "seg-restore");
}

#[test]
fn b2c_restore_key_recovery() {
    use cs_core::crypto::key_wrap;
    use cs_core::restore::key_recovery::recover_epoch_key;

    let wrapping_key = [0x42u8; 32];
    let archive_root = cs_core::crypto::key_bridge::derive_archive_root(&wrapping_key);

    // Create an epoch key and wrap it
    let epoch_key = cs_core::crypto::key_bridge::derive_archive_epoch(&archive_root, 42);
    let wrapped = key_wrap::wrap_key(&archive_root, &epoch_key).expect("wrap failed");

    // Recover it
    let recovered = recover_epoch_key(&archive_root, &wrapped).expect("recover failed");
    assert_eq!(recovered, epoch_key, "recovered key should match original");
}

#[test]
#[ignore]
fn b2c_restore_from_empty() {
    let gateway = crate::harness::GatewayHarness::start().expect("gateway failed");
    let dir = temp_dir();
    let core = make_core(&gateway.base_url, "tenant-empty", "user-a", dir.path());

    let result = core
        .restore_from_backup(cs_core::BackupSource::KdriveGateway)
        .expect("restore should not error on empty");

    assert_eq!(result.messages_restored, 0);
    assert_eq!(result.conversations_restored, 0);
}

#[test]
#[ignore]
fn b2c_restore_after_backup() {
    let gateway = crate::harness::GatewayHarness::start().expect("gateway failed");
    let dir = temp_dir();
    let core = make_core(&gateway.base_url, "tenant-restore", "user-a", dir.path());

    // Run a backup first
    let backup_result = core
        .run_incremental_backup(cs_core::BackupReason::UserInitiated)
        .expect("backup failed");
    assert!(backup_result.manifest_generation > 0);

    // Now restore
    let restore_result = core
        .restore_from_backup(cs_core::BackupSource::KdriveGateway)
        .expect("restore failed");

    // Should have restored messages from the backup
    assert!(
        restore_result.messages_restored > 0,
        "should restore messages from backup"
    );
}

#[test]
#[ignore]
fn b2b_restore_tenant_scoped() {
    let gateway = crate::harness::GatewayHarness::start().expect("gateway failed");

    let dir_a = temp_dir();
    let dir_b = temp_dir();
    let core_a = make_core(&gateway.base_url, "tenant-a", "user-a", dir_a.path());
    let core_b = make_core(&gateway.base_url, "tenant-b", "user-b", dir_b.path());

    // Tenant A backs up
    core_a
        .run_incremental_backup(cs_core::BackupReason::UserInitiated)
        .expect("backup A failed");

    // Tenant B restores — should not see tenant A's data
    let result_b = core_b
        .restore_from_backup(cs_core::BackupSource::KdriveGateway)
        .expect("restore B failed");

    assert_eq!(
        result_b.messages_restored, 0,
        "tenant B should not restore tenant A's data"
    );
}
