//! Threat detection — on-device pattern matching for malicious content.
//!
//! Detects:
//! - Suspicious URLs (known-bad domain patterns)
//! - Phishing keywords
//! - Malware hash matching (stub)
//! - Social engineering patterns

use crate::knowledge::KnowledgeError;

/// Threat level for a piece of content.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ThreatLevel {
    Safe,
    Suspicious,
    Malicious,
}

/// Result of threat detection on a message.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ThreatDetectionResult {
    pub level: ThreatLevel,
    pub reasons: Vec<String>,
    pub confidence: f32,
}

/// Known suspicious URL patterns (simplified — in production this would be
/// a regularly-updated local database).
const SUSPICIOUS_PATTERNS: &[&str] = &["bit.ly/", "tinyurl.com/", "t.co/", "goo.gl/", "amzn.to/"];

/// Phishing keywords that indicate potential social engineering.
const PHISHING_KEYWORDS: &[&str] = &[
    "verify your account",
    "confirm your password",
    "urgent action required",
    "suspended account",
    "click here to claim",
    "you've won",
    "wire transfer",
    "gift card payment",
];

/// Analyze text content for threats.
pub fn analyze_text(text: &str) -> Result<ThreatDetectionResult, KnowledgeError> {
    let mut reasons = Vec::new();
    let text_lower = text.to_lowercase();

    // Check for suspicious URL patterns
    for pattern in SUSPICIOUS_PATTERNS {
        if text_lower.contains(pattern) {
            reasons.push(format!("suspicious_url:{}", pattern));
        }
    }

    // Check for phishing keywords
    for keyword in PHISHING_KEYWORDS {
        if text_lower.contains(keyword) {
            reasons.push(format!("phishing_keyword:{}", keyword));
        }
    }

    // Check for excessive URLs (potential spam)
    let url_count = text_lower.matches("http://").count() + text_lower.matches("https://").count();
    if url_count >= 3 {
        reasons.push("excessive_urls".to_string());
    }

    let level = if reasons.iter().any(|r| r.starts_with("phishing_keyword")) {
        ThreatLevel::Malicious
    } else if !reasons.is_empty() {
        ThreatLevel::Suspicious
    } else {
        ThreatLevel::Safe
    };

    let confidence = if reasons.is_empty() {
        1.0
    } else {
        0.7 + (reasons.len() as f32 * 0.1).min(0.3)
    };

    Ok(ThreatDetectionResult {
        level,
        reasons,
        confidence,
    })
}

/// Check if a URL is suspicious based on known-bad patterns.
pub fn is_suspicious_url(url: &str) -> bool {
    let url_lower = url.to_lowercase();
    SUSPICIOUS_PATTERNS.iter().any(|p| url_lower.contains(p))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_safe_text() {
        let result = analyze_text("Hello, how are you doing today?").unwrap();
        assert_eq!(result.level, ThreatLevel::Safe);
        assert!(result.reasons.is_empty());
    }

    #[test]
    fn test_phishing_detection() {
        let result = analyze_text("Please verify your account immediately").unwrap();
        assert_eq!(result.level, ThreatLevel::Malicious);
        assert!(!result.reasons.is_empty());
    }

    #[test]
    fn test_suspicious_url() {
        let result = analyze_text("Check this out: https://bit.ly/suspicious").unwrap();
        assert_eq!(result.level, ThreatLevel::Suspicious);
    }

    #[test]
    fn test_excessive_urls() {
        let result = analyze_text("Visit http://a.com and http://b.com and http://c.com").unwrap();
        assert_eq!(result.level, ThreatLevel::Suspicious);
        assert!(result.reasons.iter().any(|r| r == "excessive_urls"));
    }
}
