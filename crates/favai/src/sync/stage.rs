use std::path::Path;

use crate::error::FavaiError;

/// Confirm staging dir has a parseable favai-pack.toml and skills_path exists.
pub fn validate_staging(staging_dir: &Path, skills_path: &str) -> Result<(), FavaiError> {
    let pack = staging_dir.join("favai-pack.toml");
    if !pack.exists() {
        return Err(FavaiError::MissingPackManifest(staging_dir.to_path_buf()));
    }
    let raw = std::fs::read_to_string(&pack)
        .map_err(|e| FavaiError::ConfigRead(e.to_string()))?;
    toml::from_str::<toml::Value>(&raw)
        .map_err(|e| FavaiError::ConfigParse(e.to_string()))?;

    let skills_dir = staging_dir.join(skills_path);
    if !skills_dir.is_dir() {
        return Err(FavaiError::MissingSkillsPath(skills_dir));
    }
    Ok(())
}
