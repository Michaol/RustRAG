/// Model file auto-download from HuggingFace.
///
/// Downloads the required ONNX model and tokenizer files if they don't
/// already exist locally. Mirrors Go version's `download.go`.
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use tracing::info;

/// Base URL for HuggingFace model files.
const HF_BASE: &str = "https://huggingface.co/intfloat/multilingual-e5-small/resolve/main";

/// Files required for the embedder, with their relative URL paths.
const MODEL_FILES: &[(&str, &str)] = &[
    ("model_O4.onnx", "onnx/model_O4.onnx"),
    ("tokenizer.json", "tokenizer.json"),
    ("config.json", "config.json"),
    ("special_tokens_map.json", "special_tokens_map.json"),
    ("tokenizer_config.json", "tokenizer_config.json"),
];

/// Return the default model directory path.
#[must_use]
pub fn default_model_dir() -> PathBuf {
    PathBuf::from("models/multilingual-e5-small")
}

/// Check whether all required model files exist in `model_dir`.
#[must_use]
pub fn all_files_present(model_dir: &Path) -> bool {
    MODEL_FILES
        .iter()
        .all(|(name, _)| model_dir.join(name).exists())
}

/// Download model files from HuggingFace if any are missing.
///
/// Creates the model directory if it doesn't exist.
/// Skips individual files that are already present.
pub fn download_model_files(model_dir: &Path) -> Result<()> {
    info!("Checking model files in {}", model_dir.display());

    // Create directory
    fs::create_dir_all(model_dir)
        .with_context(|| format!("failed to create models directory: {}", model_dir.display()))?;

    // Quick check: all files present?
    if all_files_present(model_dir) {
        info!("All model files found, skipping download");
        return Ok(());
    }

    eprintln!("[INFO] Downloading model files from HuggingFace...");
    eprintln!("[INFO] This is a one-time download (~235MB), please wait...");

    for &(filename, url_path) in MODEL_FILES {
        let dest = model_dir.join(filename);

        if dest.exists() {
            info!("File already exists: {filename}");
            continue;
        }

        let url = format!("{HF_BASE}/{url_path}");
        eprintln!("[INFO] Downloading {filename}...");
        download_file(&dest, &url).with_context(|| format!("failed to download {filename}"))?;
        eprintln!("[INFO] Downloaded {filename}");
    }

    eprintln!("[INFO] Model download complete!");
    Ok(())
}

/// Download a single file, streaming the response to disk.
fn download_file(dest: &Path, url: &str) -> Result<()> {
    let mut resp =
        reqwest::blocking::get(url).with_context(|| format!("HTTP request failed: {url}"))?;

    if !resp.status().is_success() {
        anyhow::bail!("bad status: {} for {url}", resp.status());
    }

    let mut file = fs::File::create(dest)
        .with_context(|| format!("failed to create file: {}", dest.display()))?;

    resp.copy_to(&mut file)
        .context("failed to download and write file")?;

    Ok(())
}

/// Remove legacy `model.onnx` if `model_O4.onnx` exists (one-time migration).
pub fn cleanup_legacy_model(model_dir: &Path) {
    let legacy = model_dir.join("model.onnx");
    let new_model = model_dir.join("model_O4.onnx");
    if legacy.exists() && new_model.exists() {
        if let Ok(()) = fs::remove_file(&legacy) {
            info!("Removed legacy model.onnx (replaced by model_O4.onnx)");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_all_files_present_empty_dir() {
        let dir = std::env::temp_dir().join("rustrag_test_download_empty");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        assert!(!all_files_present(&dir));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_all_files_present_complete() {
        let dir = std::env::temp_dir().join("rustrag_test_download_complete");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        // Create all required files
        for &(name, _) in MODEL_FILES {
            fs::write(dir.join(name), "dummy").unwrap();
        }

        assert!(all_files_present(&dir));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_all_files_present_partial() {
        let dir = std::env::temp_dir().join("rustrag_test_download_partial");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        // Create only some files
        fs::write(dir.join("tokenizer.json"), "dummy").unwrap();

        assert!(!all_files_present(&dir));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_default_model_dir() {
        let dir = default_model_dir();
        assert!(dir.to_str().unwrap().contains("multilingual-e5-small"));
    }
}
