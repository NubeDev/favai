use std::path::Path;

use crate::error::FavaiError;

/// Crash-recovery sweep applied at agent startup, before any sync.
///
/// Implements the rules from `favai-sync-and-registry.md` §"Agent
/// flow" step 2.iv:
///
/// | live | .old | .staging | action                                       |
/// |------|------|----------|----------------------------------------------|
/// |  ✓   |      |          | normal — nothing to do                       |
/// |      |      |    ✓     | finish swap: rename staging → live           |
/// |  ✓   |  ✓   |          | post-rename crash: remove leftover `.old`    |
/// |  ✓   |      |    ✓     | mid-sync crash: discard stale staging        |
pub fn sweep_source(sources_root: &Path, source_name: &str) -> Result<(), FavaiError> {
    let live    = sources_root.join(source_name);
    let old     = sources_root.join(format!("{source_name}.old"));
    let staging = sources_root.join(format!("{source_name}.staging"));

    // Case: live missing + staging present → finish the swap.
    if !live.exists() && staging.exists() {
        std::fs::rename(&staging, &live)
            .map_err(|e| FavaiError::SwapFailed(format!("recover staging→live: {e}")))?;
    }

    // Case: leftover `.old` after a successful swap.
    if old.exists() {
        std::fs::remove_dir_all(&old)
            .map_err(|e| FavaiError::SwapFailed(format!("remove leftover .old: {e}")))?;
    }

    // Case: live present and staging also present → staging is from
    // a mid-sync crash, drop it. The fresh-clone path will recreate
    // it on next sync.
    if live.exists() && staging.exists() {
        std::fs::remove_dir_all(&staging)
            .map_err(|e| FavaiError::SwapFailed(format!("remove stale staging: {e}")))?;
    }

    Ok(())
}
