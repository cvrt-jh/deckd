pub mod canvas;
pub mod icon;
pub mod text;

use crate::config::schema::{ButtonConfig, ButtonDefaults};
use crate::error::Result;
use canvas::create_canvas;
use std::path::Path;

/// Render a single button to raw RGBA bytes (72x72).
pub fn render_button(
    button: &ButtonConfig,
    defaults: &ButtonDefaults,
    config_dir: &Path,
) -> Result<Vec<u8>> {
    let bg = button.background.as_deref().unwrap_or(&defaults.background);
    let text_color = button.text_color.as_deref().unwrap_or(&defaults.text_color);
    let font_size = button.font_size.unwrap_or(defaults.font_size);

    let mut pm = create_canvas(bg)?;

    // Render icon if specified.
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
        // If there's an icon, shift text to bottom area.
        if button.icon.is_some() {
            // Render text in the bottom portion.
            let label_font_size = font_size.min(12.0);
            text::render_text_at_bottom(&mut pm, label, text_color, label_font_size)?;
        } else {
            text::render_text(&mut pm, label, text_color, font_size)?;
        }
    }

    Ok(pm.data().to_vec())
}

/// Render a blank (empty/black) button.
pub fn render_blank() -> Result<Vec<u8>> {
    let pm = create_canvas("#000000")?;
    Ok(pm.data().to_vec())
}
