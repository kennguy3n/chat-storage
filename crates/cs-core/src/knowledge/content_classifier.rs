//! Content classifier — categorizes messages by type (text, code, link, media, etc).

use crate::knowledge::KnowledgeError;

/// Content category.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ContentCategory {
    PlainText,
    Code,
    Link,
    MediaRef,
    SystemMessage,
    Mixed,
}

/// Result of content classification.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ClassificationResult {
    pub category: ContentCategory,
    pub confidence: f32,
    pub detected_languages: Vec<String>,
}

/// Classify message content.
pub fn classify(text: &str) -> Result<ClassificationResult, KnowledgeError> {
    let mut signals = Vec::new();

    // Code detection heuristics
    let code_indicators = [
        "fn ", "func ", "def ", "class ", "import ", "package ", "public ", "private ", "const ",
        "let ", "var ", "```", "    ", "	", // tab
    ];
    let code_hits = code_indicators
        .iter()
        .filter(|ind| text.contains(*ind))
        .count();
    if code_hits >= 2 {
        signals.push(ContentCategory::Code);
    }

    // Link detection
    if text.contains("http://") || text.contains("https://") {
        signals.push(ContentCategory::Link);
    }

    // Media reference detection
    if text.contains("[media]")
        || text.contains("[image]")
        || text.contains("[video]")
        || text.contains("[audio]")
    {
        signals.push(ContentCategory::MediaRef);
    }

    // System message detection
    let system_indicators = [
        "joined the group",
        "left the group",
        "was added",
        "was removed",
        "changed the group name",
    ];
    if system_indicators.iter().any(|s| text.contains(s)) {
        signals.push(ContentCategory::SystemMessage);
    }

    // Default to plain text
    if signals.is_empty() {
        signals.push(ContentCategory::PlainText);
    }

    let category = if signals.len() == 1 {
        signals[0]
    } else {
        ContentCategory::Mixed
    };

    // Simple language detection
    let detected_languages = detect_languages(text);

    let confidence = if signals.len() == 1 { 0.9 } else { 0.7 };

    Ok(ClassificationResult {
        category,
        confidence,
        detected_languages,
    })
}

/// Simple language detection based on script analysis.
fn detect_languages(text: &str) -> Vec<String> {
    let mut langs = Vec::new();
    let has_latin = text.chars().any(|c| c.is_ascii_alphabetic());
    let has_cjk = text.chars().any(|c| ('\u{4E00}'..='\u{9FFF}').contains(&c));
    let has_hiragana = text.chars().any(|c| ('\u{3040}'..='\u{309F}').contains(&c));
    let has_katakana = text.chars().any(|c| ('\u{30A0}'..='\u{30FF}').contains(&c));
    let has_hangul = text.chars().any(|c| ('\u{AC00}'..='\u{D7AF}').contains(&c));
    let has_cyrillic = text.chars().any(|c| ('\u{0400}'..='\u{04FF}').contains(&c));

    if has_latin {
        langs.push("en".to_string());
    }
    if has_cjk {
        langs.push("zh".to_string());
    }
    if has_hiragana || has_katakana {
        langs.push("ja".to_string());
    }
    if has_hangul {
        langs.push("ko".to_string());
    }
    if has_cyrillic {
        langs.push("ru".to_string());
    }

    if langs.is_empty() {
        langs.push("unknown".to_string());
    }

    langs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_classify_plain_text() {
        let result = classify("Hello world, how are you?").unwrap();
        assert_eq!(result.category, ContentCategory::PlainText);
    }

    #[test]
    fn test_classify_code() {
        let result = classify("fn main() {\n    println!(\"hello\");\n}").unwrap();
        assert_eq!(result.category, ContentCategory::Code);
    }

    #[test]
    fn test_classify_link() {
        let result = classify("Check out https://example.com").unwrap();
        assert_eq!(result.category, ContentCategory::Link);
    }

    #[test]
    fn test_classify_mixed() {
        let result =
            classify("Check https://example.com\n```python\ndef hello():\n    print('hi')\n```")
                .unwrap();
        assert_eq!(result.category, ContentCategory::Mixed);
    }

    #[test]
    fn test_classify_system_message() {
        let result = classify("Alice joined the group").unwrap();
        assert_eq!(result.category, ContentCategory::SystemMessage);
    }

    #[test]
    fn test_detect_languages() {
        let result = classify("Hello 你好").unwrap();
        assert!(result.detected_languages.contains(&"en".to_string()));
        assert!(result.detected_languages.contains(&"zh".to_string()));
    }
}
