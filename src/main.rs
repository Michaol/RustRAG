pub mod config;
pub mod db;
pub mod embedder;
pub mod frontmatter;
pub mod indexer;
pub mod mcp;

use crate::config::Config;
use crate::db::Db;
use crate::embedder::download::default_model_dir;
use crate::embedder::mock::MockEmbedder;
use crate::embedder::onnx::OnnxEmbedder;
use crate::indexer::core::Indexer;
use crate::mcp::server::{McpContext, McpServer};
use anyhow::{Context, Result};
use clap::Parser;
use std::sync::Arc;
use tokio::sync::Mutex as TokioMutex;
use tracing_subscriber::EnvFilter;

/// Local RAG MCP Server — Rust implementation of DevRag
#[derive(Parser, Debug)]
#[command(name = "rustrag", about = "Local RAG MCP Server", version)]
struct Cli {
    /// Path to configuration file
    #[arg(short, long, default_value = "config.json")]
    config: String,

    /// Log level (trace, debug, info, warn, error)
    #[arg(long, default_value = "info")]
    log_level: String,

    /// Skip automatic model download
    #[arg(long)]
    skip_download: bool,

    /// Skip initial differential sync
    #[arg(long)]
    skip_sync: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    // 1. Parse CLI arguments
    let cli = Cli::parse();

    // 2. Initialize tracing (output to stderr, since MCP uses stdio)
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(&cli.log_level)),
        )
        .with_writer(std::io::stderr)
        .init();

    tracing::info!("Starting RustRAG MCP Server...");

    // 3. Load and validate configuration
    let config = Config::load(&cli.config).context("Failed to load config")?;
    config.validate().context("Invalid configuration")?;
    let chunk_size = config.chunk_size;
    let config = Arc::new(config);

    tracing::info!(
        chunk_size = config.chunk_size,
        search_top_k = config.search_top_k,
        model = %config.model.name,
        "Configuration loaded"
    );

    // 4. Download model files (if needed)
    let model_dir = default_model_dir();
    if !cli.skip_download {
        tracing::info!("Checking model files...");
        if let Err(e) = crate::embedder::download::download_model_files(&model_dir) {
            tracing::warn!("Model download failed: {e}");
            tracing::warn!("Will use mock embedder as fallback");
        }
    } else {
        tracing::info!("Model download skipped (--skip-download)");
    }

    // 5. Initialize database
    tracing::info!(db_path = %config.db_path, "Opening database");
    let mut db = Db::open(&config.db_path).context("Failed to open database")?;

    // 6. Initialize embedder (ONNX with fallback to Mock)
    let embedder: Arc<dyn crate::embedder::Embedder> = match OnnxEmbedder::new(&model_dir) {
        Ok(e) => {
            tracing::info!(
                "ONNX embedder initialized (dim={})",
                config.model.dimensions
            );
            Arc::new(e)
        }
        Err(e) => {
            tracing::warn!("ONNX embedder unavailable: {e}");
            tracing::warn!("Using mock embedder — search results will be meaningless");
            Arc::new(MockEmbedder::default())
        }
    };

    // 7. Differential sync (index document_patterns directories)
    if !cli.skip_sync {
        let base_dirs = config.get_base_directories();
        tracing::info!(dirs = ?base_dirs, "Starting differential sync");

        for dir in &base_dirs {
            if !dir.exists() {
                tracing::warn!(dir = %dir.display(), "Directory does not exist, skipping");
                continue;
            }

            tracing::info!(dir = %dir.display(), "Syncing directory");
            let mut indexer = Indexer::new(&mut db, embedder.as_ref(), chunk_size);
            match indexer.index_directory(dir, false) {
                Ok(result) => {
                    tracing::info!(
                        dir = %dir.display(),
                        indexed = result.indexed,
                        added = result.added,
                        updated = result.updated,
                        skipped = result.skipped,
                        failed = result.failed,
                        "Sync completed"
                    );
                }
                Err(e) => {
                    tracing::error!(dir = %dir.display(), error = %e, "Sync failed");
                }
            }
        }
    } else {
        tracing::info!("Initial sync skipped (--skip-sync)");
    }

    // 8. Create MCP context and start server
    let db = Arc::new(TokioMutex::new(db));
    let mcp_ctx = McpContext {
        db,
        config: config.clone(),
        embedder,
        chunk_size,
    };

    tracing::info!("Starting MCP server on stdio transport...");
    let server = McpServer::new(mcp_ctx);
    server.start().await?;

    Ok(())
}
