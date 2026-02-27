use super::languages::LanguageConfig;
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use tree_sitter::{Node, Parser, Query, QueryCursor, StreamingIterator};

#[derive(Debug, Clone, PartialEq)]
pub struct CodeChunk {
    pub content: String,
    pub position: usize,
    pub symbol_name: String,
    pub symbol_type: String,
    pub language: String,
    pub start_line: usize,
    pub end_line: usize,
    pub parent_symbol: Option<String>,
    pub signature: String,
}

impl CodeChunk {
    pub fn get_embedding_text(&self) -> String {
        format!("{} {}: {}", self.language, self.symbol_name, self.content)
    }
}

pub struct CodeParser {
    queries: HashMap<String, Query>,
}

impl CodeParser {
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let mut queries = HashMap::new();
        for config in super::languages::LanguageConfig::get_all() {
            let query = Query::new(&config.language, config.query)?;
            queries.insert(config.name.to_string(), query);
        }
        Ok(Self { queries })
    }

    pub fn parse_file<P: AsRef<Path>>(
        &mut self,
        filepath: P,
    ) -> Result<Vec<CodeChunk>, Box<dyn std::error::Error>> {
        let filepath = filepath.as_ref();
        let content = fs::read(filepath)?;

        let ext = filepath.extension().and_then(|e| e.to_str()).unwrap_or("");

        let config = match LanguageConfig::get_by_extension(ext) {
            Some(c) => c,
            None => {
                // Return empty if not supported, or can return error. Let's return error so caller can skip.
                return Err(format!("unsupported file type: {}", ext).into());
            }
        };

        self.parse_code(&content, config.name)
    }

    pub fn parse_code(
        &mut self,
        source: &[u8],
        lang_name: &str,
    ) -> Result<Vec<CodeChunk>, Box<dyn std::error::Error>> {
        let config = LanguageConfig::get_by_name(lang_name).ok_or("unsupported language")?;

        let mut parser = Parser::new();
        parser.set_language(&config.language)?;

        let tree = parser.parse(source, None).ok_or("failed to parse code")?;

        self.extract_symbols(tree.root_node(), source, lang_name)
    }

    fn extract_symbols(
        &self,
        root: Node,
        source: &[u8],
        lang: &str,
    ) -> Result<Vec<CodeChunk>, Box<dyn std::error::Error>> {
        let query = self.queries.get(lang).ok_or("query not found")?;
        let mut cursor = QueryCursor::new();

        let mut chunks = Vec::new();
        let mut seen = std::collections::HashSet::new();
        let mut position = 0;

        let mut matches = cursor.matches(query, root, source);
        while let Some(m) = matches.next() {
            let mut main_node = None;
            let mut symbol_type = String::new();
            let mut symbol_name = String::new();

            for cap in m.captures {
                let capture_name = query.capture_names()[cap.index as usize].to_string();
                if capture_name == "name" {
                    if let Ok(name) = cap.node.utf8_text(source) {
                        symbol_name = name.to_string();
                    }
                } else if capture_name == "function"
                    || capture_name == "class"
                    || capture_name == "method"
                    || capture_name == "struct"
                    || capture_name == "interface"
                {
                    main_node = Some(cap.node);
                    symbol_type = capture_name;
                }
            }

            if let Some(node) = main_node {
                let start_byte = node.start_byte();
                let end_byte = node.end_byte();
                let key = format!("{}-{}-{}", start_byte, end_byte, symbol_type);
                if seen.insert(key) {
                    let content = node.utf8_text(source)?.to_string();
                    let start_line = node.start_position().row + 1;
                    let end_line = node.end_position().row + 1;

                    let signature = extract_signature(&content, lang);
                    let parent_symbol = find_parent_symbol(node, source, lang);

                    chunks.push(CodeChunk {
                        content,
                        position,
                        symbol_name,
                        symbol_type: symbol_type.clone(),
                        language: lang.to_string(),
                        start_line,
                        end_line,
                        parent_symbol,
                        signature,
                    });
                    position += 1;
                }
            }
        }

        Ok(chunks)
    }
}

fn extract_signature(content: &str, lang: &str) -> String {
    let content = content.trim();
    match lang {
        "go" | "rust" | "php" => {
            if let Some(idx) = content.find('{') {
                let sig = &content[..idx];
                sig.replace('\n', " ")
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" ")
            } else {
                content.lines().next().unwrap_or("").to_string()
            }
        }
        "python" => {
            let first_line = content.lines().next().unwrap_or("").trim();
            if let Some(stripped) = first_line.strip_suffix(':') {
                return stripped.to_string();
            }
            if let Some(idx) = content.find("):") {
                let sig = &content[..idx + 1];
                sig.replace('\n', " ")
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" ")
            } else {
                first_line.to_string()
            }
        }
        "typescript" | "javascript" => {
            if let Some(idx) = content.find("=>") {
                let sig = &content[..idx + 2];
                sig.replace('\n', " ")
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" ")
            } else if let Some(idx) = content.find('{') {
                let sig = &content[..idx];
                sig.replace('\n', " ")
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" ")
            } else {
                content.lines().next().unwrap_or("").to_string()
            }
        }
        _ => {
            if let Some(idx) = content.find('{') {
                let sig = &content[..idx];
                sig.replace('\n', " ")
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" ")
            } else {
                content.lines().next().unwrap_or("").to_string()
            }
        }
    }
}

fn find_parent_symbol(node: Node, source: &[u8], lang: &str) -> Option<String> {
    let mut parent = node.parent();
    while let Some(p) = parent {
        let kind = p.kind();
        let is_class_like = match lang {
            "go" => kind == "type_declaration",
            "python" => kind == "class_definition",
            "typescript" | "javascript" => kind == "class_declaration",
            "php" => {
                kind == "class_declaration"
                    || kind == "trait_declaration"
                    || kind == "interface_declaration"
            }
            "rust" => kind == "impl_item" || kind == "struct_item" || kind == "trait_item",
            _ => false,
        };

        if is_class_like {
            if lang == "rust" && kind == "impl_item" {
                if let Some(type_node) = p.child_by_field_name("type") {
                    if let Ok(name) = type_node.utf8_text(source) {
                        return Some(name.to_string());
                    }
                }
            } else {
                let mut cursor = p.walk();
                for child in p.children(&mut cursor) {
                    let child_kind = child.kind();
                    if child_kind.contains("identifier")
                        || child_kind == "type_identifier"
                        || child_kind == "name"
                    {
                        if let Ok(name) = child.utf8_text(source) {
                            return Some(name.to_string());
                        }
                    }
                }
            }
        }
        parent = p.parent();
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_rust_code() {
        let mut parser = CodeParser::new().expect("Failed to initialize CodeParser");
        let source_code = r#"
            struct MyStruct {
                field: i32,
            }

            impl MyStruct {
                fn my_method(&self) {
                    println!("Hello");
                }
            }

            fn my_function() {}
        "#;

        let chunks = parser
            .parse_code(source_code.as_bytes(), "rust")
            .expect("Failed to parse Rust code");

        assert!(!chunks.is_empty(), "Should extract some symbols");

        let mut found_struct = false;
        let mut found_impl_method = false;
        let mut found_function = false;

        for chunk in &chunks {
            if chunk.symbol_name == "MyStruct" && chunk.symbol_type == "struct" {
                found_struct = true;
            }
            if chunk.symbol_name == "my_method" && chunk.symbol_type == "function" {
                found_impl_method = true;
                assert_eq!(chunk.parent_symbol.as_deref(), Some("MyStruct"));
            }
            if chunk.symbol_name == "my_function" && chunk.symbol_type == "function" {
                found_function = true;
            }
        }

        assert!(found_struct, "Should find MyStruct");
        assert!(found_impl_method, "Should find my_method under MyStruct");
        assert!(found_function, "Should find my_function");
    }

    #[test]
    fn test_parse_python_code() {
        let mut parser = CodeParser::new().expect("Failed to initialize CodeParser");
        let source_code = r#"
class MyClass:
    def my_method(self):
        print("Hello")

def my_function():
    pass
        "#;

        let chunks = parser
            .parse_code(source_code.as_bytes(), "python")
            .expect("Failed to parse Python code");

        assert!(!chunks.is_empty());

        let mut found_class = false;
        let mut found_method = false;
        let mut found_function = false;

        for chunk in &chunks {
            if chunk.symbol_name == "MyClass" && chunk.symbol_type == "class" {
                found_class = true;
            }
            if chunk.symbol_name == "my_method" && chunk.symbol_type == "function" {
                found_method = true;
                assert_eq!(chunk.parent_symbol.as_deref(), Some("MyClass"));
            }
            if chunk.symbol_name == "my_function" && chunk.symbol_type == "function" {
                found_function = true;
            }
        }

        assert!(found_class, "Should find MyClass");
        assert!(found_method, "Should find my_method under MyClass");
        assert!(found_function, "Should find my_function");
    }
}
