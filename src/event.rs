use crate::config::schema::AppConfig;
use std::sync::Arc;

/// Events flowing through the broadcast channel connecting all subsystems.
#[derive(Debug, Clone)]
pub enum DeckEvent {
    /// A button was pressed (key index 0-14).
    ButtonDown(u8),

    /// A button was released (key index 0-14).
    ButtonUp(u8),

    /// Stream Deck device connected.
    DeviceConnected,

    /// Stream Deck device disconnected.
    DeviceDisconnected,

    /// Configuration was reloaded from disk.
    ConfigReloaded(Arc<AppConfig>),

    /// Navigate to a named page.
    NavigateTo(String),

    /// Go back one page in the stack.
    NavigateBack,

    /// Go to the home page.
    NavigateHome,

    /// Re-render all buttons on the current page.
    RenderAll,

    /// Re-render a single button by key index.
    RenderButton(u8),

    /// Shutdown the daemon.
    Shutdown,
}
