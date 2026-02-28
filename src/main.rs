use anyhow::{Context, Result};
use clap::Parser;
use rustrag::config::Config;
use rustrag::db::Db;
use rustrag::embedder::download::default_model_dir;
use rustrag::embedder::mock::MockEmbedder;
use rustrag::embedder::onnx::OnnxEmbedder;
use rustrag::indexer::core::Indexer;
use rustrag::mcp::server::{McpContext, McpServer};
use rustrag::updater;
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
    // Suppress ort's massive INFO-level ONNX Runtime memory allocation logs
    // (official recommendation: https://ort.pyke.io/troubleshooting/logging)
    let log_filter = format!("{},ort=warn", &cli.log_level);
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(&log_filter)),
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

    // 3b. Check for updates (best-effort, errors silently ignored)
    if config.is_update_check_enabled() {
        let ver = updater::CURRENT_VERSION;
        let _ = tokio::task::spawn_blocking(move || {
            updater::check_for_update(ver, "");
        })
        .await;
    }

    // 4. Download model files (if needed)
    let model_dir = default_model_dir();
    if !cli.skip_download {
        tracing::info!("Checking model files...");
        if let Err(e) = rustrag::embedder::download::download_model_files(&model_dir) {
            tracing::warn!("Model download failed: {e}");
            tracing::warn!("Will use mock embedder as fallback");
        }
    } else {
        tracing::info!("Model download skipped (--skip-download)");
    }

    // 5. Initialize database
    tracing::info!(db_path = %config.db_path, "Opening database");
    let db = Db::open(&config.db_path).context("Failed to open database")?;

    // 6. Initialize embedder (ONNX with fallback to Mock)
    let embedder: Arc<dyn rustrag::embedder::Embedder> = match OnnxEmbedder::new(&model_dir) {
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

    // 7. Wrap db in Arc<TokioMutex> BEFORE sync so MCP and sync can share it
    let db = Arc::new(TokioMutex::new(db));

    // 8. Create MCP context (shares db, embedder, config)
    let mcp_ctx = McpContext {
        db: db.clone(),
        config: config.clone(),
        embedder: embedder.clone(),
        chunk_size,
    };

    // 9. Spawn background sync task (non-blocking, MCP server starts immediately)
    if !cli.skip_sync {
        let sync_db = db.clone();
        let sync_embedder = embedder.clone();
        let sync_config = config.clone();
        let sync_chunk_size = chunk_size;

        tokio::spawn(async move {
            let base_dirs = sync_config.get_base_directories();
            tracing::info!(dirs = ?base_dirs, "Background sync started");

            for dir in &base_dirs {
                if !dir.exists() {
                    tracing::warn!(dir = %dir.display(), "Directory does not exist, skipping");
                    continue;
                }

                tracing::info!(dir = %dir.display(), "Syncing directory");

                // Pass the Arc<TokioMutex<Db>> directly, Indexer will lock per-file/operation
                // to minimize contention with MCP queries
                let result = {
                    let mut indexer =
                        Indexer::new(sync_db.clone(), sync_embedder.as_ref(), sync_chunk_size);
                    indexer.index_directory(dir, false).await
                };

                match result {
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

            tracing::info!("Background sync finished");
        });
    } else {
        tracing::info!("Initial sync skipped (--skip-sync)");
    }

    // 10. Start MCP server immediately (does NOT wait for sync)
    tracing::info!("Starting MCP server on stdio transport...");
    let server = McpServer::new(mcp_ctx);
    server.start().await?;

    Ok(())
}
