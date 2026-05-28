use std::fs;
use tempfile::TempDir;

use favai::sync::sweep_source;

fn touch_dir(root: &std::path::Path, name: &str) {
    fs::create_dir_all(root.join(name)).unwrap();
    fs::write(root.join(name).join("marker"), name).unwrap();
}

#[test]
fn normal_state_is_noop() {
    let tmp = TempDir::new().unwrap();
    touch_dir(tmp.path(), "src");
    sweep_source(tmp.path(), "src").unwrap();
    assert!(tmp.path().join("src/marker").exists());
}

#[test]
fn finishes_swap_when_only_staging_present() {
    // Crash between `rename(live → .old)` and `rename(.staging → live)`.
    let tmp = TempDir::new().unwrap();
    touch_dir(tmp.path(), "src.staging");
    sweep_source(tmp.path(), "src").unwrap();
    assert!(tmp.path().join("src/marker").exists());
    assert!(!tmp.path().join("src.staging").exists());
}

#[test]
fn removes_leftover_old_after_swap() {
    // Crash between `rename(.staging → live)` and `remove(.old)`.
    let tmp = TempDir::new().unwrap();
    touch_dir(tmp.path(), "src");
    touch_dir(tmp.path(), "src.old");
    sweep_source(tmp.path(), "src").unwrap();
    assert!(tmp.path().join("src/marker").exists());
    assert!(!tmp.path().join("src.old").exists());
}

#[test]
fn discards_stale_staging_when_live_already_present() {
    // Crash before the first `rename(live → .old)`, or a sync that
    // never reached the swap. Drop staging; the next sync re-clones.
    let tmp = TempDir::new().unwrap();
    touch_dir(tmp.path(), "src");
    touch_dir(tmp.path(), "src.staging");
    sweep_source(tmp.path(), "src").unwrap();
    assert!(tmp.path().join("src/marker").exists());
    assert!(!tmp.path().join("src.staging").exists());
}
