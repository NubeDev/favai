use favai::FavaiConfig;
use std::io::Write;
use tempfile::NamedTempFile;

#[test]
fn rejects_invalid_slug() {
    let mut f = NamedTempFile::new().unwrap();
    writeln!(f, r#"
[[source]]
name        = "bad/slug"
url         = "https://github.com/example/x.git"
branch      = "main"
skills_path = "skills"
"#).unwrap();
    assert!(FavaiConfig::from_file(&f.path().to_path_buf()).is_err());
}

#[test]
fn rejects_file_url_scheme() {
    let mut f = NamedTempFile::new().unwrap();
    writeln!(f, r#"
[[source]]
name        = "local"
url         = "file:///home/user/repo"
branch      = "main"
skills_path = "skills"
"#).unwrap();
    assert!(FavaiConfig::from_file(&f.path().to_path_buf()).is_err());
}
