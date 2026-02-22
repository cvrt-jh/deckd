use crate::config::schema::AppConfig;
use crate::config::watcher;
use crate::device::DeviceManager;
use crate::error::Result;
use crate::event::DeckEvent;
use crate::page::PageManager;
use arc_swap::ArcSwap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};

const CHANNEL_CAPACITY: usize = 64;

/// Run the deckd daemon.
///
/// # Errors
/// Returns `DeckError` if a fatal error occurs in any subsystem.
pub async fn run(config: AppConfig, config_path: PathBuf) -> Result<()> {
    let cancel = CancellationToken::new();
    let (tx, _) = broadcast::channel::<DeckEvent>(CHANNEL_CAPACITY);

    let shared_config = Arc::new(ArcSwap::from_pointee(config));
    let mut page_manager = PageManager::new(&shared_config.load().deckd.home_page);

    let device_handle = spawn_device_manager(&tx, &cancel, &shared_config);
    let watcher_handle = spawn_config_watcher(&tx, &cancel, &config_path);

    let mut rx = tx.subscribe();
    let event_tx = tx.clone();

    info!(
        "deckd daemon running, home page: {}",
        page_manager.current_page()
    );

    loop {
        let event = tokio::select! {
            () = cancel.cancelled() => break,
            () = async { tokio::signal::ctrl_c().await.ok(); } => {
                info!("received SIGINT, shutting down");
                cancel.cancel();
                break;
            }
            event = rx.recv() => {
                match event {
                    Ok(e) => e,
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        warn!("event loop lagged, missed {n} events");
                        continue;
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        };

        if handle_event(event, &shared_config, &mut page_manager, &tx, &event_tx) {
            cancel.cancel();
            break;
        }
    }

    info!("daemon shutting down...");
    cancel.cancel();

    let _ = tokio::time::timeout(std::time::Duration::from_secs(5), async {
        let _ = device_handle.await;
        let _ = watcher_handle.await;
    })
    .await;

    info!("daemon stopped");
    Ok(())
}

fn spawn_device_manager(
    tx: &broadcast::Sender<DeckEvent>,
    cancel: &CancellationToken,
    config: &Arc<ArcSwap<AppConfig>>,
) -> tokio::task::JoinHandle<()> {
    let device_tx = tx.clone();
    let device_cancel = cancel.clone();
    let reconnect_ms = config.load().deckd.reconnect_interval_ms;
    tokio::spawn(async move {
        let dm = DeviceManager::new(device_tx, device_cancel, reconnect_ms);
        if let Err(e) = dm.run().await {
            error!("device manager error: {e}");
        }
    })
}

fn spawn_config_watcher(
    tx: &broadcast::Sender<DeckEvent>,
    cancel: &CancellationToken,
    config_path: &std::path::Path,
) -> tokio::task::JoinHandle<()> {
    let watcher_tx = tx.clone();
    let watcher_cancel = cancel.clone();
    let watcher_path = config_path.to_path_buf();
    tokio::spawn(async move {
        if let Err(e) = watcher::watch_config(watcher_path, watcher_tx, watcher_cancel).await {
            error!("config watcher error: {e}");
        }
    })
}

/// Handle a single event. Returns `true` if the daemon should shut down.
fn handle_event(
    event: DeckEvent,
    shared_config: &Arc<ArcSwap<AppConfig>>,
    page_manager: &mut PageManager,
    tx: &broadcast::Sender<DeckEvent>,
    event_tx: &broadcast::Sender<DeckEvent>,
) -> bool {
    match event {
        DeckEvent::ButtonDown(key) => {
            let config = shared_config.load();
            if let Some(button) = page_manager.button_for_key(&config, key) {
                if let Some(ref action) = button.on_press {
                    let action = action.clone();
                    let action_tx = event_tx.clone();
                    tokio::spawn(async move {
                        if let Err(e) = crate::action::execute(&action, &action_tx).await {
                            error!("action error (key {key}): {e}");
                        }
                    });
                }
            }
        }

        DeckEvent::ButtonUp(_) => {}

        DeckEvent::DeviceConnected => {
            info!("device connected, rendering all buttons");
            let _ = tx.send(DeckEvent::RenderAll);
        }

        DeckEvent::DeviceDisconnected => {
            info!("device disconnected, waiting for reconnect...");
        }

        DeckEvent::ConfigReloaded(new_config) => {
            shared_config.store(new_config);
            let config = shared_config.load();
            page_manager.set_home_page(&config.deckd.home_page);
            if !config.pages.contains_key(page_manager.current_page()) {
                page_manager.go_home();
            }
            let _ = tx.send(DeckEvent::RenderAll);
        }

        DeckEvent::NavigateTo(page_id) => {
            let config = shared_config.load();
            if config.pages.contains_key(&page_id) {
                page_manager.navigate_to(&page_id);
                let _ = tx.send(DeckEvent::RenderAll);
            } else {
                warn!("page not found: {page_id}");
            }
        }

        DeckEvent::NavigateBack => {
            if page_manager.go_back() {
                let _ = tx.send(DeckEvent::RenderAll);
            }
        }

        DeckEvent::NavigateHome => {
            page_manager.go_home();
            let _ = tx.send(DeckEvent::RenderAll);
        }

        DeckEvent::RenderAll => {
            let config = shared_config.load();
            let page_id = page_manager.current_page();
            if let Some(page) = config.pages.get(page_id) {
                info!(
                    "rendering page '{}' ({} buttons)",
                    page.name,
                    page.buttons.len()
                );
            }
        }

        DeckEvent::RenderButton(_key) => {}

        DeckEvent::Shutdown => {
            info!("shutdown event received");
            return true;
        }
    }

    false
}
