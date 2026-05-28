use chrono::{DateTime, Utc};

#[derive(Debug, Clone)]
pub struct SyncReport {
    pub source_name:      String,
    pub new_head_sha:     String,
    pub files_changed:    usize,
    pub bytes_pulled:     u64,
    pub duration_ms:      u64,
    pub reload_triggered: bool,
    pub at:               DateTime<Utc>,
}
