use super::languages::LanguageConfig;
use std::collections::{HashMap, HashSet};
use tree_sitter::{Node, Parser, Query, QueryCursor, StreamingIterator};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum RelationType {
    Calls,
    Imports,
    Inherits,
}

impl RelationType {
    pub fn as_str(&self) -> &'static str {
        match self {
            RelationType::Calls => "calls",
            RelationType::Imports => "imports",
            RelationType::Inherits => "inherits",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct CodeRelation {
    pub source_symbol: String,
    pub target_name: String,
    pub relation_type: RelationType,
    pub source_file: String,
    pub target_file: Option<String>,
    pub source_line: usize,
}

pub struct RelationExtractor {
    call_queries: HashMap<String, Query>,
    import_queries: HashMap<String, Query>,
    inherit_queries: HashMap<String, Query>,
}

impl RelationExtractor {
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let mut call_queries = HashMap::new();
        let mut import_queries = HashMap::new();
        let mut inherit_queries = HashMap::new();

        for config in LanguageConfig::get_all() {
            let name = config.name.to_string();

            if !config.call_query.is_empty() {
                let q = Query::new(&config.language, config.call_query)?;
                call_queries.insert(name.clone(), q);
            }
            if !config.import_query.is_empty() {
                let q = Query::new(&config.language, config.import_query)?;
                import_queries.insert(name.clone(), q);
            }
            if !config.inherit_query.is_empty() {
                let q = Query::new(&config.language, config.inherit_query)?;
                inherit_queries.insert(name.clone(), q);
            }
        }

        Ok(Self {
            call_queries,
            import_queries,
            inherit_queries,
        })
    }

    pub fn extract_relations(
        &self,
        content: &[u8],
        lang: &str,
        source_file: &str,
        source_symbol: &str,
    ) -> Result<Vec<CodeRelation>, Box<dyn std::error::Error>> {
        let config = match LanguageConfig::get_by_name(lang) {
            Some(c) => c,
            None => return Ok(Vec::new()),
        };

        let mut parser = Parser::new();
        parser.set_language(&config.language)?;
        let tree = match parser.parse(content, None) {
            Some(t) => t,
            None => return Ok(Vec::new()),
        };

        let root = tree.root_node();
        let mut relations = Vec::new();

        if let Some(query) = self.call_queries.get(lang) {
            relations.extend(self.extract_with_query(
                root,
                content,
                query,
                RelationType::Calls,
                source_file,
                source_symbol,
            ));
        }
        if let Some(query) = self.import_queries.get(lang) {
            relations.extend(self.extract_with_query(
                root,
                content,
                query,
                RelationType::Imports,
                source_file,
                source_symbol,
            ));
        }
        if let Some(query) = self.inherit_queries.get(lang) {
            relations.extend(self.extract_with_query(
                root,
                content,
                query,
                RelationType::Inherits,
                source_file,
                source_symbol,
            ));
        }

        Ok(relations)
    }

    fn extract_with_query(
        &self,
        root: Node,
        source: &[u8],
        query: &Query,
        rel_type: RelationType,
        source_file: &str,
        source_symbol: &str,
    ) -> Vec<CodeRelation> {
        let mut cursor = QueryCursor::new();
        let mut relations = Vec::new();
        let mut seen = HashSet::new();

        let mut matches = cursor.matches(query, root, source);
        while let Some(m) = matches.next() {
            for cap in m.captures {
                if let Ok(name) = cap.node.utf8_text(source) {
                    let clean_name = name
                        .trim()
                        .trim_matches(|c| c == '"' || c == '\'')
                        .to_string();
                    if clean_name.is_empty() {
                        continue;
                    }

                    if rel_type == RelationType::Calls {
                        let builtins = [
                            "len", "make", "append", "delete", "print", "println", "panic",
                            "recover", "range", "return", "break", "continue",
                        ];
                        if builtins.contains(&clean_name.as_str()) {
                            continue;
                        }
                    }

                    let line = cap.node.start_position().row + 1; // 1-based

                    let key = format!("{}:{}", clean_name, rel_type.as_str());
                    if seen.insert(key) {
                        relations.push(CodeRelation {
                            source_symbol: source_symbol.to_string(),
                            target_name: clean_name,
                            relation_type: rel_type.clone(),
                            source_file: source_file.to_string(),
                            target_file: None,
                            source_line: line,
                        });
                    }
                }
            }
        }

        relations
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_relations_rust() {
        let extractor = RelationExtractor::new().expect("Failed to initialize RelationExtractor");
        let source_code = r#"
            use std::collections::HashMap;

            struct MyStruct;

            impl MyStruct {
                fn process(&self) {
                    println!("Hello");
                    self.helper();
                    external_function();
                }

                fn helper(&self) {}
            }
        "#;

        let relations = extractor
            .extract_relations(
                source_code.as_bytes(),
                "rust",
                "test.rs",
                "MyStruct::process",
            )
            .expect("Failed to extract relations");

        assert!(!relations.is_empty(), "Should extract relations");

        let mut found_import = false;
        let mut found_call_helper = false;
        let mut found_call_external = false;

        for rel in &relations {
            if rel.relation_type == RelationType::Imports && rel.target_name.contains("HashMap") {
                found_import = true;
            }
            if rel.relation_type == RelationType::Calls && rel.target_name == "helper" {
                found_call_helper = true;
            }
            if rel.relation_type == RelationType::Calls && rel.target_name == "external_function" {
                found_call_external = true;
            }
        }

        assert!(found_import, "Should find HashMap import");
        assert!(found_call_helper, "Should find self.helper() call");
        assert!(found_call_external, "Should find external_function() call");
    }
}
