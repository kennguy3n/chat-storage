//! Multilingual tokenizer for FTS5 and fuzzy indexing.

/// ISO-15924 script code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Script {
    Latn,
    Cyrl,
    Grek,
    Hani,
    Hira,
    Kana,
    Hang,
    Arab,
    Hebr,
    Deva,
    Beng,
    Thai,
    Other,
}

impl Script {
    pub fn code(&self) -> &'static str {
        match self {
            Script::Latn => "Latn",
            Script::Cyrl => "Cyrl",
            Script::Grek => "Grek",
            Script::Hani => "Hani",
            Script::Hira => "Hira",
            Script::Kana => "Kana",
            Script::Hang => "Hang",
            Script::Arab => "Arab",
            Script::Hebr => "Hebr",
            Script::Deva => "Deva",
            Script::Beng => "Beng",
            Script::Thai => "Thai",
            Script::Other => "Other",
        }
    }
}

/// Detect the dominant script of a character.
pub fn detect_script(c: char) -> Script {
    let code = c as u32;
    match code {
        0x0041..=0x005A | 0x0061..=0x007A | 0x00C0..=0x024F => Script::Latn,
        0x0400..=0x04FF => Script::Cyrl,
        0x0370..=0x03FF => Script::Grek,
        0x4E00..=0x9FFF | 0xF900..=0xFAFF => Script::Hani,
        0x3040..=0x309F => Script::Hira,
        0x30A0..=0x30FF => Script::Kana,
        0xAC00..=0xD7AF | 0x1100..=0x11FF => Script::Hang,
        0x0600..=0x06FF => Script::Arab,
        0x0590..=0x05FF => Script::Hebr,
        0x0900..=0x097F => Script::Deva,
        0x0980..=0x09FF => Script::Beng,
        0x0E00..=0x0E7F => Script::Thai,
        _ => Script::Other,
    }
}

/// Generate trigrams for a token (Latin/Cyrillic/Greek).
pub fn trigrams(token: &str) -> Vec<String> {
    let chars: Vec<char> = token.chars().collect();
    if chars.len() < 3 {
        return vec![token.to_lowercase()];
    }
    (0..=chars.len().saturating_sub(3))
        .map(|i| chars[i..i + 3].iter().collect::<String>().to_lowercase())
        .collect()
}

/// Generate bigrams for a token (CJK).
pub fn bigrams(token: &str) -> Vec<String> {
    let chars: Vec<char> = token.chars().collect();
    if chars.len() < 2 {
        return vec![token.to_string()];
    }
    (0..=chars.len().saturating_sub(2))
        .map(|i| chars[i..i + 2].iter().collect())
        .collect()
}

/// Tokenize text into script-tagged tokens.
pub fn tokenize(text: &str) -> Vec<(String, Script)> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut current_script = Script::Latn;

    for c in text.chars() {
        if c.is_whitespace() {
            if !current.is_empty() {
                tokens.push((current.clone(), current_script));
                current.clear();
            }
            continue;
        }

        let script = detect_script(c);
        if script != current_script && !current.is_empty() {
            tokens.push((current.clone(), current_script));
            current.clear();
        }
        current_script = script;
        current.push(c);
    }

    if !current.is_empty() {
        tokens.push((current, current_script));
    }

    tokens
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_script() {
        assert_eq!(detect_script('A'), Script::Latn);
        assert_eq!(detect_script('Д'), Script::Cyrl);
        assert_eq!(detect_script('会'), Script::Hani);
        assert_eq!(detect_script('あ'), Script::Hira);
    }

    #[test]
    fn test_trigrams() {
        let t = trigrams("hello");
        assert_eq!(t, vec!["hel", "ell", "llo"]);
    }

    #[test]
    fn test_bigrams() {
        let b = bigrams("会議");
        assert_eq!(b, vec!["会議"]);
    }

    #[test]
    fn test_tokenize_mixed() {
        let tokens = tokenize("Meeting at 3pm 会議室で");
        assert!(tokens.len() >= 2);
        assert!(tokens.iter().any(|(_, s)| *s == Script::Latn));
        assert!(tokens.iter().any(|(_, s)| *s == Script::Hani));
    }
}
