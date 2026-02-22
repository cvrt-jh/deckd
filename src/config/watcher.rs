use crate::error::DeckError;
use crate::event::DeckEvent;
use notify_debouncer_mini::{new_debouncer, DebouncedEventKind};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

/// Watch a config file for changes and emit `ConfigReloaded` events.
///
/// # Errors
/// Returns `DeckError::Watcher` if the file watcher cannot be initialized.
pub async fn watch_config(
    config_path: PathBuf,
    tx: broadcast::Sender<DeckEvent>,
    cancel: CancellationToken,
) -> crate::error::Result<()> {
    let (notify_tx, mut notify_rx) = tokio::sync::mpsc::channel(16);
    let watch_path = config_path.clone();

    // The notify watcher must live on a blocking thread.
    let _watcher_handle = tokio::task::spawn_blocking(move || {
        let rt_tx = notify_tx;
        let debouncer = new_debouncer(
            Duration::from_millis(500),
            move |events: Result<Vec<notify_debouncer_mini::DebouncedEvent>, notify::Error>| {
                match events {
                    Ok(evts) => {
                        for evt in evts {
                            if evt.kind == DebouncedEventKind::Any {
                                let _ = rt_tx.blocking_send(evt.path);
                            }
                        }
                    }
                    Err(e) => {
                        warn!("file watcher error: {e}");
                    }
                }
            },
        )
        .map_err(|e| DeckError::Watcher(e.to_string()));

        match debouncer {
            Ok(mut d) => {
                if let Err(e) = d
                    .watcher()
                    .watch(&watch_path, notify::RecursiveMode::NonRecursive)
                {
                    warn!("failed to watch config file: {e}");
                    return;
                }
                info!("watching config file: {}", watch_path.display());
                // Keep the debouncer alive until the thread is dropped.
                loop {
                    std::thread::sleep(Duration::from_secs(1));
                }
            }
            Err(e) => {
                warn!("failed to create file watcher: {e}");
            }
        }
    });

    loop {
        tokio::select! {
            _ = cancel.cancelled() => {
                info!("config watcher shutting down");
                return Ok(());
            }
            Some(_path) = notify_rx.recv() => {
                info!("config file changed, reloading...");
                match crate::config::load(&config_path) {
                    Ok(new_config) => {
                        let config = Arc::new(new_config);
                        let _ = tx.send(DeckEvent::ConfigReloaded(config));
                        info!("config reloaded successfully");
                    }
                    Err(e) => {
                        warn!("config reload failed, keeping old config: {e}");
                    }
                }
            }
        }
    }
}
