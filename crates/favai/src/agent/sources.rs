use chrono::{DateTime, Utc};

#[derive(Debug, Clone)]
pub struct SourceStatus {
    pub name:          String,
    pub url:           String,
    pub branch:        String,
    pub last_fetch_at: Option<DateTime<Utc>>,
    pub head_sha:      Option<String>,
    pub skill_count:   usize,
}
