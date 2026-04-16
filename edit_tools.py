import re

with open("src/mcp/tools.rs", "r", encoding="utf-8") as f:
    orig_code = f.read()

# Make search wrapper
orig_code = orig_code.replace(
"""        // Vectorize query
        let embedder = self.ctx.get_embedder().await;
        let query_vector = embedder
            .embed(&p.query)
            .map_err(|e| McpError::invalid_request(format!("embedding failed: {e}"), None))?;

        // Search DB
        let db = &self.ctx.db;
        let filter_ref = if has_filter { Some(&filter) } else { None };
        let results = db
            .search_with_filter(&query_vector, top_k, filter_ref)
            .map_err(|e| McpError::internal_error(format!("search failed: {e}"), None))?;

        // Also search by keyword fallback
        let keywords: Vec<&str> = p.query.split_whitespace().collect();
        let keyword_results = db
            .search_symbols_by_keywords(&keywords, top_k)
            .unwrap_or_default();""",
"""        // Pre-clone context limits
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

            let r = db.search_with_filter(&query_vector, top_k, filter_ref)
                .map_err(|e| McpError::internal_error(format!("search failed: {e}"), None))?;

            let keywords: Vec<&str> = query_str.split_whitespace().collect();
            let kr = db.search_symbols_by_keywords(&keywords, top_k)
                .unwrap_or_default();
            
            Ok::<_, McpError>((r, kr))
        }).await.map_err(|e| McpError::internal_error(format!("blocking failed: {e}"), None))??;""")

# index single markdown
orig_code = orig_code.replace(
"""    let text_refs: Vec<&str> = chunks.iter().map(|c| c.content.as_str()).collect();
    let embedder = ctx.get_embedder().await;
    let vectors = embedder
        .embed_batch(&text_refs)
        .map_err(|e| McpError::invalid_request(format!("embedding failed: {e}"), None))?;

    let db_path = filepath.replace('\\\\', "/");
    let db_chunks: Vec<crate::db::models::Chunk> = chunks
        .iter()
        .map(|c| crate::db::models::Chunk {
            position: c.position,
            content: c.content.as_str(),
        })
        .collect();

    let db = ctx.db.clone();
    db.insert_document(&db_path, chrono::Utc::now(), &db_chunks, &vectors)
        .map_err(|e| McpError::internal_error(format!("DB insert failed: {e}"), None))?;""",
"""    let embedder = ctx.get_embedder().await;
    let db_path = filepath.replace('\\\\', "/");
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
    }).await.map_err(|e| McpError::internal_error(format!("blocking task failed: {e}"), None))??;""")

# index code file
orig_code = orig_code.replace(
"""    let text_refs: Vec<String> = code_chunks.iter().map(|c| c.get_embedding_text()).collect();
    let text_str_refs: Vec<&str> = text_refs.iter().map(|s| s.as_str()).collect();

    let embedder = ctx.get_embedder().await;
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

    let db_path = filepath.replace('\\\\', "/");
    let db = ctx.db.clone();
    db.insert_code_document(&db_path, chrono::Utc::now(), &db_chunks, &vectors)
        .map_err(|e| McpError::internal_error(format!("DB insert failed: {e}"), None))?;""",
"""    let embedder = ctx.get_embedder().await;
    let db_path = filepath.replace('\\\\', "/");
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
    }).await.map_err(|e| McpError::internal_error(format!("blocking task failed: {e}"), None))??;""")

# manage_document delete
orig_code = orig_code.replace(
"""            "delete" => {
                let db = &self.ctx.db;
                db.delete_document(&p.filename)
                    .map_err(|e| McpError::internal_error(format!("delete failed: {e}"), None))?;""",
"""            "delete" => {
                let db = self.ctx.db.clone();
                let f_clone = p.filename.clone();
                tokio::task::spawn_blocking(move || {
                    db.delete_document(&f_clone)
                }).await.map_err(|e| McpError::internal_error(format!("blocking failed: {e}"), None))?
                    .map_err(|e| McpError::internal_error(format!("delete failed: {e}"), None))?;""")

# manage_document reindex DB block
orig_code = orig_code.replace(
"""                // Delete from DB
                {
                    let db = &self.ctx.db;
                    db.delete_document(&p.filename).map_err(|e| {
                        McpError::internal_error(format!("delete failed: {e}"), None)
                    })?;
                }""",
"""                // Delete from DB
                {
                    let db = self.ctx.db.clone();
                    let f_clone = p.filename.clone();
                    tokio::task::spawn_blocking(move || {
                        db.delete_document(&f_clone)
                    }).await.map_err(|e| McpError::internal_error(format!("blocking failed: {e}"), None))?
                    .map_err(|e| McpError::internal_error(format!("delete failed: {e}"), None))?;
                }""")

# list_documents
orig_code = orig_code.replace(
"""        let db = &self.ctx.db;
        let docs = db
            .list_documents()
            .map_err(|e| McpError::internal_error(format!("list failed: {e}"), None))?;""",
"""        let db = self.ctx.db.clone();
        let docs = tokio::task::spawn_blocking(move || {
            db.list_documents()
        }).await.map_err(|e| McpError::internal_error(format!("blocking failed: {e}"), None))?
            .map_err(|e| McpError::internal_error(format!("list failed: {e}"), None))?;""")

# search_relations
orig_code = orig_code.replace(
"""        let db = &self.ctx.db;
        let relations = db
            .find_symbol_relations(&p.symbol, direction, rel_type)
            .map_err(|e| McpError::internal_error(format!("search failed: {e}"), None))?;""",
"""        let db = self.ctx.db.clone();
        let sym_clone = p.symbol.clone();
        let dir_clone = direction.to_string();
        let rel_clone = rel_type.map(|s| s.to_string());
        
        let relations = tokio::task::spawn_blocking(move || {
            db.find_symbol_relations(&sym_clone, &dir_clone, rel_clone.as_deref())
        }).await.map_err(|e| McpError::internal_error(format!("blocking failed: {e}"), None))?
            .map_err(|e| McpError::internal_error(format!("search failed: {e}"), None))?;""")

# build dictionary docs
orig_code = orig_code.replace(
"""            // Extract from all indexed documents
            let db = &self.ctx.db;
            let docs = db.list_documents().map_err(|e| {
                McpError::internal_error(format!("list documents failed: {e}"), None)
            })?;
            // removed drop(db)""",
"""            // Extract from all indexed documents
            let db = self.ctx.db.clone();
            let docs = tokio::task::spawn_blocking(move || {
                db.list_documents()
            }).await.map_err(|e| McpError::internal_error(format!("blocking failed: {e}"), None))?
            .map_err(|e| McpError::internal_error(format!("list documents failed: {e}"), None))?;""")

# build dictionary db insertion
orig_code = orig_code.replace(
"""        // Insert into DB
        let db = &self.ctx.db;
        if !all_mappings.is_empty() {
            db.insert_word_mappings(&all_mappings).map_err(|e| {
                McpError::internal_error(format!("insert mappings failed: {e}"), None)
            })?;
        }

        let total_count = db.get_word_mapping_count().unwrap_or(0);""",
"""        // Insert into DB
        let db = self.ctx.db.clone();
        let mappings_clone = all_mappings.clone();
        let total_count = tokio::task::spawn_blocking(move || {
            if !mappings_clone.is_empty() {
                db.insert_word_mappings(&mappings_clone).map_err(|e| {
                    McpError::internal_error(format!("insert mappings failed: {e}"), None)
                })?;
            }
            Ok::<_, McpError>(db.get_word_mapping_count().unwrap_or(0))
        }).await.map_err(|e| McpError::internal_error(format!("blocking failed: {e}"), None))??;""")

with open("src/mcp/tools.rs", "w", encoding="utf-8") as f:
    f.write(orig_code)

print("Replacement Complete")
