//! Backup compaction — merge backup segments, remove stale data.

pub fn plan_compaction(
    _segments: &[(String, u64)],
    _max_segment_age_ms: i64,
) -> Result<Vec<String>, crate::Error> {
    Err(crate::Error::NotImplemented(
        "compaction planning not yet implemented",
    ))
}
