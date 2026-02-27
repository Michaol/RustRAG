use super::{Db, models::*};
use rusqlite::types::Value;
use rusqlite::{OptionalExtension, Result, Row, params};

fn map_relation_with_source(row: &Row<'_>) -> Result<CodeRelation> {
    Ok(CodeRelation {
        id: row.get(0)?,
        source_chunk_id: row.get(1)?,
        target_chunk_id: row.get(2)?,
        relation_type: row.get(3)?,
        target_name: row.get(4)?,
        target_file: row.get(5)?,
        confidence: row.get(6)?,
        source_name: row.get(7)?,
        source_file: row.get(8)?,
    })
}

fn map_basic_relation(row: &Row<'_>) -> Result<CodeRelation> {
    Ok(CodeRelation {
        id: row.get(0)?,
        source_chunk_id: row.get(1)?,
        target_chunk_id: row.get(2)?,
        relation_type: row.get(3)?,
        target_name: row.get(4)?,
        target_file: row.get(5)?,
        confidence: row.get(6)?,
        source_name: None,
        source_file: None,
    })
}

impl Db {
    /// Retrieves code metadata for a specific chunk
    pub fn get_code_metadata(&self, chunk_id: i64) -> Result<Option<CodeMetadata>> {
        self.conn
            .query_row(
                r#"
            SELECT id, chunk_id, symbol_name, symbol_type, language, start_line, end_line, parent_symbol, signature
            FROM code_metadata WHERE chunk_id = ?
            "#,
                params![chunk_id],
                |row| {
                    Ok(CodeMetadata {
                        id: row.get(0)?,
                        chunk_id: row.get(1)?,
                        symbol_name: row.get(2)?,
                        symbol_type: row.get(3)?,
                        language: row.get(4)?,
                        start_line: row.get::<_, Option<i64>>(5)?.map(|x| x as usize),
                        end_line: row.get::<_, Option<i64>>(6)?.map(|x| x as usize),
                        parent_symbol: row.get(7)?,
                        signature: row.get(8)?,
                    })
                },
            )
            .optional()
    }

    /// Inserts code relations into the database
    pub fn insert_relations(&mut self, relations: &[CodeRelation]) -> Result<()> {
        if relations.is_empty() {
            return Ok(());
        }

        let tx = self.conn.transaction()?;

        for rel in relations {
            tx.execute(
                r#"
                INSERT INTO code_relations (source_chunk_id, target_chunk_id, relation_type, target_name, target_file, confidence)
                VALUES (?, ?, ?, ?, ?, ?)
                "#,
                params![
                    rel.source_chunk_id,
                    rel.target_chunk_id,
                    rel.relation_type,
                    rel.target_name,
                    rel.target_file,
                    rel.confidence,
                ],
            )?;
        }

        tx.commit()
    }

    /// Returns the chunk ID for a symbol in a given file
    pub fn get_chunk_id_by_symbol(&self, filename: &str, symbol_name: &str) -> Result<Option<i64>> {
        self.conn
            .query_row(
                r#"
                SELECT cm.chunk_id
                FROM code_metadata cm
                JOIN chunks c ON cm.chunk_id = c.id
                JOIN documents d ON c.document_id = d.id
                WHERE d.filename = ? AND cm.symbol_name = ?
                LIMIT 1
                "#,
                params![filename, symbol_name],
                |row| row.get(0),
            )
            .optional()
    }

    fn query_basic_relations(
        &self,
        base_query: &str,
        chunk_id: i64,
        rel_type: Option<&str>,
    ) -> Result<Vec<CodeRelation>> {
        let mut query = base_query.to_string();
        let mut params: Vec<Value> = vec![Value::Integer(chunk_id)];

        if let Some(rt) = rel_type {
            query.push_str(" AND relation_type = ?");
            params.push(Value::Text(rt.to_string()));
        }

        let param_refs: Vec<&dyn rusqlite::ToSql> =
            params.iter().map(|p| p as &dyn rusqlite::ToSql).collect();

        let mut stmt = self.conn.prepare(&query)?;
        let rows = stmt.query_map(param_refs.as_slice(), map_basic_relation)?;

        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }
        Ok(results)
    }

    /// Returns relations where the given chunk is the source
    pub fn get_relations_from(
        &self,
        chunk_id: i64,
        rel_type: Option<&str>,
    ) -> Result<Vec<CodeRelation>> {
        self.query_basic_relations(
            "SELECT id, source_chunk_id, target_chunk_id, relation_type, target_name, target_file, confidence FROM code_relations WHERE source_chunk_id = ?",
            chunk_id,
            rel_type,
        )
    }

    /// Returns relations where the given chunk is the target
    pub fn get_relations_to(
        &self,
        chunk_id: i64,
        rel_type: Option<&str>,
    ) -> Result<Vec<CodeRelation>> {
        self.query_basic_relations(
            "SELECT id, source_chunk_id, target_chunk_id, relation_type, target_name, target_file, confidence FROM code_relations WHERE target_chunk_id = ?",
            chunk_id,
            rel_type,
        )
    }

    /// Finds all relations for a symbol by name
    pub fn find_symbol_relations(
        &self,
        symbol_name: &str,
        direction: &str,
        rel_type: Option<&str>,
    ) -> Result<Vec<CodeRelation>> {
        let mut query = String::from(
            r#"
            SELECT cr.id, cr.source_chunk_id, cr.target_chunk_id, cr.relation_type, cr.target_name, cr.target_file, cr.confidence,
                   cm.symbol_name as source_name, d.filename as source_file
            FROM code_relations cr
            JOIN code_metadata cm ON cr.source_chunk_id = cm.chunk_id
            JOIN chunks c ON cm.chunk_id = c.id
            JOIN documents d ON c.document_id = d.id
            "#,
        );

        let mut params: Vec<Value> = Vec::new();

        match direction {
            "incoming" => {
                query.push_str(" WHERE cr.target_name = ?");
                params.push(Value::Text(symbol_name.to_string()));
            }
            "outgoing" => {
                query.push_str(" WHERE cm.symbol_name = ?");
                params.push(Value::Text(symbol_name.to_string()));
            }
            _ => {
                // "both" or default
                query.push_str(" WHERE (cr.target_name = ? OR cm.symbol_name = ?)");
                params.push(Value::Text(symbol_name.to_string()));
                params.push(Value::Text(symbol_name.to_string()));
            }
        }

        if let Some(rt) = rel_type {
            query.push_str(" AND cr.relation_type = ?");
            params.push(Value::Text(rt.to_string()));
        }

        let param_refs: Vec<&dyn rusqlite::ToSql> =
            params.iter().map(|p| p as &dyn rusqlite::ToSql).collect();

        let mut stmt = self.conn.prepare(&query)?;
        let rows = stmt.query_map(param_refs.as_slice(), map_relation_with_source)?;

        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }

        Ok(results)
    }

    /// Looks up target words for a source word (Word Mapping dictionary)
    pub fn lookup_word_mappings(
        &self,
        source_word: &str,
        source_lang: Option<&str>,
    ) -> Result<Vec<String>> {
        let mut query = "SELECT target_word FROM word_mapping WHERE source_word = ?".to_string();
        let mut params: Vec<Value> = vec![Value::Text(source_word.to_string())];

        if let Some(lang) = source_lang {
            query.push_str(" AND source_lang = ?");
            params.push(Value::Text(lang.to_string()));
        }

        query.push_str(" ORDER BY confidence DESC");

        let param_refs: Vec<&dyn rusqlite::ToSql> =
            params.iter().map(|p| p as &dyn rusqlite::ToSql).collect();

        let mut stmt = self.conn.prepare(&query)?;
        let rows = stmt.query_map(param_refs.as_slice(), |row| row.get(0))?;

        let mut targets = Vec::new();
        for row in rows {
            targets.push(row?);
        }

        Ok(targets)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::models::{Chunk, CodeChunk};
    use chrono::Utc;

    #[test]
    fn test_relations_crud() {
        let mut db = Db::open_in_memory().unwrap();

        let code_chunks = vec![CodeChunk {
            chunk: Chunk {
                position: 0,
                content: "fn main() { hello() }",
            },
            symbol_name: Some("main"),
            symbol_type: "function",
            language: "rust",
            start_line: Some(1),
            end_line: Some(2),
            parent_symbol: None,
            signature: Some("fn main()"),
        }];
        let embeddings = vec![vec![0.1f32; 384]];
        db.insert_code_document("main.rs", Utc::now(), &code_chunks, &embeddings)
            .unwrap();

        let chunk_id = db
            .get_chunk_id_by_symbol("main.rs", "main")
            .unwrap()
            .unwrap();

        let meta = db.get_code_metadata(chunk_id).unwrap().unwrap();
        assert_eq!(meta.symbol_name.as_deref(), Some("main"));

        let rel = CodeRelation {
            id: 0,
            source_chunk_id: chunk_id,
            target_chunk_id: None,
            relation_type: "calls".to_string(),
            target_name: "hello".to_string(),
            target_file: None,
            confidence: 1.0,
            source_name: None,
            source_file: None,
        };
        db.insert_relations(&[rel]).unwrap();

        let rels = db.find_symbol_relations("hello", "incoming", None).unwrap();
        assert_eq!(rels.len(), 1);
        assert_eq!(rels[0].source_name.as_deref(), Some("main"));
        assert_eq!(rels[0].target_name, "hello");

        let from_rels = db.get_relations_from(chunk_id, Some("calls")).unwrap();
        assert_eq!(from_rels.len(), 1);
    }
}
