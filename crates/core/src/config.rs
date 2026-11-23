//! Config text to validated [`AppConfig`], without any I/O.

use crate::error::CoreError;
use crate::schema::AppConfig;

/// Expand `${VAR}` and `$VAR` references using `lookup`.
///
/// Unknown variables are kept literally, and a `$` not followed by a name is
/// kept as is. The caller decides where values come from (process
/// environment, NVS, a test map), so this function does no I/O.
pub fn expand_vars(input: &str, lookup: impl Fn(&str) -> Option<String>) -> String {
    let mut result = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch != '$' {
            result.push(ch);
            continue;
        }
        if chars.peek() == Some(&'{') {
            chars.next();
            let var_name: String = chars.by_ref().take_while(|&c| c != '}').collect();
            if let Some(val) = lookup(&var_name) {
                result.push_str(&val);
            } else {
                result.push_str("${");
                result.push_str(&var_name);
                result.push('}');
            }
        } else {
            // Peek so the character that ends the name stays in the input.
            let mut var_name = String::new();
            while let Some(c) = chars.next_if(|c| c.is_alphanumeric() || *c == '_') {
                var_name.push(c);
            }
            if var_name.is_empty() {
                result.push('$');
            } else if let Some(val) = lookup(&var_name) {
                result.push_str(&val);
            } else {
                result.push('$');
                result.push_str(&var_name);
            }
        }
    }

    result
}

/// Parse and validate config text. Variables are not expanded; run
/// [`expand_vars`] first when the text may contain them.
///
/// # Errors
/// Returns `CoreError::Toml` on syntax or schema errors and
/// `CoreError::Config` when [`validate`] rejects the config.
pub fn parse(toml_text: &str) -> Result<AppConfig, CoreError> {
    let config: AppConfig = toml::from_str(toml_text)?;
    validate(&config)?;
    Ok(config)
}

/// Check constraints the schema cannot express: brightness at most 100 and
/// every button key inside the configured layout.
///
/// # Errors
/// Returns `CoreError::Config` naming the first violation found.
pub fn validate(config: &AppConfig) -> Result<(), CoreError> {
    if config.deckd.brightness > 100 {
        return Err(CoreError::Config("brightness must be 0-100".to_string()));
    }

    let key_count = config.deckd.layout.key_count();
    for (page_id, page) in &config.pages {
        for button in &page.buttons {
            if button.key >= key_count {
                return Err(CoreError::Config(format!(
                    "page '{page_id}': button key {} out of range (0-{})",
                    button.key,
                    key_count - 1
                )));
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::Layout;

    fn page_with_key(layout: Option<&str>, key: u8) -> String {
        let layout_line = layout.map_or_else(String::new, |l| format!("layout = \"{l}\"\n"));
        format!("[deckd]\n{layout_line}\n[pages.home]\n\n[[pages.home.buttons]]\nkey = {key}\n")
    }

    #[test]
    fn default_layout_is_3x5() {
        let cfg = parse("[deckd]\n").expect("parse");
        assert_eq!(cfg.deckd.layout, Layout::Grid3x5);
    }

    #[test]
    fn grid_4x4_accepts_key_15_rejects_16() {
        let cfg = parse(&page_with_key(Some("4x4"), 15)).expect("key 15 fits a 4x4 grid");
        assert_eq!(cfg.deckd.layout, Layout::Grid4x4);
        let err = parse(&page_with_key(Some("4x4"), 16)).expect_err("key 16 is out of range");
        assert!(
            err.to_string().contains("out of range (0-15)"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn grid_3x5_rejects_key_15() {
        assert!(parse(&page_with_key(None, 14)).is_ok());
        let err = parse(&page_with_key(None, 15)).expect_err("key 15 is out of range");
        assert!(
            err.to_string()
                .contains("page 'home': button key 15 out of range (0-14)"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn expand_vars_uses_lookup() {
        let out = expand_vars("a=${A}", |k| (k == "A").then(|| "x".to_string()));
        assert_eq!(out, "a=x");
    }

    #[test]
    fn expand_vars_unknown_kept() {
        let out = expand_vars("url = \"${NOPE}/path\"", |_| None);
        assert_eq!(out, "url = \"${NOPE}/path\"");
    }

    #[test]
    fn expand_vars_bare_dollar() {
        assert_eq!(expand_vars("$", |_| Some("x".into())), "$");
        assert_eq!(expand_vars("cost $", |_| Some("x".into())), "cost $");
    }

    fn lookup_a(name: &str) -> Option<String> {
        (name == "A").then(|| "x".to_string())
    }

    #[test]
    fn expand_vars_bare_name_keeps_terminator() {
        assert_eq!(expand_vars("$A/b", lookup_a), "x/b");
        assert_eq!(expand_vars("$ 5", lookup_a), "$ 5");
        assert_eq!(expand_vars("$NOPE/b", lookup_a), "$NOPE/b");
        assert_eq!(expand_vars("${A}$A", lookup_a), "xx");
        assert_eq!(expand_vars("a$", lookup_a), "a$");
        assert_eq!(expand_vars("$A$A", lookup_a), "xx");
    }

    #[test]
    fn expand_vars_bare_name() {
        assert_eq!(expand_vars("$A", |k| (k == "A").then(|| "x".into())), "x");
        assert_eq!(expand_vars("$NOPE", |_| None), "$NOPE");
    }

    #[test]
    fn brightness_over_100_rejected() {
        let err = parse("[deckd]\nbrightness = 101\n").expect_err("101 is too bright");
        assert_eq!(err.to_string(), "config error: brightness must be 0-100");
        assert!(parse("[deckd]\nbrightness = 100\n").is_ok());
    }

    #[test]
    fn toml_syntax_error_is_toml_variant() {
        let err = parse("[deckd\n").expect_err("broken table header");
        assert!(matches!(err, CoreError::Toml(_)), "got {err:?}");
    }

    #[test]
    fn unknown_layout_rejected() {
        let err = parse("[deckd]\nlayout = \"5x5\"\n").expect_err("no such layout");
        assert!(matches!(err, CoreError::Toml(_)), "got {err:?}");
    }
}
