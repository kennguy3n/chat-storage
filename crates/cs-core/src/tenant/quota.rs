//! Per-tenant quota enforcement.

use serde::{Deserialize, Serialize};

/// Tenant plan tier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PlanTier {
    Free,
    Pro,
    Enterprise,
}

/// Tenant quota configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TenantQuota {
    pub tier: PlanTier,
    pub max_storage_bytes: u64,
    pub max_monthly_egress_bytes: u64,
    pub max_messages_per_day: u64,
}

impl Default for TenantQuota {
    fn default() -> Self {
        Self {
            tier: PlanTier::Free,
            max_storage_bytes: 1024 * 1024 * 1024, // 1 GB
            max_monthly_egress_bytes: 10 * 1024 * 1024 * 1024, // 10 GB
            max_messages_per_day: 10_000,
        }
    }
}

impl TenantQuota {
    pub fn for_tier(tier: PlanTier) -> Self {
        match tier {
            PlanTier::Free => TenantQuota::default(),
            PlanTier::Pro => Self {
                tier: PlanTier::Pro,
                max_storage_bytes: 100 * 1024 * 1024 * 1024, // 100 GB
                max_monthly_egress_bytes: 1024 * 1024 * 1024 * 1024, // 1 TB
                max_messages_per_day: 100_000,
            },
            PlanTier::Enterprise => Self {
                tier: PlanTier::Enterprise,
                max_storage_bytes: u64::MAX,
                max_monthly_egress_bytes: u64::MAX,
                max_messages_per_day: u64::MAX,
            },
        }
    }
}
