use crate::error::{DeckError, Result};
use crate::render::canvas::BUTTON_SIZE;
use image::imageops::FilterType;
use image::GenericImageView;
use std::path::Path;
use tiny_skia::Pixmap;

/// Maximum icon size — leave room for a text label below.
const ICON_MAX: u32 = 48;

/// Top padding for icon placement.
const ICON_TOP_PAD: i32 = 4;

/// Load a PNG icon, scale it to fit within the button, and return as a Pixmap.
///
/// # Errors
/// Returns `DeckError::Icon` if the image cannot be opened or decoded,
/// or `DeckError::Render` if the pixmap cannot be created.
pub fn load_icon(path: &Path) -> Result<Pixmap> {
    let img = image::open(path).map_err(|e| DeckError::Icon {
        path: path.to_path_buf(),
        source: e,
    })?;

    let (width, height) = img.dimensions();
    let scale = (ICON_MAX as f32 / width.max(height) as f32).min(1.0);
    let new_w = (width as f32 * scale) as u32;
    let new_h = (height as f32 * scale) as u32;

    let resized = img.resize(new_w, new_h, FilterType::Lanczos3);
    let rgba = resized.to_rgba8();

    let mut pixmap = Pixmap::new(new_w, new_h)
        .ok_or_else(|| DeckError::Render("failed to create icon pixmap".into()))?;

    // tiny-skia uses premultiplied alpha, so we need to premultiply.
    let src = rgba.as_raw();
    let dst = pixmap.data_mut();
    for i in 0..(new_w * new_h) as usize {
        let sr = u16::from(src[i * 4]);
        let sg = u16::from(src[i * 4 + 1]);
        let sb = u16::from(src[i * 4 + 2]);
        let sa = u16::from(src[i * 4 + 3]);
        dst[i * 4] = (sr * sa / 255) as u8;
        dst[i * 4 + 1] = (sg * sa / 255) as u8;
        dst[i * 4 + 2] = (sb * sa / 255) as u8;
        dst[i * 4 + 3] = sa as u8;
    }

    Ok(pixmap)
}

/// Calculate centered x position for an icon of given width.
#[must_use]
pub const fn center_x(icon_width: u32) -> i32 {
    ((BUTTON_SIZE - icon_width) / 2) as i32
}

/// Calculate y position for icon (top area, leaving room for label).
#[must_use]
pub const fn icon_y(has_label: bool) -> i32 {
    if has_label {
        ICON_TOP_PAD
    } else {
        ((BUTTON_SIZE - ICON_MAX) / 2) as i32
    }
}
