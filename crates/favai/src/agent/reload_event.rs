use chrono::{DateTime, Utc};

#[derive(Debug, Clone)]
pub struct ReloadEvent {
    pub trigger: ReloadTrigger,
    pub sources: Vec<String>,
    pub at:      DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub enum ReloadTrigger {
    SyncCompleted,
    WatcherDebounced,
}
