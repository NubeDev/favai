use std::path::Path;

use crate::error::FavaiError;
use crate::git;

/// Always-fresh shallow clone of `url` at `branch` into `staging_dir`.
///
/// Removes `staging_dir` first if a prior failed sync left it behind,
/// so the post-condition is "staging is exactly the upstream tip" —
/// no `git clean` needed. See `favai-sync-and-registry.md` §"Agent
/// flow" step 2.
pub async fn clone_source(url: &str, branch: &str, staging_dir: &Path) -> Result<(), FavaiError> {
    if staging_dir.exists() {
        std::fs::remove_dir_all(staging_dir)
            .map_err(|e| FavaiError::GitFailed(format!("remove stale staging: {e}")))?;
    }
    let parent = staging_dir.parent().ok_or_else(|| {
        FavaiError::GitFailed(format!("staging dir has no parent: {}", staging_dir.display()))
    })?;
    std::fs::create_dir_all(parent)
        .map_err(|e| FavaiError::GitFailed(format!("create sources root: {e}")))?;

    git::run(
        parent,
        &[
            "clone",
            "--depth=1",
            "--single-branch",
            "--branch", branch,
            "--",
            url,
            &staging_dir.to_string_lossy(),
        ],
    )
    .await
}
