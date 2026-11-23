use crate::error::{DeckError, Result};
use image::imageops::FilterType;
use image::GenericImageView;
use num_traits::ToPrimitive;
use std::path::Path;
use tiny_skia::Pixmap;

/// Edge length `edge` scaled by `scale`, truncated to whole pixels.
fn scaled_edge(edge: u32, scale: f32) -> Result<u32> {
    edge.to_f32()
        .map(|e| e * scale)
        .and_then(|e| e.to_u32())
        .ok_or_else(|| DeckError::Render(format!("icon edge {edge} cannot be scaled")))
}

/// Premultiply one 8-bit color channel by an 8-bit alpha.
///
/// `channel * alpha / 255` is at most 255, so the saturation never triggers.
fn premultiply(channel: u8, alpha: u8) -> u8 {
    u8::try_from(u16::from(channel) * u16::from(alpha) / 255).unwrap_or(u8::MAX)
}

/// Load a PNG icon, scale it to fit within `max_px`, and return as a Pixmap.
///
/// Icons smaller than `max_px` are kept at their size.
///
/// # Errors
/// Returns `DeckError::Icon` if the image cannot be opened or decoded,
/// or `DeckError::Render` if the pixmap cannot be created.
pub fn load_icon(path: &Path, max_px: u32) -> Result<Pixmap> {
    let img = image::open(path).map_err(|e| DeckError::Icon {
        path: path.to_path_buf(),
        source: e,
    })?;

    let (width, height) = img.dimensions();
    let largest = width.max(height);
    let scale = max_px
        .to_f32()
        .zip(largest.to_f32())
        .map(|(max, largest)| (max / largest).min(1.0))
        .ok_or_else(|| DeckError::Render(format!("icon size {largest} cannot be scaled")))?;
    let new_w = scaled_edge(width, scale)?;
    let new_h = scaled_edge(height, scale)?;

    let resized = img.resize(new_w, new_h, FilterType::Lanczos3);
    let rgba = resized.to_rgba8();

    let mut pixmap = Pixmap::new(new_w, new_h)
        .ok_or_else(|| DeckError::Render("failed to create icon pixmap".into()))?;

    // tiny-skia uses premultiplied alpha, so we need to premultiply.
    let (src, _) = rgba.as_raw().as_chunks::<4>();
    let (dst, _) = pixmap.data_mut().as_chunks_mut::<4>();
    if src.len() != dst.len() {
        return Err(DeckError::Render(format!(
            "icon resize gave {} pixels, expected {}",
            src.len(),
            dst.len()
        )));
    }
    for (out, &[r, g, b, a]) in dst.iter_mut().zip(src) {
        *out = [premultiply(r, a), premultiply(g, a), premultiply(b, a), a];
    }

    Ok(pixmap)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn premultiply_scales_by_alpha() {
        assert_eq!(premultiply(255, 255), 255);
        assert_eq!(premultiply(255, 0), 0);
        assert_eq!(premultiply(200, 128), 100);
    }

    #[test]
    fn scaled_edge_truncates() {
        assert_eq!(scaled_edge(96, 0.5).unwrap(), 48);
        assert_eq!(scaled_edge(31, 1.0).unwrap(), 31);
        assert_eq!(scaled_edge(75, 0.25).unwrap(), 18);
    }
}
