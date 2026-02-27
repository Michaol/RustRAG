/// End-to-end integration tests for the RustRAG pipeline.
///
/// Tests the complete flow:
///   Config → DB → Embedder → Indexer → Search → Delete
use rustrag::config::Config;
use rustrag::db::Db;
use rustrag::embedder::Embedder;
use rustrag::embedder::mock::MockEmbedder;
use rustrag::indexer::core::Indexer;
use std::fs;
use tempfile::tempdir;

/// Full pipeline: create docs → index → list → search → delete
#[test]
fn test_full_pipeline() {
    // 1. Setup temp dir with test markdown files
    let temp_dir = tempdir().unwrap();
    let docs_dir = temp_dir.path().join("documents");
    fs::create_dir_all(&docs_dir).unwrap();

    fs::write(
        docs_dir.join("hello.md"),
        "# Hello World\n\nThis is a test document about Rust programming.\n\nRust is a systems programming language focused on safety and performance.",
    ).unwrap();

    fs::write(
        docs_dir.join("guide.md"),
        "# Quick Start Guide\n\nTo get started with the application:\n\n1. Install dependencies\n2. Run the server\n3. Connect via MCP client",
    ).unwrap();

    fs::write(
        docs_dir.join("api.md"),
        "# API Reference\n\n## search\n\nPerform a vector search over indexed documents.\n\n## index_markdown\n\nIndex a single markdown file for search.",
    ).unwrap();

    // 2. Initialize DB (in-memory)
    let mut db = Db::open_in_memory().unwrap();

    // 3. Initialize MockEmbedder
    let embedder = MockEmbedder::default();

    // 4. Index via Indexer
    let mut indexer = Indexer::new(&mut db, &embedder, 500);
    let result = indexer.index_directory(&docs_dir, false).unwrap();

    assert_eq!(result.added, 3, "Should index 3 markdown files");
    assert_eq!(result.indexed, 3, "Should report 3 indexed");
    assert_eq!(result.skipped, 0, "Should skip 0 on first run");
    assert_eq!(result.failed, 0, "Should have 0 failures");

    // 5. List documents
    let docs = db.list_documents().unwrap();
    assert_eq!(docs.len(), 3, "Should have 3 documents in DB");

    // Verify document names contain our files (path-normalized)
    let doc_names: Vec<&String> = docs.keys().collect();
    assert!(
        doc_names.iter().any(|n| n.contains("hello.md")),
        "Should contain hello.md, got: {doc_names:?}"
    );
    assert!(
        doc_names.iter().any(|n| n.contains("guide.md")),
        "Should contain guide.md, got: {doc_names:?}"
    );
    assert!(
        doc_names.iter().any(|n| n.contains("api.md")),
        "Should contain api.md, got: {doc_names:?}"
    );

    // 6. Search (with mock embedder, results are based on hash similarity)
    let query_vec = embedder.embed("Rust programming").unwrap();
    let results = db.search_with_filter(&query_vec, 5, None).unwrap();
    assert!(!results.is_empty(), "Search should return results");

    // Verify result structure
    for r in &results {
        assert!(
            !r.document_name.is_empty(),
            "Document name should not be empty"
        );
        assert!(
            !r.chunk_content.is_empty(),
            "Chunk content should not be empty"
        );
        assert!(
            r.similarity >= -1.0 && r.similarity <= 1.0,
            "Similarity should be in [-1, 1]"
        );
    }

    // 7. Re-index (should skip unchanged files)
    let mut indexer2 = Indexer::new(&mut db, &embedder, 500);
    let result2 = indexer2.index_directory(&docs_dir, false).unwrap();
    assert_eq!(result2.skipped, 3, "Should skip all 3 on second run");
    assert_eq!(result2.added, 0, "Should add 0 on second run");

    // 8. Force re-index
    let mut indexer3 = Indexer::new(&mut db, &embedder, 500);
    let result3 = indexer3.index_directory(&docs_dir, true).unwrap();
    assert_eq!(result3.updated, 3, "Should update all 3 when forced");

    // 9. Delete a document
    let hello_key = doc_names.iter().find(|n| n.contains("hello.md")).unwrap();
    db.delete_document(hello_key).unwrap();

    let docs_after = db.list_documents().unwrap();
    assert_eq!(docs_after.len(), 2, "Should have 2 documents after delete");
    assert!(
        !docs_after.keys().any(|n| n.contains("hello.md")),
        "hello.md should be deleted"
    );
}

/// Test config defaults and validation
#[test]
fn test_config_defaults_and_validation() {
    let config = Config::default();

    assert_eq!(config.chunk_size, 500);
    assert_eq!(config.search_top_k, 5);
    assert_eq!(config.model.dimensions, 384);
    assert!(config.is_update_check_enabled());
    assert!(config.validate().is_ok());

    // Invalid config
    let mut bad_config = Config::default();
    bad_config.chunk_size = 0;
    assert!(bad_config.validate().is_err());
}

/// Test frontmatter operations
#[test]
fn test_frontmatter_round_trip() {
    let temp_dir = tempdir().unwrap();
    let file_path = temp_dir.path().join("test.md");

    // Create a simple markdown file
    fs::write(&file_path, "# Test\n\nSome content here.").unwrap();

    // Add frontmatter
    let metadata = rustrag::frontmatter::Metadata {
        domain: "backend".to_string(),
        doc_type: "api".to_string(),
        language: "rust".to_string(),
        tags: vec!["test".to_string(), "integration".to_string()],
        project: "rustrag".to_string(),
    };

    rustrag::frontmatter::add_frontmatter(&file_path, &metadata).unwrap();

    // Read and verify
    let content = fs::read_to_string(&file_path).unwrap();
    assert!(content.starts_with("---"), "Should start with frontmatter");
    assert!(content.contains("domain: backend"), "Should contain domain");
    assert!(
        content.contains("language: rust"),
        "Should contain language"
    );
    assert!(
        content.contains("# Test"),
        "Should preserve original content"
    );

    // Update frontmatter
    let updated_metadata = rustrag::frontmatter::Metadata {
        domain: "frontend".to_string(),
        doc_type: "guide".to_string(),
        language: "typescript".to_string(),
        tags: vec!["updated".to_string()],
        project: "new-project".to_string(),
    };

    rustrag::frontmatter::update_frontmatter(&file_path, &updated_metadata).unwrap();

    let updated_content = fs::read_to_string(&file_path).unwrap();
    assert!(
        updated_content.contains("domain: frontend"),
        "Should contain updated domain"
    );
    assert!(
        updated_content.contains("language: typescript"),
        "Should contain updated language"
    );
}

/// Test updater version comparison
#[test]
fn test_updater_version() {
    // CURRENT_VERSION must parse as valid semver
    let version = rustrag::updater::CURRENT_VERSION;
    assert!(!version.is_empty(), "Version should not be empty");

    // Should be parseable (major.minor.patch format)
    let parts: Vec<&str> = version.split('.').collect();
    assert_eq!(parts.len(), 3, "Version should have 3 parts: {version}");
    for part in &parts {
        assert!(
            part.parse::<u32>().is_ok(),
            "Each version part should be numeric: {part}"
        );
    }
}

/// Test MockEmbedder produces consistent results
#[test]
fn test_mock_embedder_consistency() {
    let embedder = MockEmbedder::default();

    let v1 = embedder.embed("hello world").unwrap();
    let v2 = embedder.embed("hello world").unwrap();

    assert_eq!(v1, v2, "Same input should produce same embedding");
    assert_eq!(v1.len(), embedder.dimensions(), "Should match dimensions");

    // Different input should (likely) produce different embedding
    let v3 = embedder.embed("different text").unwrap();
    assert_ne!(v1, v3, "Different input should produce different embedding");
}

/// Test batch embedding
#[test]
fn test_batch_embedding() {
    let embedder = MockEmbedder::default();
    let texts = vec!["first", "second", "third"];

    let results = embedder.embed_batch(&texts).unwrap();
    assert_eq!(results.len(), 3, "Should return 3 embeddings");

    for (i, result) in results.iter().enumerate() {
        assert_eq!(
            result.len(),
            embedder.dimensions(),
            "Embedding {i} should have correct dimensions"
        );
    }
}
