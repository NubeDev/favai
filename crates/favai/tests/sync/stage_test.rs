use favai::error::FavaiError;
use std::fs;
use tempfile::TempDir;

fn make_staging(dir: &TempDir, with_pack: bool, with_skills: bool) {
    if with_pack {
        fs::write(dir.path().join("favai-pack.toml"), r#"
[pack]
name = "test/pack"
description = "test"
maintainer = "test@example.com"
license = "MIT"
version = "0.1.0"
[trust]
declared = "personal"
"#).unwrap();
    }
    if with_skills {
        fs::create_dir_all(dir.path().join("skills")).unwrap();
    }
}

#[test]
fn passes_when_valid() {
    let dir = TempDir::new().unwrap();
    make_staging(&dir, true, true);
    let result = favai::sync::validate_staging(dir.path(), "skills");
    assert!(result.is_ok());
}

#[test]
fn fails_missing_pack() {
    let dir = TempDir::new().unwrap();
    make_staging(&dir, false, true);
    assert!(matches!(
        favai::sync::validate_staging(dir.path(), "skills"),
        Err(FavaiError::MissingPackManifest(_))
    ));
}

#[test]
fn fails_missing_skills_path() {
    let dir = TempDir::new().unwrap();
    make_staging(&dir, true, false);
    assert!(matches!(
        favai::sync::validate_staging(dir.path(), "skills"),
        Err(FavaiError::MissingSkillsPath(_))
    ));
}
