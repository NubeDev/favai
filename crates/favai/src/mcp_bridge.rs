//! End-to-end wiring of `starter-skills` → `starter-mcp` for the
//! favai consumer binary.
//!
//! The adapter that turns an `Arc<Skill>` into an `Arc<dyn Tool>`
//! lives in `starter-mcp` behind `feature = "skills"`. This module
//! is purely glue: it builds a `SkillRegistry` from the configured
//! source paths, folds every approved skill into a `ToolRegistry`
//! via `register_approved_skills`, and (optionally) registers the
//! `add_favorite` meta-tool against a configured user-skills dir.
//!
//! There is no adapter code here. Anything that looks like adapter
//! code is a bug — file it against `starter-mcp::skills_bridge`.

use std::path::PathBuf;
use std::sync::Arc;

use starter_mcp::skills_bridge::{
    register_approved_skills, register_approved_skills_as_prompts, AddFavoriteTool,
};
use starter_mcp::ToolRegistry;
use starter_skills::{ApprovalStore, InMemoryApprovalStore, SkillRegistry};

use crate::config::FavaiConfig;
use crate::error::FavaiError;

/// Configuration for [`build_tool_registry`].
///
/// Per `favai-sync-and-registry.md` §"Trust model", every load
/// directory is treated as quarantined-on-load. There is no
/// non-quarantined path: frontmatter `trust: approved` is ignored,
/// approval is per-bundle, per-hash, per-machine.
#[derive(Debug, Clone, Default)]
pub struct McpBridgeConfig {
    /// Synced + user-owned skill directories. All loaded via
    /// `load_dir_quarantined(...)`: every bundle starts quarantined
    /// until the operator records an approval row.
    pub quarantined_dirs: Vec<PathBuf>,
    /// If `Some`, register the `starter.add_favorite` meta-tool
    /// against this directory. New SKILL.md bundles written by the
    /// LLM land here and remain quarantined until approved.
    pub add_favorite_dir: Option<PathBuf>,
}

impl McpBridgeConfig {
    /// Build an [`McpBridgeConfig`] from a parsed [`FavaiConfig`].
    ///
    /// Every `[[source]]` block becomes one entry in
    /// [`quarantined_dirs`], resolved as
    /// `<sources_root>/<name>/<skills_path>`. `add_favorite_dir`
    /// defaults to `$HOME/.config/starter/favai/user-skills` —
    /// outside `sources/` so syncs cannot clobber it.
    pub fn from_favai_config(config: &FavaiConfig) -> Result<Self, FavaiError> {
        let root = crate::builder::sources_root()?;
        let mut quarantined_dirs = Vec::with_capacity(config.sources.len());
        for source in &config.sources {
            let dir = root.join(&source.name).join(&source.skills_path);
            if !dir.starts_with(&root) {
                return Err(FavaiError::PathEscape(dir));
            }
            quarantined_dirs.push(dir);
        }

        let home = std::env::var("HOME")
            .map_err(|_| FavaiError::ConfigRead("HOME not set".into()))?;
        let add_favorite_dir = PathBuf::from(home)
            .join(".config")
            .join("starter")
            .join("favai")
            .join("user-skills");

        Ok(Self {
            quarantined_dirs,
            add_favorite_dir: Some(add_favorite_dir),
        })
    }
}

/// Build a `(SkillRegistry, ToolRegistry)` pair ready to hand to
/// the starter-mcp server. The returned `SkillRegistry` is also
/// returned so the caller can drive `approve`/`revoke`/`reload`
/// from a separate operator UI.
pub async fn build_tool_registry(
    config: &McpBridgeConfig,
    approvals: Arc<dyn ApprovalStore>,
) -> Result<(SkillRegistry, ToolRegistry), FavaiError> {
    let mut builder = SkillRegistry::builder().with_approval_store_arc(approvals);
    for dir in &config.quarantined_dirs {
        // SkillRegistry::build walks each load_dir with fs::read_dir;
        // a missing directory is a hard error, not an empty walk.
        // First-run sources (no sync has happened yet) need the dir
        // pre-created so the registry comes up empty rather than
        // failing the boot.
        std::fs::create_dir_all(dir)
            .map_err(|e| FavaiError::ConfigRead(format!("create load dir {}: {e}", dir.display())))?;
        builder = builder.load_dir_quarantined(dir.clone());
    }
    if let Some(dir) = &config.add_favorite_dir {
        std::fs::create_dir_all(dir)
            .map_err(|e| FavaiError::ConfigRead(format!("create add_favorite dir {}: {e}", dir.display())))?;
        // The add_favorite write target is itself a quarantined
        // load-dir, so freshly-minted favourites round-trip through
        // the same approval gate as everything else.
        builder = builder.load_dir_quarantined(dir.clone());
    }
    let skills = builder
        .build()
        .await
        .map_err(|e| FavaiError::ConfigRead(format!("skill registry build: {e}")))?;

    let registry = build_tool_registry_from_skills(&skills, config.add_favorite_dir.clone());
    Ok((skills, registry))
}

/// Build a fresh [`ToolRegistry`] off an already-loaded
/// [`SkillRegistry`]. Called once at startup and again after every
/// `SkillRegistry::reload()` driven by a sync, so revoked or newly
/// quarantined bundles drop out of `tools/list` **and**
/// `prompts/list` without a server restart.
///
/// Each approved skill is registered twice: once as an MCP tool
/// (model-driven invocation via `tools/call`) and once as an MCP
/// prompt (user-driven invocation via the host's slash-command
/// surface — Claude Code only maps prompts, not tools, to
/// `/mcp__<server>__<name>`).
pub fn build_tool_registry_from_skills(
    skills: &SkillRegistry,
    add_favorite_dir: Option<PathBuf>,
) -> ToolRegistry {
    let mut registry = register_approved_skills(ToolRegistry::new(), skills);
    registry = register_approved_skills_as_prompts(registry, skills);
    if let Some(dir) = add_favorite_dir {
        registry = registry.register(AddFavoriteTool::new(dir));
    }
    registry
}

/// Convenience for callers that don't need a persistent approval
/// store yet — wires an [`InMemoryApprovalStore`] and forwards to
/// [`build_tool_registry`].
pub async fn build_tool_registry_in_memory(
    config: &McpBridgeConfig,
) -> Result<(SkillRegistry, ToolRegistry), FavaiError> {
    let store: Arc<dyn ApprovalStore> = Arc::new(InMemoryApprovalStore::new());
    build_tool_registry(config, store).await
}
