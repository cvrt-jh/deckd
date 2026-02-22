use std::path::PathBuf;

/// Central error type for deckd.
#[derive(Debug, thiserror::Error)]
pub enum DeckError {
    #[error("config error: {0}")]
    Config(String),

    #[error("config file not found: {0}")]
    ConfigNotFound(PathBuf),

    #[error("TOML parse error: {0}")]
    TomlParse(#[from] toml::de::Error),

    #[error("device error: {0}")]
    Device(String),

    #[error("no Stream Deck found")]
    NoDevice,

    #[error("render error: {0}")]
    Render(String),

    #[error("font error: {0}")]
    Font(String),

    #[error("icon error: {path}: {source}")]
    Icon {
        path: PathBuf,
        source: image::ImageError,
    },

    #[error("action error: {0}")]
    Action(String),

    #[error("HTTP action failed: {0}")]
    Http(#[from] reqwest::Error),

    #[error("shell command failed: {command}: {message}")]
    Shell { command: String, message: String },

    #[error("page not found: {0}")]
    PageNotFound(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("HID error: {0}")]
    Hid(String),

    #[error("watcher error: {0}")]
    Watcher(String),
}

pub type Result<T> = std::result::Result<T, DeckError>;
