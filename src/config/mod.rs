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
    Ok(deckd_core::config::parse(&content)?)
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

    #[test]
    fn load_reports_validation_error_text() {
        let dir = std::env::temp_dir().join(format!("deckd-config-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("bad.toml");
        std::fs::write(&path, "[deckd]\n[[pages.home.buttons]]\nkey = 15\n").unwrap();
        let err = load(&path).unwrap_err();
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(
            err.to_string(),
            "config error: page 'home': button key 15 out of range (0-14)"
        );
    }
}
