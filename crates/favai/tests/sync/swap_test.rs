use std::fs;
use tempfile::TempDir;

#[test]
fn swap_replaces_live_with_staging() {
    let root    = TempDir::new().unwrap();
    let live    = root.path().join("my-source");
    let staging = root.path().join("my-source.staging");

    fs::create_dir_all(&live).unwrap();
    fs::write(live.join("old.txt"), "old").unwrap();

    fs::create_dir_all(&staging).unwrap();
    fs::write(staging.join("new.txt"), "new").unwrap();

    favai::sync::atomic_swap(&live, &staging).unwrap();

    assert!(live.join("new.txt").exists());
    assert!(!live.join("old.txt").exists());
    assert!(!staging.exists());
}
