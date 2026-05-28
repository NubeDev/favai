use std::sync::Arc;

use tokio::sync::{broadcast, Mutex};
use tokio::task::JoinHandle;

use crate::config::FavaiConfig;
use crate::error::FavaiError;
use super::reload_event::ReloadEvent;
use super::sources::SourceStatus;
use super::sync_now::run_sync;
use crate::sync::SyncReport;

pub struct FavaiAgent {
    pub(crate) config:       FavaiConfig,
    pub(crate) reload_tx:    broadcast::Sender<ReloadEvent>,
    pub(crate) sync_mutex:   Arc<Mutex<()>>,
    pub(crate) _sync_task:   JoinHandle<()>,
    pub(crate) _watch_tasks: Vec<JoinHandle<()>>,
}

impl FavaiAgent {
    pub async fn start(config: FavaiConfig) -> Result<Self, FavaiError> {
        crate::git::check_available().await?;

        let (reload_tx, _) = broadcast::channel(16);
        let sync_mutex = Arc::new(Mutex::new(()));

        let sync_task = tokio::spawn(async {});
        let watch_tasks = vec![];

        Ok(Self {
            config,
            reload_tx,
            sync_mutex,
            _sync_task: sync_task,
            _watch_tasks: watch_tasks,
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
}
