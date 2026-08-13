//! Archive prefetch — batch-by-bucket prefetch to coarsen access patterns.

/// Plan a batch prefetch for all segments in a (conversation, bucket) pair.
pub fn plan_batch_prefetch(conversation_id: &str, bucket: &str) -> Vec<(String, String)> {
    vec![(conversation_id.to_string(), bucket.to_string())]
}
