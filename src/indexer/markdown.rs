use std::fs;
use std::path::Path;

#[derive(Debug, Clone, PartialEq)]
pub struct Chunk {
    pub content: String,
    pub position: usize,
}

/// Parses a markdown file and splits it into chunks.
pub fn parse_markdown<P: AsRef<Path>>(
    filepath: P,
    chunk_size: usize,
) -> std::io::Result<Vec<Chunk>> {
    let content = fs::read_to_string(filepath)?;
    let chunks = split_into_chunks(&content, chunk_size);
    Ok(chunks
        .into_iter()
        .enumerate()
        .map(|(position, content)| Chunk { content, position })
        .collect())
}

/// Splits text into chunks of approximately `chunk_size` characters (using `char` count).
pub fn split_into_chunks(content: &str, chunk_size: usize) -> Vec<String> {
    let char_count = content.chars().count();

    if char_count <= chunk_size {
        let trimmed = content.trim();
        if trimmed.is_empty() {
            return Vec::new();
        }
        return vec![trimmed.to_string()];
    }

    let mut chunks = Vec::new();
    let mut current_chunk = String::new();

    // Split by paragraphs (double newline)
    let paragraphs: Vec<&str> = content.split("\n\n").collect();

    for para in paragraphs {
        let para = para.trim();
        if para.is_empty() {
            continue;
        }

        let current_len = current_chunk.chars().count();
        let para_len = para.chars().count();

        // If adding this paragraph exceeds chunk size, start new chunk
        if current_len > 0 && current_len + para_len + 2 > chunk_size {
            chunks.push(current_chunk.clone());
            current_chunk.clear();
        }

        let act_current_len = current_chunk.chars().count();

        // If a single paragraph is too large, split it
        if para_len > chunk_size {
            // Flush current chunk first
            if act_current_len > 0 {
                chunks.push(current_chunk.clone());
                current_chunk.clear();
            }

            // Split by sentences or fixed size
            let sub_chunks = split_large_paragraph(para, chunk_size);
            chunks.extend(sub_chunks);
        } else {
            if act_current_len > 0 {
                current_chunk.push_str("\n\n");
            }
            current_chunk.push_str(para);
        }
    }

    // Add remaining chunk
    if !current_chunk.is_empty() {
        chunks.push(current_chunk);
    }

    chunks
}

/// Splits a large paragraph into smaller chunks, preferring sentence boundaries.
fn split_large_paragraph(para: &str, chunk_size: usize) -> Vec<String> {
    let mut chunks = Vec::new();
    let mut chars: Vec<char> = para.chars().collect();

    while chars.len() > chunk_size {
        let mut cut_point = chunk_size;

        // Search backwards from chunk_size to chunk_size/2 for a sentence boundary
        let min_search = chunk_size / 2;
        for i in (min_search..=chunk_size).rev() {
            if i < chars.len() {
                let r = chars[i];
                if r == '.' || r == '!' || r == '?' || r == '\n' || r == '。' {
                    cut_point = i + 1;
                    break;
                }
            }
        }

        if cut_point > chars.len() {
            cut_point = chars.len();
        }

        let chunk_str: String = chars[..cut_point].iter().collect();
        chunks.push(chunk_str.trim().to_string());

        let remaining: String = chars[cut_point..].iter().collect();
        chars = remaining.trim().chars().collect();
    }

    if !chars.is_empty() {
        let final_str: String = chars.into_iter().collect();
        chunks.push(final_str);
    }

    chunks
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn test_split_short_text() {
        let content = "Paragraph 1\n\nParagraph 2\n\nParagraph 3";
        let chunks = split_into_chunks(content, 500);
        assert_eq!(chunks.len(), 1);
        assert!(chunks[0].contains("Paragraph 1"));
    }

    #[test]
    fn test_split_long_text() {
        let para = "Test paragraph. ".repeat(50);
        let content = vec![para; 10].join("\n\n");
        let chunks = split_into_chunks(&content, 500);

        assert!(chunks.len() >= 2);
        for (i, chunk) in chunks.iter().enumerate() {
            assert!(!chunk.is_empty(), "Chunk {} is empty", i);
        }
    }

    #[test]
    fn test_split_empty_text() {
        let chunks = split_into_chunks("", 500);
        assert_eq!(chunks.len(), 0);
    }

    #[test]
    fn test_whitespace_only() {
        let chunks = split_into_chunks("   \n\n   \n\n   ", 500);
        assert_eq!(chunks.len(), 0);
    }

    #[test]
    fn test_split_large_paragraph() {
        let long_para = "This is a long sentence. ".repeat(100);
        let chunks = split_large_paragraph(&long_para, 500);

        assert!(chunks.len() >= 2);
        for chunk in chunks {
            assert!(!chunk.is_empty());
        }
    }

    #[test]
    fn test_split_japanese() {
        let long_para = "これは日本語のテストです。".repeat(100);
        let chunks = split_large_paragraph(&long_para, 500);

        assert!(chunks.len() >= 2);
        for chunk in chunks {
            assert!(!chunk.is_empty());
        }
    }

    #[test]
    fn test_parse_markdown_short_file() {
        let content = "# Test\n\nThis is a short file.";
        let mut temp_file = tempfile::NamedTempFile::new().unwrap();
        write!(temp_file, "{}", content).unwrap();

        let chunks = parse_markdown(temp_file.path(), 500).unwrap();
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].position, 0);
        assert!(chunks[0].content.contains("Test"));
    }
}
