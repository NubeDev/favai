use std::sync::Arc;

use favai::approvals::JsonlApprovalStore;
use starter_flow_spi::skill::SkillId;
use starter_skills::{ApprovalRow, ApprovalStore};
use tempfile::TempDir;

fn sid(s: &str) -> SkillId {
    SkillId::new(s).unwrap()
}

#[tokio::test]
async fn record_lookup_round_trip_survives_reopen() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("approvals.jsonl");

    let store = JsonlApprovalStore::open(&path).unwrap();
    let row = ApprovalRow::now(sid("favai.test.a"), "hash-1", "alice");
    store.record(row.clone()).await.unwrap();

    let reopened = JsonlApprovalStore::open(&path).unwrap();
    let found = reopened
        .lookup(&sid("favai.test.a"), "hash-1")
        .await
        .unwrap()
        .expect("row present after reopen");
    assert_eq!(found.skill_id, row.skill_id);
    assert_eq!(found.bundle_hash, "hash-1");
    assert_eq!(found.approved_by, "alice");
}

#[tokio::test]
async fn revoke_persists_across_reopen() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("approvals.jsonl");

    let store = JsonlApprovalStore::open(&path).unwrap();
    store
        .record(ApprovalRow::now(sid("favai.test.b"), "h", "bob"))
        .await
        .unwrap();
    store.revoke(&sid("favai.test.b"), "h").await.unwrap();

    let reopened = JsonlApprovalStore::open(&path).unwrap();
    assert!(reopened
        .lookup(&sid("favai.test.b"), "h")
        .await
        .unwrap()
        .is_none());
    assert!(reopened.list().await.unwrap().is_empty());
}

#[tokio::test]
async fn list_returns_all_active_rows() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("approvals.jsonl");
    let store: Arc<dyn ApprovalStore> = Arc::new(JsonlApprovalStore::open(&path).unwrap());
    store
        .record(ApprovalRow::now(sid("favai.test.x"), "1", "op"))
        .await
        .unwrap();
    store
        .record(ApprovalRow::now(sid("favai.test.y"), "2", "op"))
        .await
        .unwrap();
    let rows = store.list().await.unwrap();
    assert_eq!(rows.len(), 2);
}
