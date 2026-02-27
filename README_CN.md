# RustRAG

高性能本地 RAG（检索增强生成）MCP Server，使用 Rust 编写。

> **40× Token 节省** — 本地索引代码库，仅检索最相关的上下文提供给 AI 助手。

## 特性

- **10 个 MCP 工具** — search、index_markdown、index_code、list_documents、delete_document、reindex_document、add_frontmatter、update_frontmatter、search_relations、build_dictionary
- **向量搜索** — SQLite + sqlite-vec 实现快速本地向量相似度搜索
- **代码智能** — Tree-sitter AST 解析，支持 Rust、Go、Python、TypeScript、JavaScript
- **多语言词典** — 中日韩↔英文符号映射自动提取
- **模型自动下载** — 自动下载 `multilingual-e5-small` ONNX 模型
- **跨平台** — macOS (Intel/ARM)、Linux (x64/ARM64)、Windows (x64)

## 快速开始

### 1. 安装

从 [Releases](https://github.com/your-repo/rustrag/releases) 下载对应平台的最新版本，或从源码构建：

```bash
# 克隆并构建
git clone https://github.com/your-repo/rustrag.git
cd rustrag
cargo build --release
```

### 2. 配置

在项目根目录创建 `config.json`（首次运行时会自动生成默认配置）：

```json
{
  "document_patterns": ["./docs", "./src"],
  "db_path": "./vectors.db",
  "chunk_size": 500,
  "search_top_k": 5,
  "model": {
    "name": "multilingual-e5-small",
    "dimensions": 384
  }
}
```

### 3. 添加到 MCP 客户端

在你的 MCP 客户端配置中添加（如 Claude Desktop、Cursor 等）：

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

## 命令行参数

| 参数              | 默认值        | 说明                                    |
| ----------------- | ------------- | --------------------------------------- |
| `--config`, `-c`  | `config.json` | 配置文件路径                            |
| `--log-level`     | `info`        | 日志级别（trace/debug/info/warn/error） |
| `--skip-download` | false         | 跳过自动模型下载                        |
| `--skip-sync`     | false         | 跳过初始文档同步                        |
| `--version`       | —             | 显示版本号并退出                        |

## MCP 工具

| 工具                 | 说明                                         |
| -------------------- | -------------------------------------------- |
| `search`             | 自然语言向量搜索，支持目录和文件名模式过滤   |
| `index_markdown`     | 索引单个 Markdown 文件                       |
| `index_code`         | 使用 Tree-sitter AST 解析索引代码文件        |
| `list_documents`     | 列出所有已索引文档                           |
| `delete_document`    | 从索引中删除文档                             |
| `reindex_document`   | 强制重新索引文档                             |
| `add_frontmatter`    | 为 Markdown 文件添加 YAML frontmatter 元数据 |
| `update_frontmatter` | 更新已有 frontmatter 元数据                  |
| `search_relations`   | 搜索代码关系（调用、导入、继承）             |
| `build_dictionary`   | 从代码中提取中日韩↔英文术语映射              |

## 架构

```
src/
├── lib.rs            # 模块导出
├── main.rs           # CLI + 启动流程
├── config.rs         # 配置加载/验证
├── frontmatter.rs    # YAML 前置数据操作
├── updater.rs        # 版本更新检查（GitHub API）
├── db/               # SQLite + sqlite-vec 向量数据库
│   ├── mod.rs        # Schema + 连接管理
│   ├── models.rs     # 数据模型
│   ├── documents.rs  # 文档 CRUD
│   ├── search.rs     # 向量相似度搜索
│   └── relations.rs  # 代码关系查询
├── embedder/         # 文本嵌入引擎
│   ├── mod.rs        # Embedder trait
│   ├── onnx.rs       # ONNX Runtime 推理
│   ├── mock.rs       # Mock 嵌入器（测试用）
│   ├── tokenizer.rs  # BERT 分词器封装
│   └── download.rs   # 模型自动下载
├── indexer/          # 文档和代码索引
│   ├── core.rs       # 差异同步引擎
│   ├── markdown.rs   # Markdown 分块
│   ├── code_parser.rs # Tree-sitter 代码解析
│   ├── relations.rs  # 代码关系提取
│   ├── dictionary.rs # 多语言词典
│   └── languages.rs  # 语言特定 TS 查询
└── mcp/              # MCP 协议层
    ├── server.rs     # 服务器设置（stdio 传输）
    └── tools.rs      # 10 个工具处理器
```

## 支持的语言

| 语言       | 扩展名 | 解析器                 |
| ---------- | ------ | ---------------------- |
| Rust       | `.rs`  | tree-sitter-rust       |
| Go         | `.go`  | tree-sitter-go         |
| Python     | `.py`  | tree-sitter-python     |
| TypeScript | `.ts`  | tree-sitter-typescript |
| JavaScript | `.js`  | tree-sitter-javascript |
| Markdown   | `.md`  | pulldown-cmark         |

## 从源码构建

**前提条件：** Rust 1.85+

```bash
cargo build --release
```

编译产物位于 `target/release/rustrag`（Windows 为 `rustrag.exe`）。

## 测试

```bash
# 运行全部测试
cargo test --all

# 仅运行集成测试
cargo test --test integration_test

# 代码检查
cargo clippy -- -D warnings
```

## 许可证

MIT
