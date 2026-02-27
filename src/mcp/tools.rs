/// MCP Tool handlers for RustRAG.
///
/// Implements 10 tools mirroring Go version's `internal/mcp/tools.go`:
/// 1. search           – vector similarity search
/// 2. index_markdown   – index a single markdown file
/// 3. list_documents   – list indexed documents
/// 4. delete_document  – delete a document
/// 5. reindex_document – delete + re-index
/// 6. add_frontmatter  – add YAML frontmatter
/// 7. update_frontmatter – update YAML frontmatter
/// 8. index_code       – index source code (file/dir/batch)
/// 9. search_relations – search code symbol relations
/// 10. build_dictionary – build multilingual word dictionary
use crate::db::search::SearchFilter;
use crate::frontmatter;
use crate::indexer::{
    code_parser::CodeParser,
    dictionary::{self, DictionaryExtractor},
};
use crate::mcp::server::McpContext;
use rmcp::handler::server::ServerHandler;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::{ErrorData as McpError, handler::server::tool::ToolRouter, model::*, tool, tool_router};
use schemars::JsonSchema;
use serde::Deserialize;
use std::path::Path;

// ── Parameter structs ────────────────────────────────────────────────

#[derive(Deserialize, JsonSchema)]
struct SearchParams {
    /// Search query (natural language)
    query: String,
    /// Max results (default: 5)
    top_k: Option<usize>,
    /// Limit search to a directory (e.g. 'docs/api')
    directory: Option<String>,
    /// Filter by filename glob pattern (e.g. 'api-*.md')
    file_pattern: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
struct FilepathParam {
    /// Path to the markdown file
    filepath: String,
}

#[derive(Deserialize, JsonSchema)]
struct FilenameParam {
    /// Filename to operate on
    filename: String,
}

#[derive(Deserialize, JsonSchema)]
struct FrontmatterParams {
    /// Path to the markdown file
    filepath: String,
    /// Domain: frontend | backend | mobile | infrastructure | other
    domain: Option<String>,
    /// Document type: spec | design | api | guide | note | other
    #[serde(rename = "docType")]
    doc_type: Option<String>,
    /// Language: go | typescript | python | rust | java | kotlin | swift | other
    language: Option<String>,
    /// Tags (comma-separated): authentication, database, caching
    tags: Option<String>,
    /// Project name (optional)
    project: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
struct IndexCodeParams {
    /// Single file to index
    filepath: Option<String>,
    /// Directory to index recursively
    directory: Option<String>,
    /// Multiple files to index (comma-separated)
    filepaths: Option<String>,
    /// Force re-index even if unchanged (default: false)
    force: Option<bool>,
}

#[derive(Deserialize, JsonSchema)]
struct SearchRelationsParams {
    /// Symbol name to search (function name, class name, etc.)
    symbol: String,
    /// Relation type filter: calls | imports | inherits (all if omitted)
    relation_type: Option<String>,
    /// Direction: outgoing | incoming | both (default: both)
    direction: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
struct BuildDictionaryParams {
    /// Source language (default: ja)
    source_lang: Option<String>,
    /// Specific document path (all documents if omitted)
    document: Option<String>,
}

// ── Response helpers ─────────────────────────────────────────────────

fn json_result(value: serde_json::Value) -> Result<CallToolResult, McpError> {
    Ok(CallToolResult::success(vec![Content::text(
        serde_json::to_string_pretty(&value).unwrap_or_default(),
    )]))
}

fn error_result(msg: &str) -> Result<CallToolResult, McpError> {
    Ok(CallToolResult::error(vec![Content::text(msg.to_string())]))
}

// ── Tool implementations ─────────────────────────────────────────────

#[derive(Clone)]
pub struct AppTools {
    pub ctx: McpContext,
    pub tool_router: ToolRouter<Self>,
}

impl ServerHandler for AppTools {}

#[tool_router]
impl AppTools {
    pub fn new(ctx: McpContext) -> Self {
        Self {
            ctx,
            tool_router: Self::tool_router(),
        }
    }

    // ── Tool 1: search ──────────────────────────────────────────────

    #[tool(
        description = "Natural language vector search over indexed documents. Supports directory and filename pattern filters. If the response contains update_available, inform the user about the new version."
    )]
    async fn search(&self, params: Parameters<SearchParams>) -> Result<CallToolResult, McpError> {
        let p = params.0;
        if p.query.is_empty() {
            return error_result("query is required");
        }

        let top_k = p.top_k.unwrap_or(self.ctx.config.search_top_k);

        // Build filter
        let filter = SearchFilter {
            directory: p.directory.as_deref(),
            file_pattern: p.file_pattern.as_deref(),
        };
        let has_filter = filter.directory.is_some() || filter.file_pattern.is_some();

        // Vectorize query
        let query_vector = self
            .ctx
            .embedder
            .embed(&p.query)
            .map_err(|e| McpError::internal_error(format!("embedding failed: {e}"), None))?;

        // Search DB
        let db = self.ctx.db.lock().await;
        let filter_ref = if has_filter { Some(&filter) } else { None };
        let results = db
            .search_with_filter(&query_vector, top_k, filter_ref)
            .map_err(|e| McpError::internal_error(format!("search failed: {e}"), None))?;

        let results_json: Vec<serde_json::Value> = results
            .iter()
            .map(|r| {
                let mut obj = serde_json::json!({
                    "document": r.document_name,
                    "content": r.chunk_content,
                    "similarity": r.similarity,
                    "position": r.position,
                });
                if let Some(meta) = &r.metadata {
                    obj["symbol_name"] = serde_json::json!(meta.symbol_name);
                    obj["symbol_type"] = serde_json::json!(meta.symbol_type);
                    obj["language"] = serde_json::json!(meta.language);
                }
                obj
            })
            .collect();

        json_result(serde_json::json!({ "results": results_json }))
    }

    // ── Tool 2: index_markdown ──────────────────────────────────────

    #[tool(description = "Index a specified markdown file")]
    async fn index_markdown(
        &self,
        params: Parameters<FilepathParam>,
    ) -> Result<CallToolResult, McpError> {
        let filepath = &params.0.filepath;
        if filepath.is_empty() {
            return error_result("filepath is required");
        }

        let path = Path::new(filepath);
        if !path.exists() {
            return error_result(&format!("file not found: {filepath}"));
        }

        let chunks = crate::indexer::markdown::parse_markdown(path, self.ctx.chunk_size)
            .map_err(|e| McpError::internal_error(format!("parse failed: {e}"), None))?;

        if chunks.is_empty() {
            return json_result(serde_json::json!({
                "success": true,
                "message": "File is empty, nothing to index",
            }));
        }

        let text_refs: Vec<&str> = chunks.iter().map(|c| c.content.as_str()).collect();
        let vectors = self
            .ctx
            .embedder
            .embed_batch(&text_refs)
            .map_err(|e| McpError::internal_error(format!("embedding failed: {e}"), None))?;

        let db_path = filepath.replace('\\', "/");
        let db_chunks: Vec<crate::db::models::Chunk> = chunks
            .iter()
            .map(|c| crate::db::models::Chunk {
                position: c.position,
                content: c.content.as_str(),
            })
            .collect();

        let mut db = self.ctx.db.lock().await;
        db.insert_document(&db_path, chrono::Utc::now(), &db_chunks, &vectors)
            .map_err(|e| McpError::internal_error(format!("DB insert failed: {e}"), None))?;

        json_result(serde_json::json!({
            "success": true,
            "message": "File indexed successfully",
        }))
    }

    // ── Tool 3: list_documents ──────────────────────────────────────

    #[tool(description = "Retrieve list of indexed documents")]
    async fn list_documents(&self) -> Result<CallToolResult, McpError> {
        let db = self.ctx.db.lock().await;
        let docs = db
            .list_documents()
            .map_err(|e| McpError::internal_error(format!("list failed: {e}"), None))?;

        let documents: Vec<serde_json::Value> = docs
            .iter()
            .map(|(filename, modified_at)| {
                serde_json::json!({
                    "filename": filename,
                    "modified_at": modified_at.to_rfc3339(),
                })
            })
            .collect();

        json_result(serde_json::json!({ "documents": documents }))
    }

    // ── Tool 4: delete_document ─────────────────────────────────────

    #[tool(description = "Delete a document from the DB and optionally from the file system")]
    async fn delete_document(
        &self,
        params: Parameters<FilenameParam>,
    ) -> Result<CallToolResult, McpError> {
        let filename = &params.0.filename;
        if filename.is_empty() {
            return error_result("filename is required");
        }

        let db = self.ctx.db.lock().await;
        db.delete_document(filename)
            .map_err(|e| McpError::internal_error(format!("delete failed: {e}"), None))?;

        // Also try to remove from filesystem (warn on failure)
        if let Err(e) = std::fs::remove_file(filename) {
            log::warn!("Failed to delete file {filename}: {e}");
        }

        json_result(serde_json::json!({
            "success": true,
            "message": "Document deleted successfully",
        }))
    }

    // ── Tool 5: reindex_document ────────────────────────────────────

    #[tool(description = "Delete and re-index a document")]
    async fn reindex_document(
        &self,
        params: Parameters<FilenameParam>,
    ) -> Result<CallToolResult, McpError> {
        let filename = &params.0.filename;
        if filename.is_empty() {
            return error_result("filename is required");
        }

        // Delete from DB
        {
            let db = self.ctx.db.lock().await;
            db.delete_document(filename)
                .map_err(|e| McpError::internal_error(format!("delete failed: {e}"), None))?;
        }

        // Re-index
        let path = Path::new(filename);
        if !path.exists() {
            return error_result(&format!("file not found: {filename}"));
        }

        let chunks = crate::indexer::markdown::parse_markdown(path, self.ctx.chunk_size)
            .map_err(|e| McpError::internal_error(format!("parse failed: {e}"), None))?;

        if !chunks.is_empty() {
            let text_refs: Vec<&str> = chunks.iter().map(|c| c.content.as_str()).collect();
            let vectors =
                self.ctx.embedder.embed_batch(&text_refs).map_err(|e| {
                    McpError::internal_error(format!("embedding failed: {e}"), None)
                })?;

            let db_path = filename.replace('\\', "/");
            let db_chunks: Vec<crate::db::models::Chunk> = chunks
                .iter()
                .map(|c| crate::db::models::Chunk {
                    position: c.position,
                    content: c.content.as_str(),
                })
                .collect();

            let mut db = self.ctx.db.lock().await;
            db.insert_document(&db_path, chrono::Utc::now(), &db_chunks, &vectors)
                .map_err(|e| McpError::internal_error(format!("reindex failed: {e}"), None))?;
        }

        json_result(serde_json::json!({
            "success": true,
            "message": "Document reindexed successfully",
        }))
    }

    // ── Tool 6: add_frontmatter ─────────────────────────────────────

    #[tool(description = "Add metadata (frontmatter) to a markdown file")]
    async fn add_frontmatter(
        &self,
        params: Parameters<FrontmatterParams>,
    ) -> Result<CallToolResult, McpError> {
        let p = params.0;
        if p.filepath.is_empty() {
            return error_result("filepath is required");
        }

        let metadata = build_frontmatter_metadata(&p);

        frontmatter::add_frontmatter(Path::new(&p.filepath), &metadata)
            .map_err(|e| McpError::internal_error(format!("{e}"), None))?;

        json_result(serde_json::json!({
            "success": true,
            "message": "Frontmatter added successfully",
        }))
    }

    // ── Tool 7: update_frontmatter ──────────────────────────────────

    #[tool(description = "Update metadata (frontmatter) of a markdown file")]
    async fn update_frontmatter(
        &self,
        params: Parameters<FrontmatterParams>,
    ) -> Result<CallToolResult, McpError> {
        let p = params.0;
        if p.filepath.is_empty() {
            return error_result("filepath is required");
        }

        let metadata = build_frontmatter_metadata(&p);

        frontmatter::update_frontmatter(Path::new(&p.filepath), &metadata)
            .map_err(|e| McpError::internal_error(format!("{e}"), None))?;

        json_result(serde_json::json!({
            "success": true,
            "message": "Frontmatter updated successfully",
        }))
    }

    // ── Tool 8: index_code ──────────────────────────────────────────

    #[tool(
        description = "Index source code files with AST parsing (Tree-sitter). Supports single file, directory, or batch. Languages: Go, Python, TypeScript, JavaScript, Rust"
    )]
    async fn index_code(
        &self,
        params: Parameters<IndexCodeParams>,
    ) -> Result<CallToolResult, McpError> {
        let p = params.0;
        if p.filepath.is_none() && p.directory.is_none() && p.filepaths.is_none() {
            return error_result("filepath, directory, or filepaths is required");
        }

        // Single file
        if let Some(fp) = &p.filepath {
            let path = Path::new(fp);
            if !path.exists() {
                return error_result(&format!("file not found: {fp}"));
            }

            index_single_code_file(path, fp, &self.ctx).await?;

            return json_result(serde_json::json!({
                "success": true,
                "message": "Code file indexed successfully",
                "file": fp,
            }));
        }

        // Batch files
        if let Some(fps) = &p.filepaths {
            let files: Vec<&str> = fps
                .split(',')
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .collect();
            let mut success_count = 0u32;
            let mut error_count = 0u32;
            let mut results = Vec::new();

            for f in &files {
                let path = Path::new(f);
                match index_single_code_file(path, f, &self.ctx).await {
                    Ok(()) => {
                        success_count += 1;
                        results.push(serde_json::json!({"file": f, "success": true}));
                    }
                    Err(_) => {
                        error_count += 1;
                        results.push(serde_json::json!({"file": f, "success": false}));
                    }
                }
            }

            return json_result(serde_json::json!({
                "success": error_count == 0,
                "message": format!("Indexed {success_count} files, {error_count} errors"),
                "results": results,
                "success_count": success_count,
                "error_count": error_count,
            }));
        }

        // Directory indexing — for code, we reuse the single-file approach on each file
        if let Some(dir) = &p.directory {
            let force = p.force.unwrap_or(false);
            let walker = ignore::WalkBuilder::new(dir).hidden(false).build();
            let mut success_count = 0u32;
            let mut skip_count = 0u32;
            let mut fail_count = 0u32;

            let supported = ["go", "py", "rs", "ts", "js"];

            for entry in walker.into_iter().filter_map(|e| e.ok()) {
                let path = entry.path();
                if path.is_dir() {
                    continue;
                }
                let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("");
                if !supported.contains(&ext) {
                    continue;
                }

                // Check if already indexed (unless force)
                if !force {
                    let db_path = path.to_string_lossy().replace('\\', "/");
                    let db = self.ctx.db.lock().await;
                    let docs = db.list_documents().unwrap_or_default();
                    if docs.contains_key(&db_path) {
                        skip_count += 1;
                        continue;
                    }
                }

                let fp_str = path.to_string_lossy().to_string();
                match index_single_code_file(path, &fp_str, &self.ctx).await {
                    Ok(()) => success_count += 1,
                    Err(_) => fail_count += 1,
                }
            }

            return json_result(serde_json::json!({
                "success": true,
                "message": "Directory indexing completed",
                "directory": dir,
                "files_indexed": success_count,
                "files_skipped": skip_count,
                "files_failed": fail_count,
            }));
        }

        error_result("unexpected state")
    }

    // ── Tool 9: search_relations ────────────────────────────────────

    #[tool(
        description = "Search code symbol relations (calls, imports, inherits). Explore callers/callees, imports, and inheritance."
    )]
    async fn search_relations(
        &self,
        params: Parameters<SearchRelationsParams>,
    ) -> Result<CallToolResult, McpError> {
        let p = params.0;
        if p.symbol.is_empty() {
            return error_result("symbol is required");
        }
        let direction = p.direction.as_deref().unwrap_or("both");
        let rel_type = p.relation_type.as_deref();

        let db = self.ctx.db.lock().await;
        let relations = db
            .find_symbol_relations(&p.symbol, direction, rel_type)
            .map_err(|e| McpError::internal_error(format!("search failed: {e}"), None))?;

        let results_json: Vec<serde_json::Value> = relations
            .iter()
            .map(|r| {
                serde_json::json!({
                    "relation_type": r.relation_type,
                    "target_name": r.target_name,
                    "target_file": r.target_file,
                    "source_name": r.source_name,
                    "source_file": r.source_file,
                    "confidence": r.confidence,
                })
            })
            .collect();

        json_result(serde_json::json!({
            "symbol": p.symbol,
            "direction": direction,
            "relations": results_json,
            "count": results_json.len(),
        }))
    }

    // ── Tool 10: build_dictionary ───────────────────────────────────

    #[tool(
        description = "Build a multilingual word dictionary by extracting word mappings from indexed documents. Auto-learns source-language -> English correspondences."
    )]
    async fn build_dictionary(
        &self,
        params: Parameters<BuildDictionaryParams>,
    ) -> Result<CallToolResult, McpError> {
        let p = params.0;
        let source_lang = p.source_lang.as_deref().unwrap_or("ja");

        let extractor = DictionaryExtractor::new();
        let mut all_mappings: Vec<(String, String, String, f64, String)> = Vec::new();

        if let Some(doc_path) = &p.document {
            // Extract from a specific document
            let content = std::fs::read_to_string(doc_path).map_err(|e| {
                McpError::internal_error(format!("failed to read {doc_path}: {e}"), None)
            })?;
            let mappings = extractor.extract_from_content(&content, doc_path, source_lang);
            for m in mappings {
                all_mappings.push((
                    m.source_word,
                    m.target_word,
                    m.source_lang.clone(),
                    m.confidence as f64,
                    m.source_document.clone(),
                ));
            }
        } else {
            // Extract from all indexed documents
            let db = self.ctx.db.lock().await;
            let docs = db.list_documents().map_err(|e| {
                McpError::internal_error(format!("list documents failed: {e}"), None)
            })?;
            drop(db);

            for doc_path in docs.keys() {
                let content = match std::fs::read_to_string(doc_path) {
                    Ok(c) => c,
                    Err(_) => continue,
                };

                let lang = dictionary::detect_language(&content);
                if lang == "mixed" || lang == source_lang {
                    let mappings = extractor.extract_from_content(&content, doc_path, source_lang);
                    for m in mappings {
                        all_mappings.push((
                            m.source_word,
                            m.target_word,
                            m.source_lang.clone(),
                            m.confidence as f64, // Cast f32 to f64
                            m.source_document.clone(),
                        ));
                    }
                }
            }
        }

        // Insert into DB
        let mut db = self.ctx.db.lock().await;
        if !all_mappings.is_empty() {
            db.insert_word_mappings(&all_mappings).map_err(|e| {
                McpError::internal_error(format!("insert mappings failed: {e}"), None)
            })?;
        }

        let total_count = db.get_word_mapping_count().unwrap_or(0);

        // Sample for response (max 10)
        let sample: Vec<serde_json::Value> = all_mappings
            .iter()
            .take(10)
            .map(|(src, tgt, _, conf, _)| {
                serde_json::json!({"source": src, "target": tgt, "confidence": conf})
            })
            .collect();

        json_result(serde_json::json!({
            "success": true,
            "extracted_count": all_mappings.len(),
            "total_dictionary": total_count,
            "sample_mappings": sample,
        }))
    }
}

// ── Helper functions ─────────────────────────────────────────────────

fn build_frontmatter_metadata(p: &FrontmatterParams) -> frontmatter::Metadata {
    let tags = p
        .tags
        .as_deref()
        .unwrap_or("")
        .split(',')
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
        .collect();

    frontmatter::Metadata {
        domain: p.domain.clone().unwrap_or_default(),
        doc_type: p.doc_type.clone().unwrap_or_default(),
        language: p.language.clone().unwrap_or_default(),
        tags,
        project: p.project.clone().unwrap_or_default(),
    }
}

/// Index a single code file (parse AST + embed + insert).
async fn index_single_code_file(
    path: &Path,
    filepath: &str,
    ctx: &McpContext,
) -> Result<(), McpError> {
    let mut parser = CodeParser::new()
        .map_err(|e| McpError::internal_error(format!("parser init: {e}"), None))?;

    let code_chunks = parser
        .parse_file(path)
        .map_err(|e| McpError::internal_error(format!("parse failed: {e}"), None))?;

    if code_chunks.is_empty() {
        return Ok(());
    }

    let text_refs: Vec<String> = code_chunks.iter().map(|c| c.get_embedding_text()).collect();
    let text_str_refs: Vec<&str> = text_refs.iter().map(|s| s.as_str()).collect();

    let vectors = ctx
        .embedder
        .embed_batch(&text_str_refs)
        .map_err(|e| McpError::internal_error(format!("embedding failed: {e}"), None))?;

    // Convert to db models
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

    let db_path = filepath.replace('\\', "/");
    let mut db = ctx.db.lock().await;
    db.insert_code_document(&db_path, chrono::Utc::now(), &db_chunks, &vectors)
        .map_err(|e| McpError::internal_error(format!("DB insert failed: {e}"), None))?;

    Ok(())
}
