//! End-to-end sync against a local bare git repo.
//!
//! Drives the real `FavaiAgent::sync_now` codepath — shells out to
//! `git clone`, validates the staging dir, performs the two-rename
//! swap, calls `SkillRegistry::reload`, and broadcasts a
//! `ReloadEvent`. Uses `file://` URLs so no network is required.
//!
//! The test bypasses `FavaiConfig::from_file`'s URL-scheme validation
//! by constructing `FavaiConfig` directly — the validator is a
//! parse-time guard, and `Source` fields are intentionally public so
//! programmatic configs (tests, embedding consumers) can opt out.
//!
//! `FAVAI_SOURCES_ROOT` is set to a tempdir so the test does not
//! touch `$HOME/.config/starter/favai/`.

use std::path::Path;
use std::process::Command;
use std::sync::Arc;

use favai::config::{FavaiConfig, Source};
use favai::mcp_bridge::{build_tool_registry, McpBridgeConfig};
use favai::FavaiAgent;
use starter_skills::InMemoryApprovalStore;
use tempfile::TempDir;

fn run_git(cwd: &Path, args: &[&str]) {
    let out = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .env("GIT_AUTHOR_NAME", "favai-test")
        .env("GIT_AUTHOR_EMAIL", "test@example.invalid")
        .env("GIT_COMMITTER_NAME", "favai-test")
        .env("GIT_COMMITTER_EMAIL", "test@example.invalid")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .output()
        .expect("git invocation");
    assert!(
        out.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

fn make_upstream(work: &Path, bare: &Path) {
    std::fs::create_dir_all(work).unwrap();
    std::fs::create_dir_all(bare).unwrap();

    std::fs::write(
        work.join("favai-pack.toml"),
        "[pack]\nname=\"test/pack\"\ndescription=\"t\"\n\
         maintainer=\"t@t\"\nlicense=\"MIT\"\nversion=\"0.1.0\"\n",
    )
    .unwrap();
    let bundle = work.join("skills").join("hello");
    std::fs::create_dir_all(&bundle).unwrap();
    std::fs::write(
        bundle.join("SKILL.md"),
        "---\nid: favai.test.hello\ndescription: Test fixture.\n\
         trust: quarantined\n---\nHello, world.\n",
    )
    .unwrap();

    run_git(work, &["init", "-q", "-b", "main"]);
    run_git(work, &["add", "."]);
    run_git(work, &["commit", "-q", "-m", "init"]);
    run_git(bare, &["init", "-q", "--bare"]);
    run_git(work, &["remote", "add", "origin", &bare.to_string_lossy()]);
    run_git(work, &["push", "-q", "origin", "main"]);
}

#[tokio::test]
async fn sync_now_clones_and_publishes_skill() {
    // Probe git; skip silently if the host has no git on PATH (CI
    // sandboxes occasionally do).
    if Command::new("git").arg("--version").output().is_err() {
        eprintln!("git not available — skipping");
        return;
    }

    let tmp = TempDir::new().unwrap();
    let upstream_work = tmp.path().join("upstream-work");
    let upstream_bare = tmp.path().join("upstream.git");
    let sources_root  = tmp.path().join("sources");
    make_upstream(&upstream_work, &upstream_bare);

    // Steer favai's cache at the tempdir instead of $HOME.
    std::env::set_var("FAVAI_SOURCES_ROOT", &sources_root);

    let config = FavaiConfig {
        sources: vec![Source {
            name:        "upstream".into(),
            url:         format!("file://{}", upstream_bare.display()),
            branch:      "main".into(),
            skills_path: "skills".into(),
        }],
    };

    let bundle_dir = sources_root.join("upstream").join("skills");
    let bridge_config = McpBridgeConfig {
        quarantined_dirs: vec![bundle_dir.clone()],
        add_favorite_dir: None,
    };
    let approvals = Arc::new(InMemoryApprovalStore::new());
    let (skills, _initial_tools) = build_tool_registry(&bridge_config, approvals)
        .await
        .expect("build_tool_registry");
    let skills = Arc::new(skills);

    let agent = FavaiAgent::start(config, Arc::clone(&skills), None)
        .await
        .expect("agent start");

    let report = agent.sync_now("upstream").await.expect("sync_now");
    assert_eq!(report.source_name, "upstream");
    assert!(report.reload_triggered, "reload must have fired");
    assert!(!report.new_head_sha.is_empty(), "head sha must resolve");

    // Live dir is in place, staging is gone, .old is cleaned up.
    let live = sources_root.join("upstream");
    assert!(live.join("favai-pack.toml").exists(), "live dir populated");
    assert!(!sources_root.join("upstream.staging").exists());
    assert!(!sources_root.join("upstream.old").exists());

    // The skill landed quarantined — sync alone does not approve.
    let quar = skills.list_quarantined();
    assert!(
        quar.iter().any(|s| s.id.as_str() == "favai.test.hello"),
        "synced bundle must be quarantined until operator approves"
    );
    assert!(
        skills.list().is_empty(),
        "no skill is approved before approve() is called"
    );

    // Per-source progress is recorded after sync.
    let sources = agent.sources();
    let s = sources.iter().find(|s| s.name == "upstream").unwrap();
    assert_eq!(s.head_sha.as_deref(), Some(report.new_head_sha.as_str()));
    assert!(s.last_fetch_at.is_some());
    assert_eq!(s.skill_count, 1, "one SKILL.md committed upstream");

    std::env::remove_var("FAVAI_SOURCES_ROOT");
}
