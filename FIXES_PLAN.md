# RustRAG 代码修复计划

## 🎯 修复目标
修复所有关键问题，确保代码安全、稳定、生产就绪

## 📋 修复清单

### 1. 配置加载 panic 修复 (CRITICAL)
**文件:** `src/config.rs`
**问题:** `unwrap()` 导致 panic
**修复:** 替换为 `?` 运算符和上下文错误

### 2. 外部命令执行风险 (CRITICAL)
**文件:** `src/bin/query.rs`
**问题:** 路径未验证，`expect()` 使用不当
**修复:** 路径验证 + 错误处理

### 3. 嵌入器死锁修复 (HIGH)
**文件:** `src/mcp/server.rs`
**问题:** 嵌套加锁可能导致死锁
**修复:** 使用 `OnceLock` 模式

### 4. 数据库错误上下文 (HIGH)
**文件:** `src/db/relations.rs`
**问题:** 错误信息丢失
**修复:** 添加 `map_err` 上下文

### 5. 输入验证修复 (HIGH)
**文件:** `src/mcp/tools.rs`
**问题:** 目录遍历风险
**修复:** 路径边界检查

## 🔧 修复顺序
1. config.rs - panic 修复
2. relations.rs - 错误上下文
3. server.rs - 死锁修复
4. query.rs - 路径验证
5. tools.rs - 输入验证

## ✅ 验证标准
- 所有 `unwrap()` 替换为 `?` 或 `map_err`
- 所有用户输入路径验证
- 并发安全无死锁
- 错误信息完整上下文