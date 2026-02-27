# RustRAG 实施计划

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** 将 DevRag (Go) 完整移植为 Rust 实现的本地 RAG MCP Server，并建立上游跟踪 CI/CD。

**Architecture:** 单 crate 项目，异步运行时使用 Tokio，MCP 通信使用 `rmcp` 的 stdio transport。ONNX 推理通过 `ort` crate 零成本 FFI 调用，向量存储使用 `rusqlite` + `sqlite-vec`，AST 解析使用 Tree-sitter 官方 Rust crate。

**Tech Stack:** Rust 1.75+ / rmcp / ort / rusqlite / sqlite-vec / tree-sitter / tokenizers / pulldown-cmark / tokio / serde / clap / reqwest

---

## 阶段 1：基础脚手架

### Task 1：项目初始化 + Cargo.toml

**Files:**

- Create: `E:\DEV\RustRAG\Cargo.toml`
- Create: `E:\DEV\RustRAG\src\main.rs`
- Create: `E:\DEV\RustRAG\.gitignore`

**Step 1: 初始化 Cargo 项目**

```bash
cd E:\DEV\RustRAG
cargo init --name rustrag .
```

**Step 2: 编写 Cargo.toml**

```toml
[package]
name = "rustrag"
version = "0.1.0"
edition = "2021"
rust-version = "1.75"
description = "本地 RAG MCP Server - DevRag 的 Rust 实现"

[dependencies]
# MCP 协议
rmcp = { version = "0.1", features = ["server", "macros", "transport-io"] }
tokio = { version = "1", features = ["full"] }

# ONNX 推理
ort = { version = "2", features = ["download-binaries"] }
ndarray = "0.16"

# SQLite + 向量搜索
rusqlite = { version = "0.32", features = ["bundled"] }
sqlite-vec = "0.1"

# Tokenizer
tokenizers = "0.20"

# AST 解析
tree-sitter = "0.24"
tree-sitter-go = "0.23"
tree-sitter-python = "0.23"
tree-sitter-typescript = "0.23"
tree-sitter-javascript = "0.23"

# Markdown
pulldown-cmark = "0.12"

# 序列化
serde = { version = "1", features = ["derive"] }
serde_json = "1"
serde_yaml = "0.9"

# HTTP
reqwest = { version = "0.12", features = ["blocking", "stream"] }
indicatif = "0.17"

# CLI
clap = { version = "4", features = ["derive"] }

# 工具
bytemuck = { version = "1", features = ["derive"] }
glob = "0.3"
thiserror = "2"
anyhow = "1"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
chrono = { version = "0.4", features = ["serde"] }
schemars = "0.8"

[profile.release]
lto = true
strip = true
opt-level = "z"
```

**Step 3: 编写最小 main.rs**

```rust
fn main() {
    println!("RustRAG v0.1.0");
}
```

**Step 4: 验证编译**

Run: `cargo build`
Expected: 编译成功（首次编译依赖较慢，约 3-5 分钟）

**Step 5: 提交**

```bash
git init
git add .
git commit -m "feat: 项目初始化 + Cargo.toml 依赖定义"
```

---

### Task 2：config 模块

**Files:**

- Create: `E:\DEV\RustRAG\src\config.rs`
- Modify: `E:\DEV\RustRAG\src\main.rs`

**参考源码:** [Go config.go](file:///E:/DEV/devrag/internal/config/config.go)

**Step 1: 编写 config.rs**

实现 `Config` struct 及其加载/验证逻辑。使用 `serde` 反序列化 `config.json`：

```rust
use serde::Deserialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    #[serde(default = "default_document_patterns")]
    pub document_patterns: Vec<String>,
    #[serde(default = "default_db_path")]
    pub db_path: String,
    #[serde(default = "default_chunk_size")]
    pub chunk_size: usize,
    #[serde(default = "default_search_top_k")]
    pub search_top_k: usize,
    #[serde(default)]
    pub compute: ComputeConfig,
    #[serde(default)]
    pub model: ModelConfig,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ComputeConfig {
    #[serde(default = "default_device")]
    pub device: String,
    #[serde(default = "default_true")]
    pub fallback_to_cpu: bool,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ModelConfig {
    #[serde(default = "default_model_name")]
    pub name: String,
    #[serde(default = "default_dimensions")]
    pub dimensions: usize,
}

// 默认值函数...
// Config::load(path) -> Result<Config>
// Config::validate(&self) -> Result<()>
// Config::get_document_files(&self) -> Result<Vec<PathBuf>>
// Config::get_base_directories(&self) -> Vec<PathBuf>
```

**Step 2: 编写测试**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert_eq!(config.chunk_size, 500);
        assert_eq!(config.search_top_k, 5);
        assert_eq!(config.model.dimensions, 384);
    }

    #[test]
    fn test_load_from_json() {
        let json = r#"{"chunk_size": 1000, "db_path": "./test.db"}"#;
        let config: Config = serde_json::from_str(json).unwrap();
        assert_eq!(config.chunk_size, 1000);
    }
}
```

**Step 3: 运行测试**

Run: `cargo test config`
Expected: PASS

**Step 4: 提交**

```bash
git add src/config.rs src/main.rs
git commit -m "feat: config 模块 - 配置加载/验证/默认值"
```

---

### Task 3：Embedder trait + MockEmbedder

**Files:**

- Create: `E:\DEV\RustRAG\src\embedder\mod.rs`
- Create: `E:\DEV\RustRAG\src\embedder\mock.rs`

**参考源码:** [Go embedder.go](file:///E:/DEV/devrag/internal/embedder/embedder.go)

**Step 1: 定义 Embedder trait**

```rust
// src/embedder/mod.rs
pub mod mock;

use thiserror::Error;

#[derive(Error, Debug)]
pub enum EmbedderError {
    #[error("推理失败: {0}")]
    InferenceFailed(String),
    #[error("模型加载失败: {0}")]
    ModelLoadFailed(String),
}

pub trait Embedder: Send + Sync {
    fn embed(&self, text: &str) -> Result<Vec<f32>, EmbedderError>;
    fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, EmbedderError>;
    fn dimensions(&self) -> usize;
}
```

**Step 2: 实现 MockEmbedder**

```rust
// src/embedder/mock.rs
pub struct MockEmbedder {
    pub dimensions: usize,
}

impl Embedder for MockEmbedder {
    fn embed(&self, _text: &str) -> Result<Vec<f32>, EmbedderError> {
        Ok(vec![0.1; self.dimensions])
    }
    // ... embed_batch, dimensions
}
```

**Step 3: 运行测试**

Run: `cargo test embedder`
Expected: PASS

**Step 4: 提交**

```bash
git add src/embedder/
git commit -m "feat: Embedder trait + MockEmbedder"
```

---

### Task 4：模型自动下载

**Files:**

- Create: `E:\DEV\RustRAG\src\embedder\download.rs`

**参考源码:** [Go download.go](file:///E:/DEV/devrag/internal/embedder/download.go)

**Step 1: 实现下载逻辑**

使用 `reqwest::blocking` + `indicatif` 进度条，从 HuggingFace 下载 5 个文件（model.onnx、tokenizer.json 等）。逻辑与 Go 版本 1:1 对等。

**Step 2: 编写测试**（仅测试文件检测逻辑，不测试实际下载）

**Step 3: 运行测试**

Run: `cargo test download`
Expected: PASS

**Step 4: 提交**

```bash
git add src/embedder/download.rs
git commit -m "feat: 模型自动下载（reqwest + indicatif 进度条）"
```

---

### Task 5：BERT Tokenizer 封装

**Files:**

- Create: `E:\DEV\RustRAG\src\embedder\tokenizer.rs`

**参考源码:** [Go tokenizer.go](file:///E:/DEV/devrag/internal/embedder/tokenizer.go)

**Step 1: 封装 HuggingFace tokenizers crate**

```rust
use tokenizers::Tokenizer;

pub struct BertTokenizer {
    inner: Tokenizer,
    max_length: usize,
}

impl BertTokenizer {
    pub fn from_model_dir(dir: &Path) -> Result<Self> { ... }
    pub fn tokenize(&self, text: &str) -> Result<(Vec<i64>, Vec<i64>)> { ... }
    pub fn vocab_size(&self) -> usize { ... }
}
```

**Step 2: 测试**

Run: `cargo test tokenizer`
Expected: PASS

**Step 3: 提交**

```bash
git add src/embedder/tokenizer.rs
git commit -m "feat: BERT tokenizer 封装（HuggingFace tokenizers crate）"
```

---

### Task 6：ONNX 推理 + Mean Pooling

**Files:**

- Create: `E:\DEV\RustRAG\src\embedder\onnx.rs`

**参考源码:** [Go onnx.go](file:///E:/DEV/devrag/internal/embedder/onnx.go)

**Step 1: 实现 OnnxEmbedder**

使用 `ort` crate 加载 ONNX 模型，实现推理 → mean pooling → L2 normalize 全流程。用 `ndarray` 替代 Go 中手写的 meanPooling/normalize 函数。

```rust
pub struct OnnxEmbedder {
    session: ort::Session,
    tokenizer: BertTokenizer,
    dimensions: usize,
}

impl Embedder for OnnxEmbedder {
    fn embed(&self, text: &str) -> Result<Vec<f32>, EmbedderError> {
        let (input_ids, attention_mask) = self.tokenizer.tokenize(text)?;
        // ort Session::run → ndarray mean_pooling → normalize
    }
}
```

**Step 2: 集成测试**（需要模型文件，标记为 `#[ignore]`）

**Step 3: 运行测试**

Run: `cargo test embedder -- --ignored`（需要先下载模型）
Expected: PASS

**Step 4: 提交**

```bash
git add src/embedder/onnx.rs
git commit -m "feat: ONNX 推理引擎（ort + ndarray mean pooling）"
```

---

## 阶段 2：向量数据库

### Task 7：SQLite 初始化 + Schema

**Files:**

- Create: `E:\DEV\RustRAG\src\vectordb\mod.rs`
- Create: `E:\DEV\RustRAG\src\vectordb\schema.rs`

**参考源码:** [Go sqlite.go](file:///E:/DEV/devrag/internal/vectordb/sqlite.go)，[Go schema.go](file:///E:/DEV/devrag/internal/vectordb/schema.go)

**Step 1: 实现 DB struct 和建表**

```rust
pub struct DB {
    conn: rusqlite::Connection,
}

impl DB {
    pub fn init(db_path: &str) -> Result<Self> {
        let conn = Connection::open(db_path)?;
        sqlite_vec::load(&conn)?;
        // CREATE TABLE documents, chunks, vec_chunks, code_metadata, code_relations, word_mapping
        Ok(Self { conn })
    }
}
```

**Step 2: 测试**

Run: `cargo test vectordb::schema`
Expected: PASS（使用 `:memory:` 数据库）

**Step 3: 提交**

```bash
git add src/vectordb/
git commit -m "feat: SQLite 初始化 + schema 建表（rusqlite + sqlite-vec）"
```

---

### Task 8：文档 CRUD 操作

**Files:**

- Create: `E:\DEV\RustRAG\src\vectordb\operations.rs`

**参考源码:** [Go db.go](file:///E:/DEV/devrag/internal/vectordb/db.go)

**Step 1: 实现** ListDocuments / InsertDocument / DeleteDocument / InsertCodeDocument / InsertRelations / 词典操作等全部 CRUD。向量序列化使用 `bytemuck::cast_slice`。

**Step 2: 测试**

Run: `cargo test vectordb::operations`
Expected: PASS

**Step 3: 提交**

```bash
git add src/vectordb/operations.rs
git commit -m "feat: 文档 CRUD（insert/delete/list + bytemuck 向量序列化）"
```

---

### Task 9：向量搜索

**Files:**

- Create: `E:\DEV\RustRAG\src\vectordb\search.rs`

**参考源码:** [Go search.go](file:///E:/DEV/devrag/internal/vectordb/search.go)

**Step 1: 实现** `search` / `search_with_filter` / `search_symbols_by_keywords` / `find_symbol_relations`。使用 `vec_distance_cosine()` SQL 函数。

**Step 2: 测试**（使用 MockEmbedder 生成的固定向量）

Run: `cargo test vectordb::search`
Expected: PASS

**Step 3: 提交**

```bash
git add src/vectordb/search.rs
git commit -m "feat: 向量搜索（cosine similarity + filter + 关键词搜索）"
```

---

## 阶段 3：索引器

### Task 10：Markdown 解析 + 分块

**Files:**

- Create: `E:\DEV\RustRAG\src\indexer\mod.rs`
- Create: `E:\DEV\RustRAG\src\indexer\markdown.rs`

**参考源码:** [Go markdown.go](file:///E:/DEV/devrag/internal/indexer/markdown.go)

**Step 1: 实现** `parse_markdown()` 和 `split_into_chunks()`。使用 `pulldown-cmark` 解析 Markdown，按段落边界分块，尊重 `chunk_size` 配置。

**Step 2: 测试**

Run: `cargo test indexer::markdown`
Expected: PASS

**Step 3: 提交**

```bash
git add src/indexer/
git commit -m "feat: Markdown 解析 + 分块（pulldown-cmark）"
```

---

### Task 11：差异同步

**Files:**

- Create: `E:\DEV\RustRAG\src\indexer\sync.rs`

**参考源码:** [Go sync.go](file:///E:/DEV/devrag/internal/indexer/sync.go)

**Step 1: 实现** `Indexer::sync()` → 检测新增/更新/删除文件，自动索引/删除。逻辑与 Go 版本的 131 行完全对等。

**Step 2: 测试**（使用临时目录和 MockEmbedder）

Run: `cargo test indexer::sync`
Expected: PASS

**Step 3: 提交**

```bash
git add src/indexer/sync.rs
git commit -m "feat: 差异同步（检测新增/更新/删除 + 自动索引）"
```

---

### Task 12：Tree-sitter 代码解析

**Files:**

- Create: `E:\DEV\RustRAG\src\indexer\code.rs`
- Create: `E:\DEV\RustRAG\src\indexer\chunk.rs`
- Create: `E:\DEV\RustRAG\src\indexer\languages.rs`

**参考源码:** [Go code.go](file:///E:/DEV/devrag/internal/indexer/code.go)，[Go languages.go](file:///E:/DEV/devrag/internal/indexer/languages.go)

**Step 1: 实现** `CodeParser` struct + `parse_file()` + `extract_symbols()`。使用 Tree-sitter 官方 Rust crate + 各语言 grammar crate。

**Step 2: 使用 `E:\DEV\devrag\test_data\` 中的测试数据验证**

Run: `cargo test indexer::code`
Expected: PASS

**Step 3: 提交**

```bash
git add src/indexer/code.rs src/indexer/chunk.rs src/indexer/languages.rs
git commit -m "feat: Tree-sitter 代码解析（Go/Python/TS/JS）"
```

---

### Task 13：代码关系提取

**Files:**

- Create: `E:\DEV\RustRAG\src\indexer\relations.rs`

**参考源码:** [Go relations.go](file:///E:/DEV/devrag/internal/indexer/relations.go)

**Step 1: 实现** `RelationExtractor` → 提取 calls/imports/inherits 关系。

**Step 2: 测试**

Run: `cargo test indexer::relations`
Expected: PASS

**Step 3: 提交**

```bash
git add src/indexer/relations.rs
git commit -m "feat: 代码关系提取（calls/imports/inherits）"
```

---

### Task 14：多语言词典

**Files:**

- Create: `E:\DEV\RustRAG\src\indexer\dictionary.rs`

**参考源码:** `tools.go` 中的 `autoBuildDictionary` / `handleBuildDictionary`

**Step 1: 实现** `DictionaryExtractor` + `extract_from_content()`，日语→英语词汇映射提取。

**Step 2: 测试**

Run: `cargo test indexer::dictionary`
Expected: PASS

**Step 3: 提交**

```bash
git add src/indexer/dictionary.rs
git commit -m "feat: 多语言词典抽取（日语→英语映射）"
```

---

## 阶段 4：MCP 层 + 主程序

### Task 15：MCP Server 初始化

**Files:**

- Create: `E:\DEV\RustRAG\src\mcp\mod.rs`

**参考源码:** [Go server.go](file:///E:/DEV/devrag/internal/mcp/server.go)

**Step 1: 实现** `RustRAGServer` struct，使用 `rmcp` crate 的 `#[tool_box]` 宏注册 tool handler，stdio transport。

```rust
use rmcp::{ServerHandler, tool, tool_box};

pub struct RustRAGServer {
    indexer: Arc<Indexer>,
    db: Arc<DB>,
    embedder: Arc<dyn Embedder>,
    config: Config,
}

#[tool_box]
impl RustRAGServer {
    // tools registered here
}

impl ServerHandler for RustRAGServer { ... }
```

**Step 2: 编译验证**

Run: `cargo build`
Expected: 编译成功

**Step 3: 提交**

```bash
git add src/mcp/
git commit -m "feat: MCP Server 初始化（rmcp + stdio transport）"
```

---

### Task 16：10 个 Tool Handler

**Files:**

- Create: `E:\DEV\RustRAG\src\mcp\tools.rs`

**参考源码:** [Go tools.go](file:///E:/DEV/devrag/internal/mcp/tools.go)

**Step 1: 逐个实现** 10 个 Tool（search / index_markdown / list_documents / delete_document / reindex_document / add_frontmatter / update_frontmatter / index_code / search_relations / build_dictionary），使用 `#[tool]` 宏注册。

**Step 2: 测试**（使用 MockEmbedder + `:memory:` DB）

Run: `cargo test mcp::tools`
Expected: PASS

**Step 3: 提交**

```bash
git add src/mcp/tools.rs
git commit -m "feat: 10 个 MCP Tool handler"
```

---

### Task 17：main.rs 启动流程

**Files:**

- Modify: `E:\DEV\RustRAG\src\main.rs`

**参考源码:** [Go main.go](file:///E:/DEV/devrag/cmd/main.go)

**Step 1: 实现完整启动序列**

```rust
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // 1. 解析 CLI 参数（clap）
    // 2. 初始化 tracing 日志（stderr）
    // 3. 加载配置
    // 4. 下载模型（如需要）
    // 5. 初始化 DB
    // 6. 初始化 Embedder
    // 7. 差异同步
    // 8. 启动 MCP Server（stdio）
}
```

**Step 2: 端到端测试**

Run: `echo '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}' | cargo run`
Expected: 返回 MCP initialize 响应

**Step 3: 提交**

```bash
git add src/main.rs
git commit -m "feat: main.rs 完整启动流程"
```

---

### Task 18：版本更新检查

**Files:**

- Create: `E:\DEV\RustRAG\src\updater.rs`

**参考源码:** `internal/updater/`

**Step 1: 实现** `check_for_update()` → 使用 GitHub API 检查最新 Release，缓存 24h。

**Step 2: 提交**

```bash
git add src/updater.rs
git commit -m "feat: 版本更新检查（GitHub API + 24h 缓存）"
```

---

## 阶段 5：CI/CD + 上游跟踪

### Task 19：GitHub Actions CI

**Files:**

- Create: `E:\DEV\RustRAG\.github\workflows\ci.yml`

**Step 1: 编写 CI Workflow**

```yaml
name: CI
on: [push, pull_request]
jobs:
  test:
    runs-on: ${{ matrix.os }}
    strategy:
      matrix:
        os: [ubuntu-latest, macos-latest, windows-latest]
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - run: cargo test --all
      - run: cargo clippy -- -D warnings
```

**Step 2: 提交**

```bash
git add .github/
git commit -m "ci: GitHub Actions CI（test + clippy）"
```

---

### Task 20：多平台 Release Workflow

**Files:**

- Create: `E:\DEV\RustRAG\.github\workflows\release.yml`

**Step 1: 编写 Release Workflow**

使用 `cross` 或原生 runner matrix 编译 5 个平台（macOS Intel/ARM, Linux x64/ARM64, Windows x64）。Tag push 触发。

```yaml
name: Release
on:
  push:
    tags: ["v*"]
jobs:
  build:
    runs-on: ${{ matrix.os }}
    strategy:
      matrix:
        include:
          - os: macos-latest
            target: x86_64-apple-darwin
            artifact: rustrag-macos-intel
          - os: macos-latest
            target: aarch64-apple-darwin
            artifact: rustrag-macos-apple-silicon
          - os: ubuntu-latest
            target: x86_64-unknown-linux-gnu
            artifact: rustrag-linux-x64
          - os: ubuntu-latest
            target: aarch64-unknown-linux-gnu
            artifact: rustrag-linux-arm64
          - os: windows-latest
            target: x86_64-pc-windows-msvc
            artifact: rustrag-windows-x64.exe
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          targets: ${{ matrix.target }}
      - run: cargo build --release --target ${{ matrix.target }}
      - uses: softprops/action-gh-release@v1
        with:
          files: target/${{ matrix.target }}/release/rustrag*
```

**Step 2: 提交**

```bash
git add .github/workflows/release.yml
git commit -m "ci: 多平台 Release（5 平台自动编译 + GitHub Release）"
```

---

### Task 21：上游监控工作流（2 周一次）

**Files:**

- Create: `E:\DEV\RustRAG\.github\workflows\upstream-watch.yml`

**Step 1: 编写上游监控 Workflow**

```yaml
name: Upstream Watch
on:
  schedule:
    - cron: "0 0 1,15 * *" # 每月 1 日和 15 日（约 2 周一次）
  workflow_dispatch:

jobs:
  check:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: 检查上游更新
        id: check
        run: |
          UPSTREAM_TAG=$(gh api repos/tomohiro-owada/devrag/releases/latest --jq '.tag_name' 2>/dev/null || echo "none")
          UPSTREAM_SHA=$(gh api repos/tomohiro-owada/devrag/commits/main --jq '.sha[:7]' 2>/dev/null || echo "none")

          # 读取上次跟踪的版本
          LAST_TRACKED=$(cat .upstream-version 2>/dev/null || echo "none")

          echo "upstream_tag=$UPSTREAM_TAG" >> $GITHUB_OUTPUT
          echo "upstream_sha=$UPSTREAM_SHA" >> $GITHUB_OUTPUT
          echo "last_tracked=$LAST_TRACKED" >> $GITHUB_OUTPUT

          if [ "$UPSTREAM_TAG" != "$LAST_TRACKED" ]; then
            echo "has_update=true" >> $GITHUB_OUTPUT
          else
            echo "has_update=false" >> $GITHUB_OUTPUT
          fi
        env:
          GH_TOKEN: ${{ secrets.GITHUB_TOKEN }}

      - name: 获取上游变更摘要
        if: steps.check.outputs.has_update == 'true'
        id: diff
        run: |
          gh api repos/tomohiro-owada/devrag/compare/${{ steps.check.outputs.last_tracked }}...${{ steps.check.outputs.upstream_tag }} \
            --jq '.files[] | "- \(.status): `\(.filename)` (+\(.additions)/-\(.deletions))"' > /tmp/diff_summary.txt 2>/dev/null || echo "无法获取差异" > /tmp/diff_summary.txt
          echo "diff_file=/tmp/diff_summary.txt" >> $GITHUB_OUTPUT
        env:
          GH_TOKEN: ${{ secrets.GITHUB_TOKEN }}

      - name: 创建 Issue
        if: steps.check.outputs.has_update == 'true'
        run: |
          DIFF_CONTENT=$(cat /tmp/diff_summary.txt)
          gh issue create \
            --title "🔄 上游更新: ${{ steps.check.outputs.upstream_tag }}" \
            --body "## 上游 DevRag 发布了新版本

          **版本:** ${{ steps.check.outputs.upstream_tag }}
          **上次跟踪:** ${{ steps.check.outputs.last_tracked }}
          **仓库:** https://github.com/tomohiro-owada/devrag

          ### 变更文件

          ${DIFF_CONTENT}

          ### 操作建议

          1. 查看 [上游 Release Notes](https://github.com/tomohiro-owada/devrag/releases/tag/${{ steps.check.outputs.upstream_tag }})
          2. 评估哪些变更需要移植到 Rust 版本
          3. 完成移植后更新 \`.upstream-version\` 文件
          " \
            --label "upstream-sync"
        env:
          GH_TOKEN: ${{ secrets.GITHUB_TOKEN }}

      - name: 更新跟踪版本
        if: steps.check.outputs.has_update == 'true'
        run: |
          echo "${{ steps.check.outputs.upstream_tag }}" > .upstream-version
          git config user.name "github-actions[bot]"
          git config user.email "github-actions[bot]@users.noreply.github.com"
          git add .upstream-version
          git commit -m "chore: 更新上游跟踪版本到 ${{ steps.check.outputs.upstream_tag }}"
          git push
```

**Step 2: 创建初始版本跟踪文件**

```bash
echo "v1.2.0" > .upstream-version
```

**Step 3: 提交**

```bash
git add .github/workflows/upstream-watch.yml .upstream-version
git commit -m "ci: 上游监控工作流（2 周定时检查 + 自动创建 Issue）"
```

---

## 阶段 6：验证

### Task 22：集成测试

**Files:**

- Create: `E:\DEV\RustRAG\tests\integration_test.rs`

**Step 1:** 编写端到端测试：创建临时 config + 临时目录 → 初始化 DB → 使用 MockEmbedder 索引测试文档 → 执行搜索 → 验证结果。

**Step 2:** 运行：`cargo test --test integration_test`

**Step 3:** 提交。

---

### Task 23：与 Go 版对等测试

**Step 1:** 准备相同的测试文档集。
**Step 2:** 分别用 Go 版和 Rust 版索引同一批文件。
**Step 3:** 对比 `list_documents` 输出的文档数量和名称。
**Step 4:** 对比相同查询的搜索结果排序（注意：由于浮点精度差异，排名可能有微小偏差，但前 3 结果应一致）。

---

## 验证计划总结

| 验证方法        | 命令                                 | 说明              |
| --------------- | ------------------------------------ | ----------------- |
| 单元测试        | `cargo test`                         | 每个模块独立测试  |
| Clippy 静态检查 | `cargo clippy -- -D warnings`        | 无 warning        |
| 集成测试        | `cargo test --test integration_test` | 端到端流程        |
| MCP 协议测试    | `echo '{...}' \| cargo run`          | JSON-RPC 请求响应 |
| 多平台编译      | GitHub Actions matrix                | 5 平台编译通过    |
| 上游跟踪        | 手动触发 `upstream-watch.yml`        | Issue 自动创建    |
