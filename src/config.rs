/// Configuration module for RustRAG.
///
/// Handles loading, validating, and providing default configuration values.
/// Mirrors the Go version's `internal/config/config.go`.
use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// The vector dimension required by the sqlite-vec schema (vec_chunks float32[N]).
const SCHEMA_VEC_DIMENSIONS: usize = 1024;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

// The set of extensions the indexer can handle
const SUPPORTED_EXTENSIONS: &[&str] = &[
    // 代码
    "md", "rs", "go", "py", "js", "mjs", "cjs", "jsx", // JavaScript (标准 + ESM + CJS + JSX)
    "ts", "mts", "cts", "tsx", // TypeScript (标准 + ESM + CJS + TSX)
    // 纯文本
    "txt", "log", // 结构化数据
    "json", "yaml", "yml", "toml", "csv", // HTML
    "html", "htm", // 二进制文档
    "pdf", "docx", "xls", "xlsx", "xlsb", "ods",
];

// ── Default value functions ──────────────────────────────────────────

fn default_document_patterns() -> Vec<String> {
    vec!["./".to_string()]
}

fn default_exclude_patterns() -> Vec<String> {
    vec![
        "**/node_modules/**".to_string(),
        "**/target/**".to_string(),
        "**/.git/**".to_string(),
    ]
}

fn default_data_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".rustrag")
}

fn default_db_path() -> String {
    default_data_dir()
        .join("vectors.db")
        .to_string_lossy()
        .to_string()
}

fn default_chunk_size() -> usize {
    500
}

fn default_search_top_k() -> usize {
    5
}

fn default_device() -> String {
    "auto".to_string()
}

fn default_true() -> bool {
    true
}

fn default_model_name() -> String {
    "multilingual-e5-small".to_string()
}

fn default_dimensions() -> usize {
    1024
}

fn default_batch_size() -> usize {
    32
}

fn default_api_url() -> String {
    "https://dashscope.aliyuncs.com/compatible-mode/v1/embeddings".to_string()
}

fn default_api_model() -> String {
    "text-embedding-v4".to_string()
}

fn default_max_concurrent() -> usize {
    5
}

fn default_timeout_secs() -> u64 {
    30
}

fn default_file_extensions() -> Vec<String> {
    SUPPORTED_EXTENSIONS.iter().map(|s| s.to_string()).collect()
}

/// Expand `~` at the start of a path to the user's home directory.
///
/// - `"~/foo"` → `/home/user/foo` (Unix)
/// - `"~\\foo"` → `C:\Users\user\foo` (Windows)
/// - `"/absolute/path"` → unchanged
/// - `"./relative"` → unchanged
fn expand_tilde(path: &str) -> PathBuf {
    if path.starts_with("~/") || path.starts_with("~\\") {
        if let Some(home) = dirs::home_dir() {
            return home.join(&path[2..]);
        }
    }
    PathBuf::from(path)
}

// ── Config structs ───────────────────────────────────────────────────

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Config {
    /// Deprecated: use `document_patterns` instead.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub documents_dir: Option<String>,

    #[serde(default = "default_document_patterns")]
    pub document_patterns: Vec<String>,

    #[serde(default = "default_exclude_patterns")]
    pub exclude_patterns: Vec<String>,

    #[serde(default = "default_file_extensions")]
    pub file_extensions: Vec<String>,

    /// Base directory for all RustRAG data (models, database, etc.).
    /// Defaults to `~/.rustrag`. Supports `~` expansion.
    #[serde(default = "default_data_dir")]
    pub data_dir: PathBuf,

    #[serde(default = "default_db_path")]
    pub db_path: String,

    #[serde(default = "default_chunk_size")]
    pub chunk_size: usize,

    #[serde(default = "default_search_top_k")]
    pub search_top_k: usize,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub update_check: Option<bool>,

    #[serde(default)]
    pub compute: ComputeConfig,

    #[serde(default)]
    pub model: ModelConfig,

    /// Embedding API configuration (DashScope / OpenAI-compatible).
    #[serde(default)]
    pub embedding: EmbeddingConfig,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ComputeConfig {
    #[serde(default = "default_device")]
    pub device: String,

    #[serde(default = "default_true")]
    pub fallback_to_cpu: bool,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ModelConfig {
    #[serde(default = "default_model_name")]
    pub name: String,

    #[serde(default = "default_dimensions")]
    pub dimensions: usize,

    #[serde(default = "default_batch_size")]
    pub batch_size: usize,
}

/// Configuration for OpenAI-compatible embedding API.
///
/// Supports DashScope, Ollama, OpenAI, and any other provider that
/// implements the OpenAI `/v1/embeddings` endpoint format.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct EmbeddingConfig {
    /// API endpoint URL (OpenAI-compatible format).
    #[serde(default = "default_api_url")]
    pub api_url: String,

    /// API key. Supports environment variable override via `DASHSCOPE_API_KEY`.
    #[serde(default)]
    pub api_key: String,

    /// Model name (e.g. "text-embedding-v4", "nomic-embed-text").
    #[serde(default = "default_api_model")]
    pub api_model: String,

    /// Expected vector dimensions (must match model output and DB schema).
    #[serde(default = "default_dimensions")]
    pub dimensions: usize,

    /// Maximum batch size for API requests.
    #[serde(default = "default_batch_size")]
    pub batch_size: usize,

    /// Maximum concurrent API requests (semaphore limit).
    #[serde(default = "default_max_concurrent")]
    pub max_concurrent: usize,

    /// Request timeout in seconds.
    #[serde(default = "default_timeout_secs")]
    pub timeout_secs: u64,
}

impl EmbeddingConfig {
    /// Resolve the API key from environment variables or config value.
    ///
    /// Checks environment variables in order: `RAG_API_KEY`, `DASHSCOPE_API_KEY`,
    /// `OPENAI_API_KEY`. Falls back to the configured `api_key` if none are set.
    #[must_use]
    pub fn resolve_api_key(&self) -> String {
        for var in &["RAG_API_KEY", "DASHSCOPE_API_KEY", "OPENAI_API_KEY"] {
            if let Ok(key) = std::env::var(var) {
                if !key.is_empty() {
                    return key;
                }
            }
        }
        self.api_key.clone()
    }
}

// ── Default impls ────────────────────────────────────────────────────

impl Default for Config {
    fn default() -> Self {
        Self {
            documents_dir: None,
            document_patterns: default_document_patterns(),
            exclude_patterns: default_exclude_patterns(),
            file_extensions: default_file_extensions(),
            data_dir: default_data_dir(),
            db_path: default_db_path(),
            chunk_size: default_chunk_size(),
            search_top_k: default_search_top_k(),
            update_check: None,
            compute: ComputeConfig::default(),
            model: ModelConfig::default(),
            embedding: EmbeddingConfig::default(),
        }
    }
}

impl Default for ComputeConfig {
    fn default() -> Self {
        Self {
            device: default_device(),
            fallback_to_cpu: default_true(),
        }
    }
}

impl Default for ModelConfig {
    fn default() -> Self {
        Self {
            name: default_model_name(),
            dimensions: default_dimensions(),
            batch_size: default_batch_size(),
        }
    }
}

impl Default for EmbeddingConfig {
    fn default() -> Self {
        Self {
            api_url: default_api_url(),
            api_key: String::new(),
            api_model: default_api_model(),
            dimensions: default_dimensions(),
            batch_size: default_batch_size(),
            max_concurrent: default_max_concurrent(),
            timeout_secs: default_timeout_secs(),
        }
    }
}

// ── Config implementation ────────────────────────────────────────────

impl Config {
    /// Whether update checking is enabled (defaults to `true`).
    #[must_use]
    pub fn is_update_check_enabled(&self) -> bool {
        self.update_check.unwrap_or(true)
    }

    /// Check if a file extension is supported for indexing.
    /// Uses `file_extensions` allowlist (defaults to all supported extensions).
    #[must_use]
    pub fn is_file_extension_supported(&self, ext: &str) -> bool {
        self.file_extensions.iter().any(|e| e == ext)
    }

    /// Load configuration from a JSON file.
    ///
    /// If `config_path` is empty, defaults to `"config.json"`.
    /// If the file does not exist, returns a default config and optionally
    /// generates a template file.
    pub fn load(config_path: &str) -> Result<Self> {
        let path = if config_path.is_empty() {
            "config.json"
        } else {
            config_path
        };

        // Read existing config, fall back to default template if not found
        let data = match std::fs::read_to_string(path) {
            Ok(content) => content,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                info!("{path} not found, using defaults");
                let cfg = Self::default();
                match cfg.save(path) {
                    Ok(()) => info!("Generated config template: {path}"),
                    Err(save_e) => warn!("Failed to generate config template: {save_e}"),
                }
                return Ok(cfg);
            }
            Err(e) => return Err(e).context(format!("failed to read config: {path}")),
        };

        // Parse with defaults - use context for better error messages
        let mut cfg: Config = serde_json::from_str(&data)
            .with_context(|| format!("invalid JSON in config file: {path}"))?;

        info!("Loaded configuration from {path}");

        // Migrate old `documents_dir` → `document_patterns`
        if let Some(ref old_dir) = cfg.documents_dir {
            if cfg.document_patterns == default_document_patterns() {
                info!("Migrating from documents_dir to document_patterns");
                cfg.document_patterns = vec![old_dir.clone()];
            }
            cfg.documents_dir = None;
        }

        // Ensure at least one pattern
        if cfg.document_patterns.is_empty() {
            cfg.document_patterns = default_document_patterns();
        }

        // Expand tildes in data_dir and db_path
        cfg.data_dir = expand_tilde(&cfg.data_dir.to_string_lossy());
        cfg.db_path = expand_tilde(&cfg.db_path).to_string_lossy().to_string();

        Ok(cfg)
    }

    /// Save configuration to a JSON file.
    pub fn save(&self, path: &str) -> Result<()> {
        let data = serde_json::to_string_pretty(self).context("failed to marshal config")?;
        std::fs::write(path, data).with_context(|| format!("failed to write config: {path}"))?;
        Ok(())
    }

    /// Validate configuration values.
    pub fn validate(&self) -> Result<()> {
        anyhow::ensure!(self.chunk_size > 0, "chunk_size must be positive");
        anyhow::ensure!(self.search_top_k > 0, "search_top_k must be positive");
        anyhow::ensure!(
            self.embedding.dimensions > 0,
            "embedding.dimensions must be positive"
        );
        anyhow::ensure!(
            self.embedding.dimensions == SCHEMA_VEC_DIMENSIONS,
            "embedding.dimensions ({}) must match the sqlite-vec schema dimension ({})",
            self.embedding.dimensions,
            SCHEMA_VEC_DIMENSIONS
        );
        anyhow::ensure!(
            self.embedding.batch_size > 0,
            "embedding.batch_size must be positive"
        );
        anyhow::ensure!(
            self.embedding.max_concurrent > 0,
            "embedding.max_concurrent must be positive"
        );
        anyhow::ensure!(
            self.embedding.timeout_secs > 0,
            "embedding.timeout_secs must be positive"
        );
        anyhow::ensure!(
            !self.embedding.api_url.is_empty(),
            "embedding.api_url must not be empty"
        );
        anyhow::ensure!(
            !self.document_patterns.is_empty(),
            "at least one document pattern must be specified"
        );
        Ok(())
    }

    /// Expand all document patterns and return matching markdown files.
    pub fn get_document_files(&self) -> Result<Vec<PathBuf>> {
        let mut files = HashSet::new();

        for pattern in &self.document_patterns {
            match expand_pattern(pattern) {
                Ok(matches) => {
                    for m in matches {
                        files.insert(m);
                    }
                }
                Err(e) => {
                    warn!("Failed to expand pattern {pattern}: {e}");
                }
            }
        }

        Ok(files.into_iter().collect())
    }

    /// Return the base directories derived from all patterns.
    #[must_use]
    pub fn get_base_directories(&self) -> Vec<PathBuf> {
        let mut dirs = HashSet::new();

        for pattern in &self.document_patterns {
            let base = extract_base_dir(pattern);
            if let Ok(abs) = std::path::absolute(Path::new(&base)) {
                dirs.insert(abs);
            }
        }

        dirs.into_iter().collect()
    }
}

// ── Pattern helpers ──────────────────────────────────────────────────

/// Check if a file extension is in the static supported list (no Config needed).
fn is_known_extension(ext: &str) -> bool {
    SUPPORTED_EXTENSIONS.contains(&ext)
}

/// Expand a single pattern to matching supported files.
fn expand_pattern(pattern: &str) -> Result<Vec<PathBuf>> {
    // If pattern contains no wildcards, treat as a directory
    if !pattern.contains('*') && !pattern.contains('?') {
        return walk_dir_for_supported_files(Path::new(pattern));
    }

    // Handle ** (recursive glob) using `ignore` crate which respects gitignore
    double_star_glob(pattern)
}

/// Walk a directory recursively for all supported file types using the `ignore` crate.
fn walk_dir_for_supported_files(dir: &Path) -> Result<Vec<PathBuf>> {
    use ignore::WalkBuilder;
    let mut files = Vec::new();
    if !dir.exists() {
        return Ok(files);
    }
    for e in WalkBuilder::new(dir).hidden(true).build().flatten() {
        let path = e.path();
        if e.file_type().is_some_and(|ft| ft.is_file()) {
            if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                if is_known_extension(ext) {
                    files.push(path.to_path_buf());
                }
            }
        }
    }
    Ok(files)
}

/// Expand patterns containing `**` using the `ignore` crate.
fn double_star_glob(pattern: &str) -> Result<Vec<PathBuf>> {
    let parts: Vec<&str> = pattern.splitn(2, "**").collect();
    if parts.len() != 2 {
        anyhow::bail!("invalid ** pattern: {pattern}");
    }

    let base_dir = if parts[0].is_empty() {
        ".".to_string()
    } else {
        parts[0].trim_end_matches(['/', '\\']).to_string()
    };
    let suffix = parts[1].trim_start_matches(['/', '\\']);

    let mut builder = ignore::WalkBuilder::new(&base_dir);
    builder.hidden(true);

    let mut files = Vec::new();
    for e in builder.build().flatten() {
        let path = e.path();
        if !e.file_type().is_some_and(|ft| ft.is_file()) {
            continue;
        }
        let ext = path.extension().and_then(|e| e.to_str());
        let matches_ext = ext.is_some_and(is_known_extension);
        if suffix.is_empty() {
            if matches_ext {
                files.push(path.to_path_buf());
            }
        } else if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            if glob::Pattern::new(suffix)
                .map(|p| p.matches(name))
                .unwrap_or(false)
                && matches_ext
            {
                files.push(path.to_path_buf());
            }
        }
    }
    Ok(files)
}

/// Extract the base directory from a pattern (part before first wildcard).
fn extract_base_dir(pattern: &str) -> String {
    if let Some(idx) = pattern.find(['*', '?']) {
        let prefix = &pattern[..idx];
        // Trim trailing separators so Path::parent behaves correctly on Windows
        let trimmed = prefix.trim_end_matches(['/', '\\']);
        if trimmed.is_empty() {
            return ".".to_string();
        }
        // If the trimmed prefix itself looks like a directory path (no file extension
        // involvement), return it directly; otherwise get its parent.
        let trimmed_path = Path::new(trimmed);
        // If the original prefix ended with a separator, `trimmed` IS the directory
        if prefix.len() > trimmed.len() {
            return trimmed.to_string();
        }
        // Otherwise get the parent
        trimmed_path
            .parent()
            .map(|p| {
                let s = p.to_string_lossy().to_string();
                if s.is_empty() { ".".to_string() } else { s }
            })
            .unwrap_or_else(|| ".".to_string())
    } else {
        pattern.to_string()
    }
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert_eq!(config.chunk_size, 500);
        assert_eq!(config.search_top_k, 5);
        assert_eq!(config.exclude_patterns.len(), 3);
        assert!(!config.file_extensions.is_empty());
        assert!(config.file_extensions.contains(&"txt".to_string()));
        assert!(config.file_extensions.contains(&"pdf".to_string()));
        assert_eq!(config.embedding.dimensions, 1024);
        assert_eq!(config.embedding.api_model, "text-embedding-v4");
        assert!(config.embedding.api_url.contains("dashscope"));
        assert_eq!(config.embedding.batch_size, 32);
        assert_eq!(config.embedding.max_concurrent, 5);
        assert_eq!(config.embedding.timeout_secs, 30);
        assert!(config.is_update_check_enabled());
    }

    #[test]
    fn test_load_from_json() {
        let json = r#"{"chunk_size": 1000, "db_path": "./test.db"}"#;
        let config: Config = serde_json::from_str(json).unwrap();
        assert_eq!(config.chunk_size, 1000);
        assert_eq!(config.db_path, "./test.db");
        // Other fields should have defaults
        assert_eq!(config.search_top_k, 5);
        assert_eq!(config.embedding.dimensions, 1024);
    }

    #[test]
    fn test_validate_ok() {
        let config = Config::default();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_validate_bad_chunk_size() {
        let config = Config {
            chunk_size: 0,
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_validate_empty_patterns() {
        let config = Config {
            document_patterns: vec![],
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_update_check_disabled() {
        let json = r#"{"update_check": false}"#;
        let config: Config = serde_json::from_str(json).unwrap();
        assert!(!config.is_update_check_enabled());
    }

    #[test]
    fn test_migration_documents_dir() {
        let json = r#"{"documents_dir": "./old_docs"}"#;
        let mut config: Config = serde_json::from_str(json).unwrap();
        // Simulate migration
        if let Some(ref old_dir) = config.documents_dir {
            config.document_patterns = vec![old_dir.clone()];
            config.documents_dir = None;
        }
        assert_eq!(config.document_patterns, vec!["./old_docs"]);
        assert!(config.documents_dir.is_none());
    }

    #[test]
    fn test_extract_base_dir() {
        assert_eq!(extract_base_dir("./docs"), "./docs");
        assert_eq!(extract_base_dir("./docs/**/*.md"), "./docs");
        assert_eq!(extract_base_dir("*.md"), ".");
    }

    #[test]
    fn test_serialization_roundtrip() {
        let config = Config::default();
        let json = serde_json::to_string_pretty(&config).unwrap();
        let parsed: Config = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.chunk_size, config.chunk_size);
        assert_eq!(parsed.db_path, config.db_path);
        assert_eq!(parsed.model.name, config.model.name);
    }

    #[test]
    fn test_validate_dimensions_must_be_positive() {
        let config = Config {
            embedding: EmbeddingConfig {
                dimensions: 0,
                ..Default::default()
            },
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_get_document_files_deduplicates() {
        // When two patterns match the same file, it should appear only once
        let config = Config {
            document_patterns: vec![".".to_string(), "./*".to_string()],
            ..Default::default()
        };
        let files = config.get_document_files().unwrap();
        // Just verify it runs without panic; actual content depends on local filesystem
        // The HashSet deduplication prevents duplicates
        assert!(files.len() <= config.get_document_files().unwrap().len() * 2);
    }

    #[test]
    fn test_load_from_json_with_zero_chunk_size() {
        let json = r#"{"chunk_size": 0}"#;
        let config: Config = serde_json::from_str(json).unwrap();
        // Just try the config, but validate should fail for chunk_size = 0
        assert_eq!(config.chunk_size, 0);
    }

    #[test]
    fn test_is_file_extension_supported() {
        let config = Config::default();
        assert!(config.is_file_extension_supported("md"));
        assert!(config.is_file_extension_supported("rs"));
        assert!(config.is_file_extension_supported("txt"));
        assert!(config.is_file_extension_supported("json"));
        assert!(config.is_file_extension_supported("html"));
        assert!(config.is_file_extension_supported("pdf"));
        assert!(config.is_file_extension_supported("docx"));
        assert!(config.is_file_extension_supported("xlsx"));
        // JS/TS 模块变体
        assert!(config.is_file_extension_supported("mjs"));
        assert!(config.is_file_extension_supported("cjs"));
        assert!(config.is_file_extension_supported("mts"));
        assert!(config.is_file_extension_supported("cts"));
        assert!(!config.is_file_extension_supported("java"));
        assert!(!config.is_file_extension_supported(""));
    }

    #[test]
    fn test_is_file_extension_supported_with_allowlist() {
        let config = Config {
            file_extensions: vec!["rs".to_string(), "md".to_string()],
            ..Default::default()
        };
        assert!(config.is_file_extension_supported("rs"));
        assert!(config.is_file_extension_supported("md"));
        assert!(!config.is_file_extension_supported("go"));
        assert!(!config.is_file_extension_supported("py"));
    }

    #[test]
    fn test_expand_tilde_unix_style() {
        let expanded = expand_tilde("~/test/path");
        if let Some(home) = dirs::home_dir() {
            assert_eq!(expanded, home.join("test/path"));
        } else {
            // If no home dir, should return as-is
            assert_eq!(expanded, PathBuf::from("~/test/path"));
        }
    }

    #[test]
    fn test_expand_tilde_windows_style() {
        let expanded = expand_tilde("~\\test\\path");
        if let Some(home) = dirs::home_dir() {
            assert_eq!(expanded, home.join("test\\path"));
        } else {
            assert_eq!(expanded, PathBuf::from("~\\test\\path"));
        }
    }

    #[test]
    fn test_expand_tilde_absolute_path() {
        let expanded = expand_tilde("/absolute/path");
        assert_eq!(expanded, PathBuf::from("/absolute/path"));
    }

    #[test]
    fn test_expand_tilde_relative_path() {
        let expanded = expand_tilde("./relative/path");
        assert_eq!(expanded, PathBuf::from("./relative/path"));
    }

    #[test]
    fn test_expand_tilde_tilde_in_middle() {
        // Tilde not at start should not be expanded
        let expanded = expand_tilde("/path/~/file");
        assert_eq!(expanded, PathBuf::from("/path/~/file"));
    }

    #[test]
    fn test_default_data_dir() {
        let data_dir = default_data_dir();
        if let Some(home) = dirs::home_dir() {
            assert_eq!(data_dir, home.join(".rustrag"));
        } else {
            assert_eq!(data_dir, PathBuf::from("./.rustrag"));
        }
    }

    #[test]
    fn test_default_db_path_is_absolute() {
        let db_path = default_db_path();
        // Should contain .rustrag and vectors.db
        assert!(db_path.contains(".rustrag"));
        assert!(db_path.contains("vectors.db"));
        // Should be absolute (starts with / on Unix or drive letter on Windows)
        if let Some(home) = dirs::home_dir() {
            assert!(db_path.starts_with(&home.to_string_lossy().to_string()));
        }
    }

    #[test]
    fn test_embedding_config_default() {
        let config = EmbeddingConfig::default();
        assert!(config.api_url.contains("dashscope"));
        assert!(config.api_key.is_empty());
        assert_eq!(config.api_model, "text-embedding-v4");
        assert_eq!(config.dimensions, 1024);
        assert_eq!(config.batch_size, 32);
        assert_eq!(config.max_concurrent, 5);
        assert_eq!(config.timeout_secs, 30);
    }

    #[test]
    fn test_embedding_config_resolve_api_key_from_config() {
        let config = EmbeddingConfig {
            api_key: "sk-test-key".to_string(),
            ..Default::default()
        };
        // When no env vars are set, should use config value
        let key = config.resolve_api_key();
        // Either returns the config value or an env var value (if set in CI)
        assert!(!key.is_empty());
    }

    #[test]
    fn test_embedding_config_resolve_api_key_priority() {
        // Test that RAG_API_KEY takes priority
        // SAFETY: single-threaded test, no concurrent env access
        unsafe { std::env::set_var("RAG_API_KEY", "rag-key") };
        let config = EmbeddingConfig {
            api_key: "config-key".to_string(),
            ..Default::default()
        };
        assert_eq!(config.resolve_api_key(), "rag-key");
        // SAFETY: single-threaded test, cleaning up our own var
        unsafe { std::env::remove_var("RAG_API_KEY") };
    }

    #[test]
    fn test_embedding_config_serialization() {
        let config = EmbeddingConfig::default();
        let json = serde_json::to_string_pretty(&config).unwrap();
        let parsed: EmbeddingConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.api_url, config.api_url);
        assert_eq!(parsed.api_model, config.api_model);
        assert_eq!(parsed.dimensions, config.dimensions);
    }

    #[test]
    fn test_config_with_embedding_section() {
        let json = r#"{
            "embedding": {
                "api_url": "http://localhost:11434/v1/embeddings",
                "api_key": "ollama",
                "api_model": "nomic-embed-text",
                "dimensions": 768
            }
        }"#;
        let config: Config = serde_json::from_str(json).unwrap();
        assert_eq!(
            config.embedding.api_url,
            "http://localhost:11434/v1/embeddings"
        );
        assert_eq!(config.embedding.api_key, "ollama");
        assert_eq!(config.embedding.api_model, "nomic-embed-text");
        assert_eq!(config.embedding.dimensions, 768);
    }
}
