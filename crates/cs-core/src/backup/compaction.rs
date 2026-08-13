//! Backup compaction — merge backup segments, remove stale data.

pub fn plan_compaction(_segments: &[(String, u64)], _max_segment_age_ms: i64) -> Vec<String> {
    // TODO: implement age-based compaction
    Vec::new()
}
