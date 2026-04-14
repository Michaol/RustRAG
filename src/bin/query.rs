use anyhow::{Context, Result};
use rustrag::db::Db;
use rustrag::embedder::Embedder;
use rustrag::embedder::download::default_model_dir;
use rustrag::embedder::onnx::OnnxEmbedder;
use std::path::Path;

fn main() -> Result<()> {
    let db_path = "./vectors.db";
    let model_path = default_model_dir();

    // Validate paths before use
    let db_path = match std::fs::canonicalize(db_path) {
        Ok(path) => path.to_string_lossy().to_string(),
        Err(e) => {
            eprintln!("Failed to resolve database path: {}", e);
            std::process::exit(1);
        }
    };

    let model_path = match std::fs::canonicalize(&model_path) {
        Ok(path) => path.to_string_lossy().to_string(),
        Err(e) => {
            eprintln!("Failed to resolve model path: {}", e);
            std::process::exit(1);
        }
    };

    // Use proper error handling instead of expect()
    let db = Db::open(&db_path).with_context(|| format!("Failed to open database: {}", db_path))?;
    let embedder = OnnxEmbedder::new(Path::new(&model_path), 32, 384, "auto", true)
        .with_context(|| "Failed to load ONNX model")?;

    let queries = vec![
        "Passwall intermittent DNS resolution failures, NFTSET troubleshooting",
        "How to host a speed test page for clients LibreSpeed curl wget",
    ];

    for query in queries {
        println!("==============================================");
        println!("Query: {}", query);
        let emb = embedder
            .embed(query)
            .with_context(|| format!("Failed to embed query: {}", query))?;
        let results = db
            .search(&emb, 3)
            .with_context(|| "Failed to search database")?;

        for (i, r) in results.iter().enumerate() {
            let preview = r
                .chunk_content
                .chars()
                .take(120)
                .collect::<String>()
                .replace('\n', " ");
            println!(
                "   [{}] Score: {:.4} | File: {}",
                i + 1,
                r.similarity,
                r.document_name
            );
            println!("       -> {}", preview);
        }
    }

    Ok(())
}
