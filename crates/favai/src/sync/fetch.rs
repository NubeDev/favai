use std::path::Path;

use crate::error::FavaiError;
use crate::git;

/// Fetch from origin and hard-reset to `origin/<branch>` inside `staging_dir`.
pub async fn fetch_source(staging_dir: &Path, branch: &str) -> Result<(), FavaiError> {
    git::run(staging_dir, &["fetch", "--", "origin"]).await?;
    git::run(
        staging_dir,
        &["reset", "--hard", &format!("origin/{branch}")],
    )
    .await?;
    git::run(staging_dir, &["clean", "-ffdx"]).await
}
