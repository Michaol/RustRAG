use rustrag::db::Db;
use rustrag::embedder::{Embedder, onnx::OnnxEmbedder};
use std::path::Path;

#[tokio::main]
async fn main() {
    let db_path = "C:/Users/michaol/AppData/Local/RustRAG/vectors.db";
    let model_path = "C:/Users/michaol/AppData/Local/RustRAG/models/multilingual-e5-small";

    let db = Db::open(db_path).expect("Failed to open DB");
    let embedder = OnnxEmbedder::new(Path::new(model_path), 32).expect("Failed to load model");

    let queries = vec![
        "Passwall intermittent DNS resolution failures, NFTSET troubleshooting",
        "How to host a speed test page for clients LibreSpeed curl wget",
    ];

    for query in queries {
        println!("==============================================");
        println!("🔍 Query: {}", query);
        let emb = embedder.embed(query).expect("Failed to embed");
        let results = db.search(&emb, 3).expect("Search failed");

        for (i, r) in results.iter().enumerate() {
            let preview = r
                .chunk_content
                .chars()
                .take(120)
                .collect::<String>()
                .replace("\n", " ");
            println!(
                "   [{}] Score: {:.4} | File: {}",
                i + 1,
                r.similarity,
                r.document_name
            );
            println!("       -> {}", preview);
        }
    }
}
