//! Search target — multi-scope search targeting (B2C, B2B per-tenant, community).

use serde::{Deserialize, Serialize};

/// Controls which conversations and tenants to search across.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SearchTarget {
    /// Search only the user's personal B2C conversations.
    Personal,
    /// Search a specific tenant's B2B conversations.
    Tenant { tenant_id: String },
    /// Search a specific community.
    Community { community_id: String },
    /// Search across all accessible scopes.
    All,
}
