//! Eviction scoring — scores candidates based on age, access frequency, kind.

/// Score a message for eviction (higher = more evictable).
pub fn score_message(age_ms: i64, access_count: u32, has_media: bool, pinned: bool) -> f64 {
    if pinned {
        return 0.0;
    }
    let age_score = (age_ms as f64) / (1000.0 * 3600.0 * 24.0); // days
    let access_penalty = access_count as f64 * 10.0;
    let media_bonus = if has_media { 5.0 } else { 0.0 };
    (age_score - access_penalty + media_bonus).max(0.0)
}

/// Generic eviction score for any candidate (higher = more evictable).
/// Uses age and access count as primary factors.
pub fn eviction_score(age_ms: i64, access_count: u32, pinned: bool) -> f64 {
    if pinned {
        return 0.0;
    }
    let age_score = (age_ms as f64) / (1000.0 * 3600.0 * 24.0);
    let access_penalty = access_count as f64 * 10.0;
    (age_score - access_penalty).max(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pinned_never_evicted() {
        assert_eq!(score_message(999_999_999, 0, false, true), 0.0);
        assert_eq!(eviction_score(999_999_999, 0, true), 0.0);
    }

    #[test]
    fn test_older_messages_score_higher() {
        let young = score_message(1000, 0, false, false);
        let old = score_message(1_000_000_000, 0, false, false);
        assert!(old > young);
    }

    #[test]
    fn test_frequently_accessed_messages_score_lower() {
        let rare = score_message(1_000_000, 0, false, false);
        let frequent = score_message(1_000_000, 100, false, false);
        assert!(rare > frequent);
    }
}
