# RustRAG

[![License: MIT/Apache-2.0](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue.svg)](LICENSE-MIT)
[![CI](https://github.com/Michaol/RustRAG/actions/workflows/ci.yml/badge.svg)](https://github.com/Michaol/RustRAG/actions/workflows/ci.yml)
[![Rust](https://img.shields.io/badge/rust-1.85%2B-orange.svg)](https://www.rust-lang.org)

[🇨🇳 中文文档](README_CN.md)

A high-performance local RAG (Retrieval-Augmented Generation) MCP Server written in Rust.

> **40× token reduction** — indexes your codebase locally, retrieves only the most relevant context for AI assistants.

## Features

- **10 MCP Tools** — search, index_markdown, index_code, list_documents, delete_document, reindex_document, add_frontmatter, update_frontmatter, search_relations, build_dictionary
- **Vector Search** — SQLite + sqlite-vec for fast local vector similarity search
- **Code Intelligence** — Tree-sitter AST parsing for Rust, Go, Python, TypeScript, JavaScript
- **Multilingual Dictionary** — CJK↔English symbol mapping extraction
- **Auto Model Download** — Automatically downloads `multilingual-e5-small` ONNX model
- **Cross-Platform** — macOS (Intel/ARM), Linux (x64/ARM64), Windows (x64)

## Quick Start

### 1. Install

Download the latest release for your platform from [Releases](https://github.com/Michaol/RustRAG/releases), or build from source:

```bash
# Clone and build
git clone https://github.com/Michaol/RustRAG.git
cd RustRAG
cargo build --release
```

### 2. Configure

Create a `config.json` in your project root (auto-generated with defaults on first run):

```json
{
  "document_patterns": ["./"],
  "db_path": "./vectors.db",
  "chunk_size": 500,
  "search_top_k": 5,
  "model": {
    "name": "multilingual-e5-small",
    "dimensions": 384
  }
}
```

### 3. Add to MCP Client

#### Antigravity IDE

Add to your `mcp_config.json` (Settings → MCP Servers):

```json
{
  "mcpServers": {
    "rustrag": {
      "command": "path/to/rustrag",
      "args": ["--config", "path/to/config.json"]
    }
  }
}
```

#### Claude Desktop / Cursor

Add to the MCP client configuration file:

```json
{
  "mcpServers": {
    "rustrag": {
      "command": "path/to/rustrag",
      "args": ["--config", "path/to/config.json"]
    }
  }
}
```

## CLI Options

| Flag              | Default       | Description                             |
| ----------------- | ------------- | --------------------------------------- |
| `--config`, `-c`  | `config.json` | Path to configuration file              |
| `--log-level`     | `info`        | Log level (trace/debug/info/warn/error) |
| `--skip-download` | false         | Skip automatic model download           |
| `--skip-sync`     | false         | Skip initial document sync              |
| `--version`       | —             | Display version and exit                |

## MCP Tools

| Tool                 | Description                                                             |
| -------------------- | ----------------------------------------------------------------------- |
| `search`             | Natural language vector search with optional directory/filename filters |
| `index_markdown`     | Index a single markdown file                                            |
| `index_code`         | Index code files using Tree-sitter AST parsing                          |
| `list_documents`     | List all indexed documents                                              |
| `delete_document`    | Remove a document from the index                                        |
| `reindex_document`   | Force re-index a document                                               |
| `add_frontmatter`    | Add YAML frontmatter metadata to a markdown file                        |
| `update_frontmatter` | Update existing frontmatter metadata                                    |
| `search_relations`   | Search code relationships (calls, imports, inherits)                    |
| `build_dictionary`   | Extract CJK↔English term mappings from code                             |

## Architecture

```
src/
├── lib.rs            # Module exports
├── main.rs           # CLI + startup sequence
├── config.rs         # Configuration loading/validation
├── frontmatter.rs    # YAML frontmatter operations
├── updater.rs        # Version update checker (GitHub API)
├── db/               # SQLite + sqlite-vec vector database
│   ├── mod.rs        # Schema + connection management
│   ├── models.rs     # Data models
│   ├── documents.rs  # Document CRUD operations
│   ├── search.rs     # Vector similarity search
│   └── relations.rs  # Code relationship queries
├── embedder/         # Text embedding engine
│   ├── mod.rs        # Embedder trait
│   ├── onnx.rs       # ONNX Runtime inference
│   ├── mock.rs       # Mock embedder (testing)
│   ├── tokenizer.rs  # BERT tokenizer wrapper
│   └── download.rs   # Model auto-download
├── indexer/          # Document & code indexing
│   ├── core.rs       # Differential sync engine
│   ├── markdown.rs   # Markdown chunking
│   ├── code_parser.rs # Tree-sitter code parsing
│   ├── relations.rs  # Code relationship extraction
│   ├── dictionary.rs # Multilingual dictionary
│   └── languages.rs  # Language-specific TS queries
└── mcp/              # MCP protocol layer
    ├── server.rs     # Server setup (stdio transport)
    └── tools.rs      # 10 tool handler implementations
```

## Supported Languages

| Language   | Extension | Parser                 |
| ---------- | --------- | ---------------------- |
| Rust       | `.rs`     | tree-sitter-rust       |
| Go         | `.go`     | tree-sitter-go         |
| Python     | `.py`     | tree-sitter-python     |
| TypeScript | `.ts`     | tree-sitter-typescript |
| JavaScript | `.js`     | tree-sitter-javascript |
| Markdown   | `.md`     | pulldown-cmark         |

## Building from Source

**Prerequisites:** Rust 1.85+

```bash
cargo build --release
```

The binary will be at `target/release/rustrag` (or `rustrag.exe` on Windows).

## Testing

```bash
# Run all tests
cargo test --all

# Run integration tests only
cargo test --test integration_test

# Lint
cargo clippy -- -D warnings
```

## License

Licensed under either of

- [MIT License](LICENSE-MIT)
- [Apache License, Version 2.0](LICENSE-APACHE)

at your option.
