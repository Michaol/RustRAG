/// Configuration module for RustRAG.
///
/// Handles loading, validating, and providing default configuration values.
/// Mirrors the Go version's `internal/config/config.go`.
use std::collections::HashSet;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

// ── Default value functions ──────────────────────────────────────────

fn default_document_patterns() -> Vec<String> {
    vec!["./".to_string()]
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

// ── Config structs ───────────────────────────────────────────────────

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Config {
    /// Deprecated: use `document_patterns` instead.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub documents_dir: Option<String>,

    #[serde(default = "default_document_patterns")]
    pub document_patterns: Vec<String>,

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
}

// ── Default impls ────────────────────────────────────────────────────

impl Default for Config {
    fn default() -> Self {
        Self {
            documents_dir: None,
            document_patterns: default_document_patterns(),
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

        // Check if config file exists
        if !Path::new(path).exists() {
            info!("{path} not found, using defaults");
            let cfg = Self::default();

            // Generate template only for the default path
            if path == "config.json" {
                match cfg.save(path) {
                    Ok(()) => info!("Generated config template: {path}"),
                    Err(e) => warn!("Failed to generate config template: {e}"),
                }
            }

            return Ok(cfg);
        }

        // Read existing config
        let data = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read config: {path}"))?;

        // Parse with defaults
        let mut cfg: Config = match serde_json::from_str(&data) {
            Ok(c) => c,
            Err(e) => {
                warn!("Invalid JSON in {path}: {e}");
                warn!("Using default configuration");
                return Ok(Self::default());
            }
        };

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

/// Expand a single pattern to matching markdown files.
fn expand_pattern(pattern: &str) -> Result<Vec<PathBuf>> {
    // If pattern contains no wildcards, treat as a directory
    if !pattern.contains('*') && !pattern.contains('?') {
        return walk_dir_for_md(Path::new(pattern));
    }

    // Handle ** (recursive glob)
    if pattern.contains("**") {
        return expand_double_star(pattern);
    }

    // Simple glob
    let matches = glob::glob(pattern).context("invalid glob pattern")?;
    let mut files = Vec::new();
    for entry in matches.flatten() {
        if entry.is_file() && entry.extension().and_then(|e| e.to_str()) == Some("md") {
            files.push(entry);
        }
    }
    Ok(files)
}

/// Walk a directory recursively, collecting `.md` files.
fn walk_dir_for_md(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    if !dir.exists() {
        return Ok(files);
    }
    for entry in walkdir(dir)? {
        if entry.is_file() && entry.extension().and_then(|e| e.to_str()) == Some("md") {
            files.push(entry);
        }
    }
    Ok(files)
}

/// Simple recursive directory walk (no external dependency).
fn walkdir(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut result = Vec::new();
    if !dir.is_dir() {
        return Ok(result);
    }
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            result.extend(walkdir(&path)?);
        } else {
            result.push(path);
        }
    }
    Ok(result)
}

/// Expand patterns containing `**`.
fn expand_double_star(pattern: &str) -> Result<Vec<PathBuf>> {
    let parts: Vec<&str> = pattern.splitn(2, "**").collect();
    if parts.len() != 2 {
        anyhow::bail!("invalid ** pattern: {pattern}");
    }

    let mut base_dir = parts[0].to_string();
    let suffix = parts[1].trim_start_matches(['/', '\\']);

    if base_dir.is_empty() {
        base_dir = ".".to_string();
    } else {
        base_dir = base_dir.trim_end_matches(['/', '\\']).to_string();
    }

    let all_files = walkdir(Path::new(&base_dir))?;
    let mut files = Vec::new();

    for path in all_files {
        if !path.is_file() {
            continue;
        }
        let is_md = path.extension().and_then(|e| e.to_str()) == Some("md");
        if !is_md {
            continue;
        }

        if suffix.is_empty() || suffix == "*.md" {
            files.push(path);
        } else if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            // Check simple pattern like "*.md"
            let matched = glob::Pattern::new(suffix)
                .map(|p| p.matches(name))
                .unwrap_or(false);
            if matched {
                files.push(path);
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
        let mut config = Config::default();
        config.chunk_size = 0;
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_validate_empty_patterns() {
        let mut config = Config::default();
        config.document_patterns = vec![];
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
}
