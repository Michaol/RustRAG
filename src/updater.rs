/// Version update checker module.
///
/// Checks GitHub releases API for newer versions, caches results for 24 hours,
/// and optionally prints update notices to stderr.
/// Mirrors Go version's `internal/updater/updater.go`.
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};
use regex::Regex;
use serde::{Deserialize, Serialize};

// ── Constants ────────────────────────────────────────────────────────

const GITHUB_API_URL: &str = "https://api.github.com/repos/tomohiro-owada/devrag/releases/latest";
const RELEASE_URL: &str = "https://github.com/tomohiro-owada/devrag/releases/latest";
const CHECK_INTERVAL_SECS: u64 = 24 * 60 * 60; // 24 hours
const CACHE_FILENAME: &str = ".rustrag_update_check";

/// Current version from Cargo.toml.
pub const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");

// ── Data structures ──────────────────────────────────────────────────

#[derive(Deserialize)]
struct GitHubRelease {
    tag_name: String,
    #[allow(dead_code)]
    html_url: Option<String>,
}

#[derive(Serialize, Deserialize, Default)]
struct UpdateCache {
    /// Unix timestamp of last check
    last_check: u64,
    latest_version: String,
    notified_version: String,
}

/// Information about an available update.
#[derive(Debug, Clone, Serialize)]
pub struct UpdateInfo {
    pub available: bool,
    pub current_version: String,
    pub latest_version: String,
    pub url: String,
}

// ── Public API ───────────────────────────────────────────────────────

/// Get update info for inclusion in MCP responses.
///
/// Returns `Some(UpdateInfo)` if a newer version is available
/// and the user hasn't been notified within the last 24 hours.
/// Returns `None` otherwise (no update, recently checked, or error).
pub fn get_update_info(current_version: &str, cache_dir: &str) -> Option<UpdateInfo> {
    let cache = load_cache(cache_dir).unwrap_or_default();

    // Already notified recently?
    let now = current_unix_secs();
    if !cache.notified_version.is_empty()
        && now.saturating_sub(cache.last_check) < CHECK_INTERVAL_SECS
    {
        return None;
    }

    // Fetch latest
    let release = fetch_latest_release().ok()?;
    let latest_version = normalize_version(&release.tag_name).ok()?;

    if !is_newer_version(&latest_version, current_version).unwrap_or(false) {
        return None;
    }

    // Update cache
    let mut cache = cache;
    cache.last_check = now;
    cache.latest_version = latest_version.clone();
    cache.notified_version = latest_version.clone();
    let _ = save_cache(cache_dir, &cache);

    Some(UpdateInfo {
        available: true,
        current_version: current_version.to_string(),
        latest_version,
        url: RELEASE_URL.to_string(),
    })
}

/// Check for updates at startup. Prints a notice to stderr if a newer
/// version is available. Errors are silently ignored (best-effort).
pub fn check_for_update(current_version: &str, cache_dir: &str) {
    let mut cache = load_cache(cache_dir).unwrap_or_default();
    let now = current_unix_secs();

    // Skip if checked within 24 hours
    if now.saturating_sub(cache.last_check) < CHECK_INTERVAL_SECS {
        // Still notify if there's a version we haven't told the user about
        if !cache.latest_version.is_empty()
            && cache.notified_version != cache.latest_version
            && is_newer_version(&cache.latest_version, current_version).unwrap_or(false)
        {
            print_update_notice(current_version, &cache.latest_version);
            cache.notified_version = cache.latest_version.clone();
            let _ = save_cache(cache_dir, &cache);
        }
        return;
    }

    // Fetch latest release
    let release = match fetch_latest_release() {
        Ok(r) => r,
        Err(e) => {
            tracing::debug!("Update check failed: {e}");
            return;
        }
    };

    let latest_version = match normalize_version(&release.tag_name) {
        Ok(v) => v,
        Err(e) => {
            tracing::debug!("Invalid version tag {:?}: {e}", release.tag_name);
            return;
        }
    };

    cache.last_check = now;
    cache.latest_version = latest_version.clone();

    if is_newer_version(&latest_version, current_version).unwrap_or(false) {
        print_update_notice(current_version, &latest_version);
        cache.notified_version = latest_version;
    }

    let _ = save_cache(cache_dir, &cache);
}

// ── Internal helpers ─────────────────────────────────────────────────

fn fetch_latest_release() -> Result<GitHubRelease> {
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(5))
        .user_agent("rustrag-update-checker")
        .build()
        .context("HTTP client build failed")?;

    let resp = client
        .get(GITHUB_API_URL)
        .header("Accept", "application/vnd.github.v3+json")
        .send()
        .context("GitHub API request failed")?;

    if !resp.status().is_success() {
        bail!("GitHub API returned status {}", resp.status());
    }

    let release: GitHubRelease = resp.json().context("Failed to parse GitHub response")?;

    if release.tag_name.is_empty() {
        bail!("Empty tag_name in GitHub response");
    }

    Ok(release)
}

/// Extract and validate a semantic version string (e.g., "v1.2.3" → "1.2.3").
fn normalize_version(version: &str) -> Result<String> {
    use std::sync::LazyLock;
    static RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^v?(\d+)\.(\d+)\.(\d+)").unwrap());
    let caps = RE.captures(version).context("invalid semver format")?;

    Ok(format!("{}.{}.{}", &caps[1], &caps[2], &caps[3]))
}

/// Compare two semantic versions. Returns `true` if `latest > current`.
fn is_newer_version(latest: &str, current: &str) -> Result<bool> {
    let latest_parts = parse_version(&normalize_version(latest)?)?;
    let current_parts = parse_version(&normalize_version(current)?)?;

    for (l, c) in latest_parts.iter().zip(current_parts.iter()) {
        if l > c {
            return Ok(true);
        }
        if l < c {
            return Ok(false);
        }
    }

    Ok(latest_parts.len() > current_parts.len())
}

fn parse_version(version: &str) -> Result<Vec<u32>> {
    version
        .split('.')
        .map(|part| {
            part.parse::<u32>()
                .with_context(|| format!("invalid version part: {part}"))
        })
        .collect()
}

fn get_cache_path(cache_dir: &str) -> Result<PathBuf> {
    let dir = if cache_dir.is_empty() {
        dirs::home_dir()
            .map(|d| d.to_string_lossy().to_string())
            .unwrap_or_else(|| ".".to_string())
    } else {
        cache_dir.to_string()
    };
    Ok(Path::new(&dir).join(CACHE_FILENAME))
}

fn load_cache(cache_dir: &str) -> Result<UpdateCache> {
    let path = get_cache_path(cache_dir)?;
    if !path.exists() {
        return Ok(UpdateCache::default());
    }
    let data = fs::read_to_string(&path).context("read cache file")?;
    serde_json::from_str(&data).context("parse cache file")
}

fn save_cache(cache_dir: &str, cache: &UpdateCache) -> Result<()> {
    let path = get_cache_path(cache_dir)?;
    let data = serde_json::to_string(cache).context("serialize cache")?;
    fs::write(&path, data).context("write cache file")
}

fn current_unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_secs()
}

fn print_update_notice(current: &str, latest: &str) {
    let msg = format!("New version available: v{latest} (current: v{current})");
    let url_line = RELEASE_URL;
    let width = msg.len().max(url_line.len()) + 4;
    let border = "─".repeat(width);

    eprintln!();
    eprintln!("  ┌{border}┐");
    eprintln!("  │ {msg:width$}│", width = width - 1);
    eprintln!("  │ {url_line:width$}│", width = width - 1);
    eprintln!("  └{border}┘");
    eprintln!();
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_version() {
        assert_eq!(normalize_version("v1.2.3").unwrap(), "1.2.3");
        assert_eq!(normalize_version("1.2.3").unwrap(), "1.2.3");
        assert_eq!(normalize_version("v0.1.0").unwrap(), "0.1.0");
        assert!(normalize_version("invalid").is_err());
    }

    #[test]
    fn test_is_newer_version() {
        assert!(is_newer_version("1.2.0", "1.1.0").unwrap());
        assert!(is_newer_version("2.0.0", "1.9.9").unwrap());
        assert!(is_newer_version("1.0.1", "1.0.0").unwrap());
        assert!(!is_newer_version("1.0.0", "1.0.0").unwrap());
        assert!(!is_newer_version("0.9.0", "1.0.0").unwrap());
    }

    #[test]
    fn test_parse_version() {
        assert_eq!(parse_version("1.2.3").unwrap(), vec![1, 2, 3]);
        assert_eq!(parse_version("0.0.1").unwrap(), vec![0, 0, 1]);
        assert!(parse_version("abc").is_err());
    }

    #[test]
    fn test_current_version_defined() {
        assert!(!CURRENT_VERSION.is_empty());
        // Should parse as semver
        assert!(normalize_version(CURRENT_VERSION).is_ok());
    }

    #[test]
    fn test_cache_roundtrip() {
        let temp = tempfile::tempdir().unwrap();
        let dir = temp.path().to_string_lossy().to_string();

        let cache = UpdateCache {
            last_check: 1234567890,
            latest_version: "1.0.0".to_string(),
            notified_version: "1.0.0".to_string(),
        };

        save_cache(&dir, &cache).unwrap();
        let loaded = load_cache(&dir).unwrap();

        assert_eq!(loaded.last_check, 1234567890);
        assert_eq!(loaded.latest_version, "1.0.0");
        assert_eq!(loaded.notified_version, "1.0.0");
    }
}
