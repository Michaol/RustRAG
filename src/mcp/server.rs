/// MCP Server setup using `rmcp` with stdio transport.
///
/// Provides `McpContext` (shared state) and `McpServer` (startup logic).
use crate::mcp::tools::AppTools;
use anyhow::{Context, Result};
use rmcp::{ServiceExt, handler::server::router::Router, transport::io::stdio};
use std::path::PathBuf;
use std::sync::Arc;

use crate::{config::Config, db::Db, embedder::Embedder};
use tokio::sync::RwLock as TokioRwLock;
/// Shared application context available to all tool handlers.
#[derive(Clone)]
pub struct McpContext {
    pub db: Arc<Db>,
    pub config: Arc<TokioRwLock<Config>>,
    /// Lazy-initialized embedder, hot-swappable
    embedder: Arc<TokioRwLock<Option<Arc<dyn Embedder>>>>,
    /// Path to ONNX model directory (used by lazy init)
    model_dir: PathBuf,
    pub chunk_size: usize,
    pub config_path: String,
}

impl McpContext {
    pub fn new(
        db: Arc<Db>,
        config: Arc<Config>,
        model_dir: PathBuf,
        chunk_size: usize,
        config_path: String,
    ) -> Self {
        Self {
            db,
            config: Arc::new(TokioRwLock::new((*config).clone())),
            embedder: Arc::new(TokioRwLock::new(None)),
            model_dir,
            chunk_size,
            config_path,
        }
    }

    /// Get or lazily initialize the embedder.
    /// On first call or after hot-swap, loads the ONNX model (1-3s).
    pub async fn get_embedder(&self) -> Arc<dyn Embedder> {
        let read_guard = self.embedder.read().await;
        if let Some(embedder) = read_guard.clone() {
            return embedder;
        }
        drop(read_guard);

        let mut write_guard = self.embedder.write().await;
        // Double check after acquiring write lock
        if let Some(embedder) = write_guard.clone() {
            return embedder;
        }

        tracing::info!("Lazy-initializing ONNX embedder...");
        let config = self.config.read().await.clone();
        match crate::embedder::onnx::OnnxEmbedder::new(
            &self.model_dir,
            config.model.batch_size,
            config.model.dimensions,
            &config.compute.device,
            config.compute.fallback_to_cpu,
        ) {
            Ok(e) => {
                tracing::info!(
                    "ONNX embedder initialized (dim={})",
                    config.model.dimensions
                );
                let embedder_arc = Arc::new(e) as Arc<dyn Embedder>;
                *write_guard = Some(embedder_arc.clone());
                embedder_arc
            }
            Err(e) => {
                tracing::warn!("ONNX embedder unavailable: {e}");
                tracing::warn!("Using mock embedder — search results will be meaningless");
                let mock_arc =
                    Arc::new(crate::embedder::mock::MockEmbedder::default()) as Arc<dyn Embedder>;
                *write_guard = Some(mock_arc.clone());
                mock_arc
            }
        }
    }

    /// Hot-reloads the configuration from disk and drops the embedder if hardware settings changed.
    pub async fn reload_config(&self, new_config: Config) {
        let mut config_guard = self.config.write().await;

        // Check if embedder needs invalidation
        let mut should_invalidate_embedder = false;
        if config_guard.compute.device != new_config.compute.device
            || config_guard.compute.fallback_to_cpu != new_config.compute.fallback_to_cpu
            || config_guard.model.batch_size != new_config.model.batch_size
        {
            should_invalidate_embedder = true;
        }

        tracing::info!("Reloading configuration parameters in-memory...");
        *config_guard = new_config;
        drop(config_guard); // Free config lock before acquiring embedder lock

        if should_invalidate_embedder {
            tracing::warn!(
                "Hardware acceleration settings changed. Invalidating Embedder context for hot-swap!"
            );
            let mut embedder_guard = self.embedder.write().await;
            *embedder_guard = None;
        }
    }

    /// Create an Indexer with the current embedder and config.
    pub async fn create_indexer<'e, E: crate::embedder::Embedder>(
        &self,
        embedder: &'e E,
    ) -> crate::indexer::core::Indexer<'e, E> {
        crate::indexer::core::Indexer::new(
            self.db.clone(),
            embedder,
            self.chunk_size,
            Arc::new(self.config.read().await.clone()),
        )
    }
}

/// MCP Server wrapping the context and serving via stdio.
#[derive(Clone)]
pub struct McpServer {
    pub ctx: McpContext,
}

impl McpServer {
    pub fn new(ctx: McpContext) -> Self {
        Self { ctx }
    }

    /// Start the MCP server on stdio transport (blocks until the client disconnects).
    pub async fn start(self) -> Result<()> {
        tracing::info!("Starting MCP server on stdio...");
        let (stdin, stdout) = stdio();

        let app_tools = AppTools::new(self.ctx.clone());

        // Router wraps AppTools and dispatches JSON-RPC methods
        // to the correct handlers (list_tools, call_tool, etc.)
        let router = Router::new(app_tools.clone()).with_tools(app_tools.tool_router.clone());

        let service = router
            .serve((stdin, stdout))
            .await
            .context("MCP Server failed to initialize")?;

        // Keep the server process alive until the client exits or an error occurs
        let _ = service.waiting().await;

        tracing::info!("MCP Server exited.");
        Ok(())
    }

    /// Start the MCP server on streamable HTTP transport.
    pub async fn start_http(self, port: u16) -> Result<()> {
        tracing::info!("Starting MCP server on http://0.0.0.0:{}...", port);

        use rmcp::transport::streamable_http_server::session::local::LocalSessionManager;
        use rmcp::transport::streamable_http_server::tower::{
            StreamableHttpServerConfig, StreamableHttpService,
        };
        use tokio_util::sync::CancellationToken;

        let session_manager = Arc::new(LocalSessionManager::default());
        let cancel_token = CancellationToken::new();

        let config = StreamableHttpServerConfig {
            sse_keep_alive: Some(std::time::Duration::from_secs(30)),
            stateful_mode: false,
            cancellation_token: cancel_token.clone(),
            sse_retry: None,
        };

        let ctx = self.ctx.clone();

        let service = StreamableHttpService::new(
            move || {
                let app_tools = AppTools::new(ctx.clone());
                let router =
                    Router::new(app_tools.clone()).with_tools(app_tools.tool_router.clone());
                Ok(router)
            },
            session_manager,
            config,
        );

        let app = axum::Router::new().fallback_service(service);

        let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", port)).await?;

        tokio::select! {
            res = axum::serve(listener, app).into_future() => {
                res.context("Axum HTTP server failed")?;
            }
            _ = tokio::signal::ctrl_c() => {
                tracing::info!("Ctrl-C received, shutting down HTTP server...");
                cancel_token.cancel();
            }
        }

        tracing::info!("MCP Server exited.");
        Ok(())
    }
}
