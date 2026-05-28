use serde::Deserialize;
use std::path::PathBuf;

use crate::error::FavaiError;
use super::validate;

#[derive(Debug, Clone, Deserialize, Default)]
pub struct FavaiConfig {
    #[serde(rename = "source", default)]
    pub sources: Vec<Source>,

    /// Optional periodic sync schedule. Omitted in v1 dogfood; opt-in.
    #[serde(default)]
    pub periodic: Option<Periodic>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Source {
    pub name:        String,
    pub url:         String,
    pub branch:      String,
    pub skills_path: String,
}

/// Periodic sync schedule. Per `favai-sync-and-registry.md` §"Still
/// open" — always-polling is the wrong default; this is opt-in and
/// the interval is jittered ±10% so a fleet of PCs spreads out
/// rather than producing a sawtooth at GitHub.
#[derive(Debug, Clone, Deserialize)]
pub struct Periodic {
    /// Base interval between syncs, in seconds. Each scheduled tick
    /// is jittered uniformly in `[0.9 * interval, 1.1 * interval]`.
    /// Minimum honoured value is 60 — anything lower is clamped.
    pub interval_secs: u64,
}

impl FavaiConfig {
    pub fn from_file(path: &PathBuf) -> Result<Self, FavaiError> {
        let raw = std::fs::read_to_string(path)
            .map_err(|e| FavaiError::ConfigRead(e.to_string()))?;
        let cfg: FavaiConfig = toml::from_str(&raw)
            .map_err(|e| FavaiError::ConfigParse(e.to_string()))?;
        for source in &cfg.sources {
            validate::slug(&source.name)?;
            validate::url_scheme(&source.url)?;
        }
        Ok(cfg)
    }
}
