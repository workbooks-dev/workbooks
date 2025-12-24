use anyhow::{Context, Result};
use notify::RecursiveMode;
use notify_debouncer_mini::new_debouncer;
use std::path::{Path, PathBuf};
use std::sync::mpsc::channel;
use std::time::Duration;
use tauri::{AppHandle, Emitter};

/// Directories and files to ignore in file watching
const IGNORED_PATHS: &[&str] = &[
    ".git",
    ".workbooks",
    "node_modules",
    ".venv",
    "__pycache__",
    ".ipynb_checkpoints",
    ".DS_Store",
    "target", // Rust build artifacts
    "dist",   // Build outputs
];

/// Check if a path should be ignored
fn should_ignore_path(path: &Path) -> bool {
    path.components().any(|component| {
        if let Some(s) = component.as_os_str().to_str() {
            IGNORED_PATHS.iter().any(|&ignored| s.contains(ignored))
        } else {
            false
        }
    })
}

/// Start watching a directory for file changes
/// Emits 'file-system-changed' events to the frontend
pub fn start_watching(app_handle: AppHandle, project_root: PathBuf) -> Result<()> {
    log::info!("Starting file watcher for: {}", project_root.display());

    // Spawn a new thread for file watching
    std::thread::spawn(move || {
        if let Err(e) = watch_directory(app_handle, project_root) {
            log::error!("File watcher error: {}", e);
        }
    });

    Ok(())
}

fn watch_directory(app_handle: AppHandle, project_root: PathBuf) -> Result<()> {
    let (tx, rx) = channel();

    // Create a debounced watcher (waits 500ms before firing events)
    // This prevents spamming events when multiple files change rapidly
    let mut debouncer = new_debouncer(Duration::from_millis(500), tx)
        .context("Failed to create file watcher")?;

    // Add the project root to watch
    debouncer
        .watcher()
        .watch(&project_root, RecursiveMode::Recursive)
        .context("Failed to start watching directory")?;

    log::info!("File watcher started successfully");

    // Process events
    for result in rx {
        match result {
            Ok(events) => {
                // Filter out ignored paths
                let relevant_events: Vec<_> = events
                    .iter()
                    .filter(|event| !should_ignore_path(&event.path))
                    .collect();

                if !relevant_events.is_empty() {
                    log::debug!("File system changed: {} relevant events", relevant_events.len());

                    // Emit event to frontend
                    if let Err(e) = app_handle.emit("file-system-changed", ()) {
                        log::error!("Failed to emit file-system-changed event: {}", e);
                    }
                }
            }
            Err(e) => {
                log::error!("Watch error: {:?}", e);
            }
        }
    }

    Ok(())
}
