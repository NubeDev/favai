use favai::FavaiConfig;
use std::io::Write;
use tempfile::NamedTempFile;

#[test]
fn parses_valid_config() {
    let mut f = NamedTempFile::new().unwrap();
    writeln!(f, r#"
[[source]]
name        = "my-skills"
url         = "https://github.com/example/my-skills.git"
branch      = "main"
skills_path = "skills"
"#).unwrap();
    let cfg = FavaiConfig::from_file(&f.path().to_path_buf()).unwrap();
    assert_eq!(cfg.sources.len(), 1);
    assert_eq!(cfg.sources[0].name, "my-skills");
}
