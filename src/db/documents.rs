use super::{Db, models::*, serialize_vector};
use chrono::{DateTime, Utc};
use rusqlite::{OptionalExtension, Result, params};
use std::collections::HashMap;

impl Db {
    /// Returns a map of filename -> modified_at for all indexed documents
    pub fn list_documents(&self) -> Result<HashMap<String, DateTime<Utc>>> {
        let mut stmt = self
            .conn
            .prepare("SELECT filename, modified_at FROM documents")?;
        let rows = stmt.query_map([], |row| {
            let filename: String = row.get(0)?;
            let modified_at: DateTime<Utc> = row.get(1)?;
            Ok((filename, modified_at))
        })?;

        let mut docs = HashMap::new();
        for row in rows {
            let (filename, modified_at) = row?;
            docs.insert(filename, modified_at);
        }

        Ok(docs)
    }

    /// Deletes a document and its associated chunks from the database
    pub fn delete_document(&self, filename: &str) -> Result<bool> {
        let doc_id: Option<i64> = self
            .conn
            .query_row(
                "SELECT id FROM documents WHERE filename = ?",
                params![filename],
                |row| row.get(0),
            )
            .optional()?;

        if let Some(doc_id) = doc_id {
            // Virtual table cascade deletion workaround
            self.conn.execute(
                "DELETE FROM vec_chunks WHERE rowid IN (SELECT id FROM chunks WHERE document_id = ?)",
                params![doc_id],
            )?;

            // Cascade deletes chunks, code_metadata, code_relations
            let rows = self
                .conn
                .execute("DELETE FROM documents WHERE id = ?", params![doc_id])?;
            Ok(rows > 0)
        } else {
            Ok(false)
        }
    }

    /// Inserts or updates a markdown document with its chunks and embeddings
    pub fn insert_document(
        &mut self,
        filename: &str,
        modified_at: DateTime<Utc>,
        chunks: &[Chunk<'_>],
        embeddings: &[Vec<f32>],
    ) -> Result<()> {
        assert_eq!(
            chunks.len(),
            embeddings.len(),
            "chunks and embeddings length mismatch"
        );

        let tx = self.conn.transaction()?;

        // Insert or update document and get the stable ID
        let doc_id: i64 = tx.query_row(
            r#"
            INSERT INTO documents (filename, modified_at, indexed_at)
            VALUES (?, ?, CURRENT_TIMESTAMP)
            ON CONFLICT(filename) DO UPDATE SET
                modified_at = excluded.modified_at,
                indexed_at = CURRENT_TIMESTAMP
            RETURNING id
            "#,
            params![filename, modified_at],
            |row| row.get(0),
        )?;

        // Clean up old contents if any (re-indexing)
        tx.execute(
            "DELETE FROM vec_chunks WHERE rowid IN (SELECT id FROM chunks WHERE document_id = ?)",
            params![doc_id],
        )?;
        tx.execute("DELETE FROM chunks WHERE document_id = ?", params![doc_id])?;

        // Insert chunks and vectors
        for (i, chunk) in chunks.iter().enumerate() {
            tx.execute(
                "INSERT INTO chunks (document_id, position, content) VALUES (?, ?, ?)",
                params![doc_id, chunk.position as i64, chunk.content],
            )?;
            let chunk_id = tx.last_insert_rowid();

            let vector_blob = serialize_vector(&embeddings[i]);
            tx.execute(
                "INSERT INTO vec_chunks (rowid, embedding) VALUES (?, ?)",
                params![chunk_id, vector_blob],
            )?;
        }

        tx.commit()?;
        Ok(())
    }

    /// Inserts or updates a code document with its chunks, vectors, and metadata
    pub fn insert_code_document(
        &mut self,
        filename: &str,
        modified_at: DateTime<Utc>,
        chunks: &[CodeChunk<'_>],
        embeddings: &[Vec<f32>],
    ) -> Result<()> {
        assert_eq!(
            chunks.len(),
            embeddings.len(),
            "chunks and embeddings length mismatch"
        );

        let tx = self.conn.transaction()?;

        let doc_id: i64 = tx.query_row(
            r#"
            INSERT INTO documents (filename, modified_at, indexed_at)
            VALUES (?, ?, CURRENT_TIMESTAMP)
            ON CONFLICT(filename) DO UPDATE SET
                modified_at = excluded.modified_at,
                indexed_at = CURRENT_TIMESTAMP
            RETURNING id
            "#,
            params![filename, modified_at],
            |row| row.get(0),
        )?;

        tx.execute(
            "DELETE FROM vec_chunks WHERE rowid IN (SELECT id FROM chunks WHERE document_id = ?)",
            params![doc_id],
        )?;
        tx.execute("DELETE FROM chunks WHERE document_id = ?", params![doc_id])?;

        for (i, code_chunk) in chunks.iter().enumerate() {
            tx.execute(
                "INSERT INTO chunks (document_id, position, content) VALUES (?, ?, ?)",
                params![
                    doc_id,
                    code_chunk.chunk.position as i64,
                    code_chunk.chunk.content
                ],
            )?;
            let chunk_id = tx.last_insert_rowid();

            let vector_blob = serialize_vector(&embeddings[i]);
            tx.execute(
                "INSERT INTO vec_chunks (rowid, embedding) VALUES (?, ?)",
                params![chunk_id, vector_blob],
            )?;

            tx.execute(
                "INSERT INTO code_metadata (chunk_id, symbol_name, symbol_type, language, start_line, end_line, parent_symbol, signature) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
                params![
                    chunk_id,
                    code_chunk.symbol_name,
                    code_chunk.symbol_type,
                    code_chunk.language,
                    code_chunk.start_line.map(|x| x as i64),
                    code_chunk.end_line.map(|x| x as i64),
                    code_chunk.parent_symbol,
                    code_chunk.signature,
                ],
            )?;
        }

        tx.commit()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_documents_crud() {
        let mut db = Db::open_in_memory().unwrap();
        let now = Utc::now();
        let filename = "test.md";

        // 1. Insert empty
        let chunks = vec![
            Chunk {
                position: 0,
                content: "Hello",
            },
            Chunk {
                position: 1,
                content: "World",
            },
        ];
        let embeddings = vec![vec![0.1; 384], vec![0.2; 384]];

        db.insert_document(filename, now, &chunks, &embeddings)
            .unwrap();

        // 2. List documents
        let docs = db.list_documents().unwrap();
        assert_eq!(docs.len(), 1);
        assert!(docs.contains_key(filename));

        // 3. Count rows
        let chunks_count: i64 = db
            .conn
            .query_row("SELECT COUNT(*) FROM chunks", [], |row| row.get(0))
            .unwrap();
        assert_eq!(chunks_count, 2);

        let vec_chunks_count: i64 = db
            .conn
            .query_row("SELECT COUNT(*) FROM vec_chunks", [], |row| row.get(0))
            .unwrap();
        assert_eq!(vec_chunks_count, 2);

        // 4. Update existing document (re-index)
        let new_chunks = vec![Chunk {
            position: 0,
            content: "Replaced",
        }];
        let new_embeddings = vec![vec![0.5; 384]];
        db.insert_document(filename, Utc::now(), &new_chunks, &new_embeddings)
            .unwrap();

        // Count rows again - old chunks should be deleted
        let chunks_count: i64 = db
            .conn
            .query_row("SELECT COUNT(*) FROM chunks", [], |row| row.get(0))
            .unwrap();
        assert_eq!(chunks_count, 1);

        let vec_chunks_count: i64 = db
            .conn
            .query_row("SELECT COUNT(*) FROM vec_chunks", [], |row| row.get(0))
            .unwrap();
        assert_eq!(vec_chunks_count, 1);

        // 5. Delete document
        let deleted = db.delete_document(filename).unwrap();
        assert!(deleted);

        // Verify cascading deletes
        let chunks_count: i64 = db
            .conn
            .query_row("SELECT COUNT(*) FROM chunks", [], |row| row.get(0))
            .unwrap();
        assert_eq!(chunks_count, 0);

        let vec_chunks_count: i64 = db
            .conn
            .query_row("SELECT COUNT(*) FROM vec_chunks", [], |row| row.get(0))
            .unwrap();
        assert_eq!(vec_chunks_count, 0);
    }
}
