use std::path::Path;

use crate::error::FavaiError;
use crate::git;

/// Clone `url` at `branch` into `staging_dir`. Used for new sources only.
pub async fn clone_source(url: &str, branch: &str, staging_dir: &Path) -> Result<(), FavaiError> {
    git::run(
        staging_dir.parent().unwrap_or(staging_dir),
        &[
            "clone",
            "--branch", branch,
            "--single-branch",
            "--",
            url,
            &staging_dir.to_string_lossy(),
        ],
    )
    .await
}
