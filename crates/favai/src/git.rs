use std::path::Path;

use crate::error::FavaiError;

/// All git invocations go through here.
/// Rules enforced: Command::new("git") with arg arrays only, no sh -c,
/// working dir is canonicalized, -- before positional args where needed.
pub async fn run(cwd: &Path, args: &[&str]) -> Result<(), FavaiError> {
    let cwd = cwd.canonicalize()
        .map_err(|e| FavaiError::GitFailed(format!("canonicalize {}: {e}", cwd.display())))?;

    let out = tokio::process::Command::new("git")
        .args(args)
        .current_dir(&cwd)
        .env_clear()
        .envs(safe_env())
        .output()
        .await
        .map_err(|e| FavaiError::GitFailed(e.to_string()))?;

    if out.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&out.stderr);
        Err(FavaiError::GitFailed(stderr.into_owned()))
    }
}

/// Confirm git is on PATH at agent startup.
pub async fn check_available() -> Result<(), FavaiError> {
    let out = tokio::process::Command::new("git")
        .arg("--version")
        .output()
        .await
        .map_err(|_| FavaiError::GitNotFound)?;
    if out.status.success() { Ok(()) } else { Err(FavaiError::GitNotFound) }
}

/// Resolve HEAD sha inside `repo_dir`.
pub async fn head_sha(repo_dir: &Path) -> Result<String, FavaiError> {
    let out = tokio::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(repo_dir)
        .env_clear()
        .envs(safe_env())
        .output()
        .await
        .map_err(|e| FavaiError::GitFailed(e.to_string()))?;
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

// Only pass env vars git actually needs; drop everything else.
fn safe_env() -> impl IntoIterator<Item = (&'static str, String)> {
    let vars = ["HOME", "PATH", "SSH_AUTH_SOCK"];
    vars.into_iter()
        .filter_map(|k| std::env::var(k).ok().map(|v| (k, v)))
        .chain(
            std::env::vars()
                .filter(|(k, _)| k.starts_with("GIT_"))
                .map(|(k, v)| {
                    // SAFETY: we only forward keys that started with GIT_,
                    // which are defined at compile time by git itself.
                    let k: &'static str = Box::leak(k.into_boxed_str());
                    (k, v)
                }),
        )
}
