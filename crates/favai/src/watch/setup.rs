use std::path::Path;

use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use tokio::sync::mpsc;

use crate::error::FavaiError;

/// Create a watcher rooted at `skills_path` and `favai-pack.toml` only —
/// not the source root, so .git/ churn during fetch produces no events.
pub fn create_watcher(
    source_root: &Path,
    skills_path: &str,
    tx: mpsc::Sender<notify::Event>,
) -> Result<RecommendedWatcher, FavaiError> {
    let tx2 = tx.clone();
    let mut watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
        if let Ok(event) = res {
            let _ = tx2.blocking_send(event);
        }
    })
    .map_err(|e| FavaiError::WatcherSetup(e.to_string()))?;

    watcher
        .watch(&source_root.join(skills_path), RecursiveMode::Recursive)
        .map_err(|e| FavaiError::WatcherSetup(e.to_string()))?;
    watcher
        .watch(&source_root.join("favai-pack.toml"), RecursiveMode::NonRecursive)
        .map_err(|e| FavaiError::WatcherSetup(e.to_string()))?;

    Ok(watcher)
}
