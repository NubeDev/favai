//! File-backed [`ApprovalStore`] using JSON Lines.
//!
//! Each row is one line of JSON; appends are O(1) and atomic at the
//! syscall level for short writes. On startup the whole file is read
//! into memory; subsequent lookups are in-memory.
//!
//! The format is forward-compatible — unknown fields on a row are
//! ignored, and a v2 schema can be detected by an explicit
//! `schema: 2` discriminator. v1 omits the discriminator.
//!
//! This is the persistence layer the favai design promised so the
//! "approval click per bundle per machine per change" guarantee
//! survives `favai-cli` restarts.

use std::collections::HashMap;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use starter_flow_spi::skill::SkillId;
use starter_skills::{ApprovalRow, ApprovalStore, ApprovalStoreError};

use crate::error::FavaiError;

/// Default approvals file path:
/// `$HOME/.config/starter/favai/approvals.jsonl`.
pub fn default_approvals_path() -> Result<PathBuf, FavaiError> {
    let home = std::env::var("HOME")
        .map_err(|_| FavaiError::ConfigRead("HOME not set".into()))?;
    Ok(PathBuf::from(home)
        .join(".config")
        .join("starter")
        .join("favai")
        .join("approvals.jsonl"))
}

#[derive(Debug, Serialize, Deserialize)]
struct RowJson {
    skill_id:            String,
    bundle_hash:         String,
    approved_by:         String,
    approved_at_unix_ms: u64,
    /// Marks revoked rows so a fresh load can replay history without
    /// silently honouring a row a previous run removed.
    #[serde(default, skip_serializing_if = "is_false")]
    revoked: bool,
}

#[allow(clippy::trivially_copy_pass_by_ref)]
fn is_false(b: &bool) -> bool {
    !*b
}

type Key = (String, String);

/// Persistent [`ApprovalStore`] backed by a JSON-Lines file.
pub struct JsonlApprovalStore {
    path:  PathBuf,
    state: Mutex<HashMap<Key, ApprovalRow>>,
}

impl JsonlApprovalStore {
    /// Open (or create) the store at `path`. Replays the file into
    /// an in-memory map: append-only on the wire, last-write-wins
    /// per `(skill_id, bundle_hash)` in memory.
    pub fn open(path: impl Into<PathBuf>) -> Result<Self, FavaiError> {
        let path = path.into();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                FavaiError::ConfigRead(format!("create approvals dir {}: {e}", parent.display()))
            })?;
        }

        let mut state: HashMap<Key, ApprovalRow> = HashMap::new();
        if path.exists() {
            let raw = std::fs::read_to_string(&path).map_err(|e| {
                FavaiError::ConfigRead(format!("read approvals {}: {e}", path.display()))
            })?;
            for (lineno, line) in raw.lines().enumerate() {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                let row: RowJson = serde_json::from_str(line).map_err(|e| {
                    FavaiError::ConfigParse(format!(
                        "approvals {}:{}: {e}",
                        path.display(),
                        lineno + 1
                    ))
                })?;
                let key = (row.skill_id.clone(), row.bundle_hash.clone());
                if row.revoked {
                    state.remove(&key);
                    continue;
                }
                let skill_id = SkillId::new(row.skill_id).map_err(|e| {
                    FavaiError::ConfigParse(format!("approvals {}:{}: {e}", path.display(), lineno + 1))
                })?;
                state.insert(
                    key,
                    ApprovalRow {
                        skill_id,
                        bundle_hash:         row.bundle_hash,
                        approved_by:         row.approved_by,
                        approved_at_unix_ms: row.approved_at_unix_ms,
                    },
                );
            }
        }

        Ok(Self {
            path,
            state: Mutex::new(state),
        })
    }

    fn append_line(&self, json: &str) -> Result<(), ApprovalStoreError> {
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .map_err(ApprovalStoreError::backend)?;
        f.write_all(json.as_bytes()).map_err(ApprovalStoreError::backend)?;
        f.write_all(b"\n").map_err(ApprovalStoreError::backend)?;
        // fsync so an immediate crash after `favai approve` does not
        // lose the row. Costs ~1 ms; acceptable for an interactive
        // operator action.
        f.sync_data().map_err(ApprovalStoreError::backend)?;
        Ok(())
    }
}

#[async_trait]
impl ApprovalStore for JsonlApprovalStore {
    async fn record(&self, row: ApprovalRow) -> Result<(), ApprovalStoreError> {
        let json = serde_json::to_string(&RowJson {
            skill_id:            row.skill_id.to_string(),
            bundle_hash:         row.bundle_hash.clone(),
            approved_by:         row.approved_by.clone(),
            approved_at_unix_ms: row.approved_at_unix_ms,
            revoked:             false,
        })
        .map_err(ApprovalStoreError::backend)?;
        self.append_line(&json)?;
        let key = (row.skill_id.to_string(), row.bundle_hash.clone());
        self.state
            .lock()
            .expect("approval store poisoned")
            .insert(key, row);
        Ok(())
    }

    async fn lookup(
        &self,
        skill_id: &SkillId,
        bundle_hash: &str,
    ) -> Result<Option<ApprovalRow>, ApprovalStoreError> {
        let key = (skill_id.to_string(), bundle_hash.to_string());
        Ok(self
            .state
            .lock()
            .expect("approval store poisoned")
            .get(&key)
            .cloned())
    }

    async fn list(&self) -> Result<Vec<ApprovalRow>, ApprovalStoreError> {
        Ok(self
            .state
            .lock()
            .expect("approval store poisoned")
            .values()
            .cloned()
            .collect())
    }

    async fn revoke(
        &self,
        skill_id: &SkillId,
        bundle_hash: &str,
    ) -> Result<(), ApprovalStoreError> {
        let key = (skill_id.to_string(), bundle_hash.to_string());
        let json = serde_json::to_string(&RowJson {
            skill_id:            skill_id.to_string(),
            bundle_hash:         bundle_hash.to_string(),
            approved_by:         String::new(),
            approved_at_unix_ms: 0,
            revoked:             true,
        })
        .map_err(ApprovalStoreError::backend)?;
        self.append_line(&json)?;
        self.state
            .lock()
            .expect("approval store poisoned")
            .remove(&key);
        Ok(())
    }
}
