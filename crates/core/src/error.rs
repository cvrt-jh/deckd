/// Errors produced by config parsing, validation and rendering.
///
/// The display texts match deckd's own error variants, so moving a code path
/// into this crate does not change any message a user sees.
#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    #[error("TOML parse error: {0}")]
    Toml(#[from] toml::de::Error),

    #[error("config error: {0}")]
    Config(String),

    #[error("render error: {0}")]
    Render(String),

    #[error("font error: {0}")]
    Font(String),
}
