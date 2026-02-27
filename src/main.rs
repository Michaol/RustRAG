pub mod config;
pub mod db;
pub mod embedder;
pub mod frontmatter;
pub mod indexer;
pub mod mcp;

use crate::config::Config;
use crate::db::Db;
use crate::embedder::mock::MockEmbedder;
use crate::mcp::server::{McpContext, McpServer};
use anyhow::{Context, Result};
use std::sync::Arc;
use tokio::sync::Mutex as TokioMutex;

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init_from_env(env_logger::Env::default().default_filter_or("info"));

    log::info!("Starting RustRAG MCP Server...");

    // 1. Load config
    let config = Arc::new(Config::default());

    // 2. Init DB
    let db = Db::open(&config.db_path).context("Failed to open database")?;
    let db = Arc::new(TokioMutex::new(db));

    // 3. Init Embedder (MockEmbedder until ONNX model is downloaded)
    let embedder: Arc<dyn crate::embedder::Embedder> = Arc::new(MockEmbedder::default());

    // 4. Init MCP Context
    let mcp_ctx = McpContext {
        db,
        config: config.clone(),
        embedder,
        chunk_size: config.chunk_size,
    };

    // 5. Start Server
    let server = McpServer::new(mcp_ctx);
    server.start().await?;

    Ok(())
}
