//! Circuit breaker for transport resilience.
//!
//! States: Closed → Open → HalfOpen → Closed (or back to Open on failure).

use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BreakerState {
    Closed,
    Open,
    HalfOpen,
}

#[derive(Debug)]
pub struct CircuitBreaker {
    failure_count: AtomicU32,
    threshold: u32,
    opened_at_ms: AtomicU64,
    recovery_timeout: Duration,
}

impl CircuitBreaker {
    pub fn new(threshold: u32) -> Self {
        Self {
            failure_count: AtomicU32::new(0),
            threshold,
            opened_at_ms: AtomicU64::new(0),
            recovery_timeout: Duration::from_secs(30),
        }
    }

    pub fn with_recovery_timeout(mut self, timeout: Duration) -> Self {
        self.recovery_timeout = timeout;
        self
    }

    pub fn record_failure(&self) {
        self.failure_count.fetch_add(1, Ordering::Relaxed);
        if self.failure_count.load(Ordering::Relaxed) >= self.threshold {
            self.opened_at_ms.store(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as u64,
                Ordering::Relaxed,
            );
        }
    }

    pub fn record_success(&self) {
        self.failure_count.store(0, Ordering::Relaxed);
        self.opened_at_ms.store(0, Ordering::Relaxed);
    }

    pub fn is_open(&self) -> bool {
        let failures = self.failure_count.load(Ordering::Relaxed);
        if failures < self.threshold {
            return false;
        }

        // Check if recovery timeout has elapsed → transition to half-open
        let opened_at = self.opened_at_ms.load(Ordering::Relaxed);
        if opened_at == 0 {
            return true;
        }
        // M15: call SystemTime::now() once and reuse
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        let elapsed = Duration::from_millis(now_ms.saturating_sub(opened_at));
        elapsed < self.recovery_timeout
    }

    /// Check if the breaker allows a request (closed or half-open).
    pub fn allow_request(&self) -> bool {
        !self.is_open()
    }

    /// Current state for observability.
    pub fn state(&self) -> BreakerState {
        let failures = self.failure_count.load(Ordering::Relaxed);
        if failures < self.threshold {
            return BreakerState::Closed;
        }
        let opened_at = self.opened_at_ms.load(Ordering::Relaxed);
        if opened_at == 0 {
            return BreakerState::Open;
        }
        // M15: call SystemTime::now() once and reuse
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        let elapsed = Duration::from_millis(now_ms.saturating_sub(opened_at));
        if elapsed >= self.recovery_timeout {
            BreakerState::HalfOpen
        } else {
            BreakerState::Open
        }
    }
}
