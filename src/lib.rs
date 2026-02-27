//! # RustRAG — Local RAG MCP Server
//!
//! High-performance local Retrieval-Augmented Generation server that indexes
//! documents and code, then serves relevant context to AI assistants via the
//! Model Context Protocol (MCP).
//!
//! ## Architecture
//!
//! - **[`config`]** — Configuration loading, validation, and pattern expansion
//! - **[`db`]** — SQLite + sqlite-vec vector database (CRUD, search, relations)
//! - **[`embedder`]** — Text embedding via ONNX Runtime (multilingual-e5-small)
//! - **[`indexer`]** — Markdown chunking, Tree-sitter code parsing, dictionary extraction
//! - **[`mcp`]** — MCP server with 10 tool handlers (stdio transport via rmcp)
//! - **[`frontmatter`]** — YAML frontmatter read/write for markdown files
//! - **[`updater`]** — Version update checker (GitHub API + 24h cache)

pub mod config;
pub mod db;
pub mod embedder;
pub mod frontmatter;
pub mod indexer;
pub mod mcp;
pub mod updater;
