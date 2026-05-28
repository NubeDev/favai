use std::path::PathBuf;
use std::sync::Arc;

use starter_mcp::ToolRegistry;
use starter_skills::SkillRegistry;
use tokio::sync::{broadcast, Mutex};

use crate::config::FavaiConfig;
use crate::error::FavaiError;
use crate::mcp_bridge::build_tool_registry_from_skills;
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
///
/// The agent owns the [`SkillRegistry`] so `sync_now` and reload
/// events can drive [`SkillRegistry::reload`]. Each
/// [`Self::tool_registry`] call builds a fresh `ToolRegistry` off
/// the current `SkillRegistry::list()` snapshot — tools that have
/// been revoked or re-quarantined since the last build will not
/// appear, and newly-approved bundles will.
pub struct FavaiAgent {
    pub(crate) config:           FavaiConfig,
    pub(crate) reload_tx:        broadcast::Sender<ReloadEvent>,
    pub(crate) sync_mutex:       Arc<Mutex<()>>,
    pub(crate) skills:           Arc<SkillRegistry>,
    pub(crate) add_favorite_dir: Option<PathBuf>,
}

impl FavaiAgent {
    /// Boot the agent. Per
    /// `favai-sync-and-registry.md` §"Public surface":
    ///
    /// ```ignore
    /// pub async fn start(
    ///     config: FavaiConfig,
    ///     registry: Arc<SkillRegistry>,
    /// ) -> Result<Self, FavaiError>;
    /// ```
    ///
    /// `add_favorite_dir`, if `Some`, is the user-skills directory
    /// the [`build_tool_registry`](crate::mcp_bridge::build_tool_registry)
    /// helper registers the `starter.add_favorite` meta-tool
    /// against. Pass `None` to disable it.
    pub async fn start(
        config: FavaiConfig,
        skills: Arc<SkillRegistry>,
        add_favorite_dir: Option<PathBuf>,
    ) -> Result<Self, FavaiError> {
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
            skills,
            add_favorite_dir,
        })
    }

    /// Trigger an out-of-band sync for `source_name`. On success
    /// the agent calls `SkillRegistry::reload()` once before
    /// broadcasting the [`ReloadEvent`], so any subscriber that
    /// rebuilds its `ToolRegistry` off [`Self::tool_registry`]
    /// sees the new bundle set.
    pub async fn sync_now(&self, source_name: &str) -> Result<SyncReport, FavaiError> {
        let report = run_sync(
            &self.config,
            source_name,
            &self.sync_mutex,
            &self.reload_tx,
            Some(self.skills.clone()),
        )
        .await?;
        Ok(report)
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

    /// The [`SkillRegistry`] the agent drives. Operator UIs call
    /// `approve` / `revoke` / `list_quarantined` here.
    pub fn skill_registry(&self) -> Arc<SkillRegistry> {
        Arc::clone(&self.skills)
    }

    /// Build a fresh [`ToolRegistry`] off the agent's current
    /// `SkillRegistry::list()` snapshot.
    ///
    /// `starter_mcp::run_stdio` consumes the `ToolRegistry` by
    /// value, so v1 hosts call this once between [`Self::start`]
    /// and `run_stdio` and accept that revoked tools persist in the
    /// running stdio session until restart. The `SkillTool` adapter
    /// re-checks `SkillRegistry::list()` membership at invoke time
    /// anyway, so a revoked tool will refuse to fire even before
    /// restart — it just doesn't disappear from `tools/list`.
    pub fn tool_registry(&self) -> ToolRegistry {
        build_tool_registry_from_skills(&self.skills, self.add_favorite_dir.clone())
    }

    /// Stop the agent. v1 holds no background tasks, so this is a
    /// no-op kept for API symmetry with future periodic-sync work.
    pub async fn shutdown(self) {}
}
