use std::time::Duration;

use tokio::sync::mpsc;
use tokio::time::timeout;

/// Drain a burst of fs events into a single reload signal.
/// Returns once the channel is quiet for `window`, or the channel closes.
pub async fn debounce_events(rx: &mut mpsc::Receiver<notify::Event>, window: Duration) {
    loop {
        match timeout(window, rx.recv()).await {
            Ok(Some(_)) => continue,
            Ok(None) | Err(_) => break,
        }
    }
}
