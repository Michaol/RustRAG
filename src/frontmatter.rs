/// YAML frontmatter parsing and generation for Markdown files.
///
/// Mirrors Go version's `internal/frontmatter/frontmatter.go`.
use std::fs;
use std::path::Path;

use anyhow::{Context, Result, bail};

/// Metadata stored in YAML frontmatter.
#[derive(Debug, Clone, Default)]
pub struct Metadata {
    pub domain: String,
    pub doc_type: String,
    pub language: String,
    pub tags: Vec<String>,
    pub project: String,
}

/// Parse frontmatter from markdown content. Returns `(Option<Metadata>, body)`.
pub fn parse(content: &str) -> Result<(Option<Metadata>, String)> {
    let lines: Vec<&str> = content.lines().collect();

    if lines.len() < 3 || lines[0].trim() != "---" {
        return Ok((None, content.to_string()));
    }

    // Find closing delimiter
    let end_idx = lines[1..]
        .iter()
        .position(|l| l.trim() == "---")
        .map(|i| i + 1);

    let end_idx = match end_idx {
        Some(idx) => idx,
        None => bail!("frontmatter not closed"),
    };

    let frontmatter_lines = &lines[1..end_idx];
    let body_lines = &lines[end_idx + 1..];

    let mut metadata = Metadata::default();

    for line in frontmatter_lines {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        if let Some((key, value)) = line.split_once(':') {
            let key = key.trim();
            let value = value.trim();

            match key {
                "domain" => metadata.domain = value.to_string(),
                "docType" => metadata.doc_type = value.to_string(),
                "language" => metadata.language = value.to_string(),
                "project" => metadata.project = value.to_string(),
                "tags" => {
                    let value = value.trim_matches(|c| c == '[' || c == ']');
                    metadata.tags = value
                        .split(',')
                        .map(|t| t.trim().to_string())
                        .filter(|t| !t.is_empty())
                        .collect();
                }
                _ => {}
            }
        }
    }

    let body = body_lines.join("\n");
    Ok((Some(metadata), body))
}

/// Generate YAML frontmatter string from metadata.
pub fn generate(metadata: &Metadata) -> String {
    let mut builder = String::from("---\n");

    if !metadata.domain.is_empty() {
        builder.push_str(&format!("domain: {}\n", metadata.domain));
    }
    if !metadata.doc_type.is_empty() {
        builder.push_str(&format!("docType: {}\n", metadata.doc_type));
    }
    if !metadata.language.is_empty() {
        builder.push_str(&format!("language: {}\n", metadata.language));
    }
    if !metadata.tags.is_empty() {
        builder.push_str(&format!("tags: [{}]\n", metadata.tags.join(", ")));
    }
    if !metadata.project.is_empty() {
        builder.push_str(&format!("project: {}\n", metadata.project));
    }

    builder.push_str("---\n");
    builder
}

/// Add frontmatter to a file (errors if frontmatter already exists).
pub fn add_frontmatter(file_path: &Path, metadata: &Metadata) -> Result<()> {
    let content = fs::read_to_string(file_path)
        .with_context(|| format!("failed to read {}", file_path.display()))?;

    let (existing, _) = parse(&content)?;
    if existing.is_some() {
        bail!("frontmatter already exists");
    }

    let fm = generate(metadata);
    let new_content = format!("{}\n{}", fm, content);

    fs::write(file_path, new_content)
        .with_context(|| format!("failed to write {}", file_path.display()))?;
    Ok(())
}

/// Update existing frontmatter (merges non-empty fields; adds if none exists).
pub fn update_frontmatter(file_path: &Path, metadata: &Metadata) -> Result<()> {
    let content = fs::read_to_string(file_path)
        .with_context(|| format!("failed to read {}", file_path.display()))?;

    let (existing, body) = parse(&content)?;

    let merged = match existing {
        Some(mut existing) => {
            if !metadata.domain.is_empty() {
                existing.domain = metadata.domain.clone();
            }
            if !metadata.doc_type.is_empty() {
                existing.doc_type = metadata.doc_type.clone();
            }
            if !metadata.language.is_empty() {
                existing.language = metadata.language.clone();
            }
            if !metadata.tags.is_empty() {
                existing.tags = metadata.tags.clone();
            }
            if !metadata.project.is_empty() {
                existing.project = metadata.project.clone();
            }
            existing
        }
        None => {
            // No existing frontmatter; add the entire metadata.
            return add_frontmatter(file_path, metadata);
        }
    };

    let fm = generate(&merged);
    let new_content = format!("{}\n{}", fm, body.trim_start_matches('\n'));

    fs::write(file_path, new_content)
        .with_context(|| format!("failed to write {}", file_path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn test_parse_no_frontmatter() {
        let (meta, body) = parse("# Hello\n\nWorld").unwrap();
        assert!(meta.is_none());
        assert!(body.contains("Hello"));
    }

    #[test]
    fn test_parse_with_frontmatter() {
        let content = "---\ndomain: backend\ndocType: api\ntags: [auth, db]\n---\n# Doc\n";
        let (meta, body) = parse(content).unwrap();
        let meta = meta.unwrap();
        assert_eq!(meta.domain, "backend");
        assert_eq!(meta.doc_type, "api");
        assert_eq!(meta.tags, vec!["auth", "db"]);
        assert!(body.contains("# Doc"));
    }

    #[test]
    fn test_generate() {
        let meta = Metadata {
            domain: "frontend".into(),
            doc_type: "spec".into(),
            language: "typescript".into(),
            tags: vec!["ui".into(), "react".into()],
            project: "myapp".into(),
        };
        let fm = generate(&meta);
        assert!(fm.starts_with("---\n"));
        assert!(fm.ends_with("---\n"));
        assert!(fm.contains("domain: frontend"));
        assert!(fm.contains("tags: [ui, react]"));
    }

    #[test]
    fn test_add_frontmatter() {
        let mut temp = tempfile::NamedTempFile::new().unwrap();
        write!(temp, "# Hello\n\nContent here.").unwrap();

        let meta = Metadata {
            domain: "backend".into(),
            ..Default::default()
        };
        add_frontmatter(temp.path(), &meta).unwrap();

        let result = fs::read_to_string(temp.path()).unwrap();
        assert!(result.starts_with("---\n"));
        assert!(result.contains("domain: backend"));
        assert!(result.contains("# Hello"));
    }

    #[test]
    fn test_update_frontmatter() {
        let mut temp = tempfile::NamedTempFile::new().unwrap();
        write!(temp, "---\ndomain: old\n---\n# Doc\n").unwrap();

        let meta = Metadata {
            domain: "new".into(),
            language: "rust".into(),
            ..Default::default()
        };
        update_frontmatter(temp.path(), &meta).unwrap();

        let result = fs::read_to_string(temp.path()).unwrap();
        assert!(result.contains("domain: new"));
        assert!(result.contains("language: rust"));
    }
}
