pub mod input;

use crate::error::{DeckError, Result};
use crate::event::DeckEvent;
use arc_swap::ArcSwap;
use elgato_streamdeck::asynchronous::AsyncStreamDeck;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

/// Shared handle to the currently connected Stream Deck (if any).
pub type DeckHandle = Arc<ArcSwap<Option<Arc<AsyncStreamDeck>>>>;

/// Create a new empty deck handle.
#[must_use]
pub fn new_deck_handle() -> DeckHandle {
    Arc::new(ArcSwap::from_pointee(None))
}

/// Manages discovery, connection, and reconnection of a Stream Deck device.
pub struct DeviceManager {
    tx: broadcast::Sender<DeckEvent>,
    cancel: CancellationToken,
    reconnect_interval: Duration,
    handle: DeckHandle,
}

impl DeviceManager {
    #[must_use]
    pub fn new(
        tx: broadcast::Sender<DeckEvent>,
        cancel: CancellationToken,
        reconnect_interval_ms: u64,
        handle: DeckHandle,
    ) -> Self {
        Self {
            tx,
            cancel,
            reconnect_interval: Duration::from_millis(reconnect_interval_ms),
            handle,
        }
    }

    /// Run the device manager loop: discover -> connect -> read -> reconnect on disconnect.
    ///
    /// # Errors
    /// Returns `DeckError` if a fatal device error occurs.
    pub async fn run(self) -> Result<()> {
        loop {
            if self.cancel.is_cancelled() {
                return Ok(());
            }

            match Self::discover_and_connect() {
                Ok(deck) => {
                    info!("Stream Deck connected");
                    self.handle.store(Arc::new(Some(Arc::clone(&deck))));
                    let _ = self.tx.send(DeckEvent::DeviceConnected);

                    if let Err(e) =
                        input::read_input_loop(deck, self.tx.clone(), self.cancel.clone()).await
                    {
                        warn!("device disconnected: {e}");
                        self.handle.store(Arc::new(None));
                        let _ = self.tx.send(DeckEvent::DeviceDisconnected);
                    }
                }
                Err(e) => {
                    warn!("no device found: {e}");
                }
            }

            tokio::select! {
                () = self.cancel.cancelled() => return Ok(()),
                () = tokio::time::sleep(self.reconnect_interval) => {}
            }
        }
    }

    fn discover_and_connect() -> Result<Arc<AsyncStreamDeck>> {
        let hid = elgato_streamdeck::new_hidapi().map_err(|e| DeckError::Hid(e.to_string()))?;

        let devices = elgato_streamdeck::list_devices(&hid);
        if devices.is_empty() {
            return Err(DeckError::NoDevice);
        }

        let (kind, serial) = &devices[0];
        info!("found Stream Deck {:?} (serial: {})", kind, serial);

        let deck = AsyncStreamDeck::connect(&hid, *kind, serial)
            .map_err(|e| DeckError::Device(e.to_string()))?;

        Ok(Arc::new(deck))
    }
}
