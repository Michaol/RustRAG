use regex::Regex;
use std::collections::HashSet;

#[derive(Debug, Clone, PartialEq)]
pub struct WordMapping {
    pub source_word: String,
    pub target_word: String,
    pub source_lang: String,
    pub confidence: f32,
    pub source_document: String,
}

pub struct DictionaryExtractor {
    parenthesis_pattern: Regex,
    bracket_pattern: Regex,
    comment_pattern: Regex,
}

impl DictionaryExtractor {
    pub fn new() -> Self {
        Self {
            // Match: 中文 (English) or 中文（English）
            parenthesis_pattern: Regex::new(r"([\p{Han}]+)\s*[（(]([a-zA-Z][a-zA-Z0-9_]*)[)）]")
                .unwrap(),
            // Match: 中文 [English]
            bracket_pattern: Regex::new(r"([\p{Han}]+)\s*\[([a-zA-Z][a-zA-Z0-9_]*)\]").unwrap(),
            // Pattern: // 中文注释 for symbol or /* 中文 */ near symbol
            comment_pattern: Regex::new(r"(?://|#)\s*([\p{Han}]+)").unwrap(),
        }
    }

    pub fn extract_from_content(
        &self,
        content: &str,
        source_doc: &str,
        source_lang: &str,
    ) -> Vec<WordMapping> {
        let mut mappings = Vec::new();
        let mut seen = HashSet::new();

        // Extract from parenthesis patterns: 中文 (English)
        for caps in self.parenthesis_pattern.captures_iter(content) {
            if let (Some(zh_m), Some(en_m)) = (caps.get(1), caps.get(2)) {
                let zh_word = zh_m.as_str().trim();
                let en_word = en_m.as_str().trim();
                let key = format!("{}:{}", zh_word, en_word);

                if !seen.contains(&key) && !zh_word.is_empty() && !en_word.is_empty() {
                    seen.insert(key);
                    mappings.push(WordMapping {
                        source_word: zh_word.to_string(),
                        target_word: en_word.to_lowercase(),
                        source_lang: source_lang.to_string(),
                        confidence: 1.0,
                        source_document: source_doc.to_string(),
                    });

                    // Also add split words from camelCase
                    let split_words = split_camel_case(en_word);
                    for sw in split_words {
                        let sw_key = format!("{}:{}", zh_word, sw);
                        if !seen.contains(&sw_key) && sw != en_word {
                            seen.insert(sw_key);
                            mappings.push(WordMapping {
                                source_word: zh_word.to_string(),
                                target_word: sw.to_lowercase(),
                                source_lang: source_lang.to_string(),
                                confidence: 0.8,
                                source_document: source_doc.to_string(),
                            });
                        }
                    }
                }
            }
        }

        // Extract from bracket patterns: 中文 [English]
        for caps in self.bracket_pattern.captures_iter(content) {
            if let (Some(zh_m), Some(en_m)) = (caps.get(1), caps.get(2)) {
                let zh_word = zh_m.as_str().trim();
                let en_word = en_m.as_str().trim();
                let key = format!("{}:{}", zh_word, en_word);
                if !seen.contains(&key) && !zh_word.is_empty() && !en_word.is_empty() {
                    seen.insert(key);
                    mappings.push(WordMapping {
                        source_word: zh_word.to_string(),
                        target_word: en_word.to_lowercase(),
                        source_lang: source_lang.to_string(),
                        confidence: 0.9,
                        source_document: source_doc.to_string(),
                    });
                }
            }
        }

        mappings
    }

    pub fn extract_from_symbols_and_comments(
        &self,
        content: &str,
        symbols: &[String],
        source_doc: &str,
        source_lang: &str,
    ) -> Vec<WordMapping> {
        let mut mappings = Vec::new();
        let mut seen = HashSet::new();

        let lines: Vec<&str> = content.lines().collect();
        for (i, line) in lines.iter().enumerate() {
            for symbol in symbols {
                if line.contains(symbol) && i > 0 {
                    let prev_line = lines[i - 1];
                    for caps in self.comment_pattern.captures_iter(prev_line) {
                        if let Some(zh_m) = caps.get(1) {
                            let zh_word = zh_m.as_str().trim();
                            let symbol_parts = split_camel_case(symbol);
                            for part in symbol_parts {
                                let key = format!("{}:{}", zh_word, part);
                                if !seen.contains(&key)
                                    && zh_word.chars().count() > 1
                                    && part.len() > 1
                                {
                                    seen.insert(key);
                                    mappings.push(WordMapping {
                                        source_word: zh_word.to_string(),
                                        target_word: part.to_lowercase(),
                                        source_lang: source_lang.to_string(),
                                        confidence: 0.6,
                                        source_document: source_doc.to_string(),
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }

        mappings
    }
}

pub fn split_camel_case(s: &str) -> Vec<String> {
    if s.contains('_') {
        return s
            .split('_')
            .filter(|p| !p.is_empty())
            .map(|p| p.to_lowercase())
            .collect();
    }

    let mut words = Vec::new();
    let mut current_word = String::new();

    for (i, c) in s.chars().enumerate() {
        if i > 0 && c.is_uppercase() {
            if !current_word.is_empty() {
                words.push(current_word.to_lowercase());
                current_word.clear();
            }
        }
        current_word.push(c);
    }

    if !current_word.is_empty() {
        words.push(current_word.to_lowercase());
    }

    words
}

pub fn is_chinese(s: &str) -> bool {
    s.chars().any(|c| {
        let u = c as u32;
        // Basic Han ideographs
        (0x4E00..=0x9FFF).contains(&u)
    })
}

pub fn detect_language(s: &str) -> &'static str {
    let mut zh_count = 0;
    let mut en_count = 0;
    let mut total_count = 0;

    for c in s.chars() {
        if c.is_alphabetic() {
            total_count += 1;
            let u = c as u32;
            if (0x4E00..=0x9FFF).contains(&u) {
                zh_count += 1;
            } else if c.is_ascii_alphabetic() {
                en_count += 1;
            }
        }
    }

    if total_count == 0 {
        return "unknown";
    }

    let zh_ratio = zh_count as f64 / total_count as f64;
    let en_ratio = en_count as f64 / total_count as f64;

    if zh_ratio > 0.3 {
        "zh"
    } else if en_ratio > 0.8 {
        "en"
    } else {
        "mixed"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_from_content() {
        let extractor = DictionaryExtractor::new();
        let content =
            "这是一个测试 (TestSequence) 并加上另一个例子 测试二 [TestCaseTwo] 忽略 (English123)";
        let mappings = extractor.extract_from_content(content, "doc.txt", "zh");

        assert!(!mappings.is_empty());

        let mut found_test_sequence = false;
        let mut found_test = false;
        let mut found_case_two = false;

        for m in mappings {
            if m.source_word == "这是一个测试" && m.target_word == "testsequence" {
                found_test_sequence = true;
            }
            if m.source_word == "这是一个测试" && m.target_word == "test" {
                found_test = true;
            }
            if m.source_word == "测试二" && m.target_word == "testcasetwo" {
                found_case_two = true;
            }
        }

        assert!(found_test_sequence);
        assert!(found_test);
        assert!(found_case_two);
    }

    #[test]
    fn test_split_camel_case() {
        assert_eq!(
            split_camel_case("camelCaseWord"),
            vec!["camel", "case", "word"]
        );
        assert_eq!(split_camel_case("PascalCase"), vec!["pascal", "case"]);
        assert_eq!(
            split_camel_case("snake_case_word"),
            vec!["snake", "case", "word"]
        );
    }
}
