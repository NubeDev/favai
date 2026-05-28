use std::path::PathBuf;

use crate::config::FavaiConfig;
use crate::error::FavaiError;

/// Returns the canonical root where all source checkouts live.
pub fn sources_root() -> Result<PathBuf, FavaiError> {
    let home = std::env::var("HOME")
        .map_err(|_| FavaiError::ConfigRead("HOME not set".into()))?;
    Ok(PathBuf::from(home)
        .join(".config")
        .join("starter")
        .join("favai")
        .join("sources"))
}

/// Wire every source in `config` as a quarantined load-dir on the builder.
/// Returns the builder so callers can chain further `.load_dir_*` calls.
///
/// All paths are validated to live under `sources_root()` before being
/// passed through. A misconfigured rename or symlink that escapes the
/// root is rejected here.
pub fn apply_to_builder(config: &FavaiConfig) -> Result<Vec<PathBuf>, FavaiError> {
    let root = sources_root()?.canonicalize()
        .unwrap_or_else(|_| sources_root().unwrap());

    config
        .sources
        .iter()
        .map(|s| {
            let skills_dir = root.join(&s.name).join(&s.skills_path);
            let canonical = skills_dir.canonicalize()
                .map_err(|e| FavaiError::ConfigRead(e.to_string()))?;
            if !canonical.starts_with(&root) {
                return Err(FavaiError::PathEscape(canonical));
            }
            Ok(canonical)
        })
        .collect()
}
