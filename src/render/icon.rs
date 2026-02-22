use crate::error::{DeckError, Result};
use crate::render::canvas::BUTTON_SIZE;
use image::imageops::FilterType;
use image::GenericImageView;
use std::path::Path;
use tiny_skia::Pixmap;

/// Maximum icon size — leave room for a text label below.
const ICON_MAX: u32 = 48;

/// Top padding for icon placement.
const ICON_TOP_PAD: u32 = 4;

/// Load a PNG icon, scale it to fit within the button, and return as a Pixmap.
pub fn load_icon(path: &Path) -> Result<Pixmap> {
    let img = image::open(path).map_err(|e| DeckError::Icon {
        path: path.to_path_buf(),
        source: e,
    })?;

    // Scale to fit within ICON_MAX while preserving aspect ratio.
    let (w, h) = img.dimensions();
    let scale = (ICON_MAX as f32 / w.max(h) as f32).min(1.0);
    let new_w = (w as f32 * scale) as u32;
    let new_h = (h as f32 * scale) as u32;

    let resized = img.resize(new_w, new_h, FilterType::Lanczos3);
    let rgba = resized.to_rgba8();

    let mut pixmap = Pixmap::new(new_w, new_h)
        .ok_or_else(|| DeckError::Render("failed to create icon pixmap".into()))?;

    // Copy pixel data.
    let src = rgba.as_raw();
    let dst = pixmap.data_mut();
    // tiny-skia uses premultiplied alpha, so we need to premultiply.
    for i in 0..(new_w * new_h) as usize {
        let r = src[i * 4] as u16;
        let g = src[i * 4 + 1] as u16;
        let b = src[i * 4 + 2] as u16;
        let a = src[i * 4 + 3] as u16;
        dst[i * 4] = (r * a / 255) as u8;
        dst[i * 4 + 1] = (g * a / 255) as u8;
        dst[i * 4 + 2] = (b * a / 255) as u8;
        dst[i * 4 + 3] = a as u8;
    }

    Ok(pixmap)
}

/// Calculate centered x position for an icon of given width.
pub fn center_x(icon_width: u32) -> i32 {
    ((BUTTON_SIZE - icon_width) / 2) as i32
}

/// Calculate y position for icon (top area, leaving room for label).
pub fn icon_y(has_label: bool) -> i32 {
    if has_label {
        ICON_TOP_PAD as i32
    } else {
        ((BUTTON_SIZE - ICON_MAX) / 2) as i32
    }
}
