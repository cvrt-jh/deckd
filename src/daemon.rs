use crate::config::schema::AppConfig;
use crate::config::watcher;
use crate::device::{DeckHandle, DeviceManager};
use crate::error::Result;
use crate::event::DeckEvent;
use crate::page::PageManager;
use arc_swap::ArcSwap;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};

const CHANNEL_CAPACITY: usize = 64;
/// Stream Deck MK.2 has 15 keys (0-14).
const NUM_KEYS: u8 = 15;

/// Run the deckd daemon.
///
/// # Errors
/// Returns `DeckError` if a fatal error occurs in any subsystem.
pub async fn run(config: AppConfig, config_path: PathBuf) -> Result<()> {
    let cancel = CancellationToken::new();
    let (tx, _) = broadcast::channel::<DeckEvent>(CHANNEL_CAPACITY);

    let shared_config = Arc::new(ArcSwap::from_pointee(config));
    let mut page_manager = PageManager::new(&shared_config.load().deckd.home_page);
    let deck_handle = crate::device::new_deck_handle();

    let config_dir = config_path
        .parent()
        .map_or_else(|| PathBuf::from("."), PathBuf::from);

    let device_handle = spawn_device_manager(&tx, &cancel, &shared_config, &deck_handle);
    let watcher_handle = spawn_config_watcher(&tx, &cancel, &config_path);

    let mut rx = tx.subscribe();

    // Cached HA entity states for optimistic rendering on button press.
    let last_states: Arc<std::sync::Mutex<HashMap<String, String>>> =
        Arc::new(std::sync::Mutex::new(HashMap::new()));

    // Periodic state poll interval (re-render to reflect HA state changes).
    let mut state_poll = tokio::time::interval(std::time::Duration::from_secs(5));
    state_poll.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

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
            _ = state_poll.tick() => {
                // Check if any buttons on the current page use state_entity.
                let config = shared_config.load();
                let page_id = page_manager.current_page();
                let has_stateful = config.pages.get(page_id).is_some_and(|p| {
                    p.buttons.iter().any(|b| {
                        b.state_entity.is_some()
                            || b.label_entity.is_some()
                            || !b.count_entities.is_empty()
                    })
                });
                if has_stateful {
                    let _ = tx.send(DeckEvent::RenderAll);
                }
                continue;
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

        if handle_event(
            event,
            &shared_config,
            &mut page_manager,
            &tx,
            &deck_handle,
            &config_dir,
            &last_states,
        ) {
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
    deck_handle: &DeckHandle,
) -> tokio::task::JoinHandle<()> {
    let device_tx = tx.clone();
    let device_cancel = cancel.clone();
    let reconnect_ms = config.load().deckd.reconnect_interval_ms;
    let handle = Arc::clone(deck_handle);
    tokio::spawn(async move {
        let dm = DeviceManager::new(device_tx, device_cancel, reconnect_ms, handle);
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

/// Handle a key press: optimistically flip a stateful key, then run its action.
fn handle_button_down(
    key: u8,
    shared_config: &Arc<ArcSwap<AppConfig>>,
    page_manager: &PageManager,
    tx: &broadcast::Sender<DeckEvent>,
    deck_handle: &DeckHandle,
    config_dir: &std::path::Path,
    last_states: &Arc<std::sync::Mutex<HashMap<String, String>>>,
) {
    let config = shared_config.load();
    let Some(button) = page_manager.button_for_key(&config, key) else {
        return;
    };

    // Optimistic render: immediately flip the cached visual state.
    if let Some(ref entity_id) = button.state_entity {
        let mut cache = last_states.lock().unwrap();
        let current = cache.get(entity_id).map(String::as_str);
        let flipped = match current {
            Some("on") => "off",
            _ => "on",
        };
        cache.insert(entity_id.clone(), flipped.to_string());
        let states = cache.clone();
        drop(cache);

        let button = button.clone();
        let defaults = config.deckd.defaults.clone();
        let handle = Arc::clone(deck_handle);
        let dir = config_dir.to_path_buf();
        tokio::spawn(async move {
            render_single_button_with_states(&button, &defaults, &handle, &dir, key, &states).await;
        });
    }

    if let Some(ref action) = button.on_press {
        let action = action.clone();
        let action_tx = tx.clone();
        let has_state = button.state_entity.is_some();
        let render_tx = tx.clone();
        tokio::spawn(async move {
            if let Err(e) = crate::action::execute(&action, &action_tx).await {
                error!("action error (key {key}): {e}");
            }
            // Wait for HA to process the state change before syncing.
            if has_state {
                tokio::time::sleep(std::time::Duration::from_secs(3)).await;
                let _ = render_tx.send(DeckEvent::RenderAll);
            }
        });
    }
}

/// Handle a single event. Returns `true` if the daemon should shut down.
fn handle_event(
    event: DeckEvent,
    shared_config: &Arc<ArcSwap<AppConfig>>,
    page_manager: &mut PageManager,
    tx: &broadcast::Sender<DeckEvent>,
    deck_handle: &DeckHandle,
    config_dir: &std::path::Path,
    last_states: &Arc<std::sync::Mutex<HashMap<String, String>>>,
) -> bool {
    match event {
        DeckEvent::ButtonDown(key) => handle_button_down(
            key,
            shared_config,
            page_manager,
            tx,
            deck_handle,
            config_dir,
            last_states,
        ),

        DeckEvent::ButtonUp(_) => {}

        DeckEvent::DeviceConnected => {
            info!("device connected, rendering all buttons");
            // Set brightness on connect.
            let brightness = shared_config.load().deckd.brightness;
            let handle = Arc::clone(deck_handle);
            tokio::spawn(async move {
                if let Some(deck) = handle.load().as_deref() {
                    if let Err(e) = deck.set_brightness(brightness).await {
                        warn!("failed to set brightness: {e}");
                    }
                }
            });
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
            let page_id = page_manager.current_page().to_string();
            if let Some(page) = config.pages.get(&page_id) {
                info!(
                    "rendering page '{}' ({} buttons)",
                    page.name,
                    page.buttons.len()
                );
                let config = Arc::clone(&config);
                let handle = Arc::clone(deck_handle);
                let dir = config_dir.to_path_buf();
                let cache = Arc::clone(last_states);
                tokio::spawn(async move {
                    render_all_buttons(&config, &page_id, &handle, &dir, &cache).await;
                });
            }
        }

        DeckEvent::RenderButton(key) => {
            let config = shared_config.load();
            if let Some(button) = page_manager.button_for_key(&config, key) {
                let button = button.clone();
                let defaults = config.deckd.defaults.clone();
                let handle = Arc::clone(deck_handle);
                let dir = config_dir.to_path_buf();
                tokio::spawn(async move {
                    render_single_button(&button, &defaults, &handle, &dir, key).await;
                });
            }
        }

        DeckEvent::Shutdown => {
            info!("shutdown event received");
            return true;
        }
    }

    false
}

/// Collect every HA entity ID referenced by buttons on a page.
///
/// Covers both `state_entity` (color swapping) and `label_entity` (live value),
/// de-duplicated so one entity used by several buttons is fetched once.
fn collect_state_entities(config: &AppConfig, page_id: &str) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    config.pages.get(page_id).map_or_default(|page| {
        page.buttons
            .iter()
            .flat_map(|b| {
                b.state_entity
                    .iter()
                    .chain(b.label_entity.iter())
                    .chain(b.count_entities.iter())
            })
            .filter(|id| seen.insert((*id).clone()))
            .cloned()
            .collect()
    })
}

/// Merge freshly fetched HA states into the cache and return the merged view.
///
/// A failed fetch yields no entry for that entity, which is indistinguishable
/// from "off" at render time and made the deck visibly drop state whenever HA
/// was briefly slow. Holding the last known value keeps the display stable
/// across transient HA or DNS stalls.
fn merge_states(
    cache: &mut HashMap<String, String>,
    fetched: HashMap<String, String>,
    requested: &[String],
) -> HashMap<String, String> {
    let fetched_count = fetched.len();
    for (k, v) in fetched {
        cache.insert(k, v);
    }

    let missing = requested.len().saturating_sub(fetched_count);
    if missing > 0 {
        warn!(
            "HA state fetch incomplete: {missing}/{} entities missing, holding last known values",
            requested.len()
        );
    }

    cache.clone()
}

/// Render all 15 buttons to the device. Fetches HA states first for stateful buttons.
/// Updates the shared state cache with fresh values from HA.
async fn render_all_buttons(
    config: &AppConfig,
    page_id: &str,
    deck_handle: &DeckHandle,
    config_dir: &std::path::Path,
    state_cache: &std::sync::Mutex<HashMap<String, String>>,
) {
    let Some(page) = config.pages.get(page_id) else {
        return;
    };

    let entities = collect_state_entities(config, page_id);
    let fetched = crate::state::fetch_ha_states(&entities).await;

    let entity_states = if let Ok(mut cache) = state_cache.lock() {
        merge_states(&mut cache, fetched, &entities)
    } else {
        fetched
    };

    let defaults = &config.deckd.defaults;
    let handle = Arc::clone(deck_handle);

    let mut images: Vec<(u8, image::DynamicImage)> = Vec::with_capacity(NUM_KEYS as usize);

    for key in 0..NUM_KEYS {
        let button = page.buttons.iter().find(|b| b.key == key);
        let rgba_data = match button {
            Some(btn) => {
                match crate::render::render_button(btn, defaults, config_dir, &entity_states) {
                    Ok(data) => data,
                    Err(e) => {
                        warn!("render error (key {key}): {e}");
                        continue;
                    }
                }
            }
            None => match crate::render::render_blank() {
                Ok(data) => data,
                Err(e) => {
                    warn!("render blank error (key {key}): {e}");
                    continue;
                }
            },
        };

        if let Some(img_buf) =
            image::RgbaImage::from_raw(crate::render::KEY_PX, crate::render::KEY_PX, rgba_data)
        {
            images.push((key, image::DynamicImage::from(img_buf)));
        }
    }

    let guard = handle.load();
    let Some(deck) = guard.as_deref() else {
        return;
    };
    for (key, img) in images {
        if let Err(e) = deck.set_button_image(key, img).await {
            warn!("failed to set button image (key {key}): {e}");
        }
    }
    if let Err(e) = deck.flush().await {
        warn!("failed to flush button images: {e}");
    }
}

/// Render a single button with pre-supplied entity states (no HA fetch).
/// Used for optimistic rendering on button press.
async fn render_single_button_with_states(
    button: &crate::config::schema::ButtonConfig,
    defaults: &crate::config::schema::ButtonDefaults,
    deck_handle: &DeckHandle,
    config_dir: &std::path::Path,
    key: u8,
    entity_states: &HashMap<String, String>,
) {
    let rgba_data = match crate::render::render_button(button, defaults, config_dir, entity_states)
    {
        Ok(data) => data,
        Err(e) => {
            warn!("render error (key {key}): {e}");
            return;
        }
    };

    let Some(img_buf) =
        image::RgbaImage::from_raw(crate::render::KEY_PX, crate::render::KEY_PX, rgba_data)
    else {
        return;
    };

    let img = image::DynamicImage::from(img_buf);
    let guard = deck_handle.load();
    let Some(deck) = guard.as_deref() else {
        return;
    };
    if let Err(e) = deck.set_button_image(key, img).await {
        warn!("failed to set button image (key {key}): {e}");
    }
    if let Err(e) = deck.flush().await {
        warn!("failed to flush button image: {e}");
    }
}

/// Render a single button to the device. Fetches HA state if needed.
async fn render_single_button(
    button: &crate::config::schema::ButtonConfig,
    defaults: &crate::config::schema::ButtonDefaults,
    deck_handle: &DeckHandle,
    config_dir: &std::path::Path,
    key: u8,
) {
    let entities: Vec<String> = button
        .state_entity
        .iter()
        .chain(button.label_entity.iter())
        .chain(button.count_entities.iter())
        .cloned()
        .collect();
    let entity_states = crate::state::fetch_ha_states(&entities).await;

    let rgba_data = match crate::render::render_button(button, defaults, config_dir, &entity_states)
    {
        Ok(data) => data,
        Err(e) => {
            warn!("render error (key {key}): {e}");
            return;
        }
    };

    let Some(img_buf) =
        image::RgbaImage::from_raw(crate::render::KEY_PX, crate::render::KEY_PX, rgba_data)
    else {
        return;
    };

    let img = image::DynamicImage::from(img_buf);
    let guard = deck_handle.load();
    let Some(deck) = guard.as_deref() else {
        return;
    };
    if let Err(e) = deck.set_button_image(key, img).await {
        warn!("failed to set button image (key {key}): {e}");
    }
    if let Err(e) = deck.flush().await {
        warn!("failed to flush button image: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::schema::{ButtonConfig, ButtonDefaults, PageConfig};
    use deckd_core::Layout;

    fn states(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect()
    }

    fn button(key: u8, state: Option<&str>, label: Option<&str>) -> ButtonConfig {
        ButtonConfig {
            key,
            label: None,
            icon: None,
            background: None,
            text_color: None,
            font_size: None,
            font: None,
            on_press: None,
            state_entity: state.map(str::to_string),
            on_background: None,
            on_text_color: None,
            label_entity: label.map(str::to_string),
            precision: None,
            unknown_text: None,
            thresholds: Vec::new(),
            count_entities: Vec::new(),
            active_text_color: None,
            active_background: None,
        }
    }

    #[test]
    fn merge_holds_last_known_value_when_fetch_fails() {
        let mut cache = states(&[("switch.server", "on"), ("sensor.keller", "27.2")]);
        let requested = vec!["switch.server".to_string(), "sensor.keller".to_string()];

        // Total fetch failure (HA timed out): both values must be held.
        let merged = merge_states(&mut cache, HashMap::new(), &requested);

        assert_eq!(merged.get("switch.server").unwrap(), "on");
        assert_eq!(merged.get("sensor.keller").unwrap(), "27.2");
    }

    #[test]
    fn merge_holds_only_the_missing_entity() {
        let mut cache = states(&[("switch.server", "on"), ("sensor.keller", "27.2")]);
        let requested = vec!["switch.server".to_string(), "sensor.keller".to_string()];

        // Partial failure: server updated, keller missing.
        let merged = merge_states(&mut cache, states(&[("switch.server", "off")]), &requested);

        assert_eq!(
            merged.get("switch.server").unwrap(),
            "off",
            "fresh value must win"
        );
        assert_eq!(
            merged.get("sensor.keller").unwrap(),
            "27.2",
            "missing value must be held"
        );
    }

    #[test]
    fn merge_applies_fresh_values_over_stale_cache() {
        let mut cache = states(&[("switch.server", "on")]);
        let requested = vec!["switch.server".to_string()];

        let merged = merge_states(&mut cache, states(&[("switch.server", "off")]), &requested);

        assert_eq!(merged.get("switch.server").unwrap(), "off");
        // Cache itself must be updated for the next tick.
        assert_eq!(cache.get("switch.server").unwrap(), "off");
    }

    #[test]
    fn merge_on_empty_cache_yields_only_fetched() {
        let mut cache = HashMap::new();
        let requested = vec!["switch.server".to_string()];

        let merged = merge_states(&mut cache, states(&[("switch.server", "on")]), &requested);

        assert_eq!(merged.len(), 1);
        assert_eq!(merged.get("switch.server").unwrap(), "on");
    }

    #[test]
    fn collect_gathers_both_state_and_label_entities() {
        let mut pages = HashMap::new();
        pages.insert(
            "home".to_string(),
            PageConfig {
                name: "Home".into(),
                buttons: vec![
                    button(0, Some("switch.server"), None),
                    button(5, None, Some("sensor.keller_temperature")),
                ],
            },
        );
        let config = AppConfig {
            deckd: crate::config::schema::DeckdConfig {
                brightness: 80,
                reconnect_interval_ms: 2000,
                home_page: "home".into(),
                defaults: ButtonDefaults::default(),
                layout: Layout::default(),
            },
            pages,
        };

        let entities = collect_state_entities(&config, "home");
        assert_eq!(entities.len(), 2);
        assert!(entities.contains(&"switch.server".to_string()));
        assert!(entities.contains(&"sensor.keller_temperature".to_string()));
    }

    #[test]
    fn collect_deduplicates_shared_entities() {
        let mut pages = HashMap::new();
        pages.insert(
            "home".to_string(),
            PageConfig {
                name: "Home".into(),
                buttons: vec![
                    // Same entity used for both color and value on one button,
                    // plus a second button referencing it again.
                    button(
                        0,
                        Some("sensor.keller_temperature"),
                        Some("sensor.keller_temperature"),
                    ),
                    button(1, Some("sensor.keller_temperature"), None),
                ],
            },
        );
        let config = AppConfig {
            deckd: crate::config::schema::DeckdConfig {
                brightness: 80,
                reconnect_interval_ms: 2000,
                home_page: "home".into(),
                defaults: ButtonDefaults::default(),
                layout: Layout::default(),
            },
            pages,
        };

        let entities = collect_state_entities(&config, "home");
        assert_eq!(
            entities.len(),
            1,
            "one entity must be fetched once, got {entities:?}"
        );
    }
}
