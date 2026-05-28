use serde::Deserialize;
use std::path::PathBuf;

use crate::error::FavaiError;
use super::validate;

#[derive(Debug, Clone, Deserialize)]
pub struct FavaiConfig {
    #[serde(rename = "source", default)]
    pub sources: Vec<Source>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Source {
    pub name:        String,
    pub url:         String,
    pub branch:      String,
    pub skills_path: String,
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
