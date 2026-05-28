use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use starter_skills::SkillRegistry;
use tokio::sync::{broadcast, Mutex};
use tokio::task::JoinHandle;

use crate::config::{FavaiConfig, Periodic};
use super::reload_event::ReloadEvent;
use super::sync_now::run_sync;

/// Spawn the periodic sync task per
/// `favai-sync-and-registry.md` §"Still open" — interval is jittered
/// uniformly in `[0.9, 1.1] * interval_secs` so a fleet of PCs does
/// not stamp `:00 :15 :30 :45` at GitHub.
///
/// The task syncs every configured source on each tick. Failures are
/// logged at `warn` and the schedule keeps running — a single
/// network blip should not stop later syncs.
pub(crate) fn spawn(
    periodic:   Periodic,
    config:     FavaiConfig,
    sync_mutex: Arc<Mutex<()>>,
    reload_tx:  broadcast::Sender<ReloadEvent>,
    skills:     Arc<SkillRegistry>,
) -> JoinHandle<()> {
    let base = periodic.interval_secs.max(60);
    tokio::spawn(async move {
        loop {
            let wait = jittered(base);
            tokio::time::sleep(wait).await;
            for source in &config.sources {
                match run_sync(&config, &source.name, &sync_mutex, &reload_tx, Some(skills.clone()))
                    .await
                {
                    Ok(report) => tracing::info!(
                        source = %report.source_name,
                        head   = %report.new_head_sha,
                        ms     = report.duration_ms,
                        "favai: periodic sync"
                    ),
                    Err(e) => tracing::warn!(
                        source = %source.name,
                        error  = %e,
                        "favai: periodic sync failed; will retry next tick"
                    ),
                }
            }
        }
    })
}

/// Uniform jitter on `[0.9 * base, 1.1 * base]` seconds. The "random"
/// source is `SystemTime::now() % 1000`, which is plenty for fleet
/// spread — we are not deriving keys here.
fn jittered(base_secs: u64) -> Duration {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos() as u64)
        .unwrap_or(0);
    let permille = 900 + (nanos % 201); // 900..=1100
    Duration::from_millis((base_secs * 1000) * permille / 1000)
}

#[cfg(test)]
mod tests {
    use super::jittered;

    #[test]
    fn jitter_within_ten_percent_window() {
        for _ in 0..200 {
            let d = jittered(600);
            assert!(d.as_millis() >= 540_000, "below 90% floor: {:?}", d);
            assert!(d.as_millis() <= 660_000, "above 110% ceiling: {:?}", d);
        }
    }
}
