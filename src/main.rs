use anyhow::{Context, Result};
use clap::Parser;
use rustrag::config::Config;
use rustrag::db::Db;
use rustrag::indexer::core::Indexer;
use rustrag::mcp::server::{McpContext, McpServer};
use rustrag::updater;
use std::sync::Arc;
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

    /// Skip initial differential sync
    #[arg(long)]
    skip_sync: bool,

    /// Transport mode: "stdio" or "http"
    #[arg(long, default_value = "stdio")]
    transport: String,

    /// HTTP port (used if transport="http")
    #[arg(long, default_value_t = 8765)]
    port: u16,
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
        api_model = %config.embedding.api_model,
        dimensions = config.embedding.dimensions,
        "Configuration loaded"
    );

    // 3b. Check for updates (best-effort, errors silently ignored)
    if config.is_update_check_enabled() {
        let ver = updater::CURRENT_VERSION;
        tokio::spawn(async move {
            updater::check_for_update(ver, "").await;
        });
    }

    // 4. Ensure data directory exists
    let data_dir = &config.data_dir;
    if let Err(e) = std::fs::create_dir_all(data_dir) {
        tracing::warn!(
            "Failed to create data directory {}: {e}",
            data_dir.display()
        );
    }

    // 5. Initialize database
    tracing::info!(db_path = %config.db_path, "Opening database");
    let db = Db::open(&config.db_path).context("Failed to open database")?;

    // 6. Wrap db in Arc so MCP and sync can share it
    let db = Arc::new(db);

    // 7. Create MCP context (embedder is lazy-loaded on first search/index call)
    let mcp_ctx = McpContext::new(db.clone(), config.clone(), chunk_size, cli.config.clone());

    // 8. Spawn background sync task (non-blocking, MCP server starts immediately)
    if !cli.skip_sync {
        let sync_ctx = mcp_ctx.clone();

        tokio::spawn(async move {
            let base_dirs = sync_ctx.config.read().await.get_base_directories();
            tracing::info!(dirs = ?base_dirs, "Background sync started");

            // Trigger embedder lazy-init now (in background, not blocking MCP startup)
            let sync_embedder = sync_ctx.get_embedder().await;

            for dir in &base_dirs {
                if !dir.exists() {
                    tracing::warn!(dir = %dir.display(), "Directory does not exist, skipping");
                    continue;
                }

                tracing::info!(dir = %dir.display(), "Syncing directory");

                let result = {
                    let mut indexer = Indexer::new(
                        sync_ctx.db.clone(),
                        sync_embedder.as_ref(),
                        sync_ctx.chunk_size,
                        Arc::new(sync_ctx.config.read().await.clone()),
                    );
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

    // 9. Start background file watcher (hot reload)
    rustrag::watcher::start_watcher(mcp_ctx.clone()).await;

    // 10. Start MCP server immediately
    let server = McpServer::new(mcp_ctx);

    match cli.transport.as_str() {
        "http" => {
            server.start_http(cli.port).await?;
        }
        _ => {
            tracing::info!("Starting MCP server on stdio transport...");
            server.start().await?;
        }
    }

    Ok(())
}
