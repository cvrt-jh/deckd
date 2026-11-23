//! Stream Deck key rendering: loads icons from disk and hands the drawing to
//! `deckd_core::render` at the Stream Deck's key size.

pub mod icon;

use crate::config::schema::{ButtonConfig, ButtonDefaults};
use crate::error::Result;
use std::collections::HashMap;
use std::hash::BuildHasher;
use std::path::{Path, PathBuf};
use tiny_skia::Pixmap;

/// Stream Deck MK.2 key size in pixels.
pub const KEY_PX: u32 = 72;

/// Load the button's icon, if it has one, scaled to fit the key.
///
/// A missing or undecodable icon is logged and the key renders without it,
/// so one bad file never blanks a whole page.
fn load_button_icon(button: &ButtonConfig, config_dir: &Path) -> Option<Pixmap> {
    let icon_path = button.icon.as_deref()?;
    let full_path = if Path::new(icon_path).is_absolute() {
        PathBuf::from(icon_path)
    } else {
        config_dir.join(icon_path)
    };

    if !full_path.exists() {
        tracing::warn!("icon not found: {}", full_path.display());
        return None;
    }

    let max_px = deckd_core::render::icon_max_px(KEY_PX);
    match icon::load_icon(&full_path, max_px) {
        Ok(pm) => Some(pm),
        Err(e) => {
            tracing::warn!("failed to load icon {}: {e}", full_path.display());
            None
        }
    }
}

/// Render a single button to raw RGBA bytes (72x72).
///
/// `entity_states` maps HA entity IDs to their current state string.
/// When a button has `state_entity` and the state is "on", the `on_background`
/// and `on_text_color` overrides are used.
///
/// # Errors
/// Returns `DeckError::Core` if canvas creation or text rendering fails.
pub fn render_button<S: BuildHasher>(
    button: &ButtonConfig,
    defaults: &ButtonDefaults,
    config_dir: &Path,
    entity_states: &HashMap<String, String, S>,
) -> Result<Vec<u8>> {
    let icon = load_button_icon(button, config_dir);
    let pm =
        deckd_core::render::render_button(button, defaults, entity_states, KEY_PX, icon.as_ref())?;
    Ok(pm.take())
}

/// Render a blank (empty/black) button.
///
/// # Errors
/// Returns `DeckError::Core` if canvas creation fails.
pub fn render_blank() -> Result<Vec<u8>> {
    Ok(deckd_core::render::render_blank(KEY_PX)?.take())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bare_button() -> ButtonConfig {
        ButtonConfig {
            key: 0,
            label: Some("A".into()),
            icon: None,
            background: Some("#000000".into()),
            text_color: None,
            font_size: None,
            font: None,
            on_press: None,
            state_entity: None,
            on_background: None,
            on_text_color: None,
            label_entity: None,
            precision: None,
            unknown_text: None,
            thresholds: Vec::new(),
            count_entities: Vec::new(),
            active_text_color: None,
            active_background: None,
        }
    }

    #[test]
    fn render_button_returns_72px_rgba() {
        let bytes = render_button(
            &bare_button(),
            &ButtonDefaults::default(),
            Path::new("."),
            &HashMap::new(),
        )
        .unwrap();
        assert_eq!(bytes.len(), 72 * 72 * 4);
    }

    #[test]
    fn missing_icon_renders_the_key_without_it() {
        let mut with_missing_icon = bare_button();
        with_missing_icon.icon = Some("does/not/exist.png".into());
        let states: HashMap<String, String> = HashMap::new();
        let defaults = ButtonDefaults::default();
        let dir = Path::new(".");

        let missing = render_button(&with_missing_icon, &defaults, dir, &states).unwrap();
        let plain = render_button(&bare_button(), &defaults, dir, &states).unwrap();
        assert_eq!(missing, plain);
    }

    #[test]
    fn render_blank_is_black_72px() {
        let bytes = render_blank().unwrap();
        assert_eq!(bytes.len(), 72 * 72 * 4);
        let (pixels, rest) = bytes.as_chunks::<4>();
        assert!(rest.is_empty());
        assert!(pixels.iter().all(|px| *px == [0, 0, 0, 255]));
    }
}
