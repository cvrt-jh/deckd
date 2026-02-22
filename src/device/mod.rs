pub mod input;

use crate::error::{DeckError, Result};
use crate::event::DeckEvent;
use elgato_streamdeck::asynchronous::AsyncStreamDeck;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

/// Manages discovery, connection, and reconnection of a Stream Deck device.
pub struct DeviceManager {
    tx: broadcast::Sender<DeckEvent>,
    cancel: CancellationToken,
    reconnect_interval: Duration,
}

impl DeviceManager {
    pub fn new(
        tx: broadcast::Sender<DeckEvent>,
        cancel: CancellationToken,
        reconnect_interval_ms: u64,
    ) -> Self {
        Self {
            tx,
            cancel,
            reconnect_interval: Duration::from_millis(reconnect_interval_ms),
        }
    }

    /// Run the device manager loop: discover -> connect -> read -> reconnect on disconnect.
    pub async fn run(self) -> Result<()> {
        loop {
            if self.cancel.is_cancelled() {
                return Ok(());
            }

            match self.discover_and_connect().await {
                Ok(deck) => {
                    info!("Stream Deck connected");
                    let _ = self.tx.send(DeckEvent::DeviceConnected);

                    if let Err(e) = input::read_input_loop(deck, self.tx.clone(), self.cancel.clone()).await {
                        warn!("device disconnected: {e}");
                        let _ = self.tx.send(DeckEvent::DeviceDisconnected);
                    }
                }
                Err(e) => {
                    warn!("no device found: {e}");
                }
            }

            tokio::select! {
                _ = self.cancel.cancelled() => return Ok(()),
                _ = tokio::time::sleep(self.reconnect_interval) => {}
            }
        }
    }

    async fn discover_and_connect(&self) -> Result<Arc<AsyncStreamDeck>> {
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
