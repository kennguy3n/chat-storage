//! Module 12: Tenant quotas + multi-tenant isolation tests.

use cs_core::tenant::quota::{PlanTier, TenantQuota};
use cs_core::transport::ChatStorageTransport;

// --- Quota tests ---

#[test]
fn quota_free_tier_limits() {
    let quota = TenantQuota::for_tier(PlanTier::Free);
    assert_eq!(quota.tier, PlanTier::Free);
    assert_eq!(quota.max_storage_bytes, 1024 * 1024 * 1024); // 1 GB
    assert_eq!(quota.max_monthly_egress_bytes, 10 * 1024 * 1024 * 1024); // 10 GB
    assert_eq!(quota.max_messages_per_day, 10_000);
}

#[test]
fn quota_pro_tier_limits() {
    let quota = TenantQuota::for_tier(PlanTier::Pro);
    assert_eq!(quota.tier, PlanTier::Pro);
    assert_eq!(quota.max_storage_bytes, 100 * 1024 * 1024 * 1024); // 100 GB
    assert_eq!(quota.max_monthly_egress_bytes, 1024 * 1024 * 1024 * 1024); // 1 TB
    assert_eq!(quota.max_messages_per_day, 100_000);
}

#[test]
fn quota_enterprise_unlimited() {
    let quota = TenantQuota::for_tier(PlanTier::Enterprise);
    assert_eq!(quota.tier, PlanTier::Enterprise);
    assert_eq!(quota.max_storage_bytes, u64::MAX);
    assert_eq!(quota.max_monthly_egress_bytes, u64::MAX);
    assert_eq!(quota.max_messages_per_day, u64::MAX);
}

#[test]
fn quota_default_is_free() {
    let quota = TenantQuota::default();
    assert_eq!(quota.tier, PlanTier::Free);
}

// --- Cross-tenant isolation (require gateway) ---

#[test]
#[ignore]
fn x_tenant_archive_isolation() {
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

    let segment_id = format!("seg-iso-{}", uuid::Uuid::now_v7());
    let data = b"tenant A secret archive data";

    // Tenant A uploads
    transport_a
        .upload_archive_segment(&segment_id, data)
        .expect("upload A failed");

    // Tenant B tries to download — should fail (different blob key prefix)
    let result = transport_b.download_archive_segment(&segment_id);
    assert!(
        result.is_err(),
        "tenant B should not access tenant A's archive segment"
    );
}

#[test]
#[ignore]
fn x_tenant_search_shard_isolation() {
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

    let shard_key = format!("shard-{}", uuid::Uuid::now_v7());
    let data = b"encrypted shard data";

    // Tenant A uploads
    transport_a
        .upload_search_shard(&shard_key, data)
        .expect("upload shard A failed");

    // Tenant B tries to download — should fail
    let result = transport_b.download_search_shard(&shard_key);
    assert!(
        result.is_err(),
        "tenant B should not access tenant A's search shard"
    );
}

#[test]
#[ignore]
fn x_tenant_backup_manifest_isolation() {
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

    // Tenant A uploads a backup manifest
    let manifest = b"{\"generation\":0,\"segments\":[]}";
    transport_a
        .upload_backup_manifest(manifest)
        .expect("upload manifest A failed");

    // Tenant B fetches — should not see tenant A's manifests
    let manifests = transport_b
        .fetch_backup_manifests(0)
        .expect("fetch B failed");
    assert!(
        manifests.is_empty(),
        "tenant B should not see tenant A's backup manifests"
    );
}

#[test]
#[ignore]
fn x_tenant_cross_fetch_messages() {
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

    let conv_id = format!("conv-{}", uuid::Uuid::now_v7());

    // Tenant A fetches messages for a conversation — should get empty (no data)
    let result_a = transport_a
        .fetch_messages(&conv_id, None)
        .expect("fetch A failed");
    assert!(
        result_a.messages.is_empty(),
        "no messages should exist for new conversation"
    );

    // Tenant B fetches same conversation — should also get empty
    let result_b = transport_b
        .fetch_messages(&conv_id, None)
        .expect("fetch B failed");
    assert!(
        result_b.messages.is_empty(),
        "tenant B should get empty for new conversation"
    );
}

// --- B2B vs B2C scoping ---

#[test]
fn b2b_tenant_scoped_conversation() {
    use crate::helpers::*;

    let db = make_in_memory_db();

    // B2B conversation with tenant_id
    seed_conversation(&db, "conv-b2b-scope", "b2b", Some("tenant-corp"));
    ingest_one(
        &db,
        "conv-b2b-scope",
        "emp-1",
        "B2B message",
        1_700_000_000_000,
    );

    // Verify conversation has tenant_id set
    let conn = db.read().expect("read failed");
    let tenant: Option<String> = conn
        .query_row(
            "SELECT tenant_id FROM conversation WHERE id = ?1",
            rusqlite::params!["conv-b2b-scope"],
            |row| row.get(0),
        )
        .expect("query failed");
    assert_eq!(tenant, Some("tenant-corp".to_string()));
}

#[test]
fn b2c_personal_no_tenant() {
    use crate::helpers::*;

    let db = make_in_memory_db();

    // B2C personal conversation — no tenant_id
    seed_conversation(&db, "conv-b2c-personal", "b2c", None);
    ingest_one(
        &db,
        "conv-b2c-personal",
        "user-a",
        "personal message",
        1_700_000_000_000,
    );

    // Verify conversation has no tenant_id
    {
        let conn = db.read().expect("read failed");
        let tenant: Option<String> = conn
            .query_row(
                "SELECT tenant_id FROM conversation WHERE id = ?1",
                rusqlite::params!["conv-b2c-personal"],
                |row| row.get(0),
            )
            .expect("query failed");
        assert!(
            tenant.is_none(),
            "B2C personal conversation should have no tenant_id"
        );
    }

    // Search should still work for personal conversations
    let results = db.search_fts("personal", 10).expect("search failed");
    assert_eq!(results.len(), 1);
}
