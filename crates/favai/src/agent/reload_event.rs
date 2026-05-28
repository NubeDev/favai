use chrono::{DateTime, Utc};
use starter_flow_spi::skill::SkillId;

/// Fires after every successful sync-driven reload.
///
/// Shape is frozen by `favai-sync-and-registry.md` so v2 can switch
/// to incremental `ToolRegistry` updates without a breaking change.
/// v1 consumers ignore `added`/`removed`/`changed_hash` and rebuild
/// the registry from `SkillRegistry::list()`.
#[derive(Debug, Clone)]
pub struct ReloadEvent {
    /// The source name that synced.
    pub source:       String,
    /// Bundles newly present after this sync.
    pub added:        Vec<SkillId>,
    /// Bundles deleted upstream by this sync.
    pub removed:      Vec<SkillId>,
    /// Bundles whose id is unchanged but `bundle_hash` is different.
    pub changed_hash: Vec<SkillId>,
    pub at:           DateTime<Utc>,
}
