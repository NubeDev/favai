use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use starter_mcp::ToolRegistry;
use starter_skills::SkillRegistry;
use tokio::sync::{broadcast, Mutex};
use tokio::task::JoinHandle;

use crate::config::FavaiConfig;
use crate::error::FavaiError;
use crate::mcp_bridge::build_tool_registry_from_skills;
use crate::sync::sweep_source;
use super::periodic;
use super::reload_event::ReloadEvent;
use super::sources::SourceStatus;
use super::sync_now::run_sync;
use crate::sync::SyncReport;

/// Per-source progress the agent tracks across syncs. Persisted only
/// in memory — restart re-derives `head_sha` from the live dir's
/// `.git/HEAD` and leaves `last_fetch_at` as `None` until the next
/// sync.
#[derive(Debug, Clone, Default)]
pub(crate) struct SourceProgress {
    pub head_sha:      Option<String>,
    pub last_fetch_at: Option<DateTime<Utc>>,
}

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
    pub(crate) progress:         Arc<Mutex<HashMap<String, SourceProgress>>>,
    pub(crate) periodic_task:    Option<JoinHandle<()>>,
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

        // Seed per-source progress: for any source whose live dir
        // already exists, resolve its current HEAD so `sources()` and
        // `favai list` show useful data before the next sync.
        let mut progress: HashMap<String, SourceProgress> = HashMap::new();
        for source in &config.sources {
            let live = sources_root.join(&source.name);
            if live.join(".git").exists() {
                if let Ok(sha) = crate::git::head_sha(&live).await {
                    if !sha.is_empty() {
                        progress.insert(
                            source.name.clone(),
                            SourceProgress { head_sha: Some(sha), last_fetch_at: None },
                        );
                    }
                }
            }
        }

        let (reload_tx, _) = broadcast::channel(16);
        let sync_mutex = Arc::new(Mutex::new(()));

        // Opt-in periodic sync — disabled unless the config has a
        // [periodic] block (see favai-sync-and-registry.md §"Still
        // open"). Always-polling is the wrong default.
        let periodic_task = config.periodic.clone().map(|p| {
            periodic::spawn(
                p,
                config.clone(),
                Arc::clone(&sync_mutex),
                reload_tx.clone(),
                Arc::clone(&skills),
            )
        });

        Ok(Self {
            config,
            reload_tx,
            sync_mutex,
            skills,
            add_favorite_dir,
            progress: Arc::new(Mutex::new(progress)),
            periodic_task,
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

        // Record progress so a later `sources()` / `favai list` shows
        // the fresh head sha and fetch timestamp without re-shelling
        // out to git.
        let mut progress = self.progress.lock().await;
        progress.insert(
            source_name.to_owned(),
            SourceProgress {
                head_sha:      Some(report.new_head_sha.clone()),
                last_fetch_at: Some(report.at),
            },
        );
        Ok(report)
    }

    pub fn sources(&self) -> Vec<SourceStatus> {
        let progress = self.progress.try_lock().ok();
        let sources_root = crate::builder::sources_root().ok();
        self.config
            .sources
            .iter()
            .map(|s| {
                let snap = progress.as_ref().and_then(|m| m.get(&s.name).cloned()).unwrap_or_default();
                let skill_count = sources_root
                    .as_ref()
                    .map(|r| count_bundles(&r.join(&s.name).join(&s.skills_path)))
                    .unwrap_or(0);
                SourceStatus {
                    name:          s.name.clone(),
                    url:           s.url.clone(),
                    branch:        s.branch.clone(),
                    last_fetch_at: snap.last_fetch_at,
                    head_sha:      snap.head_sha,
                    skill_count,
                }
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

    /// Stop the agent. Aborts the periodic-sync task if one was
    /// spawned; otherwise no-op.
    pub async fn shutdown(self) {
        if let Some(h) = self.periodic_task {
            h.abort();
        }
    }
}

/// Count direct subdirectories of `dir` that contain a `SKILL.md`.
/// Matches `SkillRegistry::walk_load_dir`'s one-level scan rule.
fn count_bundles(dir: &std::path::Path) -> usize {
    let Ok(entries) = std::fs::read_dir(dir) else { return 0 };
    entries
        .flatten()
        .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
        .filter(|e| e.path().join("SKILL.md").is_file())
        .count()
}
