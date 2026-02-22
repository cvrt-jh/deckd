use crate::error::{DeckError, Result};
use crate::event::DeckEvent;
use elgato_streamdeck::asynchronous::AsyncStreamDeck;
use elgato_streamdeck::StreamDeckInput;
use std::sync::Arc;
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;
use tracing::debug;

/// Read button events from the Stream Deck, forwarding to broadcast channel.
pub async fn read_input_loop(
    deck: Arc<AsyncStreamDeck>,
    tx: broadcast::Sender<DeckEvent>,
    cancel: CancellationToken,
) -> Result<()> {
    loop {
        if cancel.is_cancelled() {
            return Ok(());
        }

        // read_input uses block_in_place internally, poll at 60Hz.
        let input = deck
            .read_input(60.0)
            .await
            .map_err(|e| DeckError::Hid(e.to_string()))?;

        match input {
            StreamDeckInput::ButtonStateChange(buttons) => {
                for (idx, &pressed) in buttons.iter().enumerate() {
                    let key = idx as u8;
                    if pressed {
                        debug!("button {key} down");
                        let _ = tx.send(DeckEvent::ButtonDown(key));
                    } else {
                        debug!("button {key} up");
                        let _ = tx.send(DeckEvent::ButtonUp(key));
                    }
                }
            }
            StreamDeckInput::NoData => {}
            _ => {
                // Encoder, touchscreen — ignore for MK.2.
            }
        }
    }
}
