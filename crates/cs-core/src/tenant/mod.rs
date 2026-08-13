//! Multi-tenancy — per-tenant quotas and isolation.

pub mod quota;

/// Tenant errors.
#[derive(Debug, thiserror::Error)]
pub enum TenantError {
    #[error("quota exceeded: {0}")]
    QuotaExceeded(String),

    #[error("tenant not found: {0}")]
    NotFound(String),

    #[error("{0}")]
    Custom(String),
}

impl From<String> for TenantError {
    fn from(s: String) -> Self {
        TenantError::Custom(s)
    }
}
