# RustRAG

[![License: MIT/Apache-2.0](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue.svg)](LICENSE-MIT)
[![CI](https://github.com/Michaol/RustRAG/actions/workflows/ci.yml/badge.svg)](https://github.com/Michaol/RustRAG/actions/workflows/ci.yml)
[![Rust](https://img.shields.io/badge/rust-1.85%2B-orange.svg)](https://www.rust-lang.org)

[🇬🇧 English Version](README_EN.md)

RustRAG 是一个使用 Rust 编写的高性能、纯本地的检索增强生成 (RAG) MCP (Model Context Protocol) 服务器。

> **为您的大模型缩减高达 40 倍的 Token 消耗** —— 在本地极速构建代码库和文档的向量索引，确保 AI 助手仅获取最核心、最高相关的知识片段。

---

## 最新版本发布 (v2.1.0)

v2.1.0 引入高级功能和改进，增强性能、可靠性和开发者体验：

- **新增功能**：增强的功能和改善的用户体验。
- **性能优化**：更快的处理和减少资源使用。
- **稳定性改进**：提高可靠性和错误修复。

---

<details>
<summary><b>展开查看历史演进 (v2.0.0 及更早版本)</b></summary>
<br>

### v2.0.0 从 ONNX 模型迁移

v2.0.0 将嵌入模型从 `model.onnx`（470MB）迁移至 HuggingFace 官方提供的 `model_O4.onnx`（235MB），实现模型文件与运行时内存占用减半：

- **ONNX O4 图优化模型**：采用 ONNX Graph Optimization Level 4 预优化模型，向量输出与原版完全一致，数据库 100% 兼容，无需重建索引。
- **模型体积减半**：下载体积从 ~470MB 降至 ~235MB，运行时内存从 ~500MB 降至 ~250MB。
- **自动迁移清理**：已有旧版 `model.onnx` 的用户，程序将自动检测并清理旧文件，无感升级。

### v1.3.7 配置热重载

v1.3.7 版本引入了基于 `RwLock` 的配置与模型实例热重载机制：

- **GPU 推理引擎热重载**：核心模型运行环境已解耦为读写锁 (`RwLock`) 控制。修改 `config.json` 中的 `device` 硬件策略或相关参数后，底层 ONNX 推理图将在下次请求时自动释放并重新初始化，无需重启服务即可应用新的硬件配置。
- **配置与文件监控自动同步**：系统现已支持监听 `config.json` 本身的变更。配置文件修改后，系统将自动重载配置，并根据新的目录与文件后缀过滤规则，实时调整后台文件监听任务。

### v1.3.6 硬件加速优化版

- **多平台 GPU 加速**：支持跨平台 (Windows/Linux/macOS) 的 CUDA、TensorRT、DirectML 与 CoreML 动态加载，并提供安全的 CPU 回退机制。
- **配置与容错提升**：配置文件新增支持 `batch_size` 调优与 `compute.fallback_to_cpu` 设备降级处理，减少加速器加载失败导致的程序异常。
- **文件系统热重载 (File Watching)**：引入跨平台原生后台文件监听机制。受控文档增、删、改变动将自动触发增量同步。
- **SQLite WAL 模式**：底层默认激活 Write-Ahead Logging 模式，优化高频并发写入时的保护策略，解决数据库锁定错误。
- **颗粒度错误处理**：优化 MCP 接口报错路由，将系统级异常（如 SQLite 锁定等）信息传递至客户端日志，便于定位排查定位核心问题。

### v1.2.0 & v1.1.0 性能与体积优化版

- **INT8 标量量化**：底层向量层由 `FLOAT[384]` 重构为 `INT8[384]`。在维持检索质量的前提下，将向量数据库磁盘占用空间降低约 75%。
- **ONNX Level 3 图优化**：在嵌入提取前的前置引擎阶段启用了 ORT Level 3 图优化策略，提升单纯 CPU 的推理性能。
- **动态清理机制**：修改 `exclude_patterns`（黑名单）后再次检索时系统会自动清空过期的排除文件；并引入过时文档回收机制，物理删除文件将同步清理数据库相关词典映射记录。

> ⚠️ **数据兼容提示**：若由 v1.1.x 升级至后续版本，由于底层表结构量化更改，请手动删除旧版的 `vectors.db` 文件，以初始化新库。

</details>

---

## 核心特性

- **7 个强大的 MCP 工具集** — 囊括语义检索 (`search`)、文件打标入库 (`index`)、元数据管理 (`frontmatter`)、关联图谱 (`search_relations`) 等全面能力。
- **纯粹的本地向量搜索** — SQLite 联手强韧的 sqlite-vec，让毫秒级检索在本地数据库流畅翻飞。
- **全息代码解析智网** — Native Tree-sitter AST 解析矩阵，目前深度支持：Rust, Go, Python, TypeScript, JavaScript 源文件。
- **跨语种词典结网** — 首创从代码级提取 "中/日韩文↔英语" 的函数与注释符号映射。
- **高并发与稳定流** — 全异步无阻塞 (`Arc<TokioMutex>`) 背景同步设计，针对超大型项目 (1W+ 文件) 也可通过分页游标避让 MCP 标准 I/O 流断流风险。
- **零配置环境** — 首次启动自动下载并映射 `multilingual-e5-small` ONNX 高密词嵌入基座，无需 Python 环境。
- **全平台通杀** — 完美支持 macOS (Intel/ARM), Linux (x64/ARM64), Windows (x64)。

## 快速组网入门

### 1. 下载安装

请前往 [Releases 页面](https://github.com/Michaol/RustRAG/releases) 获取最匹配您环境的部署包：

| 操作系统 | 下载包范例 |
| ------------------- | ------------------------------------ |
| Windows x64 | `rustrag-windows-x64.exe.zip` |
| macOS Apple Silicon | `rustrag-macos-apple-silicon.tar.gz` |
| Linux x64 | `rustrag-linux-x64.tar.gz` |
| Linux ARM64 | `rustrag-linux-arm64.tar.gz` |

**详细安装解压步骤：**

#### Windows

```powershell
# 找个安全的目录安家
Expand-Archive rustrag-windows-x64.zip -DestinationPath "$env:LOCALAPPDATA\RustRAG"
```

> ⚠️ **致命警告**: 在 Windows 下，请始终保持 `rustrag.exe` 与旁边的 `.dll` 文件 (例如 `onnxruntime.dll`) 形影不离！千万**不要**单独把 exe 提走去用。

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

解压收尾之后，请一定要使用 **绝对路径** (`absolute path`) 配置你的 IDE 客户端。

当然，身为 Rust 极客，您也可以：

```bash
# 源码克隆并编译
git clone https://github.com/Michaol/RustRAG.git
cd RustRAG
cargo build --release
```

### 2. 生成配置

在您的工程项目根目录丢一个 `config.json`（如果没创建，首次执行会自动根据预设生成）：

```json
{
  "document_patterns": ["./"],
  "exclude_patterns": ["node_modules", "target", ".git", "dist"],
  "file_extensions": ["rs", "md", "go", "py", "ts", "js"],
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

### 3. 接驳进 MCP 大脑

#### Antigravity IDE

在设置中注入您的 `mcp_config.json`：

```json
{
  "mcpServers": {
    "rustrag": {
      "command": "path/绝对/路径/到/rustrag",
      "args": ["--config", "您的项目/config.json"]
    }
  }
}
```

#### Claude Desktop / Cursor

同样，填补相应的配置文件即可：

```json
{
  "mcpServers": {
    "rustrag": {
      "command": "path/绝对/路径/到/rustrag",
      "args": ["--config", "您的项目/config.json"]
    }
  }
}
```

#### 🌩️ 高阶：远程部署，本地调用 (Remote SSH Mode)

如果您的代码库、运行环境和模型部署在远端高性能服务器（或局域网 NAS）上，而您日常使用本地笔电办公，完全可以**按上述步骤将 RustRAG 装在远端服务器**，然后通过 MCP 的标准 I/O 管道流特性，在本地 IDE 中进行**跨端挂载**！

**前置认证要求（必读）：**
MCP 客户端（如 Cursor / Claude Desktop）在后台静默拉起子进程时，**无法弹出密码输入框**供您交互。因此，必须配置无交互登录：

- 🔑 **方案一：配置 SSH 密钥（强烈推荐，全平台原生兼容）**
本地机器生成密钥 (`ssh-keygen -t ed25519`) 并推送到远端 (`ssh-copy-id user@ip`)，实现免密安全的直连。原生支持 Windows / macOS / Linux。
- 🔓 **方案二：使用 `sshpass` (仅限密码及 Linux / macOS)**
如果您无法配置密钥必须使用密码，可以将 `command` 替换为 `sshpass`（例如：`args: ["-p", "您的密码", "ssh", "user@ip", ...] `）。**注意**：`sshpass` 在 Linux 与 macOS (`brew install sshpass`) 下可用，但在 Windows 原生极其难装，Windows 用户请务必采用方案一。

**连接配置示例 (以原生 SSH 为例)：**

```json
{
  "mcpServers": {
    "rustrag-remote": {
      "command": "ssh",
      "args": [
        "user@remote.server.ip", // 替换为您的远端服务器地址
        "/绝对路径/rustrag", // 远端 rustrag 程序的路径
        "--config",
        "/远端代码库/config.json" // 远端项目里的配置文件路径
      ]
    }
  }
}
```
有了它，您的本地 AI 助手将能够光速洞悉远端服务器中数百万行的代码库，且本地完全不耗费任何性能卡顿。

#### 💻 进阶级：解锁本地 GPU / N卡加速 (CUDA & TensorRT)

为了保持项目仓库精简并确保所有平台用户（尤其是 Mac 或是没有独立显卡的设备）开箱即用，RustRAG 默认以 **纯 CPU 模式**（`fallback_to_cpu: true`）轻量级流转。然而，如果您拥有如 RTX 30/40 系的 **NVIDIA 独立显卡** 且希望体验微秒级向量检索，可通过以下两步无缝热拔插开启 TensorRT/CUDA 加速：

1. **下载官方 GPU 运行时**
前往 [ONNX Runtime v1.23.2 Release 页面](https://github.com/microsoft/onnxruntime/releases/tag/v1.23.2)，根据您的系统下载带 GPU 支持的压缩包（体积约 300+MB）：
- **Windows:** 下载 `onnxruntime-win-x64-gpu-1.23.2.zip`
- **Linux:** 下载 `onnxruntime-linux-x64-gpu-1.23.2.tgz`
- **macOS:** Apple Silicon 在 CPU 下已经极快且默认支持 CoreML，无需下载额外 Nvidia 包。

2. **提取动态链接库**
解压该压缩包，将里面的所有 `.dll` (Windows) 或 `.so` (Linux) 文件（如 `onnxruntime.dll`, `libonnxruntime_providers_cuda.so`）直接**移动到 `rustrag` 编译好的运行文件同级目录下**。

3. **开启全自动推流点亮**
打开您的项目的 `config.json`，确认配置如下：

```json
"compute": {
  "device": "auto", // <-- 代码会首先寻租 TensorRT，其次 CUDA，最后 DirectML/CoreML
  "fallback_to_cpu": true // <-- 保底安全带，探测失败则回滚 CPU，不崩溃
}
```
当有显卡且 DLL/SO 库齐备时，MCP 启动日志将会向您霸气宣布 `🚀 ONNX Execution Provider Activated: [TensorRT]` 或 `[CUDA]`。**此配置完全与主代码库隔离，永远不会污染他人的基础工程环境**！

## CLI 控制台指引

| 开关命令 | 默认值 | 释义描述 |
| ----------------- | ------------- | -------------------------------------------------------------- |
| `--config`, `-c` | `config.json` | 强制指明 configuration 并锚定作用域 |
| `--log-level` | `info` | 设置日志水位 (trace/debug/info/warn/error) |
| `--skip-download` | false | 越过模型自检下载（适用于离线模式或内网） |
| `--skip-sync` | false | 跳过庞大的启动期文档初筛（直接启动服务，适用于超大库常驻进程） |
| `--version` | — | 版本鉴权 |

## MCP Tools 弹药库览表

| 装备 | 战术职能 |
| -------------------- | ------------------------------------------------------------------------------- |
| `search` | NLU 语义向量降维搜索，支持强制后缀或路径通配拦截符查询 |
| `index` | 针对文档（Markdown/代码）进行 AST 横切、切割并建立块级与函数级特征向量聚合入库 |
| `manage_document` | 从索引中移除文档，或先移除索引后重新索引指定文件 |
| `list_documents` | 调取全部被管制的长文本凭证清单 |
| `frontmatter` | 给 Markdown 植入或覆盖更新 YAML 元信息头部 |
| `search_relations` | 深潜追剿代码间调用链路 (Calls)、继承 (Inherits) 与引入依赖 (Imports) 的树状图谱 |
| `build_dictionary` | 从源码字里行间蒸馏中日韩英全息词典 |

## 源码树全景

```
src/
├── lib.rs # 系统核心接口暴露
├── main.rs # 骨干调度与入口
├── config.rs # 配置拦截层与生命周期管理
├── frontmatter.rs # Markdown 元数据解析
├── updater.rs # 自动探测版本进化机制
├── db/ # SQLite + sqlite-vec 持久矩阵
│   ├── mod.rs # Schema 与数据库总线
│   ├── models.rs # 数据结构体集
│   ├── documents.rs # 文档层 CRUD 桥接
│   ├── search.rs # 向量内积求导搜索核心
│   └── relations.rs # AST 层关系映射
├── embedder/ # 文本张量引擎
│   ├── mod.rs # Embedder 特征约束
│   ├── onnx.rs # 搭载 ORT Level3 的 ONNX 主引擎
│   ├── mock.rs # 测试隔离沙箱
│   ├── tokenizer.rs # HuggingFace BERT 词符切割
│   └── download.rs # 模型远端直拖链路
├── indexer/ # 知识打散与重塑核心
│   ├── core.rs # Hash防腐与增量比对守护进程
│   ├── markdown.rs # MarkDown 分块剥离器
│   ├── code_parser.rs# Tree-sitter 全息代码解析
│   ├── relations.rs # 结构层关系汲取
│   ├── dictionary.rs # 本地化译丛词典
│   └── languages.rs # 语言语法探查器组件
└── mcp/ # MCP 桥接通讯层
    ├── server.rs # stdio 指令生命周期守护长驻进程
    └── tools.rs # 7 大 Tool 的参数注册并实施路由反射
```

## 语言适配支持层

| 开发语言 | 后缀 | 特化切割探针 |
| ---------- | ----- | ---------------------- |
| Rust | `.rs` | tree-sitter-rust |
| Go | `.go` | tree-sitter-go |
| Python | `.py` | tree-sitter-python |
| TypeScript | `.ts` | tree-sitter-typescript |
| JavaScript | `.js` | tree-sitter-javascript |
| Markdown | `.md` | pulldown-cmark |

## 纯源码编译说明

**基本环境索取:** Rust 1.85+

```bash
cargo build --release
```

您的杀器会被锻造在 `target/release/rustrag` (如果身处 Windows 将是 `rustrag.exe`)。

## 测试场域

```bash
# 全库矩阵压力测试
cargo test --all
# 集成端到端流转测试
cargo test --test integration_test
# 严谨规范审查
cargo clippy -- -D warnings
```

## 开源协议

RustRAG 始终秉持双路自由开源协议：

- [MIT License](LICENSE-MIT)
- [Apache License, Version 2.0](LICENSE-APACHE)

任您挑选。