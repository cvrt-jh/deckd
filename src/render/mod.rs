pub mod canvas;
pub mod icon;
pub mod text;

use crate::config::schema::{ButtonConfig, ButtonDefaults};
use crate::error::Result;
use canvas::create_canvas;
use std::collections::HashMap;
use std::path::Path;

/// Render a single button to raw RGBA bytes (72x72).
///
/// `entity_states` maps HA entity IDs to their current state string.
/// When a button has `state_entity` and the state is "on", the `on_background`
/// and `on_text_color` overrides are used.
///
/// # Errors
/// Returns `DeckError::Render` if canvas creation, icon loading, or text rendering fails.
pub fn render_button(
    button: &ButtonConfig,
    defaults: &ButtonDefaults,
    config_dir: &Path,
    entity_states: &HashMap<String, String>,
) -> Result<Vec<u8>> {
    // Check if entity is "on" for stateful color swapping.
    let entity_on = button
        .state_entity
        .as_ref()
        .and_then(|eid| entity_states.get(eid))
        .is_some_and(|s| s == "on");

    let bg = if entity_on {
        button.on_background.as_deref()
            .or(button.background.as_deref())
            .unwrap_or(&defaults.background)
    } else {
        button.background.as_deref().unwrap_or(&defaults.background)
    };

    let text_color = if entity_on {
        button.on_text_color.as_deref()
            .or(button.text_color.as_deref())
            .unwrap_or(&defaults.text_color)
    } else {
        button.text_color.as_deref().unwrap_or(&defaults.text_color)
    };

    let font_size = button.font_size.unwrap_or(defaults.font_size);
    let font_name = button.font.as_deref().unwrap_or(&defaults.font);

    let mut pm = create_canvas(bg)?;

    // Render icon if specified. Track whether it actually loaded.
    let mut icon_rendered = false;
    if let Some(ref icon_path) = button.icon {
        let full_path = if Path::new(icon_path).is_absolute() {
            std::path::PathBuf::from(icon_path)
        } else {
            config_dir.join(icon_path)
        };

        if full_path.exists() {
            match icon::load_icon(&full_path) {
                Ok(icon_pm) => {
                    let x = icon::center_x(icon_pm.width());
                    let y = icon::icon_y(button.label.is_some());
                    canvas::composite(&mut pm, &icon_pm, x, y);
                    icon_rendered = true;
                }
                Err(e) => {
                    tracing::warn!("failed to load icon {}: {e}", full_path.display());
                }
            }
        } else {
            tracing::warn!("icon not found: {}", full_path.display());
        }
    }

    // Render text label.
    if let Some(ref label) = button.label {
        if icon_rendered {
            // Icon present: render text in the bottom portion.
            let label_font_size = font_size.min(12.0);
            text::render_text_at_bottom(&mut pm, label, text_color, label_font_size, font_name)?;
        } else {
            // No icon: center text.
            text::render_text(&mut pm, label, text_color, font_size, font_name)?;
        }
    }

    Ok(pm.data().to_vec())
}

/// Render a blank (empty/black) button.
///
/// # Errors
/// Returns `DeckError::Render` if canvas creation fails.
pub fn render_blank() -> Result<Vec<u8>> {
    let pm = create_canvas("#000000")?;
    Ok(pm.data().to_vec())
}
