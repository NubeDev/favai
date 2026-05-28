//! End-to-end "host's view" of favai with **git skipped entirely**.
//!
//! Why this test exists: the v1 acceptance story is "a host
//! (Claude/Codex/Copilot) sees an approved skill in `tools/list`
//! and invoking it returns the body." Git is incidental — the
//! design doc is explicit that sync only ever produces a tree on
//! disk that `SkillRegistry::load_dir_quarantined` consumes. So
//! to prove the *idea* we can pre-stage the post-swap live tree
//! and skip clone/fetch/swap.
//!
//! The flow this test exercises:
//!
//! 1. Stage `<sources_root>/<name>/live/<skills_path>/<bundle>/SKILL.md`
//!    (what a successful sync would have produced).
//! 2. Boot the full bridge — same code path `favai serve` uses,
//!    minus `run_stdio`.
//! 3. Assert the bundle starts quarantined and is NOT in `tools/list`.
//! 4. Approve via the persistent `JsonlApprovalStore`.
//! 5. Rebuild `ToolRegistry` (what host reconnect would trigger)
//!    and assert the skill is now visible AND invokes successfully.
//! 6. Revoke, rebuild, assert the tool is gone.

use std::sync::Arc;

use favai::approvals::JsonlApprovalStore;
use favai::mcp_bridge::{build_tool_registry, McpBridgeConfig};
use favai::{FavaiAgent, FavaiConfig};
use favai::config::Source;
use starter_flow_spi::skill::SkillId;
use starter_skills::ApprovalStore;
use starter_spi::auth::{Principal, Role};

const SKILL_BODY: &str = "Hello from favai. The demo skill ran.";

fn write_demo_bundle(dir: &std::path::Path) {
    std::fs::create_dir_all(dir).unwrap();
    let md = format!(
        "---\n\
         id: com.demo.skills.hello\n\
         description: Demo skill — proves favai end-to-end.\n\
         trust: approved\n\
         ---\n\
         {SKILL_BODY}\n"
    );
    std::fs::write(dir.join("SKILL.md"), md).unwrap();
}

fn operator() -> Principal {
    Principal {
        subject:   "demo-operator".into(),
        role:      Role::Admin,
        scopes:    vec![],
        tenant_id: None,
        teams:     vec![],
        extra:     serde_json::Value::Null,
    }
}

#[tokio::test]
async fn host_view_quarantine_approve_invoke_revoke() {
    let tmp = tempfile::tempdir().unwrap();
    let sources_root = tmp.path().join("sources");
    let approvals_path = tmp.path().join("approvals.jsonl");
    let user_skills_dir = tmp.path().join("user-skills");

    // 1. Stage what a successful sync would have produced. No git.
    //    Layout: <sources_root>/<name>/<skills_path>/<bundle>/SKILL.md
    let bundle_dir = sources_root
        .join("demo-source")
        .join("skills")
        .join("hello");
    write_demo_bundle(&bundle_dir);

    std::env::set_var("FAVAI_SOURCES_ROOT", &sources_root);

    let config = FavaiConfig {
        sources: vec![Source {
            name:        "demo-source".into(),
            // url+branch are not exercised in this test — we never
            // call sync_now. Any valid-looking values pass config
            // validation.
            url:         "https://example.invalid/demo.git".into(),
            branch:      "main".into(),
            skills_path: "skills".into(),
        }],
        periodic: None,
    };

    // 2. Boot bridge with the persistent store at a tempdir path.
    //    We deliberately use McpBridgeConfig::from_favai_config — the
    //    same constructor `favai serve` uses — so this test covers
    //    the production path rather than a hand-crafted shortcut.
    let mut bridge_config = McpBridgeConfig::from_favai_config(&config)
        .expect("bridge config");
    bridge_config.add_favorite_dir = Some(user_skills_dir.clone());
    std::fs::create_dir_all(&user_skills_dir).unwrap();

    let approvals: Arc<dyn ApprovalStore> =
        Arc::new(JsonlApprovalStore::open(&approvals_path).unwrap());
    let (skills, _registry) = build_tool_registry(&bridge_config, approvals.clone())
        .await
        .expect("bridge boots");
    let skills = Arc::new(skills);

    let agent = FavaiAgent::start(
        config,
        Arc::clone(&skills),
        Some(user_skills_dir.clone()),
    )
    .await
    .expect("agent starts");

    // 3. Pre-approval: skill is quarantined, NOT in tools/list.
    let quarantined = skills.list_quarantined();
    assert_eq!(quarantined.len(), 1, "exactly one quarantined bundle");
    let demo_id = SkillId::new("com.demo.skills.hello").unwrap();
    assert_eq!(quarantined[0].id, demo_id);
    let bundle_hash = quarantined[0].bundle_hash.clone();

    let pre = agent.tool_registry();
    assert!(
        pre.get("com.demo.skills.hello").is_none(),
        "quarantined bundle must not appear in tools/list"
    );

    // 4. Operator approves.
    skills.approve(&demo_id, &bundle_hash, &operator()).await
        .expect("approve");

    // 5. Host reconnect = rebuild ToolRegistry. Skill now visible.
    let post = agent.tool_registry();
    let tool = post.get("com.demo.skills.hello")
        .expect("approved skill in tools/list");

    // tools/call → expect SkillTool to return {"body": <skill body>}.
    let out = tool.invoke(serde_json::json!({})).await
        .expect("invoke");
    let body = out.get("body").and_then(|v| v.as_str()).unwrap_or("");
    assert!(
        body.contains(SKILL_BODY),
        "tools/call returned the skill body. got={out}",
    );

    // 6. Revoke → rebuild → tool gone.
    skills.revoke(&demo_id, &bundle_hash, &operator()).await
        .expect("revoke");
    let after_revoke = agent.tool_registry();
    assert!(
        after_revoke.get("com.demo.skills.hello").is_none(),
        "revoked skill must drop from tools/list on next host reconnect"
    );

    // 7. Persistence: the revoke must survive a "restart" — reopen
    //    the JSONL store and re-list approvals.
    drop(skills);
    drop(agent);
    let reopened = JsonlApprovalStore::open(&approvals_path).unwrap();
    let rows = reopened.list().await.unwrap();
    assert!(
        rows.iter().all(|r| r.skill_id != demo_id),
        "after revoke + reopen, approval row must be gone. rows={rows:?}"
    );

    agent_shutdown_noop();
}

// `agent.shutdown()` consumes; we already moved out of it above.
// Keep a marker so future maintainers know the lifecycle was
// considered.
fn agent_shutdown_noop() {}
