//! Integration test: sync → reload → re-register loop.
//!
//! Bypasses the actual git+sync codepath (no network in tests) and
//! drives `SkillRegistry::reload()` directly after dropping a new
//! `SKILL.md` into a quarantined load-dir. Asserts:
//!
//!   1. Newly-loaded bundles are *quarantined*, so they do **not**
//!      appear in `tool_registry().list()`.
//!   2. After `SkillRegistry::approve(id, hash, principal)` and a
//!      fresh `tool_registry()` build, the skill is now visible as
//!      a tool.
//!
//! This is the contract the favai-cli's serve loop relies on: every
//! sync_now triggers a reload, every reload re-quarantines anything
//! whose bundle_hash changed, and the operator's approve() is what
//! promotes it back into tools/list.

use std::sync::Arc;

use favai::mcp_bridge::build_tool_registry_from_skills;
use starter_skills::approval::hash_bundle;
use starter_skills::{InMemoryApprovalStore, SkillRegistry};
use starter_spi::auth::{Principal, Role};
use starter_flow_spi::skill::SkillId;
use tempfile::TempDir;

const SKILL_ID: &str = "favai.test.hello";

fn write_skill_md(dir: &std::path::Path) {
    let bundle = dir.join("hello");
    std::fs::create_dir_all(&bundle).unwrap();
    std::fs::write(
        bundle.join("SKILL.md"),
        "---\n\
         id: favai.test.hello\n\
         description: Test fixture skill.\n\
         trust: quarantined\n\
         ---\n\
         Hello, world.\n",
    )
    .unwrap();
}

fn principal() -> Principal {
    Principal {
        subject:   "test-operator".into(),
        role:      Role::Admin,
        scopes:    vec![],
        tenant_id: None,
        teams:     vec![],
        extra:     serde_json::Value::Null,
    }
}

#[tokio::test]
async fn sync_reload_reregister_loop() {
    let tmp = TempDir::new().unwrap();
    let dir = tmp.path().to_path_buf();
    // Empty load_dir is fine — SkillRegistry::build walks zero
    // bundles on first call.
    let approvals = Arc::new(InMemoryApprovalStore::new());
    let skills = SkillRegistry::builder()
        .with_approval_store_arc(approvals.clone())
        .load_dir_quarantined(dir.clone())
        .build()
        .await
        .unwrap();

    let tools = build_tool_registry_from_skills(&skills, None);
    assert!(
        !tools.list().iter().any(|t| t.name == SKILL_ID),
        "skill must not appear before its bundle exists"
    );

    // Simulate a sync: write a new SKILL.md, then reload.
    write_skill_md(&dir);
    skills.reload().await.unwrap();

    // Quarantined-on-load: present in the registry, but not in the
    // *approved* list, so not registered as a tool.
    assert!(
        skills.list_quarantined().iter().any(|s| s.id.as_str() == SKILL_ID),
        "newly-loaded bundle must land in the quarantined list"
    );
    let tools = build_tool_registry_from_skills(&skills, None);
    assert!(
        !tools.list().iter().any(|t| t.name == SKILL_ID),
        "quarantined skill must not be visible as a tool"
    );

    // Operator approval — per-bundle, per-hash, per machine.
    let bundle_hash = hash_bundle(dir.join("hello")).unwrap();
    let skill_id = SkillId::new(SKILL_ID).unwrap();
    skills
        .approve(&skill_id, &bundle_hash, &principal())
        .await
        .unwrap();

    // A fresh ToolRegistry now sees the skill.
    let tools = build_tool_registry_from_skills(&skills, None);
    assert!(
        tools.list().iter().any(|t| t.name == SKILL_ID),
        "approved skill must appear in the rebuilt ToolRegistry"
    );
}
