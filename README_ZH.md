# RustRAG

[![License: MIT/Apache-2.0](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue.svg)](LICENSE-MIT)
[![CI](https://github.com/Michaol/RustRAG/actions/workflows/ci.yml/badge.svg)](https://github.com/Michaol/RustRAG/actions/workflows/ci.yml)
[![Rust](https://img.shields.io/badge/rust-1.85%2B-orange.svg)](https://www.rust-lang.org)

[🇬🇧 English Version](README.md) · [📋 更新日志](CHANGELOG_ZH.md)

RustRAG 是一个使用 Rust 编写的高性能检索增强生成 (RAG) MCP (Model Context Protocol) 服务器。

> **为您的大模型缩减高达 40 倍的 Token 消耗** —— 构建代码库和文档的 1024 维向量索引，确保 AI 助手仅获取最核心、最高相关的知识片段。

---

## 最新版本 (v3.0.0)

v3.0.0 是一次重大重构版本，将本地 ONNX 推理引擎替换为 **OpenAI 兼容的 Embedding API** 后端，带来更高的检索精度和极简的部署体验。

### 核心变更

- **API 向量化** — 从本地 ONNX 模型（~235MB 下载、~500MB 内存）切换到任意 OpenAI 兼容的 `/v1/embeddings` API。支持 DashScope、Ollama、OpenAI、Azure OpenAI 等。
- **1024 维向量** — 从 384 维升级到 1024 维（float32），语义检索精度大幅提升。
- **秒级启动** — 无需下载模型或加载 ONNX Runtime，服务器 1 秒内启动。
- **内存降低 90%** — 运行时内存从 ~500MB（ONNX）降至 ~50MB（API 客户端）。
- **减少 4 个依赖** — 移除 `ort`、`tokenizers`、`bytemuck`、`indicatif`。
- **智能批处理** — 根据文本长度自适应调整批次大小，避免 API 请求体超限。
- **指数退避重试** — 最多 3 次重试，自动分类可重试错误（429/5xx）与不可重试错误（4xx）。
- **新增文件格式** — 新增 `.mjs`、`.cjs`、`.mts`、`.cts`（共 28 种支持格式）。
- **绝对路径数据目录** — 数据库和状态文件默认存储在 `~/.rustrag/`，不再污染项目目录。

### 从 v2.x 迁移

这是一个**破坏性变更**，迁移步骤：

1. 删除旧的 `vectors.db`（schema 不兼容：384 vs 1024 维）
2. 更新 `config.json`，添加新的 `embedding` 配置段（参见[快速开始](#2-配置)）
3. 通过配置文件或环境变量设置 API Key（`RAG_API_KEY`、`DASHSCOPE_API_KEY` 或 `OPENAI_API_KEY`）
4. 从配置中移除 `compute` 和 `model` 段（已不再使用）

[📋 完整更新日志](CHANGELOG_ZH.md)

---

## 核心特性

- **7 个 MCP 工具** — search、index、list_documents、manage_document、frontmatter、search_relations、build_dictionary
- **28 种支持格式** — 代码（Rust、Go、Python、TypeScript、JavaScript + ESM/CJS 变体）、Markdown、纯文本、结构化数据（JSON、YAML、TOML、CSV）、HTML、PDF、DOCX、电子表格
- **1024 维向量搜索** — SQLite + sqlite-vec，float32 精度，高质量语义检索
- **代码智能解析** — Tree-sitter AST 解析 Rust、Go、Python、TypeScript、JavaScript
- **跨语种词典** — CJK↔English 符号映射提取
- **任意 OpenAI 兼容 API** — DashScope、Ollama（本地）、OpenAI、Azure OpenAI、DeepSeek、SiliconFlow
- **高并发稳定流** — 异步后台同步，支持 10k+ 文件的大项目
- **全平台支持** — macOS (Intel/ARM)、Linux (x64/ARM64)、Windows (x64)

## 快速开始

### 1. 安装

从 [Releases 页面](https://github.com/Michaol/RustRAG/releases) 下载或从源码编译：

```bash
git clone https://github.com/Michaol/RustRAG.git
cd RustRAG
cargo build --release
```

编译产物位于 `target/release/rustrag`（Windows 为 `rustrag.exe`）。

### 2. 配置

在项目根目录创建 `config.json`（首次运行自动生成默认配置）：

```json
{
  "document_patterns": ["./"],
  "exclude_patterns": ["**/node_modules/**", "**/target/**", "**/.git/**"],
  "file_extensions": [
    "md", "rs", "go", "py",
    "js", "mjs", "cjs", "jsx",
    "ts", "mts", "cts", "tsx",
    "txt", "log",
    "json", "yaml", "yml", "toml", "csv",
    "html", "htm",
    "pdf", "docx", "xls", "xlsx", "xlsb", "ods"
  ],
  "chunk_size": 500,
  "search_top_k": 5,
  "embedding": {
    "api_url": "https://dashscope.aliyuncs.com/compatible-mode/v1/embeddings",
    "api_key": "",
    "api_model": "text-embedding-v4",
    "dimensions": 1024,
    "batch_size": 32,
    "max_concurrent": 5,
    "timeout_secs": 30
  }
}
```

通过环境变量设置 API Key（推荐），或直接写入 `api_key` 字段：

```bash
# 以下环境变量均支持（按优先级排列）：
export RAG_API_KEY="sk-your-api-key"
export DASHSCOPE_API_KEY="sk-your-api-key"
export OPENAI_API_KEY="sk-your-api-key"
```

#### 切换 Embedding 提供商

| 提供商 | `api_url` | `api_model` | 维度 |
|---|---|---|---|
| 阿里云 DashScope | `https://dashscope.aliyuncs.com/compatible-mode/v1/embeddings` | `text-embedding-v4` | 1024 |
| Ollama（本地） | `http://localhost:11434/v1/embeddings` | `nomic-embed-text` | 768 |
| OpenAI | `https://api.openai.com/v1/embeddings` | `text-embedding-3-small` | 1536 |
| DeepSeek | `https://api.deepseek.com/v1/embeddings` | `deepseek-embedding` | 1024 |
| SiliconFlow | `https://api.siliconflow.cn/v1/embeddings` | `BAAI/bge-large-zh-v1.5` | 1024 |

> **注意**：切换提供商时，需更新 `dimensions` 以匹配模型输出，并删除已有的 `vectors.db`（schema 必须匹配）。

### 3. 接入 MCP 客户端

#### Claude Desktop / Cursor / Antigravity IDE

在 MCP 配置中添加：

```json
{
  "mcpServers": {
    "rustrag": {
      "command": "/绝对路径/rustrag",
      "args": ["--config", "/项目路径/config.json"]
    }
  }
}
```

#### 🌩️ 进阶：远程 SSH 模式

将 RustRAG 安装在远程服务器上，通过 SSH 管道连接到本地 IDE：

```json
{
  "mcpServers": {
    "rustrag-remote": {
      "command": "ssh",
      "args": [
        "user@remote.server.ip",
        "/绝对路径/rustrag",
        "--config",
        "/远程项目/config.json"
      ]
    }
  }
}
```

> 需配置 SSH 密钥免密登录（`ssh-keygen -t ed25519` + `ssh-copy-id`），MCP 客户端无法弹出密码输入框。

## CLI 参数

| 参数             | 默认值        | 说明                                    |
| ---------------- | ------------- | --------------------------------------- |
| `--config`, `-c` | `config.json` | 配置文件路径                            |
| `--log-level`    | `info`        | 日志级别 (trace/debug/info/warn/error)  |
| `--skip-sync`    | false         | 跳过启动时的初始文档同步                |
| `--transport`    | `stdio`       | 传输模式：`stdio` 或 `http`             |
| `--port`         | `8765`        | HTTP 端口（仅 transport=`http` 时生效） |
| `--version`      | —             | 显示版本号并退出                        |

## MCP 工具列表

| 工具               | 说明                                                                |
| ------------------ | ------------------------------------------------------------------- |
| `search`           | 语义向量搜索，支持目录/文件名过滤                                   |
| `index`            | 使用 AST 感知分块对文档或代码文件建立索引                           |
| `manage_document`  | 从索引中移除文档或强制重新索引                                      |
| `list_documents`   | 列出所有已索引文档                                                  |
| `frontmatter`      | 为 Markdown 文件添加或更新 YAML 元信息                              |
| `search_relations` | 搜索代码关系（调用、导入、继承）                                    |
| `build_dictionary` | 从代码中提取 CJK↔English 术语映射                                   |

## 源码结构

```
src/
├── lib.rs              # 模块导出
├── main.rs             # CLI + 启动流程
├── config.rs           # 配置加载/校验
├── frontmatter.rs      # YAML frontmatter 操作
├── updater.rs          # 版本更新检查（GitHub API）
├── watcher.rs          # 文件系统监听（热重载）
├── db/                 # SQLite + sqlite-vec 向量数据库
│   ├── mod.rs          # Schema（float32[1024]）+ 连接池
│   ├── models.rs       # 数据模型
│   ├── documents.rs    # 文档 CRUD 操作
│   ├── search.rs       # 向量相似度搜索（余弦距离）
│   └── relations.rs    # 代码关系查询
├── embedder/           # 文本向量化
│   ├── mod.rs          # Embedder trait 定义
│   ├── api.rs          # OpenAI 兼容 API 客户端（智能批处理 + 重试）
│   └── mock.rs         # Mock embedder（测试用）
├── indexer/            # 文档和代码索引
│   ├── core.rs         # 增量同步引擎
│   ├── markdown.rs     # Markdown 分块
│   ├── text_parser.rs  # 多格式文档提取（PDF/DOCX/XLSX 等）
│   ├── code_parser.rs  # Tree-sitter 代码解析
│   ├── relations.rs    # 代码关系提取
│   ├── dictionary.rs   # 多语种词典
│   └── languages.rs    # 语言专属 Tree-sitter 查询
└── mcp/                # MCP 协议层
    ├── server.rs       # 服务端设置（stdio + HTTP 传输）
    └── tools.rs        # 7 个工具处理器实现
```

## 语言支持

| 语言       | 扩展名                              | 解析器                 |
| ---------- | ----------------------------------- | ---------------------- |
| Rust       | `.rs`                               | tree-sitter-rust       |
| Go         | `.go`                               | tree-sitter-go         |
| Python     | `.py`                               | tree-sitter-python     |
| TypeScript | `.ts` `.tsx` `.mts` `.cts`         | tree-sitter-typescript |
| JavaScript | `.js` `.jsx` `.mjs` `.cjs`         | tree-sitter-javascript |
| Markdown   | `.md`                               | pulldown-cmark         |

## 文档格式支持

| 文档格式   | 扩展名                               | 解析引擎               |
| ---------- | ------------------------------------ | ---------------------- |
| 纯文本     | `.txt`, `.log`                       | `fs::read_to_string`   |
| JSON       | `.json`                              | `serde_json`           |
| YAML       | `.yaml`, `.yml`                      | `serde_yaml`           |
| TOML       | `.toml`                              | `toml`                 |
| CSV        | `.csv`                               | `csv`                  |
| HTML       | `.html`, `.htm`                      | `scraper`              |
| PDF        | `.pdf`                               | `lopdf`                |
| Word       | `.docx`                              | `docx-rs`              |
| 电子表格   | `.xls`, `.xlsx`, `.xlsb`, `.ods`    | `calamine`             |

## 源码编译

**环境要求：** Rust 1.85+

```bash
cargo build --release
```

## 测试

```bash
# 全量测试（88 单元测试 + 6 集成测试）
cargo test --all

# 仅集成测试
cargo test --test integration_test

# 代码规范检查（预期零警告）
cargo clippy -- -D warnings
```

## 开源协议

RustRAG 采用双协议开源：

- [MIT License](LICENSE-MIT)
- [Apache License, Version 2.0](LICENSE-APACHE)

任您选择。
