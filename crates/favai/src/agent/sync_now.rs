use std::sync::Arc;
use std::time::Instant;

use chrono::Utc;
use starter_skills::SkillRegistry;
use tokio::sync::{broadcast, Mutex};

use crate::config::FavaiConfig;
use crate::error::FavaiError;
use crate::git;
use crate::sync::{clone_source, validate_staging, atomic_swap, SyncReport};
use super::reload_event::ReloadEvent;

pub async fn run_sync(
    config: &FavaiConfig,
    source_name: &str,
    sync_mutex: &Arc<Mutex<()>>,
    reload_tx: &broadcast::Sender<ReloadEvent>,
    skills: Option<Arc<SkillRegistry>>,
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

    // Always-fresh shallow clone (doc §"Agent flow" step 2).
    clone_source(&source.url, &source.branch, &staging_dir).await?;
    validate_staging(&staging_dir, &source.skills_path)?;
    atomic_swap(&live_dir, &staging_dir)?;

    let head_sha = git::head_sha(&live_dir).await.unwrap_or_default();

    // Reload the skill registry so the swapped-in bundles are
    // visible to the next tool_registry() build. SkillRegistry
    // re-walks every load_dir_quarantined source; quarantine drift
    // for changed bundle hashes happens here, not in the bridge.
    let mut reload_triggered = false;
    if let Some(skills) = skills.as_ref() {
        skills
            .reload()
            .await
            .map_err(|e| FavaiError::ConfigRead(format!("skill registry reload: {e}")))?;
        reload_triggered = true;
    }

    // v1 ReloadEvent: source name + timestamp. The diff fields stay
    // empty until v2 wires incremental ToolRegistry updates; the
    // surface is frozen so v2 is non-breaking.
    let _ = reload_tx.send(ReloadEvent {
        source:       source.name.clone(),
        added:        Vec::new(),
        removed:      Vec::new(),
        changed_hash: Vec::new(),
        at:           Utc::now(),
    });

    Ok(SyncReport {
        source_name:      source.name.clone(),
        new_head_sha:     head_sha,
        files_changed:    0,
        bytes_pulled:     0,
        duration_ms:      started.elapsed().as_millis() as u64,
        reload_triggered,
        at:               Utc::now(),
    })
}
