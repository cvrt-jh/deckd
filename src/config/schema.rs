use serde::Deserialize;
use std::collections::HashMap;

/// Root configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct AppConfig {
    pub deckd: DeckdConfig,
    #[serde(default)]
    pub pages: HashMap<String, PageConfig>,
}

/// Global daemon settings.
#[derive(Debug, Clone, Deserialize)]
pub struct DeckdConfig {
    /// Display brightness 0-100.
    #[serde(default = "default_brightness")]
    pub brightness: u8,

    /// Milliseconds between reconnect attempts.
    #[serde(default = "default_reconnect_interval")]
    pub reconnect_interval_ms: u64,

    /// The page to show on startup.
    #[serde(default = "default_home_page")]
    pub home_page: String,

    /// Default style for buttons.
    #[serde(default)]
    pub defaults: ButtonDefaults,
}

/// Default styling applied to all buttons unless overridden.
#[derive(Debug, Clone, Deserialize)]
pub struct ButtonDefaults {
    /// Hex color, e.g. "#1a1a2e".
    #[serde(default = "default_background")]
    pub background: String,

    /// Hex color for text.
    #[serde(default = "default_text_color")]
    pub text_color: String,

    /// Font size in pixels.
    #[serde(default = "default_font_size")]
    pub font_size: f32,
}

impl Default for ButtonDefaults {
    fn default() -> Self {
        Self {
            background: default_background(),
            text_color: default_text_color(),
            font_size: default_font_size(),
        }
    }
}

/// A page of buttons.
#[derive(Debug, Clone, Deserialize)]
pub struct PageConfig {
    /// Display name.
    #[serde(default)]
    pub name: String,

    /// Buttons on this page.
    #[serde(default)]
    pub buttons: Vec<ButtonConfig>,
}

/// A single button definition.
#[derive(Debug, Clone, Deserialize)]
pub struct ButtonConfig {
    /// Key index 0-14.
    pub key: u8,

    /// Text label rendered on the button.
    #[serde(default)]
    pub label: Option<String>,

    /// Path to a PNG icon (relative to config dir or absolute).
    #[serde(default)]
    pub icon: Option<String>,

    /// Background color override (hex).
    #[serde(default)]
    pub background: Option<String>,

    /// Text color override (hex).
    #[serde(default)]
    pub text_color: Option<String>,

    /// Font size override.
    #[serde(default)]
    pub font_size: Option<f32>,

    /// Action to execute on press.
    #[serde(default)]
    pub on_press: Option<ActionConfig>,
}

/// An action to execute.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum ActionConfig {
    Http {
        #[serde(default = "default_http_method")]
        method: String,
        url: String,
        #[serde(default)]
        headers: HashMap<String, String>,
        #[serde(default)]
        body: Option<String>,
    },
    Shell {
        command: String,
    },
    Navigate {
        page: String,
    },
    Back,
    Home,
}

// --- Defaults ---

fn default_brightness() -> u8 {
    80
}

fn default_reconnect_interval() -> u64 {
    2000
}

fn default_home_page() -> String {
    "home".to_string()
}

fn default_background() -> String {
    "#1a1a2e".to_string()
}

fn default_text_color() -> String {
    "#e0e0e0".to_string()
}

fn default_font_size() -> f32 {
    14.0
}

fn default_http_method() -> String {
    "GET".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_minimal_config() {
        let toml_str = r#"
[deckd]
brightness = 90

[pages.home]
name = "Home"
"#;
        let config: AppConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.deckd.brightness, 90);
        assert!(config.pages.contains_key("home"));
    }

    #[test]
    fn parse_full_config() {
        let toml_str = r##"
[deckd]
brightness = 80
reconnect_interval_ms = 2000

[deckd.defaults]
background = "#1a1a2e"
text_color = "#e0e0e0"
font_size = 14

[pages.home]
name = "Home"

[[pages.home.buttons]]
key = 0
label = "Deploy"
icon = "rocket.png"
on_press = { action = "http", method = "POST", url = "https://n8n.local/webhook/deploy" }

[[pages.home.buttons]]
key = 1
label = "Lights"
on_press = { action = "navigate", page = "lights" }

[[pages.home.buttons]]
key = 14
label = "Reboot"
background = "#c0392b"
on_press = { action = "shell", command = "sudo reboot" }
"##;
        let config: AppConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.deckd.brightness, 80);
        let home = &config.pages["home"];
        assert_eq!(home.buttons.len(), 3);
        assert_eq!(home.buttons[0].key, 0);
        assert!(matches!(
            home.buttons[0].on_press,
            Some(ActionConfig::Http { .. })
        ));
        assert!(matches!(
            home.buttons[1].on_press,
            Some(ActionConfig::Navigate { .. })
        ));
        assert!(matches!(
            home.buttons[2].on_press,
            Some(ActionConfig::Shell { .. })
        ));
    }

    #[test]
    fn parse_back_and_home_actions() {
        let toml_str = r#"
[deckd]

[pages.sub]
name = "Sub"

[[pages.sub.buttons]]
key = 0
label = "Back"
on_press = { action = "back" }

[[pages.sub.buttons]]
key = 1
label = "Home"
on_press = { action = "home" }
"#;
        let config: AppConfig = toml::from_str(toml_str).unwrap();
        let sub = &config.pages["sub"];
        assert!(matches!(sub.buttons[0].on_press, Some(ActionConfig::Back)));
        assert!(matches!(sub.buttons[1].on_press, Some(ActionConfig::Home)));
    }
}
