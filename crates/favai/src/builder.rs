use std::path::PathBuf;

use starter_skills::SkillRegistryBuilder;

use crate::config::FavaiConfig;
use crate::error::FavaiError;

/// Returns the canonical root where all source checkouts live.
///
/// Defaults to `$HOME/.config/starter/favai/sources`. Override via
/// the `FAVAI_SOURCES_ROOT` environment variable — useful for
/// integration tests, sandboxed CI, and operators who keep their
/// favai cache outside `$HOME`.
pub fn sources_root() -> Result<PathBuf, FavaiError> {
    if let Ok(p) = std::env::var("FAVAI_SOURCES_ROOT") {
        if !p.is_empty() {
            return Ok(PathBuf::from(p));
        }
    }
    let home = std::env::var("HOME")
        .map_err(|_| FavaiError::ConfigRead("HOME not set".into()))?;
    Ok(PathBuf::from(home)
        .join(".config")
        .join("starter")
        .join("favai")
        .join("sources"))
}

/// Wire every source in `config` as a *quarantined* load-dir on the
/// builder.
///
/// Per `favai-sync-and-registry.md` §"Trust model" — every synced
/// source goes through `load_dir_quarantined(...)`. There is no
/// `load_dir(...)` path: frontmatter `trust: approved` is ignored
/// for synced bundles, and approval is per-bundle, per-hash, per
/// machine.
///
/// Each `<sources_root>/<name>/<skills_path>` directory is created
/// if missing — `SkillRegistry::build()` does **not** tolerate a
/// missing load-dir, and on first run no sync has happened yet.
pub fn apply_to_builder(
    config: &FavaiConfig,
    mut builder: SkillRegistryBuilder,
) -> Result<SkillRegistryBuilder, FavaiError> {
    let root = sources_root()?;
    for source in &config.sources {
        let skills_dir = root.join(&source.name).join(&source.skills_path);
        // Defence in depth against a malformed slug that escapes
        // the sources root. Config-parse validation should already
        // have caught it, but cost is one starts_with check.
        if !skills_dir.starts_with(&root) {
            return Err(FavaiError::PathEscape(skills_dir));
        }
        std::fs::create_dir_all(&skills_dir).map_err(|e| {
            FavaiError::ConfigRead(format!("create load dir {}: {e}", skills_dir.display()))
        })?;
        builder = builder.load_dir_quarantined(skills_dir);
    }
    Ok(builder)
}
