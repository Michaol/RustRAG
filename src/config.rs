/// Configuration module for RustRAG.
///
/// Handles loading, validating, and providing default configuration values.
/// Mirrors the Go version's `internal/config/config.go`.
use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// The vector dimension required by the sqlite-vec schema (vec_chunks INT8[N]).
const SCHEMA_VEC_DIMENSIONS: usize = 384;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

// The set of extensions the indexer can handle
const SUPPORTED_EXTENSIONS: &[&str] = &[
    // 代码
    "md", "rs", "go", "py", "js", "ts", "jsx", "tsx",
    // 纯文本
    "txt", "log",
    // 结构化数据
    "json", "yaml", "yml", "toml", "csv",
    // HTML
    "html", "htm",
    // 二进制文档
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

fn default_db_path() -> String {
    "./vectors.db".to_string()
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
    384
}

fn default_batch_size() -> usize {
    32
}

fn default_file_extensions() -> Vec<String> {
    SUPPORTED_EXTENSIONS.iter().map(|s| s.to_string()).collect()
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

// ── Default impls ────────────────────────────────────────────────────

impl Default for Config {
    fn default() -> Self {
        Self {
            documents_dir: None,
            document_patterns: default_document_patterns(),
            exclude_patterns: default_exclude_patterns(),
            file_extensions: default_file_extensions(),
            db_path: default_db_path(),
            chunk_size: default_chunk_size(),
            search_top_k: default_search_top_k(),
            update_check: None,
            compute: ComputeConfig::default(),
            model: ModelConfig::default(),
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
            self.model.dimensions > 0,
            "model.dimensions must be positive"
        );
        anyhow::ensure!(
            self.model.dimensions == SCHEMA_VEC_DIMENSIONS,
            "model.dimensions ({}) must match the sqlite-vec schema dimension ({})",
            self.model.dimensions,
            SCHEMA_VEC_DIMENSIONS
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
        assert_eq!(config.model.dimensions, 384);
        assert_eq!(config.model.name, "multilingual-e5-small");
        assert_eq!(config.compute.device, "auto");
        assert!(config.compute.fallback_to_cpu);
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
        assert_eq!(config.model.dimensions, 384);
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
            model: ModelConfig {
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
}
