/// MCP Server setup using `rmcp` with stdio transport.
///
/// Provides `McpContext` (shared state) and `McpServer` (startup logic).
use crate::mcp::tools::AppTools;
use anyhow::{Context, Result};
use rmcp::{ServiceExt, handler::server::router::Router, transport::io::stdio};
use std::sync::Arc;

use crate::{config::Config, db::Db, embedder::Embedder};
use tokio::sync::Mutex as TokioMutex;

/// Shared application context available to all tool handlers.
#[derive(Clone)]
pub struct McpContext {
    pub db: Arc<TokioMutex<Db>>,
    pub config: Arc<Config>,
    pub embedder: Arc<dyn Embedder>,
    pub chunk_size: usize,
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
        log::info!("Starting MCP server on stdio...");
        let (stdin, stdout) = stdio();

        let app_tools = AppTools::new(self.ctx.clone());
        let router = Router::new(app_tools.clone()).with_tools(app_tools.tool_router.clone());

        router
            .serve((stdin, stdout))
            .await
            .context("MCP Server encountered an error during stdio transport")?;

        Ok(())
    }
}
