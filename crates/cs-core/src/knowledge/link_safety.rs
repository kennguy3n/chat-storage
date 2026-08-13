//! Link safety — check URLs against known-bad patterns and reputation lists.

use crate::knowledge::KnowledgeError;

/// Link safety verdict.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum LinkSafetyVerdict {
    Safe,
    Suspicious,
    Malicious,
    Unknown,
}

/// Result of a link safety check.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LinkSafetyResult {
    pub url: String,
    pub verdict: LinkSafetyVerdict,
    pub reasons: Vec<String>,
}

/// Known malicious domains (simplified — in production this would be a
/// regularly-updated local database, possibly synced from a reputation service).
const KNOWN_MALICIOUS_DOMAINS: &[&str] = &[
    "evil-phishing.example",
    "malware-download.test",
    "fake-login.scam",
];

/// Check a single URL for safety.
pub fn check_url(url: &str) -> Result<LinkSafetyResult, KnowledgeError> {
    let mut reasons = Vec::new();
    let url_lower = url.to_lowercase();

    // Check against known malicious domains
    for domain in KNOWN_MALICIOUS_DOMAINS {
        if url_lower.contains(domain) {
            reasons.push(format!("known_malicious_domain:{}", domain));
        }
    }

    // Check for suspicious TLDs (in the domain part)
    let domain_part = url_lower
        .strip_prefix("https://")
        .or_else(|| url_lower.strip_prefix("http://"))
        .unwrap_or(&url_lower);
    let domain = domain_part.split('/').next().unwrap_or(domain_part);
    if domain.ends_with(".tk") || domain.ends_with(".ml") || domain.ends_with(".ga") {
        reasons.push("suspicious_tld".to_string());
    }

    // Check for IP-only URLs (no domain)
    if let Some(after_scheme) = url_lower.strip_prefix("http://") {
        if after_scheme.starts_with(|c: char| c.is_ascii_digit()) {
            reasons.push("ip_only_url".to_string());
        }
    }

    // Check for URL shorteners
    let shorteners = ["bit.ly/", "tinyurl.com/", "t.co/", "goo.gl/", "ow.ly/"];
    for s in &shorteners {
        if url_lower.contains(s) {
            reasons.push(format!("url_shortener:{}", s));
        }
    }

    let verdict = if reasons
        .iter()
        .any(|r| r.starts_with("known_malicious_domain"))
    {
        LinkSafetyVerdict::Malicious
    } else if !reasons.is_empty() {
        LinkSafetyVerdict::Suspicious
    } else {
        LinkSafetyVerdict::Safe
    };

    Ok(LinkSafetyResult {
        url: url.to_string(),
        verdict,
        reasons,
    })
}

/// Extract URLs from text content.
pub fn extract_urls(text: &str) -> Vec<String> {
    let mut urls = Vec::new();
    let mut found_ranges: Vec<(usize, usize)> = Vec::new();

    for (idx, _) in text
        .match_indices("https://")
        .chain(text.match_indices("http://"))
    {
        // Skip if this start is within an already-found URL
        if found_ranges
            .iter()
            .any(|(start, end)| idx >= *start && idx < *end)
        {
            continue;
        }
        let end = text[idx..]
            .find(|c: char| c.is_whitespace() || c == '<' || c == '"' || c == '\'')
            .map(|e| idx + e)
            .unwrap_or(text.len());
        let url = text[idx..end].to_string();
        if !url.is_empty() {
            found_ranges.push((idx, end));
            urls.push(url);
        }
    }
    urls
}

/// Check all URLs in a text block.
pub fn check_urls_in_text(text: &str) -> Result<Vec<LinkSafetyResult>, KnowledgeError> {
    let urls = extract_urls(text);
    urls.iter().map(|u| check_url(u)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_safe_url() {
        let result = check_url("https://example.com/page").unwrap();
        assert_eq!(result.verdict, LinkSafetyVerdict::Safe);
    }

    #[test]
    fn test_malicious_domain() {
        let result = check_url("https://evil-phishing.example/login").unwrap();
        assert_eq!(result.verdict, LinkSafetyVerdict::Malicious);
    }

    #[test]
    fn test_suspicious_tld() {
        let result = check_url("https://suspicious.tk/page").unwrap();
        assert_eq!(result.verdict, LinkSafetyVerdict::Suspicious);
    }

    #[test]
    fn test_ip_only_url() {
        let result = check_url("http://192.168.1.1/admin").unwrap();
        assert_eq!(result.verdict, LinkSafetyVerdict::Suspicious);
    }

    #[test]
    fn test_extract_urls() {
        let urls = extract_urls("Visit https://a.com and http://b.com/page");
        assert_eq!(urls.len(), 2);
        assert_eq!(urls[0], "https://a.com");
        assert_eq!(urls[1], "http://b.com/page");
    }
}
