pub mod schema;
pub mod watcher;

use crate::error::{DeckError, Result};
use schema::AppConfig;
use std::path::Path;

/// Load and parse configuration from a TOML file.
///
/// Reads the file, expands `${VAR}`/`$VAR` from the process environment, then
/// parses and validates with `deckd_core::config::parse`.
///
/// # Errors
/// Returns `DeckError::ConfigNotFound` if the file doesn't exist,
/// `DeckError::Io` on read errors, and `DeckError::Core` on TOML syntax errors
/// or validation failures.
pub fn load(path: &Path) -> Result<AppConfig> {
    if !path.exists() {
        return Err(DeckError::ConfigNotFound(path.to_path_buf()));
    }

    let content = std::fs::read_to_string(path)?;
    let content = expand_env_vars(&content);
    let config = deckd_core::config::parse(&content)?;
    check_device_layout(&config)?;
    Ok(config)
}

/// deckd drives a 15-key Stream Deck MK.2, so only the 3x5 grid is valid
/// here; other layouts belong to the panel firmware.
fn check_device_layout(config: &AppConfig) -> Result<()> {
    let layout = config.deckd.layout;
    if layout == deckd_core::Layout::Grid3x5 {
        return Ok(());
    }
    Err(DeckError::Config(format!(
        "layout '{}' is not supported by deckd (Stream Deck MK.2 is 3x5)",
        layout.name()
    )))
}

/// Expand `${VAR}` and `$VAR` patterns from the process environment.
fn expand_env_vars(input: &str) -> String {
    deckd_core::expand_vars(input, |name| std::env::var(name).ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn env_var_expansion() {
        std::env::set_var("DECKD_TEST_VAR", "hello");
        let result = expand_env_vars("url = \"${DECKD_TEST_VAR}/path\"");
        assert_eq!(result, "url = \"hello/path\"");
        std::env::remove_var("DECKD_TEST_VAR");
    }

    #[test]
    fn load_example_config() {
        let dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
        let path = std::path::PathBuf::from(dir).join("config.example.toml");
        let config = load(&path).unwrap();
        assert!(config.pages.contains_key("home"));
    }

    #[test]
    fn load_missing_file_is_not_found() {
        let err = load(Path::new("/nonexistent/deckd.toml")).unwrap_err();
        assert!(matches!(err, DeckError::ConfigNotFound(_)), "got {err:?}");
    }

    /// Write `text` to a temp file unique to `name` and this process, load
    /// it, and remove the file again.
    fn load_text(name: &str, text: &str) -> Result<AppConfig> {
        let path =
            std::env::temp_dir().join(format!("deckd-config-{name}-{}.toml", std::process::id()));
        std::fs::write(&path, text).unwrap();
        let result = load(&path);
        std::fs::remove_file(&path).unwrap();
        result
    }

    #[test]
    fn load_reports_validation_error_text() {
        let err = load_text("key15", "[deckd]\n[[pages.home.buttons]]\nkey = 15\n").unwrap_err();
        assert_eq!(
            err.to_string(),
            "config error: page 'home': button key 15 out of range (0-14)"
        );
    }

    #[test]
    fn load_rejects_4x4_layout() {
        let err = load_text("layout4x4", "[deckd]\nlayout = \"4x4\"\n").unwrap_err();
        assert!(
            matches!(
                &err,
                DeckError::Config(msg)
                    if msg == "layout '4x4' is not supported by deckd (Stream Deck MK.2 is 3x5)"
            ),
            "got {err:?}"
        );
    }

    #[test]
    fn load_accepts_explicit_and_default_3x5_layout() {
        let cfg = load_text("nolayout", "[deckd]\n").unwrap();
        assert_eq!(cfg.deckd.layout, deckd_core::Layout::Grid3x5);
        let cfg = load_text("layout3x5", "[deckd]\nlayout = \"3x5\"\n").unwrap();
        assert_eq!(cfg.deckd.layout, deckd_core::Layout::Grid3x5);
    }
}
