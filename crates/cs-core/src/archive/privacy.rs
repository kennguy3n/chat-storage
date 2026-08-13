//! Archive privacy — dummy request padding for high privacy mode.

/// Generate dummy fetch requests to mix with real ones.
pub fn generate_dummy_requests(count: usize) -> Vec<String> {
    (0..count).map(|i| format!("dummy-segment-{}", i)).collect()
}
