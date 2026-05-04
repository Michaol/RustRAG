/// MCP Tool handlers for RustRAG.
///
/// Implements 7 tools (consolidated from original 10):
/// 1. search           – vector similarity search
/// 2. index            – index files (markdown or code, auto-detected by extension)
/// 3. list_documents   – list indexed documents
/// 4. manage_document  – delete or reindex a document
/// 5. frontmatter      – add or update YAML frontmatter
/// 6. search_relations – search code symbol relations
/// 7. build_dictionary – build multilingual word dictionary
use crate::db::search::SearchFilter;
use crate::frontmatter;
use crate::indexer::core::Indexer;
use crate::indexer::{
    code_parser::CodeParser,
    dictionary::{self, DictionaryExtractor},
};
use crate::mcp::server::McpContext;
use rmcp::handler::server::ServerHandler;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::service::RequestContext;
use rmcp::{
    ErrorData as McpError, RoleServer, handler::server::tool::ToolRouter, model::*, tool,
    tool_router,
};
use schemars::JsonSchema;
use serde::Deserialize;
use std::path::Path;
use std::sync::Arc;

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
struct IndexParams {
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
struct ManageDocumentParams {
    /// Filename to operate on
    filename: String,
    /// Action to perform: "delete" or "reindex" (default: "delete")
    action: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
struct FrontmatterParams {
    /// Path to the markdown file
    filepath: String,
    /// Mode: "add" or "update" (default: "update")
    mode: Option<String>,
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
    /// Specific document path (must be an indexed document)
    document: Option<String>,
    /// Max number of documents to process when extracting from all (default: 100)
    limit: Option<usize>,
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

impl ServerHandler for AppTools {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            instructions: Some(
                "RustRAG — Local RAG MCP Server for indexing and searching documents and code"
                    .into(),
            ),
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            server_info: Implementation::from_build_env(),
            ..Default::default()
        }
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, McpError> {
        let items = self.tool_router.list_all();
        tracing::info!(count = items.len(), "list_tools called");
        Ok(ListToolsResult {
            tools: items,
            next_cursor: None,
            meta: None,
        })
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let tcc = rmcp::handler::server::tool::ToolCallContext::new(self, request, context);
        self.tool_router.call(tcc).await
    }
}

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
            return Err(McpError::invalid_params(
                "query is required".to_string(),
                None,
            ));
        }
        let top_k = p.top_k.unwrap_or(5);

        // Pre-clone context limits
        let embedder = self.ctx.get_embedder().await;
        let db = self.ctx.db.clone();

        let query_str = p.query.clone();
        let p_directory = p.directory.clone();
        let p_file_pattern = p.file_pattern.clone();

        let (results, keyword_results) = tokio::task::spawn_blocking(move || {
            let query_vector = embedder
                .embed(&query_str)
                .map_err(|e| McpError::invalid_request(format!("embedding failed: {e}"), None))?;

            let filter = SearchFilter {
                directory: p_directory.as_deref(),
                file_pattern: p_file_pattern.as_deref(),
            };
            let has_filter = filter.directory.is_some() || filter.file_pattern.is_some();
            let filter_ref = if has_filter { Some(&filter) } else { None };

            let r = db
                .search_with_filter(&query_vector, top_k, filter_ref)
                .map_err(|e| McpError::internal_error(format!("search failed: {e}"), None))?;

            let keywords: Vec<&str> = query_str.split_whitespace().collect();
            let kr = db
                .search_symbols_by_keywords(&keywords, top_k)
                .unwrap_or_default();

            Ok::<_, McpError>((r, kr))
        })
        .await
        .map_err(|e| McpError::internal_error(format!("blocking failed: {e}"), None))??;
        // removed drop(db)

        // Check for updates (non-blocking, best-effort)
        let config_guard = self.ctx.config.read().await;
        let update_info = if config_guard.is_update_check_enabled() {
            crate::updater::get_update_info(crate::updater::CURRENT_VERSION, &config_guard.db_path)
                .await
        } else {
            None
        };
        drop(config_guard);

        // Merge vector + keyword results, deduplicating by (document_name, position)
        let mut seen = std::collections::HashSet::new();
        let results_json: Vec<serde_json::Value> = results
            .iter()
            .chain(keyword_results.iter())
            .filter_map(|r| {
                let key = (r.document_name.clone(), r.position);
                if !seen.insert(key) {
                    return None; // Already seen this chunk from vector search
                }
                let mut obj = serde_json::json!({
                    "document": r.document_name,
                    "content": r.chunk_content,
                    "similarity": format!("{:.4}", r.similarity),
                    "position": r.position,
                });
                if let Some(meta) = &r.metadata {
                    obj["symbol_name"] = serde_json::json!(meta.symbol_name);
                    obj["symbol_type"] = serde_json::json!(meta.symbol_type);
                    obj["language"] = serde_json::json!(meta.language);
                    obj["start_line"] = serde_json::json!(meta.start_line);
                    obj["end_line"] = serde_json::json!(meta.end_line);
                    obj["parent_symbol"] = serde_json::json!(meta.parent_symbol);
                    obj["signature"] = serde_json::json!(meta.signature);
                }
                Some(obj)
            })
            .collect();

        let mut response = serde_json::json!({ "results": results_json });
        if let Some(info) = update_info {
            response["update_available"] = serde_json::json!({
                "current_version": info.current_version,
                "latest_version": info.latest_version,
                "url": info.url,
            });
        }

        json_result(response)
    }

    // ── Tool 2: index (merged index_markdown + index_code) ──────────

    #[tool(
        description = "Index files (markdown or code). Auto-detects type by file extension. Supports single file, directory, or batch (comma-separated paths). Languages: Go, Python, TypeScript, JavaScript, Rust, Markdown."
    )]
    async fn index(&self, params: Parameters<IndexParams>) -> Result<CallToolResult, McpError> {
        let p = params.0;
        if p.filepath.is_none() && p.directory.is_none() && p.filepaths.is_none() {
            return Err(McpError::invalid_params(
                "filepath, directory, or filepaths is required".to_string(),
                None,
            ));
        }

        // Single file
        if let Some(fp) = &p.filepath {
            let path = Path::new(fp);
            if !path.exists() {
                return Err(McpError::invalid_params(
                    format!("file not found: {fp}"),
                    None,
                ));
            }
            return index_single_file(path, fp, &self.ctx).await;
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
                match index_single_file(path, f, &self.ctx).await {
                    Ok(_) => {
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

        // Directory indexing — delegate to Indexer to avoid duplicating walker logic
        if let Some(dir) = &p.directory {
            // Security validation: ensure directory exists and is within allowed paths
            let dir_path = Path::new(dir);
            if !dir_path.exists() {
                return error_result(&format!("Directory does not exist: {}", dir));
            }

            // Canonicalize to resolve symlinks and validate
            let canonical_dir = match dir_path.canonicalize() {
                Ok(path) => {
                    let s = path.to_string_lossy();
                    // Strip Windows UNC prefix (\\?\) for consistency with normalize_system_path
                    let s = s.strip_prefix(r"\\?\").unwrap_or(&s);
                    s.replace('\\', "/")
                }
                Err(e) => {
                    return error_result(&format!(
                        "Failed to resolve directory path {}: {}",
                        dir, e
                    ));
                }
            };

            let force = p.force.unwrap_or(false);
            let embedder = self.ctx.get_embedder().await;
            let config = self.ctx.config.read().await.clone();
            let mut indexer = Indexer::new(
                self.ctx.db.clone(),
                embedder.as_ref(),
                self.ctx.chunk_size,
                Arc::new(config),
            );

            let result = match indexer.index_directory(&canonical_dir, force).await {
                Ok(r) => r,
                Err(e) => return error_result(&format!("directory indexing failed: {e}")),
            };

            return json_result(serde_json::json!({
                "success": true,
                "message": "Directory indexing completed",
                "directory": dir,
                "files_indexed": result.indexed,
                "files_added": result.added,
                "files_updated": result.updated,
                "files_skipped": result.skipped,
                "files_removed": result.removed,
                "files_failed": result.failed,
            }));
        }

        error_result("unexpected state")
    }

    // ── Tool 3: list_documents ──────────────────────────────────────

    #[tool(
        description = "Retrieve list of indexed documents (limited to 500 results for stability)"
    )]
    async fn list_documents(&self) -> Result<CallToolResult, McpError> {
        let db = self.ctx.db.clone();
        let docs = tokio::task::spawn_blocking(move || db.list_documents())
            .await
            .map_err(|e| McpError::internal_error(format!("blocking failed: {e}"), None))?
            .map_err(|e| McpError::internal_error(format!("list failed: {e}"), None))?;

        let total_count = docs.len();
        let limit = 500;
        let has_more = total_count > limit;

        let documents: Vec<serde_json::Value> = docs
            .iter()
            .take(limit)
            .map(|(filename, modified_at)| {
                serde_json::json!({
                    "filename": filename,
                    "modified_at": modified_at.to_rfc3339(),
                })
            })
            .collect();

        json_result(serde_json::json!({
            "total_count": total_count,
            "has_more": has_more,
            "limit": limit,
            "documents": documents
        }))
    }

    // ── Tool 4: manage_document (merged delete + reindex) ───────────

    #[tool(
        description = "Manage an indexed document. Actions: 'delete' removes it from the DB, 'reindex' deletes and re-indexes it."
    )]
    async fn manage_document(
        &self,
        params: Parameters<ManageDocumentParams>,
    ) -> Result<CallToolResult, McpError> {
        let p = params.0;
        if p.filename.is_empty() {
            return Err(McpError::invalid_params(
                "filename is required".to_string(),
                None,
            ));
        }

        let action = p.action.as_deref().unwrap_or("delete");

        match action {
            "delete" => {
                let db = self.ctx.db.clone();
                let f_clone = p.filename.clone();
                tokio::task::spawn_blocking(move || db.delete_document(&f_clone))
                    .await
                    .map_err(|e| McpError::internal_error(format!("blocking failed: {e}"), None))?
                    .map_err(|e| McpError::internal_error(format!("delete failed: {e}"), None))?;

                json_result(serde_json::json!({
                    "success": true,
                    "action": "delete",
                    "message": "Document deleted successfully",
                }))
            }
            "reindex" => {
                // Delete from DB
                {
                    let db = self.ctx.db.clone();
                    let f_clone = p.filename.clone();
                    tokio::task::spawn_blocking(move || db.delete_document(&f_clone))
                        .await
                        .map_err(|e| {
                            McpError::internal_error(format!("blocking failed: {e}"), None)
                        })?
                        .map_err(|e| {
                            McpError::internal_error(format!("delete failed: {e}"), None)
                        })?;
                }

                // Re-index
                let path = Path::new(&p.filename);
                if !path.exists() {
                    return Err(McpError::invalid_params(
                        format!("file not found: {}", p.filename),
                        None,
                    ));
                }

                index_single_file(path, &p.filename, &self.ctx).await?;

                json_result(serde_json::json!({
                    "success": true,
                    "action": "reindex",
                    "message": "Document reindexed successfully",
                }))
            }
            _ => Err(McpError::invalid_params(
                format!("unknown action: {action}. Use 'delete' or 'reindex'."),
                None,
            )),
        }
    }

    // ── Tool 5: frontmatter (merged add + update) ───────────────────

    #[tool(
        description = "Add or update metadata (frontmatter) of a markdown file. Mode: 'add' creates new frontmatter, 'update' modifies existing (default: 'update')."
    )]
    async fn frontmatter(
        &self,
        params: Parameters<FrontmatterParams>,
    ) -> Result<CallToolResult, McpError> {
        let p = params.0;
        if p.filepath.is_empty() {
            return Err(McpError::invalid_params(
                "filepath is required".to_string(),
                None,
            ));
        }

        let metadata = build_frontmatter_metadata(&p);
        let mode = p.mode.as_deref().unwrap_or("update");

        match mode {
            "add" => {
                frontmatter::add_frontmatter(Path::new(&p.filepath), &metadata)
                    .map_err(|e| McpError::internal_error(format!("{e}"), None))?;
                json_result(serde_json::json!({
                    "success": true,
                    "mode": "add",
                    "message": "Frontmatter added successfully",
                }))
            }
            "update" => {
                frontmatter::update_frontmatter(Path::new(&p.filepath), &metadata)
                    .map_err(|e| McpError::internal_error(format!("{e}"), None))?;
                json_result(serde_json::json!({
                    "success": true,
                    "mode": "update",
                    "message": "Frontmatter updated successfully",
                }))
            }
            _ => Err(McpError::invalid_params(
                format!("unknown mode: {mode}. Use 'add' or 'update'."),
                None,
            )),
        }
    }

    // ── Tool 6: search_relations ────────────────────────────────────

    #[tool(
        description = "Search code symbol relations (calls, imports, inherits). Explore callers/callees, imports, and inheritance."
    )]
    async fn search_relations(
        &self,
        params: Parameters<SearchRelationsParams>,
    ) -> Result<CallToolResult, McpError> {
        let p = params.0;
        if p.symbol.is_empty() {
            return Err(McpError::invalid_params(
                "symbol is required".to_string(),
                None,
            ));
        }
        let direction = p.direction.as_deref().unwrap_or("both");
        let rel_type = p.relation_type.as_deref();

        let db = self.ctx.db.clone();
        let sym_clone = p.symbol.clone();
        let dir_clone = direction.to_string();
        let rel_clone = rel_type.map(|s| s.to_string());

        let relations = tokio::task::spawn_blocking(move || {
            db.find_symbol_relations(&sym_clone, &dir_clone, rel_clone.as_deref())
        })
        .await
        .map_err(|e| McpError::internal_error(format!("blocking failed: {e}"), None))?
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

    // ── Tool 7: build_dictionary ───────────────────────────────────

    #[tool(
        description = "Build a multilingual word dictionary by extracting word mappings from indexed documents. Auto-learns source-language -> English correspondences."
    )]
    async fn build_dictionary(
        &self,
        params: Parameters<BuildDictionaryParams>,
    ) -> Result<CallToolResult, McpError> {
        let p = params.0;
        let source_lang = p.source_lang.as_deref().unwrap_or("ja");
        let limit = p.limit.unwrap_or(100);

        let extractor = DictionaryExtractor::new();
        let mut all_mappings: Vec<(String, String, String, f64, String)> = Vec::new();

        if let Some(doc_path) = &p.document {
            // Validate the document is indexed before reading
            let db = self.ctx.db.clone();
            let doc_path_clone = doc_path.clone();
            let is_indexed = tokio::task::spawn_blocking(move || {
                db.list_documents()
                    .map(|docs| docs.contains_key(&doc_path_clone))
                    .unwrap_or(false)
            })
            .await
            .unwrap_or(false);

            if !is_indexed {
                return Err(McpError::invalid_params(
                    format!("document not found in index: {doc_path}"),
                    None,
                ));
            }

            // Extract from a specific document
            let content = std::fs::read_to_string(doc_path).map_err(|e| {
                McpError::invalid_params(format!("failed to read {doc_path}: {e}"), None)
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
            let db = self.ctx.db.clone();
            let docs = tokio::task::spawn_blocking(move || db.list_documents())
                .await
                .map_err(|e| McpError::internal_error(format!("blocking failed: {e}"), None))?
                .map_err(|e| {
                    McpError::internal_error(format!("list documents failed: {e}"), None)
                })?;

            for doc_path in docs.keys().take(limit) {
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
        let db = self.ctx.db.clone();
        let mappings_clone = all_mappings.clone();
        let total_count = tokio::task::spawn_blocking(move || {
            if !mappings_clone.is_empty() {
                db.insert_word_mappings(&mappings_clone).map_err(|e| {
                    McpError::internal_error(format!("insert mappings failed: {e}"), None)
                })?;
            }
            Ok::<_, McpError>(db.get_word_mapping_count().unwrap_or(0))
        })
        .await
        .map_err(|e| McpError::internal_error(format!("blocking failed: {e}"), None))??;

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

/// Returns true if the file extension indicates a markdown file.
fn is_markdown(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e == "md")
        .unwrap_or(false)
}

/// Index a single file — auto-detects markdown vs code by extension.
async fn index_single_file(
    path: &Path,
    filepath: &str,
    ctx: &McpContext,
) -> Result<CallToolResult, McpError> {
    if !path.exists() {
        return Err(McpError::invalid_params(
            format!("file not found: {filepath}"),
            None,
        ));
    }

    if is_markdown(path) {
        index_single_markdown_file(path, filepath, ctx).await
    } else {
        index_single_code_file(path, filepath, ctx).await?;
        json_result(serde_json::json!({
            "success": true,
            "message": "Code file indexed successfully",
            "file": filepath,
        }))
    }
}

/// Index a single markdown file.
async fn index_single_markdown_file(
    path: &Path,
    filepath: &str,
    ctx: &McpContext,
) -> Result<CallToolResult, McpError> {
    let chunks = crate::indexer::markdown::parse_markdown(path, ctx.chunk_size)
        .map_err(|e| McpError::invalid_params(format!("parse failed: {e}"), None))?;

    if chunks.is_empty() {
        return json_result(serde_json::json!({
            "success": true,
            "message": "File is empty, nothing to index",
        }));
    }

    let embedder = ctx.get_embedder().await;
    let db_path = filepath.replace('\\', "/");
    let db = ctx.db.clone();

    tokio::task::spawn_blocking(move || {
        let text_refs: Vec<&str> = chunks.iter().map(|c| c.content.as_str()).collect();
        let vectors = embedder
            .embed_batch(&text_refs)
            .map_err(|e| McpError::invalid_request(format!("embedding failed: {e}"), None))?;

        let db_chunks: Vec<crate::db::models::Chunk> = chunks
            .iter()
            .map(|c| crate::db::models::Chunk {
                position: c.position,
                content: c.content.as_str(),
            })
            .collect();

        db.insert_document(&db_path, chrono::Utc::now(), &db_chunks, &vectors)
            .map_err(|e| McpError::internal_error(format!("DB insert failed: {e}"), None))?;

        Ok::<_, McpError>(())
    })
    .await
    .map_err(|e| McpError::internal_error(format!("blocking task failed: {e}"), None))??;

    json_result(serde_json::json!({
        "success": true,
        "message": "Markdown file indexed successfully",
        "file": filepath,
    }))
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
        .map_err(|e| McpError::invalid_params(format!("parse failed: {e}"), None))?;

    if code_chunks.is_empty() {
        return Ok(());
    }

    let embedder = ctx.get_embedder().await;
    let db_path = filepath.replace('\\', "/");
    let db = ctx.db.clone();

    tokio::task::spawn_blocking(move || {
        let text_refs: Vec<String> = code_chunks.iter().map(|c| c.get_embedding_text()).collect();
        let text_str_refs: Vec<&str> = text_refs.iter().map(|s| s.as_str()).collect();

        let vectors = embedder
            .embed_batch(&text_str_refs)
            .map_err(|e| McpError::invalid_request(format!("embedding failed: {e}"), None))?;

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

        db.insert_code_document(&db_path, chrono::Utc::now(), &db_chunks, &vectors)
            .map_err(|e| McpError::internal_error(format!("DB insert failed: {e}"), None))?;

        Ok::<_, McpError>(())
    })
    .await
    .map_err(|e| McpError::internal_error(format!("blocking task failed: {e}"), None))??;

    Ok(())
}
