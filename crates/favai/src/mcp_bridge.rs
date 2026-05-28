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

use starter_mcp::skills_bridge::{register_approved_skills, AddFavoriteTool};
use starter_mcp::ToolRegistry;
use starter_skills::{ApprovalStore, InMemoryApprovalStore, SkillRegistry};

use crate::error::FavaiError;

/// Configuration for [`build_tool_registry`].
#[derive(Debug, Clone, Default)]
pub struct McpBridgeConfig {
    /// Repo-owned skill directories. Loaded via `load_dir(...)` —
    /// frontmatter `trust: approved` is honoured.
    pub repo_dirs: Vec<PathBuf>,
    /// User-owned skill directories. Loaded via
    /// `load_dir_quarantined(...)` — frontmatter is ignored, every
    /// bundle starts quarantined until the operator approves.
    pub user_dirs: Vec<PathBuf>,
    /// If `Some`, register the `starter.add_favorite` meta-tool
    /// against this directory. New SKILL.md bundles written by the
    /// LLM land here and remain quarantined until approved.
    pub add_favorite_dir: Option<PathBuf>,
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
    for dir in &config.repo_dirs {
        builder = builder.load_dir(dir.clone());
    }
    for dir in &config.user_dirs {
        builder = builder.load_dir_quarantined(dir.clone());
    }
    let skills = builder
        .build()
        .await
        .map_err(|e| FavaiError::ConfigRead(format!("skill registry build: {e}")))?;

    let mut registry = register_approved_skills(ToolRegistry::new(), &skills);
    if let Some(dir) = &config.add_favorite_dir {
        registry = registry.register(AddFavoriteTool::new(dir.clone()));
    }
    Ok((skills, registry))
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
