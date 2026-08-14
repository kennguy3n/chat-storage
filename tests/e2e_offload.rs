//! Module 8: Storage offload + eviction tests.

use crate::helpers::*;
use cs_core::config::StorageBudgetConfig;
use cs_core::offload::budget::StorageBudgetEnforcer;
use cs_core::offload::eviction::{plan_eviction_tiers, EvictionTier};
use cs_core::offload::hydration::{HydrationQueue, HydrationRequest};
use cs_core::offload::scoring::{eviction_score, score_message};
use cs_core::HydrationReason;
use uuid::Uuid;

#[test]
fn b2c_eviction_under_budget() {
    let db = make_in_memory_db();
    let config = StorageBudgetConfig {
        max_db_bytes: 1024 * 1024 * 1024, // 1GB
        max_cache_bytes: 512 * 1024 * 1024,
    };
    let enforcer = StorageBudgetEnforcer::new(config);

    let candidates = enforcer.check_and_plan_eviction(&db).expect("check failed");
    assert!(
        candidates.is_empty(),
        "should have no eviction candidates under budget"
    );
}

#[test]
fn b2c_eviction_over_budget() {
    let db = make_in_memory_db();
    seed_conversation(&db, "conv-evict", "b2c", None);

    // Insert a message and mark it as archived (required for body eviction)
    let id = ingest_one(
        &db,
        "conv-evict",
        "user-a",
        "evict me please",
        1_700_000_000_000,
    );
    mark_archived(&db, &id);

    // Set a tiny budget to trigger eviction
    let config = StorageBudgetConfig {
        max_db_bytes: 1, // 1 byte — always over budget
        max_cache_bytes: 1,
    };
    let enforcer = StorageBudgetEnforcer::new(config);

    let candidates = enforcer.check_and_plan_eviction(&db).expect("check failed");
    assert!(
        !candidates.is_empty(),
        "should have eviction candidates when over budget"
    );
}

#[test]
fn b2c_eviction_updates_state() {
    let db = make_in_memory_db();
    seed_conversation(&db, "conv-state", "b2c", None);

    let id = ingest_one(
        &db,
        "conv-state",
        "user-a",
        "archive then evict",
        1_700_000_000_000,
    );
    mark_archived(&db, &id);

    // Manually update body state (simulating eviction)
    db.update_body_state(&id, "remote_archive_only")
        .expect("update failed");

    let skeleton = db
        .fetch_skeleton(&id)
        .expect("fetch failed")
        .expect("skeleton missing");
    assert_eq!(
        skeleton.body_state,
        cs_core::local_store::state_machines::BodyState::RemoteArchiveOnly
    );
}

#[test]
fn b2c_eviction_pinned_not_evicted() {
    // Pinned messages have score 0.0
    let pinned_score = score_message(999_999_999, 0, false, true);
    assert_eq!(pinned_score, 0.0);

    let unpinned_score = score_message(999_999_999, 0, false, false);
    assert!(
        unpinned_score > 0.0,
        "unpinned old message should have positive score"
    );

    // In plan_eviction, pinned items would have score 0.0 and sort to the end
    let score = eviction_score(999_999_999, 0, true);
    assert_eq!(score, 0.0, "pinned items should have eviction score 0");
}

#[test]
fn b2c_eviction_tier_order() {
    let tiers = plan_eviction_tiers(1024);
    assert_eq!(tiers[0], EvictionTier::MediaOriginals);
    assert_eq!(tiers[1], EvictionTier::ColdSearchShards);
    assert_eq!(tiers[2], EvictionTier::MediaThumbnails);
    assert_eq!(tiers[3], EvictionTier::MessageBodies);
}

#[test]
fn b2c_hydration_dedup() {
    let mut q = HydrationQueue::new();
    let id = Uuid::now_v7();

    q.push(HydrationRequest {
        message_id: id,
        reason: HydrationReason::UserTap,
    });
    q.push(HydrationRequest {
        message_id: id,
        reason: HydrationReason::SearchHit,
    });

    assert_eq!(
        q.len(),
        1,
        "duplicate hydration requests should be deduplicated"
    );
}

#[test]
fn b2c_hydration_empty_queue() {
    let mut q = HydrationQueue::new();
    assert!(q.is_empty());
    let drained = q.drain();
    assert!(drained.is_empty());
}

#[test]
fn b2b_eviction_tenant_quotas() {
    // Free tier: 1GB
    let free =
        cs_core::tenant::quota::TenantQuota::for_tier(cs_core::tenant::quota::PlanTier::Free);
    assert_eq!(free.max_storage_bytes, 1024 * 1024 * 1024);

    // Pro tier: 100GB
    let pro = cs_core::tenant::quota::TenantQuota::for_tier(cs_core::tenant::quota::PlanTier::Pro);
    assert_eq!(pro.max_storage_bytes, 100 * 1024 * 1024 * 1024);

    // Enterprise: unlimited
    let ent =
        cs_core::tenant::quota::TenantQuota::for_tier(cs_core::tenant::quota::PlanTier::Enterprise);
    assert_eq!(ent.max_storage_bytes, u64::MAX);

    // Different tiers → different eviction thresholds
    let free_config = StorageBudgetConfig {
        max_db_bytes: free.max_storage_bytes,
        max_cache_bytes: free.max_storage_bytes / 2,
    };
    let pro_config = StorageBudgetConfig {
        max_db_bytes: pro.max_storage_bytes,
        max_cache_bytes: pro.max_storage_bytes / 2,
    };

    let free_enforcer = StorageBudgetEnforcer::new(free_config);
    let pro_enforcer = StorageBudgetEnforcer::new(pro_config);

    assert!(free_enforcer.eviction_threshold() < pro_enforcer.eviction_threshold());
}
