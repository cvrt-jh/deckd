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
pub async fn run(config: AppConfig, config_path: PathBuf) -> Result<()> {
    let cancel = CancellationToken::new();
    let (tx, _) = broadcast::channel::<DeckEvent>(CHANNEL_CAPACITY);

    let shared_config = Arc::new(ArcSwap::from_pointee(config));
    let mut page_manager = PageManager::new(&shared_config.load().deckd.home_page);

    // Spawn device manager.
    let device_tx = tx.clone();
    let device_cancel = cancel.clone();
    let reconnect_ms = shared_config.load().deckd.reconnect_interval_ms;
    let device_handle = tokio::spawn(async move {
        let dm = DeviceManager::new(device_tx, device_cancel, reconnect_ms);
        if let Err(e) = dm.run().await {
            error!("device manager error: {e}");
        }
    });

    // Spawn config watcher.
    let watcher_tx = tx.clone();
    let watcher_cancel = cancel.clone();
    let watcher_path = config_path.clone();
    let watcher_handle = tokio::spawn(async move {
        if let Err(e) = watcher::watch_config(watcher_path, watcher_tx, watcher_cancel).await {
            error!("config watcher error: {e}");
        }
    });

    // Main event loop.
    let mut rx = tx.subscribe();
    let event_tx = tx.clone();

    info!(
        "deckd daemon running, home page: {}",
        page_manager.current_page()
    );

    loop {
        let event = tokio::select! {
            _ = cancel.cancelled() => break,
            _ = tokio::signal::ctrl_c() => {
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

            DeckEvent::ButtonUp(_) => {
                // No action on release for MVP.
            }

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
                // If current page no longer exists, go home.
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
                // TODO: Actually push rendered images to the device.
                // For now, log it. Full device rendering requires holding
                // a reference to the AsyncStreamDeck which will be added
                // when device and render are integrated.
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

            DeckEvent::RenderButton(_key) => {
                // TODO: Single button re-render.
            }

            DeckEvent::Shutdown => {
                info!("shutdown event received");
                cancel.cancel();
                break;
            }
        }
    }

    info!("daemon shutting down...");
    cancel.cancel();

    // Wait for spawned tasks (with timeout).
    let _ = tokio::time::timeout(std::time::Duration::from_secs(5), async {
        let _ = device_handle.await;
        let _ = watcher_handle.await;
    })
    .await;

    info!("daemon stopped");
    Ok(())
}
