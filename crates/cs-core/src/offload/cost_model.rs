//! Eviction cost model — estimates the cost of evicting vs. keeping data.

/// Estimate the cost of evicting a message (lower = cheaper to evict).
pub fn eviction_cost(age_ms: i64, access_count: u32, bytes: u64) -> f64 {
    let age_factor = 1.0 / (1.0 + (age_ms as f64 / (1000.0 * 3600.0)).max(0.1));
    let access_factor = 1.0 / (1.0 + access_count as f64);
    let size_factor = bytes as f64 / (1024.0 * 1024.0); // MB
    age_factor * access_factor - size_factor * 0.01
}
