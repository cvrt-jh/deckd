pub mod schema;
pub mod watcher;

use crate::error::{DeckError, Result};
use schema::AppConfig;
use std::path::Path;

/// Load and parse configuration from a TOML file.
///
/// # Errors
/// Returns `DeckError::ConfigNotFound` if the file doesn't exist,
/// `DeckError::Io` on read errors, `DeckError::TomlParse` on syntax errors,
/// or `DeckError::Config` on validation failures.
pub fn load(path: &Path) -> Result<AppConfig> {
    if !path.exists() {
        return Err(DeckError::ConfigNotFound(path.to_path_buf()));
    }

    let content = std::fs::read_to_string(path)?;
    let content = expand_env_vars(&content);
    let config: AppConfig = toml::from_str(&content)?;

    validate(&config)?;
    Ok(config)
}

/// Expand `${VAR}` and `$VAR` patterns in the config string.
fn expand_env_vars(input: &str) -> String {
    let mut result = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '$' {
            if chars.peek() == Some(&'{') {
                chars.next(); // consume '{'
                let var_name: String = chars.by_ref().take_while(|&c| c != '}').collect();
                if let Ok(val) = std::env::var(&var_name) {
                    result.push_str(&val);
                } else {
                    // Keep original if env var not found
                    use std::fmt::Write;
                    let _ = write!(result, "${{{var_name}}}");
                }
            } else {
                let var_name: String = chars
                    .by_ref()
                    .take_while(|c| c.is_alphanumeric() || *c == '_')
                    .collect();
                if var_name.is_empty() {
                    result.push('$');
                } else if let Ok(val) = std::env::var(&var_name) {
                    result.push_str(&val);
                } else {
                    result.push('$');
                    result.push_str(&var_name);
                }
            }
        } else {
            result.push(ch);
        }
    }

    result
}

/// Validate config constraints.
fn validate(config: &AppConfig) -> Result<()> {
    if config.deckd.brightness > 100 {
        return Err(DeckError::Config("brightness must be 0-100".to_string()));
    }

    for (page_id, page) in &config.pages {
        for button in &page.buttons {
            if button.key > 14 {
                return Err(DeckError::Config(format!(
                    "page '{page_id}': button key {} out of range (0-14)",
                    button.key
                )));
            }
        }
    }

    Ok(())
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
    fn env_var_missing_kept() {
        let result = expand_env_vars("url = \"${DECKD_NONEXISTENT}/path\"");
        assert_eq!(result, "url = \"${DECKD_NONEXISTENT}/path\"");
    }

    #[test]
    fn load_example_config() {
        let dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
        let path = std::path::PathBuf::from(dir).join("config.example.toml");
        if path.exists() {
            let config = load(&path).unwrap();
            assert!(config.pages.contains_key("home"));
        }
    }
}
