use rustrag::db::Db;
use rustrag::embedder::download::default_model_dir;
use rustrag::embedder::onnx::OnnxEmbedder;
use rustrag::embedder::Embedder;

fn main() {
    let db_path = "./vectors.db";
    let model_path = default_model_dir();

    let db = Db::open(db_path).expect("Failed to open DB");
    let embedder =
        OnnxEmbedder::new(&model_path, 32, 384, "auto", true).expect("Failed to load model");

    let queries = vec![
        "Passwall intermittent DNS resolution failures, NFTSET troubleshooting",
        "How to host a speed test page for clients LibreSpeed curl wget",
    ];

    for query in queries {
        println!("==============================================");
        println!("Query: {}", query);
        let emb = embedder.embed(query).expect("Failed to embed");
        let results = db.search(&emb, 3).expect("Search failed");

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
}
