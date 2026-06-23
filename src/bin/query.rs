use anyhow::{Context, Result};
use rustrag::config::Config;
use rustrag::db::Db;
use rustrag::embedder::Embedder;
use rustrag::embedder::api::ApiEmbedder;

fn main() -> Result<()> {
    // Load config
    let config = Config::load("config.json").context("Failed to load config")?;
    config.validate().context("Invalid configuration")?;

    // Open database
    let db = Db::open(&config.db_path)
        .map_err(|e| anyhow::anyhow!("Failed to open database: {}", e))?;

    // Create API embedder
    let embedder = ApiEmbedder::new(&config.embedding)
        .map_err(|e| anyhow::anyhow!("Failed to create embedder: {e}"))?;

    let queries = vec![
        "Passwall intermittent DNS resolution failures, NFTSET troubleshooting",
        "How to host a speed test page for clients LibreSpeed curl wget",
    ];

    for query in queries {
        println!("==============================================");
        println!("Query: {query}");
        let emb = embedder
            .embed(query)
            .map_err(|e| anyhow::anyhow!("Failed to embed query: {e}"))?;
        let results = db
            .search(&emb, 3)
            .map_err(|e| anyhow::anyhow!("Failed to search database: {e}"))?;

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
            println!("       -> {preview}");
        }
    }

    Ok(())
}
