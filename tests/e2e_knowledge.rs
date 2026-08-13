//! Module 10: Knowledge + threat detection tests.

use cs_core::knowledge::content_classifier::{classify, ContentCategory};
use cs_core::knowledge::link_safety::{check_url, extract_urls, LinkSafetyVerdict};
use cs_core::knowledge::pii_redaction::{detect_pii, redact_text, PiiType};
use cs_core::knowledge::threat_detection::{analyze_text, ThreatLevel};

// --- Content classification ---

#[test]
fn b2c_classify_plain_text() {
    let result = classify("Hello world, how are you?").expect("classify failed");
    assert_eq!(result.category, ContentCategory::PlainText);
}

#[test]
fn b2c_classify_code_block() {
    let result = classify("```rust\nfn main() {}\n```").expect("classify failed");
    assert_eq!(result.category, ContentCategory::Code);
}

#[test]
fn b2c_classify_link() {
    let result = classify("Check https://example.com for details").expect("classify failed");
    assert_eq!(result.category, ContentCategory::Link);
}

// --- Link safety ---

#[test]
fn b2c_link_safety_safe_url() {
    let result = check_url("https://google.com").expect("check failed");
    assert_eq!(result.verdict, LinkSafetyVerdict::Safe);
}

#[test]
fn b2c_link_safety_malicious() {
    let result = check_url("https://evil-phishing.example/login").expect("check failed");
    assert_eq!(result.verdict, LinkSafetyVerdict::Malicious);
}

#[test]
fn b2c_link_safety_ip_url() {
    let result = check_url("http://192.168.1.1/path").expect("check failed");
    assert_eq!(result.verdict, LinkSafetyVerdict::Suspicious);
    assert!(result.reasons.iter().any(|r| r.contains("ip_only")));
}

#[test]
fn b2c_link_safety_shortener() {
    let result = check_url("https://bit.ly/abc123").expect("check failed");
    assert_eq!(result.verdict, LinkSafetyVerdict::Suspicious);
    assert!(result.reasons.iter().any(|r| r.contains("url_shortener")));
}

#[test]
fn b2c_link_safety_suspicious_tld() {
    let result = check_url("https://suspicious.tk/page").expect("check failed");
    assert_eq!(result.verdict, LinkSafetyVerdict::Suspicious);
    assert!(result.reasons.iter().any(|r| r.contains("suspicious_tld")));
}

// --- URL extraction ---

#[test]
fn b2c_extract_urls_multiple() {
    let text = "Visit https://a.com and http://b.com and https://c.org for info";
    let urls = extract_urls(text);
    assert_eq!(urls.len(), 3, "should extract 3 URLs");
    assert!(urls.iter().any(|u| u.contains("a.com")));
    assert!(urls.iter().any(|u| u.contains("b.com")));
    assert!(urls.iter().any(|u| u.contains("c.org")));
}

#[test]
fn b2c_extract_urls_http_https_mix() {
    let text = "http://a.com https://b.com";
    let urls = extract_urls(text);
    assert_eq!(urls.len(), 2, "should extract both http and https URLs");
}

#[test]
fn b2c_extract_urls_no_submatch_duplicates() {
    // "https://a.com" contains "http://" as a substring at position 1
    // The fix should prevent "http://" from matching inside "https://"
    let text = "https://a.com";
    let urls = extract_urls(text);
    assert_eq!(urls.len(), 1, "should not produce sub-match duplicates");
    assert_eq!(urls[0], "https://a.com");
}

#[test]
fn b2c_extract_urls_empty() {
    let urls = extract_urls("no urls here");
    assert!(urls.is_empty());
}

// --- PII detection ---

#[test]
fn b2c_pii_detect_email() {
    let matches = detect_pii("Contact alice@example.com please").expect("detect failed");
    assert!(matches.iter().any(|m| m.pii_type == PiiType::Email));
}

#[test]
fn b2c_pii_detect_phone() {
    let matches = detect_pii("Call +1-555-123-4567 now").expect("detect failed");
    assert!(matches.iter().any(|m| m.pii_type == PiiType::PhoneNumber));
}

#[test]
fn b2c_pii_detect_credit_card() {
    let matches = detect_pii("Card: 4532015112830366").expect("detect failed");
    assert!(matches.iter().any(|m| m.pii_type == PiiType::CreditCard));
}

#[test]
fn b2c_pii_redact_email() {
    let redacted = redact_text("Email: bob@test.com").expect("redact failed");
    assert!(!redacted.contains("bob@test.com"));
    assert!(redacted.contains("[REDACTED:EMAIL:"));
}

#[test]
fn b2c_pii_no_false_positives() {
    let matches = detect_pii("Hello world, how are you?").expect("detect failed");
    assert!(matches.is_empty(), "should not detect PII in plain text");
}

// --- Threat detection ---

#[test]
fn b2c_threat_safe_text() {
    let result = analyze_text("Hello, how are you doing today?").expect("analyze failed");
    assert_eq!(result.level, ThreatLevel::Safe);
}

#[test]
fn b2c_threat_phishing() {
    let result = analyze_text("Please verify your account immediately").expect("analyze failed");
    assert_eq!(result.level, ThreatLevel::Malicious);
    assert!(!result.reasons.is_empty());
}

#[test]
fn b2c_threat_suspicious_url() {
    let result = analyze_text("Check this out: https://bit.ly/suspicious").expect("analyze failed");
    assert_eq!(result.level, ThreatLevel::Suspicious);
}

#[test]
fn b2c_threat_excessive_urls() {
    let text = "Visit http://a.com and http://b.com and http://c.com";
    let result = analyze_text(text).expect("analyze failed");
    assert_eq!(result.level, ThreatLevel::Suspicious);
    assert!(result.reasons.iter().any(|r| r == "excessive_urls"));
}
