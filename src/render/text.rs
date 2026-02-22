use crate::error::{DeckError, Result};
use crate::render::canvas::{parse_hex_color, BUTTON_SIZE};
use ab_glyph::{Font, FontRef, PxScale, ScaleFont};
use tiny_skia::Pixmap;

/// Embedded fallback font (Inter Regular).
const FALLBACK_FONT: &[u8] = include_bytes!("../../assets/fonts/Inter-Regular.ttf");

/// Rasterize text onto a pixmap.
pub fn render_text(
    canvas: &mut Pixmap,
    text: &str,
    color_hex: &str,
    font_size: f32,
) -> Result<()> {
    let font =
        FontRef::try_from_slice(FALLBACK_FONT).map_err(|e| DeckError::Font(e.to_string()))?;

    let color = parse_hex_color(color_hex)?;
    let r = (color.red() * 255.0) as u8;
    let g = (color.green() * 255.0) as u8;
    let b = (color.blue() * 255.0) as u8;

    let scale = PxScale::from(font_size);
    let scaled_font = font.as_scaled(scale);

    // Calculate text layout — simple single-line center, or multiline if '\n' present.
    let lines: Vec<&str> = text.split('\n').collect();
    let line_height = scaled_font.height();
    let total_height = line_height * lines.len() as f32;
    let start_y = ((BUTTON_SIZE as f32 - total_height) / 2.0).max(2.0);

    let canvas_w = canvas.width() as i32;
    let canvas_h = canvas.height() as i32;
    let data = canvas.data_mut();

    for (line_idx, line) in lines.iter().enumerate() {
        let line_width = measure_line(&scaled_font, line);
        let x_offset = ((BUTTON_SIZE as f32 - line_width) / 2.0).max(1.0);
        let y_baseline = start_y + line_height * (line_idx as f32 + 0.8);

        let mut cursor_x = x_offset;
        let mut prev_glyph_id = None;

        for ch in line.chars() {
            let glyph_id = scaled_font.glyph_id(ch);

            if let Some(prev) = prev_glyph_id {
                cursor_x += scaled_font.kern(prev, glyph_id);
            }

            if let Some(outlined) = scaled_font.outline_glyph(glyph_id.with_scale_and_position(
                scale,
                ab_glyph::point(cursor_x, y_baseline),
            )) {
                let bounds = outlined.px_bounds();
                outlined.draw(|px, py, coverage| {
                    let x = px as i32 + bounds.min.x as i32;
                    let y = py as i32 + bounds.min.y as i32;
                    if x >= 0 && x < canvas_w && y >= 0 && y < canvas_h {
                        let idx = (y * canvas_w + x) as usize * 4;
                        let alpha = (coverage * 255.0) as u8;
                        // Simple alpha blend.
                        let inv = 255 - alpha;
                        data[idx] = ((r as u16 * alpha as u16 + data[idx] as u16 * inv as u16) / 255) as u8;
                        data[idx + 1] = ((g as u16 * alpha as u16 + data[idx + 1] as u16 * inv as u16) / 255) as u8;
                        data[idx + 2] = ((b as u16 * alpha as u16 + data[idx + 2] as u16 * inv as u16) / 255) as u8;
                        data[idx + 3] = 255;
                    }
                });
            }

            cursor_x += scaled_font.h_advance(glyph_id);
            prev_glyph_id = Some(glyph_id);
        }
    }

    Ok(())
}

/// Rasterize text anchored to the bottom of the canvas (for icon+label buttons).
pub fn render_text_at_bottom(
    canvas: &mut Pixmap,
    text: &str,
    color_hex: &str,
    font_size: f32,
) -> Result<()> {
    let font =
        FontRef::try_from_slice(FALLBACK_FONT).map_err(|e| DeckError::Font(e.to_string()))?;

    let color = parse_hex_color(color_hex)?;
    let r = (color.red() * 255.0) as u8;
    let g = (color.green() * 255.0) as u8;
    let b = (color.blue() * 255.0) as u8;

    let scale = PxScale::from(font_size);
    let scaled_font = font.as_scaled(scale);

    // Position text near the bottom.
    let y_baseline = BUTTON_SIZE as f32 - 4.0;
    let line_width = measure_line(&scaled_font, text);
    let x_offset = ((BUTTON_SIZE as f32 - line_width) / 2.0).max(1.0);

    let canvas_w = canvas.width() as i32;
    let canvas_h = canvas.height() as i32;
    let data = canvas.data_mut();

    let mut cursor_x = x_offset;
    let mut prev_glyph_id = None;

    for ch in text.chars() {
        let glyph_id = scaled_font.glyph_id(ch);
        if let Some(prev) = prev_glyph_id {
            cursor_x += scaled_font.kern(prev, glyph_id);
        }

        if let Some(outlined) = scaled_font.outline_glyph(glyph_id.with_scale_and_position(
            scale,
            ab_glyph::point(cursor_x, y_baseline),
        )) {
            let bounds = outlined.px_bounds();
            outlined.draw(|px, py, coverage| {
                let x = px as i32 + bounds.min.x as i32;
                let y = py as i32 + bounds.min.y as i32;
                if x >= 0 && x < canvas_w && y >= 0 && y < canvas_h {
                    let idx = (y * canvas_w + x) as usize * 4;
                    let alpha = (coverage * 255.0) as u8;
                    let inv = 255 - alpha;
                    data[idx] = ((r as u16 * alpha as u16 + data[idx] as u16 * inv as u16) / 255) as u8;
                    data[idx + 1] = ((g as u16 * alpha as u16 + data[idx + 1] as u16 * inv as u16) / 255) as u8;
                    data[idx + 2] = ((b as u16 * alpha as u16 + data[idx + 2] as u16 * inv as u16) / 255) as u8;
                    data[idx + 3] = 255;
                }
            });
        }

        cursor_x += scaled_font.h_advance(glyph_id);
        prev_glyph_id = Some(glyph_id);
    }

    Ok(())
}

fn measure_line(font: &ab_glyph::PxScaleFont<&FontRef>, text: &str) -> f32 {
    let mut width = 0.0f32;
    let mut prev = None;
    for ch in text.chars() {
        let glyph_id = font.glyph_id(ch);
        if let Some(prev_id) = prev {
            width += font.kern(prev_id, glyph_id);
        }
        width += font.h_advance(glyph_id);
        prev = Some(glyph_id);
    }
    width
}
