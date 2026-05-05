# RustRAG

[![License: MIT/Apache-2.0](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue.svg)](LICENSE-MIT)
[![CI](https://github.com/Michaol/RustRAG/actions/workflows/ci.yml/badge.svg)](https://github.com/Michaol/RustRAG/actions/workflows/ci.yml)
[![Rust](https://img.shields.io/badge/rust-1.85%2B-orange.svg)](https://www.rust-lang.org)

[🇨🇳 中文文档](README_ZH.md)

A high-performance local RAG (Retrieval-Augmented Generation) MCP Server written in Rust.

> **40× token reduction** — indexes your codebase locally, retrieves only the most relevant context for AI assistants.

---

## Latest Release (v2.4.3)

v2.4.3 fixes the sqlite-vec auto-extension registration order — `sqlite3_auto_extension` is now called **before** `Connection::open()`, ensuring the extension is loaded on every connection in the r2d2 pool.

- **Fix sqlite-vec init order**: Auto-extension must be registered before the connection is opened; SQLite only applies auto-extensions to connections created after registration.
- **Note**: Requires v2.4.1+ (`sqlite-vec 0.1.9`).

---

<details>
<summary><b>Expand to view History (v2.4.2 and prior)</b></summary>

### v2.4.2 File Watcher Fix

v2.4.2 fixes a bug in the background file watcher where ignored directories (like `target` or `node_modules`) were still being indexed during hot-reloads despite being listed in `exclude_patterns`.

- **Watcher Exclude Patterns**: The file watcher now fully respects `exclude_patterns` using the `ignore` crate's `OverrideBuilder`, preventing unnecessary indexing of dynamically generated files.

### v2.4.1 sqlite-vec Upgrade

v2.4.1 is a maintenance release upgrading `sqlite-vec` from `0.1.7-alpha.10` to the stable `0.1.9` release, fixing a runtime error where the `vec_version()` function was not found on some platforms.

- **Upgrade sqlite-vec to 0.1.9**: Resolves `no such function: vec_version` errors caused by the alpha pre-release build of the vector extension.
- **Note**: If upgrading from v2.4.0 or earlier, delete the existing `vectors.db` file and restart to re-initialize the database schema.

### v2.4.0 Multi-Format Document Support

v2.4.0 adds multi-format document support, expanding RustRAG from code-only indexing to a universal document RAG engine:

- **Multi-Format Document Support**: Index plain text (`.txt`, `.log`), structured data (`.json`, `.yaml`, `.yml`, `.toml`, `.csv`), HTML (`.html`, `.htm`), PDF (`.pdf`), Word (`.docx`), and spreadsheets (`.xls`, `.xlsx`, `.xlsb`, `.ods`).
- **Format-Specific Chunking**: Each format uses a tailored extraction and chunking strategy that preserves structural information (JSON key paths, CSV headers, spreadsheet sheet names, etc.).
- **Configurable Extensions**: All 24 supported file types are enabled by default in `config.json`. Users can remove extensions to filter unwanted formats. Config hot-reload is fully supported.
- **New Dependencies** (all pure Rust, no C bindings): `lopdf` (PDF), `docx-rs` (DOCX), `calamine` (XLS/XLSX/ODS), `scraper` (HTML), `toml`, `csv`.
- **Also**: Added `.jsx`/`.tsx` to supported code extensions (Tree-sitter already supported them).
<br>

### v2.3.0 Security & Code Quality

v2.3.0 is a security and code quality hardening release, addressing 26 issues found through systematic code review:

- **Security**: Fixed path validation on Windows, restricted arbitrary file reads via MCP tools, bound HTTP server to localhost by default.
- **Reliability**: Replaced production `assert_eq!` panics with proper error propagation, fixed indexer counter logic, wrapped blocking downloads in `spawn_blocking`.
- **Config**: Invalid JSON now returns an error instead of silently falling back to defaults; vector dimension is validated against the sqlite-vec schema at startup.
- **Internationalization**: Language detection now recognizes Japanese (Hiragana/Katakana) and Korean (Hangul); YAML frontmatter properly escapes special characters.
- **Performance**: ONNX thread count auto-detects via `available_parallelism()`; `LanguageConfig` cached with `LazyLock`; `build_dictionary` limits iteration to 100 documents by default.
- **Code Quality**: Removed dead PHP code paths, fixed TOCTOU race in file watcher, added `// SAFETY:` documentation for unsafe blocks.

### v2.2.0 Architecture Refactor

v2.2.0 introduces a major architecture refactor focusing on high concurrency and asynchronous reliability:

- **Database Connection Pooling**: Integrated `r2d2` with `sqlite-vec` to enable safe, multi-threaded database access.
- **Async Networking**: Migrated the update checker from `reqwest::blocking` to native async `reqwest` to eliminate Tokio thread starvation.
- **Config Safety**: Resolved TOCTOU (Time-of-check to time-of-use) race conditions in configuration loading for improved reliability.
- **Performance**: Optimized lazy initialization of the ONNX embedder and improved internal error bubbling.

### v2.1.0 Advanced Improvements

v2.1.0 introduced advanced features and improvements to enhance performance, reliability, and developer experience:

- **New Features**: Enhanced functionality and improved user experience.
- **Performance Optimizations**: Faster processing and reduced resource usage.
- **Stability Improvements**: Enhanced reliability and bug fixes.

### v2.0.0 Migration from ONNX Model

v2.0.0 migrates the embedding model from `model.onnx` (470MB) to the official `model_O4.onnx` (235MB) provided by HuggingFace, halving both file size and runtime memory:

- **ONNX O4 Graph-Optimized Model**: Uses the pre-optimized ONNX Graph Optimization Level 4 model. Vector output is identical to the original — existing databases are 100% compatible with no re-indexing required.
- **Model Size Halved**: Download size reduced from ~470MB to ~235MB, runtime memory from ~500MB to ~250MB.
- **Automatic Migration Cleanup**: Users with existing `model.onnx` files will have the old model automatically detected and removed on startup.

### v1.3.7 Config Hot-Reload

v1.3.7 introduced a native hot-reloading mechanism for configurations and model instances via `RwLock`:

- **GPU Inference Engine Hot-Reloading**: The core model execution environment is now decoupled using read-write locks (`RwLock`). Modifying hardware strategies (`device`) or parameters in `config.json` will automatically release the previous ONNX inference graph and reinitialize it with the new settings on the next request, requiring no service restart.
- **Dynamic Config & Watcher Sync**: The system now monitors `config.json` for changes. Any modification immediately reloads the configuration and adjusts the background file-watching processes in real-time according to updated inclusion/exclusion filtering rules.

### v1.3.6 Hardware Acceleration Update

- **Multi-Platform GPU Acceleration**: Supports native CUDA, TensorRT, DirectML, and CoreML dynamic library loading across platforms, featuring an intelligent fallback to CPU.
- **Configuration & Fault Tolerance**: `config.json` supports custom Embedder `batch_size` and toggling `compute.fallback_to_cpu` mode to prevent hardware initialization failures from causing panics.
- **Real-Time File Watching**: Integrated native background filesystem events. Modifications to tracked directories trigger incremental background synchronization.
- **SQLite WAL Mode**: The SQLite vector storage enables Write-Ahead Logging by default, preventing `database is locked` contention during concurrent operations.
- **Granular MCP Error Reporting**: Revamped error handling to propagate localized exceptions directly to the client logs.

### v1.2.0 & v1.1.0 Performance & Compression Update

- **INT8 Scalar Quantization**: Redesigned the DB virtual table replacing `FLOAT[384]` with `INT8[384]`. This achieved a 75% vector storage size reduction without noticeable recall degradation.
- **ONNX Level 3 Graph Optimization**: Upgraded the ONNX inference session builder to fully support Level 3 Graph Optimization, improving pure CPU inference performance.
- **Automated Cascade Cleanup**: Changing filter patterns (`exclude_patterns`) prompts the system to purge stale documents upon the next index update; deleting physical files also automatically cleans up corresponding records in the database.

> ⚠️ **Data Compatibility Note**: If upgrading from v1.1.x, please manually remove the existing `vectors.db` file to initialize the new INT8 schema DB.

</details>

---

## Features

- **7 MCP Tools** — search, index, list_documents, manage_document, frontmatter, search_relations, build_dictionary
- **24 Supported Formats** — Code (Rust, Go, Python, TypeScript, JavaScript), Markdown, plain text, structured data (JSON, YAML, TOML, CSV), HTML, PDF, DOCX, spreadsheets (XLS, XLSX, XLSB, ODS)
- **Vector Search** — SQLite + sqlite-vec for fast local vector similarity search
- **Code Intelligence** — Tree-sitter AST parsing for Rust, Go, Python, TypeScript, JavaScript
- **Multilingual Dictionary** — CJK↔English symbol mapping extraction
- **High Concurrency & Stability** — Asynchronous non-blocking background syncing (`Arc<TokioMutex>`) with robust pagination to safeguard against MCP stdio transport buffer limits (zero EOF dropouts) for 10k+ files.
- **Auto Model Download** — Automatically downloads `multilingual-e5-small` ONNX model
- **Cross-Platform** — macOS (Intel/ARM), Linux (x64/ARM64), Windows (x64)

## Quick Start

### 1. Install

Download the latest release package for your platform from [Releases](https://github.com/Michaol/RustRAG/releases):

| Platform            | Package Example                      |
| ------------------- | ------------------------------------ |
| Windows x64         | `rustrag-windows-x64.exe.zip`        |
| macOS Apple Silicon | `rustrag-macos-apple-silicon.tar.gz` |
| Linux x64           | `rustrag-linux-x64.tar.gz`           |
| Linux ARM64         | `rustrag-linux-arm64.tar.gz`         |

**Installation Steps:**

#### Windows

```powershell
# Extract to a permanent directory
Expand-Archive rustrag-windows-x64.zip -DestinationPath "$env:LOCALAPPDATA\RustRAG"
```

> ⚠️ **IMPORTANT**: Keep `rustrag.exe` in the same directory as the accompanying `.dll` files (e.g., `onnxruntime.dll`). Do **not** move the exe individually — the ONNX Runtime libraries must remain alongside it.

#### macOS

```bash
mkdir -p ~/rustrag && tar xzf rustrag-macos-apple-silicon.tar.gz -C ~/rustrag
chmod +x ~/rustrag/rustrag
```

#### Linux

```bash
mkdir -p ~/rustrag && tar xzf rustrag-linux-x64.tar.gz -C ~/rustrag
chmod +x ~/rustrag/rustrag
```

After extraction, use the **absolute path** to the `rustrag` binary when configuring your IDE MCP settings.

Alternatively, you can build from source:

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
  "exclude_patterns": ["**/node_modules/**", "**/target/**", "**/.git/**"],
  "file_extensions": [
    "md", "rs", "go", "py", "js", "ts", "jsx", "tsx",
    "txt", "log",
    "json", "yaml", "yml", "toml", "csv",
    "html", "htm",
    "pdf", "docx", "xls", "xlsx", "xlsb", "ods"
  ],
  "db_path": "./vectors.db",
  "chunk_size": 500,
  "search_top_k": 5,
  "compute": {
    "device": "auto",
    "fallback_to_cpu": true
  },
  "model": {
    "name": "multilingual-e5-small",
    "dimensions": 384,
    "batch_size": 32
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

#### 🌩️ Advanced: Remote Installation, Local Invocation (SSH Mode)

If your massive codebases, dev environments, and model weights reside on a remote high-performance server (or local NAS) while you code on a lightweight laptop, you can install RustRAG remotely and **mount it seamlessly over SSH**. Since MCP uses standard streams (stdio), SSH easily pipes it to your local IDE!

**Authentication Requirements (Important):**
MCP clients (like Cursor or Claude Desktop) run the processes silently in the background and **cannot prompt you for a password**. Therefore, non-interactive login must be configured:

- 🔑 **Option 1: SSH Keys (Highly Recommended, Native Cross-Platform)**
  Generate a key pair on your local machine (`ssh-keygen -t ed25519`) and push it to the remote (`ssh-copy-id user@ip`) for secure, passwordless mounting. Works natively on Windows, macOS, and Linux.
- 🔓 **Option 2: `sshpass` (Password-based, Linux/macOS Only)**
  If you must use a password, replace the `command` with `sshpass` (e.g., `args: ["-p", "YOUR_PASSWORD", "ssh", "user@ip", ...]`). **Note**: `sshpass` is easily available on Linux and macOS (via `brew install sshpass`), but extremely difficult to install natively on Windows. Windows users should strictly stick to Option 1.

**Configuration Example (Native SSH setup):**

```json
{
  "mcpServers": {
    "rustrag-remote": {
      "command": "ssh",
      "args": [
        "user@remote.server.ip", // Replace with your remote host
        "/absolute/path/to/rustrag", // Remote path to rustrag binary
        "--config",
        "/remote/project/config.json" // Remote path to config
      ]
    }
  }
}
```

This setup grants your local AI assistant instantaneous insight into millions of lines of remote code with absolutely zero CPU or memory footprint on your local machine.

#### 💻 Advanced: Unlock Local GPU Acceleration (CUDA / TensorRT)

To keep the repository footprint minimal and ensure out-of-the-box compatibility for all users on any platform (specifically Apple Silicon Macs or laptops without discrete GPUs), RustRAG defaults to a lightweight **CPU-only Mode** (`fallback_to_cpu: true`). However, if you possess a dedicated NVIDIA GPU (e.g. RTX 30/40 series) and desire microsecond-level vector search throughput, you can effortlessly unlock TensorRT/CUDA acceleration:

1. **Download Official GPU Runtimes**
   Navigate to the [ONNX Runtime v1.25.1 Release Page](https://github.com/microsoft/onnxruntime/releases/tag/v1.25.1) and download the appropriate OS GPU package (approx 300+MB):

- **Windows:** Download `onnxruntime-win-x64-gpu-1.25.1.zip`
- **Linux:** Download `onnxruntime-linux-x64-gpu-1.25.1.tgz`
- **macOS:** Apple Silicon Macs run natively fast on CPU with CoreML support. Do not download the Nvidia packages.

2. **Setup the Dynamic Libraries**
   Extract the archive and drop all the `.dll` (for Windows) or `.so` (for Linux) files (e.g., `onnxruntime.dll`, `libonnxruntime_providers_cuda.so`) precisely **into the same directory of your `rustrag` backend executable binary**.

3. **Enable Auto-Detection**
   Open your project configuration (`config.json`) and ensure:

```json
"compute": {
  "device": "auto", // <-- Will auto-seek TensorRT, then CUDA, DML/CoreML, etc.
  "fallback_to_cpu": true // <-- Safety net to quietly fallback to CPU if GPU dlls are missing
}
```

If the requirements are met, upon startup the MCP log will confidently announce `🚀 ONNX Execution Provider Activated: [TensorRT]` or `[CUDA]`. **This configuration is entirely isolated to your execution folder; it will never pollute the core project repository!**

## CLI Options

| Flag              | Default       | Description                             |
| ----------------- | ------------- | --------------------------------------- |
| `--config`, `-c`  | `config.json` | Path to configuration file              |
| `--log-level`     | `info`        | Log level (trace/debug/info/warn/error) |
| `--skip-download` | false         | Skip automatic model download           |
| `--skip-sync`     | false         | Skip initial document sync              |
| `--transport`     | `stdio`       | Transport mode: `stdio` or `http`       |
| `--port`          | `8765`        | HTTP port (used if transport=`http`)    |
| `--version`       | —             | Display version and exit                |

## MCP Tools

| Tool               | Description                                                             |
| ------------------ | ----------------------------------------------------------------------- |
| `search`           | Natural language vector search with optional directory/filename filters |
| `index`            | Index markdown or code files using logical AST chunking & abstraction   |
| `manage_document`  | Remove a document from the index or force re-index an existing one      |
| `list_documents`   | List all indexed documents                                              |
| `frontmatter`      | Add or update YAML frontmatter metadata to a markdown file              |
| `search_relations` | Search code relationships (calls, imports, inherits)                    |
| `build_dictionary` | Extract CJK↔English term mappings from code                             |

## Architecture

```
src/
├── lib.rs # Module exports
├── main.rs # CLI + startup sequence
├── config.rs # Configuration loading/validation
├── frontmatter.rs # YAML frontmatter operations
├── updater.rs # Version update checker (GitHub API)
├── db/ # SQLite + sqlite-vec vector database
│   ├── mod.rs # Schema + connection management
│   ├── models.rs # Data models
│   ├── documents.rs # Document CRUD operations
│   ├── search.rs # Vector similarity search
│   └── relations.rs # Code relationship queries
├── embedder/ # Text embedding engine
│   ├── mod.rs # Embedder trait
│   ├── onnx.rs # ONNX Runtime inference
│   ├── mock.rs # Mock embedder (testing)
│   ├── tokenizer.rs # BERT tokenizer wrapper
│   └── download.rs # Model auto-download
├── indexer/ # Document & code indexing
│   ├── core.rs # Differential sync engine
│   ├── markdown.rs # Markdown chunking
│   ├── text_parser.rs # Multi-format document extraction (PDF, DOCX, XLSX, etc.)
│   ├── code_parser.rs # Tree-sitter code parsing
│   ├── relations.rs # Code relationship extraction
│   ├── dictionary.rs # Multilingual dictionary
│   └── languages.rs # Language-specific TS queries
└── mcp/ # MCP protocol layer
    ├── server.rs # Server setup (stdio + HTTP transport)
    └── tools.rs # 7 tool handler implementations
```

## Supported Languages

| Language   | Extension | Parser                 |
| ---------- | --------- | ---------------------- |
| Rust       | `.rs`     | tree-sitter-rust       |
| Go         | `.go`     | tree-sitter-go         |
| Python     | `.py`     | tree-sitter-python     |
| TypeScript | `.ts` `.tsx` | tree-sitter-typescript |
| JavaScript | `.js` `.jsx` | tree-sitter-javascript |
| Markdown   | `.md`     | pulldown-cmark         |

## Supported Document Formats

| Format              | Extensions                         | Parser / Library          |
| ------------------- | ---------------------------------- | ------------------------- |
| Plain Text          | `.txt`, `.log`                     | `fs::read_to_string`      |
| JSON                | `.json`                            | `serde_json`              |
| YAML                | `.yaml`, `.yml`                    | `serde_yaml`              |
| TOML                | `.toml`                            | `toml`                    |
| CSV                 | `.csv`                             | `csv`                     |
| HTML                | `.html`, `.htm`                    | `scraper`                 |
| PDF                 | `.pdf`                             | `lopdf`                   |
| Word                | `.docx`                            | `docx-rs`                 |
| Spreadsheet         | `.xls`, `.xlsx`, `.xlsb`, `.ods`   | `calamine`                |

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
