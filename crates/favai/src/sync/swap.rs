use std::path::Path;

use crate::error::FavaiError;

/// Atomically rotate: live → old, staging → live, then remove old.
pub fn atomic_swap(live_dir: &Path, staging_dir: &Path) -> Result<(), FavaiError> {
    let old_dir = live_dir.with_extension("old");

    if live_dir.exists() {
        std::fs::rename(live_dir, &old_dir)
            .map_err(|e| FavaiError::SwapFailed(e.to_string()))?;
    }
    std::fs::rename(staging_dir, live_dir)
        .map_err(|e| FavaiError::SwapFailed(e.to_string()))?;
    if old_dir.exists() {
        std::fs::remove_dir_all(&old_dir)
            .map_err(|e| FavaiError::SwapFailed(e.to_string()))?;
    }
    Ok(())
}
