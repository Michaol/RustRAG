use crate::indexer::core::{Indexer, normalize_system_path};
use crate::mcp::server::McpContext;
use notify::{EventKind, RecursiveMode, Watcher};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tracing::{error, info, warn};

/// Starts the file system watcher in the background.
pub async fn start_watcher(ctx: McpContext) {
    let watch_dirs = ctx.config.read().await.get_base_directories();
    let config_path = PathBuf::from(&ctx.config_path);

    let (tx, mut rx) = mpsc::channel::<PathBuf>(1000);

    let watcher_res = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
        if let Ok(event) = res {
            match event.kind {
                EventKind::Modify(_) | EventKind::Create(_) | EventKind::Remove(_) => {
                    for path in event.paths {
                        let _ = tx.blocking_send(path);
                    }
                }
                _ => {}
            }
        }
    });

    let mut watcher = match watcher_res {
        Ok(w) => w,
        Err(e) => {
            error!("Failed to initialize file watcher: {}", e);
            return;
        }
    };

    // Watch config file itself
    if config_path.exists() {
        if let Err(e) = watcher.watch(&config_path, RecursiveMode::NonRecursive) {
            warn!("Failed to watch config file {}: {}", config_path.display(), e);
        } else {
            info!("Watching config file for hot-reloads: {}", config_path.display());
        }
    }

    // Watch data directories
    for dir in &watch_dirs {
        if dir.exists() {
            if let Err(e) = watcher.watch(dir, RecursiveMode::Recursive) {
                warn!("Failed to watch directory {}: {}", dir.display(), e);
            } else {
                info!("Watching directory for changes: {}", dir.display());
            }
        }
    }

    // Spawn tokio task to process events and keep watcher alive
    tokio::spawn(async move {
        // Tie watcher lifetime to this long-running async task
        let mut _keep_watcher_alive = watcher;
        let mut current_watch_dirs = watch_dirs;
        let config_path_clone = config_path.clone();

        loop {
            let mut paths_to_process = std::collections::HashSet::new();

            // Wait for the first event
            if let Some(path) = rx.recv().await {
                paths_to_process.insert(path);

                // Wait up to 2 seconds for more events to batch them (debounce)
                let _ = tokio::time::timeout(Duration::from_millis(2000), async {
                    while let Some(p) = rx.recv().await {
                        paths_to_process.insert(p);
                    }
                })
                .await;

                let mut config_reloaded = false;
                let config_canon = config_path_clone.canonicalize().unwrap_or_else(|_| config_path_clone.clone());

                for p in &paths_to_process {
                    let p_canon = p.canonicalize().unwrap_or_else(|_| p.clone());
                    
                    if p_canon == config_canon {
                        info!("Config file modified! Triggering hot reload...");
                        match crate::config::Config::load(&ctx.config_path) {
                            Ok(new_config) => {
                                ctx.reload_config(new_config).await;
                                config_reloaded = true;
                            }
                            Err(e) => {
                                error!("Failed to parse new config, ignoring reload: {}", e);
                            }
                        }
                    }
                }

                // If configuration altered, reset directory physical watchers
                if config_reloaded {
                    let new_dirs = ctx.config.read().await.get_base_directories();
                    
                    // Unwatch old and watch new
                    for old_dir in &current_watch_dirs {
                        let _ = _keep_watcher_alive.unwatch(old_dir);
                    }
                    for new_dir in &new_dirs {
                        if new_dir.exists() {
                            if let Err(e) = _keep_watcher_alive.watch(new_dir, RecursiveMode::Recursive) {
                                warn!("Failed to dynamically watch new directory {}: {}", new_dir.display(), e);
                            } else {
                                info!("Dynamically watching directory for changes: {}", new_dir.display());
                            }
                        }
                    }
                    current_watch_dirs = new_dirs;
                }

                // Process standard indexing operations for non-config files
                for path in paths_to_process {
                    let p_canon = path.canonicalize().unwrap_or_else(|_| path.clone());
                    if p_canon != config_canon {
                        process_file_change(&path, &ctx).await;
                    }
                }
            } else {
                break; // channel closed
            }
        }
    });
}

async fn process_file_change(path: &Path, ctx: &McpContext) {
    let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("");
    let base_supported = matches!(ext, "md" | "rs" | "go" | "py" | "js" | "ts");
    let config_guard = ctx.config.read().await;
    let is_supported = match &config_guard.file_extensions {
        Some(exts) => base_supported && exts.iter().any(|e| e == ext),
        None => base_supported,
    };
    drop(config_guard);

    if !is_supported {
        return;
    }

    let db_path = normalize_system_path(path);

    if !path.exists() {
        // File was removed
        info!("File removed, deleting from index: {}", db_path);
        let db = ctx.db.lock().await;
        let _ = db.delete_document(&db_path);
        return;
    }

    // File was created or modified -> Reindex
    info!("File changed, reindexing: {}", db_path);

    let embedder = ctx.get_embedder().await;
    let indexer = Indexer::new(
        ctx.db.clone(),
        embedder.as_ref(),
        ctx.chunk_size,
        Arc::new(ctx.config.read().await.clone()),
    );

    match indexer.index_file(path).await {
        Ok(true) => info!("Successfully reindexed: {}", db_path),
        Ok(false) => { /* Skipped due to unsupported ext, already checked though */ }
        Err(e) => error!("Failed to reindex {}: {}", db_path, e),
    }
}
