use std::sync::Arc;

use tokio::sync::{broadcast, Mutex};

use crate::config::FavaiConfig;
use crate::error::FavaiError;
use crate::sync::sweep_source;
use super::reload_event::ReloadEvent;
use super::sources::SourceStatus;
use super::sync_now::run_sync;
use crate::sync::SyncReport;

/// Agent that owns per-source sync state and the reload broadcaster.
///
/// v1 has no periodic sync loop and no filesystem watcher (see
/// `favai-sync-and-registry.md` §"Agent flow" step 3 — "No
/// filesystem watcher"). Reloads are driven by explicit
/// [`FavaiAgent::sync_now`] calls. Crash-recovery for half-completed
/// swaps runs once at startup.
pub struct FavaiAgent {
    pub(crate) config:     FavaiConfig,
    pub(crate) reload_tx:  broadcast::Sender<ReloadEvent>,
    pub(crate) sync_mutex: Arc<Mutex<()>>,
}

impl FavaiAgent {
    pub async fn start(config: FavaiConfig) -> Result<Self, FavaiError> {
        crate::git::check_available().await?;

        // Crash-recovery sweep across every configured source before
        // we accept the first sync_now call. See
        // `favai-sync-and-registry.md` §"Agent flow" step 2.iv.
        let sources_root = crate::builder::sources_root()?;
        std::fs::create_dir_all(&sources_root)
            .map_err(|e| FavaiError::ConfigRead(format!("create sources root: {e}")))?;
        for source in &config.sources {
            sweep_source(&sources_root, &source.name)?;
        }

        let (reload_tx, _) = broadcast::channel(16);
        let sync_mutex = Arc::new(Mutex::new(()));

        Ok(Self {
            config,
            reload_tx,
            sync_mutex,
        })
    }

    pub async fn sync_now(&self, source_name: &str) -> Result<SyncReport, FavaiError> {
        run_sync(&self.config, source_name, &self.sync_mutex, &self.reload_tx).await
    }

    pub fn sources(&self) -> Vec<SourceStatus> {
        self.config
            .sources
            .iter()
            .map(|s| SourceStatus {
                name:          s.name.clone(),
                url:           s.url.clone(),
                branch:        s.branch.clone(),
                last_fetch_at: None,
                head_sha:      None,
                skill_count:   0,
            })
            .collect()
    }

    pub fn subscribe_reloads(&self) -> broadcast::Receiver<ReloadEvent> {
        self.reload_tx.subscribe()
    }

    /// Stop the agent. v1 holds no background tasks, so this is a
    /// no-op kept for API symmetry with future periodic-sync work.
    pub async fn shutdown(self) {}
}
