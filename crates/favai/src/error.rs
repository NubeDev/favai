use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum FavaiError {
    #[error("config read error: {0}")]
    ConfigRead(String),

    #[error("config parse error: {0}")]
    ConfigParse(String),

    #[error("invalid source name slug: '{0}'")]
    InvalidSlug(String),

    #[error("invalid URL scheme (only https:// and git@… allowed): '{0}'")]
    InvalidUrlScheme(String),

    #[error("git not found on PATH — install git and retry")]
    GitNotFound,

    #[error("git command failed: {0}")]
    GitFailed(String),

    #[error("missing favai-pack.toml in staging dir: {0}")]
    MissingPackManifest(PathBuf),

    #[error("skills_path does not exist: {0}")]
    MissingSkillsPath(PathBuf),

    #[error("atomic swap failed: {0}")]
    SwapFailed(String),

    #[error("watcher setup failed: {0}")]
    WatcherSetup(String),

    #[error("unknown source: '{0}'")]
    UnknownSource(String),

    #[error("path escapes sources root: {0}")]
    PathEscape(PathBuf),
}
