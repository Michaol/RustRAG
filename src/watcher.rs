use crate::indexer::core::{Indexer, normalize_system_path};
use crate::mcp::server::McpContext;
use notify::{EventKind, RecursiveMode, Watcher};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::sync::mpsc;
use tracing::{error, info, warn};

/// Starts the file system watcher in the background.
pub fn start_watcher(ctx: McpContext) {
    let watch_dirs = ctx.config.get_base_directories();
    if watch_dirs.is_empty() {
        return;
    }

    let (tx, mut rx) = mpsc::channel::<PathBuf>(1000);

    let watcher_res = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
        if let Ok(event) = res {
            match event.kind {
                EventKind::Modify(notify::event::ModifyKind::Data(_))
                | EventKind::Create(_)
                | EventKind::Remove(_) => {
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

    for dir in watch_dirs {
        if dir.exists() {
            if let Err(e) = watcher.watch(&dir, RecursiveMode::Recursive) {
                warn!("Failed to watch directory {}: {}", dir.display(), e);
            } else {
                info!("Watching directory for changes: {}", dir.display());
            }
        }
    }

    // Spawn tokio task to process events and keep watcher alive
    tokio::spawn(async move {
        // Tie watcher lifetime to this long-running async task
        let _keep_watcher_alive = watcher;
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

                // Process them sequentially to avoid locking too heavily
                for path in paths_to_process {
                    process_file_change(&path, &ctx).await;
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
    let is_supported = match &ctx.config.file_extensions {
        Some(exts) => base_supported && exts.iter().any(|e| e == ext),
        None => base_supported,
    };

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
        ctx.config.clone(),
    );

    match indexer.index_file(path).await {
        Ok(true) => info!("Successfully reindexed: {}", db_path),
        Ok(false) => { /* Skipped due to unsupported ext, already checked though */ }
        Err(e) => error!("Failed to reindex {}: {}", db_path, e),
    }
}
