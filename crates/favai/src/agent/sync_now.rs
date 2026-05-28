use std::sync::Arc;
use std::time::Instant;

use chrono::Utc;
use tokio::sync::{broadcast, Mutex};

use crate::config::FavaiConfig;
use crate::error::FavaiError;
use crate::git;
use crate::sync::{clone_source, fetch_source, validate_staging, atomic_swap, SyncReport};
use super::reload_event::{ReloadEvent, ReloadTrigger};

pub async fn run_sync(
    config: &FavaiConfig,
    source_name: &str,
    sync_mutex: &Arc<Mutex<()>>,
    reload_tx: &broadcast::Sender<ReloadEvent>,
) -> Result<SyncReport, FavaiError> {
    let source = config
        .sources
        .iter()
        .find(|s| s.name == source_name)
        .ok_or_else(|| FavaiError::UnknownSource(source_name.to_string()))?;

    let sources_root = crate::builder::sources_root()?;
    let live_dir     = sources_root.join(&source.name);
    let staging_dir  = sources_root.join(format!("{}.staging", source.name));

    let started = Instant::now();
    let _guard  = sync_mutex.lock().await;

    if live_dir.exists() {
        fetch_source(&staging_dir, &source.branch).await?;
    } else {
        clone_source(&source.url, &source.branch, &staging_dir).await?;
    }

    validate_staging(&staging_dir, &source.skills_path)?;
    atomic_swap(&live_dir, &staging_dir)?;

    let head_sha = git::head_sha(&live_dir).await.unwrap_or_default();

    let _ = reload_tx.send(ReloadEvent {
        trigger: ReloadTrigger::SyncCompleted,
        sources: vec![source.name.clone()],
        at:      Utc::now(),
    });

    Ok(SyncReport {
        source_name:      source.name.clone(),
        new_head_sha:     head_sha,
        files_changed:    0,
        bytes_pulled:     0,
        duration_ms:      started.elapsed().as_millis() as u64,
        reload_triggered: true,
        at:               Utc::now(),
    })
}
