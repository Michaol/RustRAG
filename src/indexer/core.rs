use crate::db::Db;
use crate::embedder::Embedder;
use crate::indexer::markdown;
use chrono::{DateTime, Utc};
use ignore::WalkBuilder;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::Mutex as TokioMutex;

#[derive(Debug, Default, PartialEq, Eq)]
pub struct CodeSyncResult {
    pub indexed: usize,
    pub skipped: usize,
    pub failed: usize,
    pub added: usize,
    pub updated: usize,
}

pub struct Indexer<'a, E: Embedder + ?Sized> {
    pub db: Arc<TokioMutex<Db>>,
    pub embedder: &'a E,
    pub chunk_size: usize,
}

impl<'a, E: Embedder + ?Sized> Indexer<'a, E> {
    pub fn new(db: Arc<TokioMutex<Db>>, embedder: &'a E, chunk_size: usize) -> Self {
        Self {
            db,
            embedder,
            chunk_size,
        }
    }

    /// Checks if a file extension is supported
    fn is_supported_extension(ext: &str) -> bool {
        matches!(ext, "md" | "rs" | "go" | "py" | "js" | "ts")
    }

    /// Indexes all supported files in a directory with differential sync
    pub async fn index_directory<P: AsRef<Path>>(
        &mut self,
        dir: P,
        force: bool,
    ) -> Result<CodeSyncResult, Box<dyn std::error::Error>> {
        let dir = dir.as_ref();

        // Get existing documents from DB map(filename -> modified_at)
        let existing_docs = {
            let db_guard = self.db.lock().await;
            db_guard.list_documents()?
        };

        let mut result = CodeSyncResult::default();

        // Walk builder respects .gitignore by default
        let walker = WalkBuilder::new(dir).hidden(false).build();

        for entry in walker.into_iter().filter_map(|e| e.ok()) {
            let path = entry.path();
            if path.is_dir() {
                continue;
            }

            let ext = path
                .extension()
                .and_then(|s| s.to_str())
                .unwrap_or_default();
            if !Self::is_supported_extension(ext) {
                continue;
            }

            // In Windows, path separator is '\', but we should store consistent paths.
            // Using to_string_lossy() provides the OS path, which is fine as a unique key.
            // Replace backslashes with forward slashes for cross-platform consistency.
            let path_str = path.to_string_lossy().replace("\\", "/");

            let metadata = entry.metadata()?;
            let mod_time: DateTime<Utc> = metadata.modified()?.into();

            let mut needs_indexing = true;

            if let Some(existing_time) = existing_docs.get(&path_str) {
                if !force && mod_time.timestamp() == existing_time.timestamp() {
                    result.skipped += 1;
                    needs_indexing = false;
                } else {
                    result.updated += 1;
                }
            } else {
                result.added += 1;
            }

            if needs_indexing {
                let success = if ext == "md" {
                    self.index_markdown(path, &path_str, mod_time).await.is_ok()
                } else {
                    self.index_code_file(path, &path_str, mod_time)
                        .await
                        .is_ok()
                };

                if success {
                    result.indexed += 1;
                } else {
                    result.failed += 1;
                    if result.updated > 0 {
                        result.updated -= 1;
                    } else if result.added > 0 {
                        result.added -= 1;
                    }
                }
            }
        }

        Ok(result)
    }

    async fn index_markdown(
        &mut self,
        real_path: &Path,
        db_path: &str,
        mod_time: DateTime<Utc>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let chunks = markdown::parse_markdown(real_path, self.chunk_size)?;
        if chunks.is_empty() {
            return Ok(());
        }

        let text_refs: Vec<&str> = chunks.iter().map(|c| c.content.as_str()).collect();

        // Vectorize chunks
        let vectors = self.embedder.embed_batch(&text_refs)?;

        // Map to models::Chunk for DB insertion
        let db_chunks: Vec<crate::db::models::Chunk> = chunks
            .iter()
            .map(|c| crate::db::models::Chunk {
                position: c.position,
                content: c.content.as_str(),
            })
            .collect();

        // Write to DB
        {
            let mut db_guard = self.db.lock().await;
            db_guard.insert_document(db_path, mod_time, &db_chunks, &vectors)?;
        }

        Ok(())
    }

    /// Index a code file using Tree-sitter AST parsing.
    ///
    /// Parses the file into symbol-level chunks (functions, classes, methods),
    /// generates embeddings from enriched text (`language symbol_name: content`),
    /// and stores them with full code metadata.
    async fn index_code_file(
        &mut self,
        real_path: &Path,
        db_path: &str,
        mod_time: DateTime<Utc>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        use crate::indexer::code_parser::CodeParser;

        let mut parser = CodeParser::new()?;
        let code_chunks = parser.parse_file(real_path)?;
        if code_chunks.is_empty() {
            return Ok(());
        }

        // Generate embedding text enriched with language + symbol context
        let text_refs: Vec<String> = code_chunks.iter().map(|c| c.get_embedding_text()).collect();
        let text_str_refs: Vec<&str> = text_refs.iter().map(|s| s.as_str()).collect();

        // Vectorize
        let vectors = self.embedder.embed_batch(&text_str_refs)?;

        // Convert indexer::CodeChunk → db::models::CodeChunk
        let db_chunks: Vec<crate::db::models::CodeChunk> = code_chunks
            .iter()
            .enumerate()
            .map(|(i, c)| crate::db::models::CodeChunk {
                chunk: crate::db::models::Chunk {
                    position: i,
                    content: &c.content,
                },
                symbol_name: Some(c.symbol_name.as_str()),
                symbol_type: &c.symbol_type,
                language: &c.language,
                start_line: Some(c.start_line),
                end_line: Some(c.end_line),
                parent_symbol: c.parent_symbol.as_deref(),
                signature: Some(c.signature.as_str()),
            })
            .collect();

        // Write to DB with code metadata
        {
            let mut db_guard = self.db.lock().await;
            db_guard.insert_code_document(db_path, mod_time, &db_chunks, &vectors)?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;
    use crate::embedder::mock::MockEmbedder;
    use std::fs;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_indexer_differential_sync() {
        let temp_dir = tempdir().unwrap();
        let dir_path = temp_dir.path();

        // Create some files
        let file1 = dir_path.join("file1.md");
        fs::write(&file1, "Content 1").unwrap();

        let file2 = dir_path.join("file2.md");
        fs::write(&file2, "Content 2").unwrap();

        let db = Db::open_in_memory().unwrap();
        let db_arc = Arc::new(TokioMutex::new(db));
        let embedder = MockEmbedder::default();
        let mut indexer = Indexer::new(db_arc.clone(), &embedder, 500);

        // First sync
        let res1 = indexer.index_directory(dir_path, false).await.unwrap();
        assert_eq!(res1.added, 2);
        assert_eq!(res1.indexed, 2);
        assert_eq!(res1.skipped, 0);

        // Second sync immediately - should skip both
        let res2 = indexer.index_directory(dir_path, false).await.unwrap();
        assert_eq!(res2.added, 0);
        assert_eq!(res2.updated, 0);
        assert_eq!(res2.indexed, 0);
        assert_eq!(res2.skipped, 2);

        // Third sync with force=true - should update both
        let res3 = indexer.index_directory(dir_path, true).await.unwrap();
        assert_eq!(res3.added, 0);
        assert_eq!(res3.updated, 2);
        assert_eq!(res3.indexed, 2);
        assert_eq!(res3.skipped, 0);

        // Check DB
        let docs = {
            let db_lock = db_arc.lock().await;
            db_lock.list_documents().unwrap()
        };
        assert_eq!(docs.len(), 2);
    }
}
