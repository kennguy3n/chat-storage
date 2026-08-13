//! Archive compaction — merge small segments, remove tombstoned messages.

/// Plan compaction of small segments within a (conversation, bucket) pair.
pub fn plan_compaction(
    segments: &[(String, u64)], // (segment_id, size)
    min_segment_size: u64,
) -> Vec<String> {
    segments
        .iter()
        .filter(|(_, size)| *size < min_segment_size)
        .map(|(id, _)| id.clone())
        .collect()
}
