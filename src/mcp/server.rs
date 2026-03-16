/// MCP Server setup using `rmcp` with stdio transport.
///
/// Provides `McpContext` (shared state) and `McpServer` (startup logic).
use crate::mcp::tools::AppTools;
use anyhow::{Context, Result};
use rmcp::{ServiceExt, handler::server::router::Router, transport::io::stdio};
use std::path::PathBuf;
use std::sync::Arc;

use crate::{config::Config, db::Db, embedder::Embedder};
use tokio::sync::Mutex as TokioMutex;
use tokio::sync::OnceCell;

/// Shared application context available to all tool handlers.
#[derive(Clone)]
pub struct McpContext {
    pub db: Arc<TokioMutex<Db>>,
    pub config: Arc<Config>,
    /// Lazy-initialized embedder (loaded on first search/index call)
    embedder: Arc<OnceCell<Arc<dyn Embedder>>>,
    /// Path to ONNX model directory (used by lazy init)
    model_dir: PathBuf,
    pub chunk_size: usize,
}

impl McpContext {
    pub fn new(
        db: Arc<TokioMutex<Db>>,
        config: Arc<Config>,
        model_dir: PathBuf,
        chunk_size: usize,
    ) -> Self {
        Self {
            db,
            config,
            embedder: Arc::new(OnceCell::new()),
            model_dir,
            chunk_size,
        }
    }

    /// Get or lazily initialize the embedder.
    ///
    /// On first call, loads the ONNX model (1-3s). Subsequent calls return instantly.
    /// Falls back to MockEmbedder if ONNX loading fails.
    pub async fn get_embedder(&self) -> Arc<dyn Embedder> {
        self.embedder
            .get_or_init(|| async {
                tracing::info!("Lazy-initializing ONNX embedder...");
                match crate::embedder::onnx::OnnxEmbedder::new(
                    &self.model_dir,
                    self.config.model.batch_size,
                ) {
                    Ok(e) => {
                        tracing::info!(
                            "ONNX embedder initialized (dim={})",
                            self.config.model.dimensions
                        );
                        Arc::new(e) as Arc<dyn Embedder>
                    }
                    Err(e) => {
                        tracing::warn!("ONNX embedder unavailable: {e}");
                        tracing::warn!("Using mock embedder — search results will be meaningless");
                        Arc::new(crate::embedder::mock::MockEmbedder::default())
                            as Arc<dyn Embedder>
                    }
                }
            })
            .await
            .clone()
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
