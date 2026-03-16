# RustRAG

[![License: MIT/Apache-2.0](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue.svg)](LICENSE-MIT)
[![CI](https://github.com/Michaol/RustRAG/actions/workflows/ci.yml/badge.svg)](https://github.com/Michaol/RustRAG/actions/workflows/ci.yml)
[![Rust](https://img.shields.io/badge/rust-1.85%2B-orange.svg)](https://www.rust-lang.org)

[🇬🇧 English Version](README_EN.md)

RustRAG 是一个使用 Rust 编写的高性能、纯本地的检索增强生成 (RAG) MCP (Model Context Protocol) 服务器。

> **为您的大模型缩减高达 40 倍的 Token 消耗** —— 在本地极速构建代码库和文档的向量索引，确保 AI 助手仅获取最核心、最高相关的知识片段。

---

## 🚀 最新版本发布 (v1.3.5)

RustRAG 迎来了深度体验与健壮性优化版本：

- 💻 **全平台 GPU 满血硬核加速**：新增零侵入式的跨平台 (Win/Linux/Mac) CUDA/TensorRT/DirectML/CoreML 动态库热加载！极速点亮硬件引擎，并支持无驱动环境的安全智能回退。

- ⚙️ **配置灵活性与容错提升**：配置文件现在支持 `batch_size` 容量调优与 `compute.fallback_to_cpu` 设备自降级保护。不再畏惧显存耗尽或加速器 (CUDA/CoreML) 加载故障。
- 🔄 **文件系统实时热重载 (File Watching)**：引入跨平台原生的后台文件系统监听。无需频繁调用 `index` 工具，任何对受控文档的增、删、改都会立刻触发自动的差量同步，真正的 RAG 「热」重载体验降临！
- 🗄️ **SQLite WAL 高并发守护**：底层默认激活了 Write-Ahead Logging 模式与更合理的超时保护。告别因高频查询与并发写入造成的 `database is locked` 错误，读写性能大幅双升。
- 🧯 **精细化 MCP 异常反馈**：优化 MCP 接口报错路由与粒度，将 `Embedder` 计算瓶颈、文件解析错误与 `SQLite` 逻辑锁定等精确分发到客户端日志，方便终端用户定位问题核心。

---

## 🚀 历史演进 (v1.2.0 & v1.1.0)

RustRAG 迎来了底层向量库引擎和推理架构的双重进化：

- ⚡️ **INT8 极限标量量化**：底层虚拟表由 `FLOAT[384]` 重构为 `INT8[384]`。能在几乎 **零精度损耗**（余弦相似度计算）的前提下，将数百兆的向量数据库硬盘**占用空间锐减 75%**！
- 🧠 **ONNX Level 3 推理图加速**：在嵌入提取前置引擎激活了 ORT 的最高等级图优化（常量折叠与节点融合），让纯 CPU 的推理嵌入速度飙升，获得无缝加速体验。
- ⚙️ **配置热更新自愈网路**：配置引擎目前拥有了哈希校验能力。当用户更改 `config.json` 中的 `exclude_patterns`（目录黑名单）或 `file_extensions`（文件白名单）后，再次检索时系统会自动无痕清理旧规则遗留的过期文档并执行高净度重建。
- 🧹 **幽灵文档联带清理 (Cascade Cleanup)**：引入 Stale Document 过期回收机制，删除物理文件将自动清空其内部 Chunk 和相关连接词典。

> ⚠️ **强烈更新提示**：由于我们在最新版本中对向量数据库进行了 INT8 量化的降维重构。当您升级到 v1.2.0 后，**请务必手动删除旧版本的 `vectors.db` 文件！** 随后系统重启将为您在仅仅 1/4 的空间里重生一个极速新库。

---

## 核心特性

- **7 个强大的 MCP 工具集** — 囊括语义检索 (`search`)、文件打标入库 (`index`)、元数据管理 (`frontmatter`)、关联图谱 (`search_relations`) 等全面能力。
- **纯粹的本地向量搜索** — SQLite 联手强韧的 sqlite-vec，让毫秒级检索在本地数据库流畅翻飞。
- **全息代码解析智网** — Native Tree-sitter AST 解析矩阵，目前深度支持：Rust, Go, Python, TypeScript, JavaScript 源文件。
- **跨语种词典结网** — 首创从代码级提取 “中/日韩文↔英语” 的函数与注释符号映射。
- **高并发与稳定流** — 全异步无阻塞 (`Arc<TokioMutex>`) 背景同步设计，针对超大型项目 (1W+ 文件) 也可通过分页游标避让 MCP 标准 I/O 流断流风险。
- **零配置环境** — 首次启动自动下载并映射 `multilingual-e5-small` ONNX 高密词嵌入基座，无需 Python 环境。
- **全平台通杀** — 完美支持 macOS (Intel/ARM), Linux (x64/ARM64), Windows (x64)。

## 快速组网入门

### 1. 下载安装

请前往 [Releases 页面](https://github.com/Michaol/RustRAG/releases) 获取最匹配您环境的部署包：

| 操作系统            | 下载包范例                           |
| ------------------- | ------------------------------------ |
| Windows x64         | `rustrag-windows-x64.exe.zip`        |
| macOS Apple Silicon | `rustrag-macos-apple-silicon.tar.gz` |
| Linux x64           | `rustrag-linux-x64.tar.gz`           |
| Linux ARM64         | `rustrag-linux-arm64.tar.gz`         |

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
        "user@remote.server.ip",    // 替换为您的远端服务器地址
        "/绝对路径/rustrag",          // 远端 rustrag 程序的路径
        "--config",
        "/远端代码库/config.json"     // 远端项目里的配置文件路径
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
     "device": "auto",       // <-- 代码会首先寻租 TensorRT，其次 CUDA，最后 DirectML/CoreML
     "fallback_to_cpu": true // <-- 保底安全带，探测失败则回滚 CPU，不崩溃
   }
   ```
当有显卡且 DLL/SO 库齐备时，MCP 启动日志将会向您霸气宣布 `🚀 ONNX Execution Provider Activated: [TensorRT]` 或 `[CUDA]`。**此配置完全与主代码库隔离，永远不会污染他人的基础工程环境**！

## CLI 控制台指引

| 开关命令          | 默认值        | 释义描述                                                       |
| ----------------- | ------------- | -------------------------------------------------------------- |
| `--config`, `-c`  | `config.json` | 强制指明 configuration 并锚定作用域                            |
| `--log-level`     | `info`        | 设置日志水位 (trace/debug/info/warn/error)                     |
| `--skip-download` | false         | 越过模型自检下载（适用于离线模式或内网）                       |
| `--skip-sync`     | false         | 跳过庞大的启动期文档初筛（直接启动服务，适用于超大库常驻进程） |
| `--version`       | —             | 版本鉴权                                                       |

## MCP Tools 弹药库览表

| 装备                 | 战术职能                                                                        |
| -------------------- | ------------------------------------------------------------------------------- |
| `search`             | NLU 语义向量降维搜索，支持强制后缀或路径通配拦截符查询                          |
| `index`              | 针对文档（Markdown/代码）进行 AST 横切、切割并建立块级与函数级特征向量聚合入库  |
| `manage_document`    | 物理粉碎文档的向量历史，或者摧毁旧记录后热重建                                  |
| `list_documents`     | 调取全部被管制的长文本凭证清单                                                  |
| `frontmatter`        | 给 Markdown 植入或覆盖更新 YAML 元信息头部                                      |
| `search_relations`   | 深潜追剿代码间调用链路 (Calls)、继承 (Inherits) 与引入依赖 (Imports) 的树状图谱 |
| `build_dictionary`   | 从源码字里行间蒸馏中日韩英全息词典                                              |

## 源码树全景

```
src/
├── lib.rs            # 系统核心接口暴露
├── main.rs           # 骨干调度与入口
├── config.rs         # 配置拦截层与生命周期管理
├── frontmatter.rs    # Markdown 元数据解析
├── updater.rs        # 自动探测版本进化机制
├── db/               # SQLite + sqlite-vec 持久矩阵
│   ├── mod.rs        # Schema 与数据库总线
│   ├── models.rs     # 数据结构体集
│   ├── documents.rs  # 文档层 CRUD 桥接
│   ├── search.rs     # 向量内积求导搜索核心
│   └── relations.rs  # AST 层关系映射
├── embedder/         # 文本张量引擎
│   ├── mod.rs        # Embedder 特征约束
│   ├── onnx.rs       # 搭载 ORT Level3 的 ONNX 主引擎
│   ├── mock.rs       # 测试隔离沙箱
│   ├── tokenizer.rs  # HuggingFace BERT 词符切割
│   └── download.rs   # 模型远端直拖链路
├── indexer/          # 知识打散与重塑核心
│   ├── core.rs       # Hash防腐与增量比对守护进程
│   ├── markdown.rs   # MarkDown 分块剥离器
│   ├── code_parser.rs# Tree-sitter 全息代码解析
│   ├── relations.rs  # 结构层关系汲取
│   ├── dictionary.rs # 本地化译丛词典
│   └── languages.rs  # 语言语法探查器组件
└── mcp/              # MCP 桥接通讯层
    ├── server.rs     # stdio 指令生命周期守护长驻进程
    └── tools.rs      # 7 大 Tool 的参数注册并实施路由反射
```

## 语言适配支持层

| 开发语言   | 后缀  | 特化切割探针           |
| ---------- | ----- | ---------------------- |
| Rust       | `.rs` | tree-sitter-rust       |
| Go         | `.go` | tree-sitter-go         |
| Python     | `.py` | tree-sitter-python     |
| TypeScript | `.ts` | tree-sitter-typescript |
| JavaScript | `.js` | tree-sitter-javascript |
| Markdown   | `.md` | pulldown-cmark         |

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
