//! Storage budget enforcement.

use crate::config::StorageBudgetConfig;
use crate::local_store::LocalStoreDb;
use crate::offload::eviction::{plan_eviction, EvictionCandidate};

/// Storage budget enforcer.
#[derive(Debug)]
pub struct StorageBudgetEnforcer {
    config: StorageBudgetConfig,
}

impl StorageBudgetEnforcer {
    pub fn new(config: StorageBudgetConfig) -> Self {
        Self { config }
    }

    pub fn max_db_bytes(&self) -> u64 {
        self.config.max_db_bytes
    }

    pub fn max_cache_bytes(&self) -> u64 {
        self.config.max_cache_bytes
    }

    /// Check if the current usage exceeds the budget.
    pub fn is_over_budget(&self, db_bytes: u64, cache_bytes: u64) -> bool {
        db_bytes > self.config.max_db_bytes || cache_bytes > self.config.max_cache_bytes
    }

    /// Check current DB size and plan eviction if over budget.
    /// Returns a list of eviction candidates to free space.
    pub fn check_and_plan_eviction(
        &self,
        db: &LocalStoreDb,
    ) -> Result<Vec<EvictionCandidate>, crate::Error> {
        let db_bytes = db
            .db_size_bytes()
            .map_err(|e| crate::Error::Storage(e.to_string().into()))?;

        if db_bytes <= self.config.max_db_bytes {
            return Ok(Vec::new());
        }

        let excess = db_bytes - self.config.max_db_bytes;
        let candidates = plan_eviction(db, excess).map_err(crate::Error::Storage)?;
        Ok(candidates)
    }

    /// Compute the eviction threshold (90% of max).
    pub fn eviction_threshold(&self) -> u64 {
        self.config.max_db_bytes * 90 / 100
    }

    /// Compute the critical threshold (95% of max).
    pub fn critical_threshold(&self) -> u64 {
        self.config.max_db_bytes * 95 / 100
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_budget_enforcer() {
        let config = StorageBudgetConfig {
            max_db_bytes: 1000,
            max_cache_bytes: 500,
        };
        let enforcer = StorageBudgetEnforcer::new(config);

        assert!(!enforcer.is_over_budget(500, 200));
        assert!(enforcer.is_over_budget(1001, 200));
        assert!(enforcer.is_over_budget(500, 501));
        assert_eq!(enforcer.eviction_threshold(), 900);
        assert_eq!(enforcer.critical_threshold(), 950);
    }
}
