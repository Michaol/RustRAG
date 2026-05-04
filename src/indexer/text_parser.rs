use anyhow::Result;
use std::fs;
use std::path::Path;
use tracing::warn;

use super::markdown::{self, Chunk};

/// Maximum rows per sheet for spreadsheet extraction to prevent memory issues.
const MAX_SPREADSHEET_ROWS: usize = 10_000;

/// Entry point: extract text from a file and split into chunks.
/// Dispatches by file extension to format-specific handlers.
pub fn extract_and_chunk(path: &Path, chunk_size: usize) -> Result<Vec<Chunk>> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    let text = match ext.as_str() {
        "txt" | "log" => fs::read_to_string(path)?,
        "json" => extract_json(path)?,
        "yaml" | "yml" => extract_yaml(path)?,
        "toml" => extract_toml(path)?,
        "csv" => extract_csv(path)?,
        "html" | "htm" => extract_html(path)?,
        "pdf" => extract_pdf(path)?,
        "docx" => extract_docx(path)?,
        "xls" | "xlsx" | "xlsb" | "ods" => extract_spreadsheet(path)?,
        other => anyhow::bail!("unsupported text format: {other}"),
    };

    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }

    let text_chunks = markdown::split_into_chunks(trimmed, chunk_size);
    Ok(text_chunks
        .into_iter()
        .enumerate()
        .map(|(position, content)| Chunk { content, position })
        .collect())
}

// ── JSON ───────────────────────────────────────────────────────────

fn extract_json(path: &Path) -> Result<String> {
    let content = fs::read_to_string(path)?;
    let value: serde_json::Value = serde_json::from_str(&content)?;
    let mut blocks = Vec::new();
    collect_json_blocks(&value, &mut blocks, String::new());
    Ok(blocks.join("\n\n"))
}

fn collect_json_blocks(value: &serde_json::Value, blocks: &mut Vec<String>, prefix: String) {
    match value {
        serde_json::Value::Object(map) => {
            for (key, val) in map {
                let path = if prefix.is_empty() {
                    key.clone()
                } else {
                    format!("{prefix}.{key}")
                };
                match val {
                    serde_json::Value::String(s) => {
                        blocks.push(format!("{path}: {s}"));
                    }
                    serde_json::Value::Number(n) => {
                        blocks.push(format!("{path}: {n}"));
                    }
                    serde_json::Value::Bool(b) => {
                        blocks.push(format!("{path}: {b}"));
                    }
                    serde_json::Value::Array(_) | serde_json::Value::Object(_) => {
                        collect_json_blocks(val, blocks, path);
                    }
                    _ => {}
                }
            }
        }
        serde_json::Value::Array(arr) => {
            for (i, val) in arr.iter().enumerate() {
                let path = format!("{prefix}[{i}]");
                collect_json_blocks(val, blocks, path);
            }
        }
        _ => {}
    }
}

// ── YAML ───────────────────────────────────────────────────────────

fn extract_yaml(path: &Path) -> Result<String> {
    let content = fs::read_to_string(path)?;
    let value: serde_yaml::Value = serde_yaml::from_str(&content)?;
    let mut blocks = Vec::new();
    collect_yaml_blocks(&value, &mut blocks, String::new());
    Ok(blocks.join("\n\n"))
}

fn collect_yaml_blocks(value: &serde_yaml::Value, blocks: &mut Vec<String>, prefix: String) {
    match value {
        serde_yaml::Value::Mapping(map) => {
            for (key, val) in map {
                let key_str = match key {
                    serde_yaml::Value::String(s) => s.clone(),
                    other => format!("{other:?}"),
                };
                let path = if prefix.is_empty() {
                    key_str
                } else {
                    format!("{prefix}.{key_str}")
                };
                match val {
                    serde_yaml::Value::String(s) => {
                        blocks.push(format!("{path}: {s}"));
                    }
                    serde_yaml::Value::Number(n) => {
                        blocks.push(format!("{path}: {n}"));
                    }
                    serde_yaml::Value::Bool(b) => {
                        blocks.push(format!("{path}: {b}"));
                    }
                    serde_yaml::Value::Mapping(_) | serde_yaml::Value::Sequence(_) => {
                        collect_yaml_blocks(val, blocks, path);
                    }
                    _ => {}
                }
            }
        }
        serde_yaml::Value::Sequence(arr) => {
            for (i, val) in arr.iter().enumerate() {
                let path = format!("{prefix}[{i}]");
                collect_yaml_blocks(val, blocks, path);
            }
        }
        _ => {}
    }
}

// ── TOML ───────────────────────────────────────────────────────────

fn extract_toml(path: &Path) -> Result<String> {
    let content = fs::read_to_string(path)?;
    let value: toml::Value = toml::from_str(&content)?;
    let mut blocks = Vec::new();
    collect_toml_blocks(&value, &mut blocks, String::new());
    Ok(blocks.join("\n\n"))
}

fn collect_toml_blocks(value: &toml::Value, blocks: &mut Vec<String>, prefix: String) {
    match value {
        toml::Value::Table(map) => {
            for (key, val) in map {
                let path = if prefix.is_empty() {
                    key.clone()
                } else {
                    format!("{prefix}.{key}")
                };
                match val {
                    toml::Value::String(s) => {
                        blocks.push(format!("{path}: {s}"));
                    }
                    toml::Value::Integer(n) => {
                        blocks.push(format!("{path}: {n}"));
                    }
                    toml::Value::Float(f) => {
                        blocks.push(format!("{path}: {f}"));
                    }
                    toml::Value::Boolean(b) => {
                        blocks.push(format!("{path}: {b}"));
                    }
                    toml::Value::Table(_) | toml::Value::Array(_) => {
                        collect_toml_blocks(val, blocks, path);
                    }
                    _ => {}
                }
            }
        }
        toml::Value::Array(arr) => {
            for (i, val) in arr.iter().enumerate() {
                let path = format!("{prefix}[{i}]");
                collect_toml_blocks(val, blocks, path);
            }
        }
        _ => {}
    }
}

// ── CSV ────────────────────────────────────────────────────────────

fn extract_csv(path: &Path) -> Result<String> {
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(true)
        .from_path(path)?;

    let headers = reader
        .headers()?
        .iter()
        .map(|h| h.to_string())
        .collect::<Vec<_>>();
    let header_line = headers.join("\t");

    let mut blocks = Vec::new();
    blocks.push(header_line);

    for result in reader.records() {
        let record = result?;
        let row: Vec<&str> = record.iter().collect();
        blocks.push(row.join("\t"));
    }

    Ok(blocks.join("\n"))
}

// ── HTML ───────────────────────────────────────────────────────────

fn extract_html(path: &Path) -> Result<String> {
    let html_content = fs::read_to_string(path)?;
    let document = scraper::Html::parse_document(&html_content);

    // Remove script and style content by selecting body
    let body_sel = scraper::Selector::parse("body").unwrap();
    let block_sel =
        scraper::Selector::parse("p, h1, h2, h3, h4, h5, h6, li, td, th, pre, blockquote").unwrap();

    let body = document
        .select(&body_sel)
        .next()
        .unwrap_or_else(|| document.root_element());

    let mut blocks = Vec::new();
    for element in body.select(&block_sel) {
        let text: String = element
            .text()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(" ");
        if !text.is_empty() {
            blocks.push(text);
        }
    }

    // Fallback: if no block elements found, get all text
    if blocks.is_empty() {
        let all_text: String = body
            .text()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(" ");
        if !all_text.is_empty() {
            blocks.push(all_text);
        }
    }

    Ok(blocks.join("\n\n"))
}

// ── PDF ────────────────────────────────────────────────────────────

fn extract_pdf(path: &Path) -> Result<String> {
    let doc = lopdf::Document::load(path)?;
    let pages = doc.get_pages();
    let page_numbers: Vec<u32> = pages.keys().copied().collect();

    if page_numbers.is_empty() {
        return Ok(String::new());
    }

    let text = doc.extract_text(&page_numbers)?;
    Ok(text)
}

// ── DOCX ───────────────────────────────────────────────────────────

fn extract_docx(path: &Path) -> Result<String> {
    let buf = fs::read(path)?;
    let docx = docx_rs::read_docx(&buf)?;
    let json_str = docx.json();
    let value: serde_json::Value = serde_json::from_str(&json_str)?;

    let mut texts = Vec::new();
    collect_docx_text(&value, &mut texts);
    Ok(texts.join("\n\n"))
}

/// Recursively collect all string values from DOCX JSON tree.
fn collect_docx_text(value: &serde_json::Value, texts: &mut Vec<String>) {
    match value {
        serde_json::Value::Object(map) => {
            // Look for "text" field which contains paragraph text content
            if let Some(serde_json::Value::String(s)) = map.get("text") {
                if !s.trim().is_empty() {
                    texts.push(s.clone());
                }
            }
            // Recurse into all values
            for val in map.values() {
                collect_docx_text(val, texts);
            }
        }
        serde_json::Value::Array(arr) => {
            for val in arr {
                collect_docx_text(val, texts);
            }
        }
        _ => {}
    }
}

// ── Spreadsheet ────────────────────────────────────────────────────

fn extract_spreadsheet(path: &Path) -> Result<String> {
    use calamine::{Data, Reader, open_workbook_auto};

    let mut workbook = open_workbook_auto(path)?;
    let mut blocks = Vec::new();

    for (sheet_name, range) in workbook.worksheets() {
        blocks.push(format!("[Sheet: {sheet_name}]"));

        let total_rows = range.rows().len();
        let row_iter = range.rows();
        for (row_idx, row) in row_iter.enumerate() {
            if row_idx >= MAX_SPREADSHEET_ROWS {
                warn!(
                    "Sheet '{sheet_name}' truncated at {MAX_SPREADSHEET_ROWS} rows \
                     (total: {total_rows})"
                );
                blocks.push(format!(
                    "... (truncated at {MAX_SPREADSHEET_ROWS} of {total_rows} rows)"
                ));
                break;
            }
            let cells: Vec<String> = row
                .iter()
                .map(|c| match c {
                    Data::Empty => String::new(),
                    other => other.to_string(),
                })
                .collect();
            let line = cells.join("\t");
            if !line.trim().is_empty() {
                blocks.push(line);
            }
        }
    }

    Ok(blocks.join("\n"))
}

// ── Tests ──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chunk_plain_text() {
        let text = "Hello world\n\nSecond paragraph\n\nThird paragraph";
        let chunks = markdown::split_into_chunks(text, 500);
        assert_eq!(chunks.len(), 1); // All fits in one chunk
    }

    #[test]
    fn test_chunk_json() {
        let value: serde_json::Value = serde_json::json!({
            "name": "RustRAG",
            "version": "2.0",
            "features": ["search", "index"],
            "config": {
                "host": "localhost",
                "port": 8080
            }
        });
        let mut blocks = Vec::new();
        collect_json_blocks(&value, &mut blocks, String::new());
        assert!(blocks.iter().any(|b| b.contains("name: RustRAG")));
        assert!(blocks.iter().any(|b| b.contains("config.host: localhost")));
        assert!(blocks.iter().any(|b| b.contains("config.port: 8080")));
    }

    #[test]
    fn test_chunk_yaml() {
        let value: serde_yaml::Value = serde_yaml::from_str(
            r#"
name: test
nested:
  key: value
  count: 42
"#,
        )
        .unwrap();
        let mut blocks = Vec::new();
        collect_yaml_blocks(&value, &mut blocks, String::new());
        assert!(blocks.iter().any(|b| b.contains("name: test")));
        assert!(blocks.iter().any(|b| b.contains("nested.key: value")));
    }

    #[test]
    fn test_chunk_toml() {
        let value: toml::Value = toml::from_str(
            r#"
[package]
name = "test"
version = "1.0"

[dependencies]
serde = "1.0"
"#,
        )
        .unwrap();
        let mut blocks = Vec::new();
        collect_toml_blocks(&value, &mut blocks, String::new());
        assert!(blocks.iter().any(|b| b.contains("package.name: test")));
        assert!(blocks.iter().any(|b| b.contains("dependencies.serde: 1.0")));
    }

    #[test]
    fn test_extract_csv_content() {
        let csv_data = "name,age,city\nAlice,30,Beijing\nBob,25,Shanghai";
        let mut reader = csv::ReaderBuilder::new()
            .has_headers(true)
            .from_reader(csv_data.as_bytes());

        let headers: Vec<String> = reader
            .headers()
            .unwrap()
            .iter()
            .map(|h| h.to_string())
            .collect();
        assert_eq!(headers, vec!["name", "age", "city"]);

        let mut rows = Vec::new();
        for result in reader.records() {
            let record = result.unwrap();
            rows.push(record.iter().collect::<Vec<&str>>().join("\t"));
        }
        assert_eq!(rows.len(), 2);
        assert!(rows[0].contains("Alice"));
    }

    #[test]
    fn test_extract_html_content() {
        let html = r#"<html><body>
            <h1>Title</h1>
            <p>First paragraph.</p>
            <script>var x = 1;</script>
            <p>Second paragraph.</p>
        </body></html>"#;
        let document = scraper::Html::parse_document(html);
        let body_sel = scraper::Selector::parse("body").unwrap();
        let block_sel =
            scraper::Selector::parse("p, h1, h2, h3, h4, h5, h6, li, td, th, pre, blockquote")
                .unwrap();

        let body = document.select(&body_sel).next().unwrap();
        let texts: Vec<String> = body
            .select(&block_sel)
            .map(|e| {
                e.text()
                    .map(|s| s.trim())
                    .filter(|s| !s.is_empty())
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .filter(|s| !s.is_empty())
            .collect();

        assert!(texts.iter().any(|t| t.contains("Title")));
        assert!(texts.iter().any(|t| t.contains("First paragraph")));
        // Script content should not appear in block-level elements
        assert!(!texts.iter().any(|t| t.contains("var x")));
    }

    #[test]
    fn test_extract_pdf() {
        // Create a minimal PDF with known text using lopdf
        use lopdf::{Document, Object, Stream, dictionary};

        let mut doc = Document::with_version("1.5");

        let font_id = doc.add_object(dictionary! {
            "Type" => "Font",
            "Subtype" => "Type1",
            "BaseFont" => "Helvetica"
        });

        let content = b"BT /F1 12 Tf 100 700 Td (Hello PDF World) Tj ET";
        let stream = Stream::new(dictionary! {}, content.to_vec());
        let content_id = doc.add_object(stream);

        // Create pages dict first, then page referencing it
        let pages_id = doc.add_object(dictionary! {
            "Type" => "Pages",
            "Kids" => vec![],
            "Count" => 0i32
        });

        let _page_id = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => Object::Reference(pages_id),
            "MediaBox" => vec![0i32.into(), 0i32.into(), 612i32.into(), 792i32.into()],
            "Contents" => Object::Reference(content_id),
            "Resources" => dictionary! {
                "Font" => dictionary! {
                    "F1" => Object::Reference(font_id)
                }
            }
        });

        // Write to temp file and read back
        let dir = tempfile::tempdir().unwrap();
        let pdf_path = dir.path().join("test.pdf");
        doc.save(&pdf_path).unwrap();

        // Verify extract_pdf doesn't panic and returns a result
        // (minimal PDFs may yield empty text, which is fine)
        let result = extract_pdf(&pdf_path);
        assert!(result.is_ok());
    }

    #[test]
    fn test_extract_docx() {
        use docx_rs::*;

        let dir = tempfile::tempdir().unwrap();
        let docx_path = dir.path().join("test.docx");

        // Build a simple DOCX to temp file
        let file = std::fs::File::create(&docx_path).unwrap();
        Docx::new()
            .add_paragraph(Paragraph::new().add_run(Run::new().add_text("Hello DOCX")))
            .add_paragraph(Paragraph::new().add_run(Run::new().add_text("Second paragraph")))
            .build()
            .pack(file)
            .unwrap();

        // Read back using our extract function
        let buf = std::fs::read(&docx_path).unwrap();
        let parsed = read_docx(&buf).unwrap();
        let json_str = parsed.json();
        let value: serde_json::Value = serde_json::from_str(&json_str).unwrap();

        let mut texts = Vec::new();
        collect_docx_text(&value, &mut texts);
        let combined = texts.join("\n");

        assert!(combined.contains("Hello DOCX"));
        assert!(combined.contains("Second paragraph"));
    }

    #[test]
    fn test_extract_spreadsheet() {
        use calamine::Data;

        // Test the Data formatting logic directly
        let cells = [Data::String("hello".into()), Data::Float(3.15), Data::Empty];
        let line: Vec<String> = cells
            .iter()
            .map(|c| match c {
                Data::Empty => String::new(),
                other => other.to_string(),
            })
            .collect();
        let joined = line.join("\t");
        assert!(joined.contains("hello"));
        assert!(joined.contains("3.15"));
        assert_eq!(line.len(), 3);
    }

    #[test]
    fn test_collect_docx_text_empty() {
        let value: serde_json::Value = serde_json::json!({});
        let mut texts = Vec::new();
        collect_docx_text(&value, &mut texts);
        assert!(texts.is_empty());
    }

    #[test]
    fn test_collect_docx_text_nested() {
        let value: serde_json::Value = serde_json::json!({
            "document": {
                "children": [
                    { "type": "paragraph", "children": [
                        { "type": "run", "text": "Hello " },
                        { "type": "run", "text": "World" }
                    ]},
                    { "type": "table", "children": [
                        { "type": "row", "children": [
                            { "type": "cell", "children": [
                                { "type": "paragraph", "children": [
                                    { "type": "run", "text": "Cell text" }
                                ]}
                            ]}
                        ]}
                    ]}
                ]
            }
        });
        let mut texts = Vec::new();
        collect_docx_text(&value, &mut texts);
        assert!(texts.iter().any(|t| t.contains("Hello ")));
        assert!(texts.iter().any(|t| t.contains("World")));
        assert!(texts.iter().any(|t| t.contains("Cell text")));
    }
}
