use super::{Db, serialize_vector};
use rusqlite::Result;
use rusqlite::types::Value;

#[derive(Debug, Default)]
pub struct SearchFilter<'a> {
    pub directory: Option<&'a str>,
    pub file_pattern: Option<&'a str>,
}

#[derive(Debug)]
pub struct SearchResult {
    pub document_name: String,
    pub chunk_content: String,
    pub similarity: f64,
    pub position: usize,
    pub chunk_id: i64,
    pub metadata: Option<CodeMetadataResult>,
}

#[derive(Debug)]
pub struct CodeMetadataResult {
    pub symbol_name: Option<String>,
    pub symbol_type: String,
    pub language: String,
    pub start_line: Option<usize>,
    pub end_line: Option<usize>,
    pub parent_symbol: Option<String>,
    pub signature: Option<String>,
}

fn glob_to_like(pattern: &str) -> String {
    let mut result = pattern.replace("%", "\\%");
    result = result.replace("_", "\\_");
    result = result.replace("*", "%");
    result = result.replace("?", "_");
    result
}

fn map_search_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<SearchResult> {
    let distance: f64 = row.get(4)?;
    let similarity = 1.0 - (distance / 2.0);

    let symbol_type: Option<String> = row.get(6)?;

    let metadata = if let Some(st) = symbol_type {
        Some(CodeMetadataResult {
            symbol_name: row.get(5)?,
            symbol_type: st,
            language: row.get(7)?,
            start_line: row.get::<_, Option<i64>>(8)?.map(|v| v as usize),
            end_line: row.get::<_, Option<i64>>(9)?.map(|v| v as usize),
            parent_symbol: row.get(10)?,
            signature: row.get(11)?,
        })
    } else {
        None
    };

    Ok(SearchResult {
        document_name: row.get(0)?,
        chunk_content: row.get(1)?,
        position: row.get::<_, i64>(2)? as usize,
        chunk_id: row.get(3)?,
        similarity,
        metadata,
    })
}

impl Db {
    /// Perform vector similarity search using cosine distance
    pub fn search(&self, query_vector: &[f32], top_k: usize) -> Result<Vec<SearchResult>> {
        self.search_with_filter(query_vector, top_k, None)
    }

    /// Perform vector similarity search with optional filtering
    pub fn search_with_filter(
        &self,
        query_vector: &[f32],
        top_k: usize,
        filter: Option<&SearchFilter<'_>>,
    ) -> Result<Vec<SearchResult>> {
        let mut query = String::from(
            r#"
            SELECT
                d.filename,
                c.content,
                c.position,
                c.id as chunk_id,
                vec_distance_cosine(v.embedding, ?) as distance,
                cm.symbol_name,
                cm.symbol_type,
                cm.language,
                cm.start_line,
                cm.end_line,
                cm.parent_symbol,
                cm.signature
            FROM vec_chunks v
            JOIN chunks c ON v.rowid = c.id
            JOIN documents d ON c.document_id = d.id
            LEFT JOIN code_metadata cm ON c.id = cm.chunk_id
            "#,
        );

        let mut where_clauses = Vec::new();
        let mut params: Vec<Value> = vec![Value::Blob(serialize_vector(query_vector))];

        if let Some(f) = filter {
            if let Some(dir) = f.directory {
                let d = dir
                    .trim_end_matches('/')
                    .trim_end_matches(std::path::MAIN_SEPARATOR);
                where_clauses.push("(d.filename LIKE ? OR d.filename LIKE ?)".to_string());
                params.push(Value::Text(format!("{}/%", d)));
                params.push(Value::Text(format!("{}\\%", d)));
            }
            if let Some(pat) = f.file_pattern {
                let like_pat = glob_to_like(pat);
                where_clauses.push("(d.filename LIKE ? OR d.filename LIKE ?)".to_string());
                params.push(Value::Text(format!("%/{}", like_pat)));
                params.push(Value::Text(like_pat));
            }
        }

        if !where_clauses.is_empty() {
            query.push_str(" WHERE ");
            query.push_str(&where_clauses.join(" AND "));
        }

        query.push_str(" ORDER BY distance ASC LIMIT ?");
        params.push(Value::Integer(top_k as i64));

        let param_refs: Vec<&dyn rusqlite::ToSql> =
            params.iter().map(|p| p as &dyn rusqlite::ToSql).collect();

        let mut stmt = self.conn.prepare(&query)?;
        let rows = stmt.query_map(param_refs.as_slice(), map_search_row)?;

        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }

        Ok(results)
    }

    /// Search code_metadata for symbols matching keywords
    pub fn search_symbols_by_keywords(
        &self,
        keywords: &[&str],
        limit: usize,
    ) -> Result<Vec<SearchResult>> {
        if keywords.is_empty() {
            return Ok(Vec::new());
        }

        let mut query = String::from(
            r#"
            SELECT
                d.filename,
                c.content,
                c.position,
                c.id as chunk_id,
                0.0 as distance,
                cm.symbol_name,
                cm.symbol_type,
                cm.language,
                cm.start_line,
                cm.end_line,
                cm.parent_symbol,
                cm.signature
            FROM code_metadata cm
            JOIN chunks c ON cm.chunk_id = c.id
            JOIN documents d ON c.document_id = d.id
            WHERE 
            "#,
        );

        let mut conditions = Vec::new();
        let mut params: Vec<Value> = Vec::new();

        for kw in keywords {
            conditions.push("LOWER(cm.symbol_name) LIKE ?".to_string());
            params.push(Value::Text(format!("%{}%", kw.to_lowercase())));
        }

        query.push_str(&format!("({}) LIMIT ?", conditions.join(" OR ")));
        params.push(Value::Integer(limit as i64));

        let param_refs: Vec<&dyn rusqlite::ToSql> =
            params.iter().map(|p| p as &dyn rusqlite::ToSql).collect();

        let mut stmt = self.conn.prepare(&query)?;
        let rows = stmt.query_map(param_refs.as_slice(), map_search_row)?;

        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }

        Ok(results)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::models::{Chunk, CodeChunk};
    use chrono::Utc;

    #[test]
    fn test_search() {
        let mut db = Db::open_in_memory().unwrap();

        // Let's insert some mock documents
        let chunks = vec![Chunk {
            position: 0,
            content: "Rust programming language",
        }];
        let padded_embedding = {
            let mut v = vec![0.0f32; 384];
            v[0] = 0.1;
            v[1] = 0.2;
            v[2] = 0.3;
            v
        };
        db.insert_document("rust.md", Utc::now(), &chunks, &[padded_embedding.clone()])
            .unwrap();

        let code_chunks = vec![CodeChunk {
            chunk: Chunk {
                position: 0,
                content: "fn hello() {}",
            },
            symbol_name: Some("hello"),
            symbol_type: "function",
            language: "rust",
            start_line: Some(1),
            end_line: Some(1),
            parent_symbol: None,
            signature: Some("fn hello()"),
        }];
        let code_padded_embedding = {
            let mut v = vec![0.0f32; 384];
            v[0] = 0.9;
            v[1] = 0.8;
            v[2] = 0.7;
            v
        };
        db.insert_code_document(
            "src/main.rs",
            Utc::now(),
            &code_chunks,
            &[code_padded_embedding.clone()],
        )
        .unwrap();

        // Search near rust.md
        let results = db.search(&padded_embedding, 5).unwrap();
        assert_eq!(results.len(), 2);

        // Nearest should be rust.md
        assert_eq!(results[0].document_name, "rust.md");
        assert!(results[0].similarity > 0.99); // completely similar
        assert!(results[0].metadata.is_none());

        // Second nearest is src/main.rs
        assert_eq!(results[1].document_name, "src/main.rs");
        assert!(results[1].metadata.is_some());
        let meta = results[1].metadata.as_ref().unwrap();
        assert_eq!(meta.symbol_type, "function");
        assert_eq!(meta.language, "rust");
    }

    #[test]
    fn test_search_with_filter() {
        let mut db = Db::open_in_memory().unwrap();

        let padded_embedding = vec![0.1f32; 384];

        let chunks = vec![Chunk {
            position: 0,
            content: "Doc A",
        }];
        db.insert_document(
            "docs/a.md",
            Utc::now(),
            &chunks,
            &[padded_embedding.clone()],
        )
        .unwrap();

        let chunks_b = vec![Chunk {
            position: 0,
            content: "Doc B",
        }];
        db.insert_document(
            "src/b.rs",
            Utc::now(),
            &chunks_b,
            &[padded_embedding.clone()],
        )
        .unwrap();

        let chunks_c = vec![Chunk {
            position: 0,
            content: "Doc C",
        }];
        db.insert_document(
            "docs/nested/c.md",
            Utc::now(),
            &chunks_c,
            &[padded_embedding.clone()],
        )
        .unwrap();

        // 1. Filter by directory "docs"
        let filter_dir = SearchFilter {
            directory: Some("docs"),
            file_pattern: None,
        };
        let res1 = db
            .search_with_filter(&padded_embedding, 10, Some(&filter_dir))
            .unwrap();
        assert_eq!(res1.len(), 2); // docs/a.md, docs/nested/c.md

        // 2. Filter by file_pattern "*.md"
        let filter_pat = SearchFilter {
            directory: None,
            file_pattern: Some("*.md"),
        };
        let res2 = db
            .search_with_filter(&padded_embedding, 10, Some(&filter_pat))
            .unwrap();
        assert_eq!(res2.len(), 2); // a.md, c.md

        // 3. Filter by file_pattern "*.rs"
        let filter_rs = SearchFilter {
            directory: None,
            file_pattern: Some("*.rs"),
        };
        let res3 = db
            .search_with_filter(&padded_embedding, 10, Some(&filter_rs))
            .unwrap();
        assert_eq!(res3.len(), 1); // b.rs
    }
}
