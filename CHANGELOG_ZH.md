# 更新日志

RustRAG 全部重要版本变更记录。

[🇬🇧 English Changelog](CHANGELOG.md)

---

## v2.4.3 — sqlite-vec 初始化顺序修复

修复 sqlite-vec 自动扩展注册顺序 —— 将 `sqlite3_auto_extension` 调用移至 `Connection::open()` **之前**，确保 r2d2 连接池中每个连接都正确加载扩展。

- **修复初始化顺序**：SQLite 自动扩展仅对注册后创建的连接生效。
- **注意**：需搭配 v2.4.1+（`sqlite-vec 0.1.9`）使用。

## v2.4.2 — 文件监听器修复

修复后台文件监听器的 Bug：即使 `exclude_patterns` 中配置了 `target`、`node_modules` 等忽略目录，热重载期间这些目录内的文件仍会被意外索引。

- **监听器过滤增强**：全面应用 `exclude_patterns` 规则（底层 `ignore::OverrideBuilder`）。

## v2.4.1 — sqlite-vec 升级

维护版本，将 `sqlite-vec` 从 `0.1.7-alpha.10` 升级至稳定版 `0.1.9`。

- 修复部分平台 `no such function: vec_version` 报错。
- **注意**：从 v2.4.0 或更早升级时，需删除 `vectors.db` 并重启。

## v2.4.0 — 多格式文档支持

将 RustRAG 从代码索引引擎升级为通用文档 RAG 引擎。

- **24 种文件类型**：纯文本、JSON、YAML、TOML、CSV、HTML、PDF、DOCX、电子表格（XLS/XLSX/XLSB/ODS）。
- **格式专属分块**：保留结构信息（JSON 键路径、CSV 表头等）。
- **新依赖**（纯 Rust）：`lopdf`、`docx-rs`、`calamine`、`scraper`、`toml`、`csv`。
- 补入 `.jsx`/`.tsx` 到支持的代码扩展名。

## v2.3.0 — 安全加固与代码质量

系统化代码审查修复 26 项问题：

- **安全**：修复 Windows 路径校验，限制 MCP 工具任意文件读取，HTTP 绑定 localhost。
- **可靠性**：`assert_eq!` panic 替换为错误传播，修复索引计数器。
- **配置**：无效 JSON 返回错误而非静默回退；启动时校验维度一致性。
- **国际化**：新增日语（平假名/片假名）和韩语（韩文）识别。
- **性能**：ONNX 线程自动检测，`LazyLock` 缓存 `LanguageConfig`。
- **代码质量**：移除 PHP 死代码，修复 TOCTOU 竞态，添加 `// SAFETY:` 文档。

## v2.2.0 — 架构深度重构

针对高并发和异步可靠性的架构重构：

- **连接池**：`r2d2` 封装 `sqlite-vec`，安全多线程访问。
- **异步网络**：版本检查器从 `reqwest::blocking` 迁移至原生异步。
- **配置安全**：消除配置加载中的 TOCTOU 竞态。

## v2.1.0 — 高级功能改进

增强功能、性能和可靠性。

## v2.0.0 — ONNX 模型迁移

从 `model.onnx`（470MB）迁移至 `model_O4.onnx`（235MB）：

- ONNX Level 4 图优化 —— 向量输出一致，无需重建索引。
- 下载体积和运行时内存均减半（~500MB → ~250MB）。
- 自动检测并清理旧版模型文件。

## v1.3.7 — 配置热重载

基于 `RwLock` 的配置与模型实例热重载：

- GPU 推理引擎热重载（修改 `device` 策略后下次请求自动生效）。
- 配置文件变更自动同步文件监听任务。

## v1.3.6 — 硬件加速优化版

- 多平台 GPU 加速（CUDA、TensorRT、DirectML、CoreML），安全 CPU 回退。
- `batch_size` 调优与 `compute.fallback_to_cpu` 降级处理。
- 后台文件监听与增量同步。
- SQLite WAL 模式，解决并发写入锁定。
- 颗粒度 MCP 错误处理。

## v1.2.0 & v1.1.0 — 性能与体积优化版

- **INT8 标量量化**：`FLOAT[384]` → `INT8[384]`，磁盘占用降低约 75%。
- **ONNX Level 3 图优化**：提升 CPU 推理性能。
- **动态清理**：修改 `exclude_patterns` 后自动清除过期文档。

> ⚠️ 从 v1.1.x 升级需手动删除旧 `vectors.db`。
