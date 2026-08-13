//! PII redaction — detect and redact personally identifiable information.

use crate::knowledge::KnowledgeError;

/// Type of PII detected.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum PiiType {
    Email,
    PhoneNumber,
    CreditCard,
    Ssn,
    IpAddress,
}

/// A PII detection match.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PiiMatch {
    pub pii_type: PiiType,
    pub start: usize,
    pub end: usize,
    pub redacted: String,
}

/// Detect PII in text content.
pub fn detect_pii(text: &str) -> Result<Vec<PiiMatch>, KnowledgeError> {
    let mut matches = Vec::new();

    // Email detection
    for (idx, _) in text.match_indices('@') {
        // Simple heuristic: word@word.tld
        let start = text[..idx]
            .rfind(|c: char| !c.is_alphanumeric() && c != '.' && c != '-' && c != '_')
            .map(|s| s + 1)
            .unwrap_or(0);
        let end = text[idx..]
            .find(|c: char| c.is_whitespace())
            .map(|e| idx + e)
            .unwrap_or(text.len());
        if end > start && text[start..end].contains('.') {
            let redacted = format!("[REDACTED:EMAIL:{}]", end - start);
            matches.push(PiiMatch {
                pii_type: PiiType::Email,
                start,
                end,
                redacted,
            });
        }
    }

    // Phone number detection (simple: 10+ digits with optional separators)
    let phone_pattern = regex_like_phone(text);
    for (start, end) in phone_pattern {
        let redacted = format!("[REDACTED:PHONE:{}]", end - start);
        matches.push(PiiMatch {
            pii_type: PiiType::PhoneNumber,
            start,
            end,
            redacted,
        });
    }

    // Credit card detection (13-19 digit sequences)
    let cc_matches = find_digit_sequences(text, 13, 19);
    for (start, end) in cc_matches {
        let redacted = format!("[REDACTED:CC:{}]", end - start);
        matches.push(PiiMatch {
            pii_type: PiiType::CreditCard,
            start,
            end,
            redacted,
        });
    }

    // Sort by position
    matches.sort_by_key(|m| m.start);
    Ok(matches)
}

/// Redact PII in text, replacing matches with redaction markers.
pub fn redact_text(text: &str) -> Result<String, KnowledgeError> {
    let matches = detect_pii(text)?;
    if matches.is_empty() {
        return Ok(text.to_string());
    }

    let mut result = String::with_capacity(text.len());
    let mut last_end = 0;
    for m in &matches {
        result.push_str(&text[last_end..m.start]);
        result.push_str(&m.redacted);
        last_end = m.end;
    }
    result.push_str(&text[last_end..]);
    Ok(result)
}

/// Simple phone number pattern matching (10+ consecutive digits with separators).
fn regex_like_phone(text: &str) -> Vec<(usize, usize)> {
    let mut results = Vec::new();
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        if chars[i].is_ascii_digit() {
            let start = i;
            let mut digit_count = 1;
            i += 1;
            while i < chars.len()
                && (chars[i].is_ascii_digit()
                    || chars[i] == '-'
                    || chars[i] == ' '
                    || chars[i] == '('
                    || chars[i] == ')')
            {
                if chars[i].is_ascii_digit() {
                    digit_count += 1;
                }
                i += 1;
            }
            if digit_count >= 10 {
                results.push((start, i));
            }
        } else {
            i += 1;
        }
    }
    results
}

/// Find sequences of digits within a length range.
fn find_digit_sequences(text: &str, min_len: usize, max_len: usize) -> Vec<(usize, usize)> {
    let mut results = Vec::new();
    let bytes = text.as_bytes();
    let mut i = 0;

    while i < bytes.len() {
        if bytes[i].is_ascii_digit() {
            let start = i;
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
            let len = i - start;
            if len >= min_len && len <= max_len {
                results.push((start, i));
            }
        } else {
            i += 1;
        }
    }
    results
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_email() {
        let matches = detect_pii("Contact me at alice@example.com please").unwrap();
        assert!(matches.iter().any(|m| m.pii_type == PiiType::Email));
    }

    #[test]
    fn test_redact_email() {
        let redacted = redact_text("Email: bob@test.com").unwrap();
        assert!(!redacted.contains("bob@test.com"));
        assert!(redacted.contains("[REDACTED:EMAIL:"));
    }

    #[test]
    fn test_detect_phone() {
        let matches = detect_pii("Call 555-123-4567 now").unwrap();
        assert!(matches.iter().any(|m| m.pii_type == PiiType::PhoneNumber));
    }

    #[test]
    fn test_detect_credit_card() {
        let matches = detect_pii("Card: 4532015112830366").unwrap();
        assert!(matches.iter().any(|m| m.pii_type == PiiType::CreditCard));
    }

    #[test]
    fn test_no_false_positives() {
        let matches = detect_pii("Hello world, how are you?").unwrap();
        assert!(matches.is_empty());
    }
}
