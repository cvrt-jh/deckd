use crate::error::{DeckError, Result};
use crate::render::canvas::{parse_hex_color, BUTTON_SIZE};
use ab_glyph::{Font, FontRef, PxScale, ScaleFont};
use tiny_skia::Pixmap;

/// Embedded fallback font (Inter Regular).
const FALLBACK_FONT: &[u8] = include_bytes!("../../assets/fonts/Inter-Regular.ttf");

/// RGB color components for blending.
struct Rgb {
    red: u8,
    green: u8,
    blue: u8,
}

impl Rgb {
    fn from_hex(hex: &str) -> Result<Self> {
        let color = parse_hex_color(hex)?;
        Ok(Self {
            red: (color.red() * 255.0) as u8,
            green: (color.green() * 255.0) as u8,
            blue: (color.blue() * 255.0) as u8,
        })
    }
}

/// Canvas write target for glyph rasterization.
struct Canvas<'a> {
    data: &'a mut [u8],
    width: i32,
    height: i32,
}

/// Alpha-blend a glyph pixel onto the canvas data buffer.
fn blend_pixel(data: &mut [u8], idx: usize, color: &Rgb, alpha: u8) {
    let inv = 255 - alpha;
    let blend = |fg: u8, bg: u8| -> u8 {
        ((u16::from(fg) * u16::from(alpha) + u16::from(bg) * u16::from(inv)) / 255) as u8
    };
    data[idx] = blend(color.red, data[idx]);
    data[idx + 1] = blend(color.green, data[idx + 1]);
    data[idx + 2] = blend(color.blue, data[idx + 2]);
    data[idx + 3] = 255;
}

/// Rasterize a line of glyphs onto the canvas at a given baseline.
fn rasterize_glyphs(
    canvas: &mut Canvas<'_>,
    text: &str,
    font: &ab_glyph::PxScaleFont<&FontRef<'_>>,
    scale: PxScale,
    x_start: f32,
    y_baseline: f32,
    color: &Rgb,
) {
    let mut cursor_x = x_start;
    let mut prev_glyph_id = None;

    for ch in text.chars() {
        let glyph_id = font.glyph_id(ch);

        if let Some(prev) = prev_glyph_id {
            cursor_x += font.kern(prev, glyph_id);
        }

        if let Some(outlined) = font.outline_glyph(
            glyph_id.with_scale_and_position(scale, ab_glyph::point(cursor_x, y_baseline)),
        ) {
            let bounds = outlined.px_bounds();
            let cw = canvas.width;
            let ch = canvas.height;
            outlined.draw(|px, py, coverage| {
                let x = px as i32 + bounds.min.x as i32;
                let y = py as i32 + bounds.min.y as i32;
                if x >= 0 && x < cw && y >= 0 && y < ch {
                    let idx = (y * cw + x) as usize * 4;
                    blend_pixel(canvas.data, idx, color, (coverage * 255.0) as u8);
                }
            });
        }

        cursor_x += font.h_advance(glyph_id);
        prev_glyph_id = Some(glyph_id);
    }
}

/// Rasterize text centered on the pixmap.
///
/// # Errors
/// Returns `DeckError::Font` if the embedded font fails to load,
/// or `DeckError::Render` if the color is invalid.
pub fn render_text(pixmap: &mut Pixmap, text: &str, color_hex: &str, font_size: f32) -> Result<()> {
    let font =
        FontRef::try_from_slice(FALLBACK_FONT).map_err(|e| DeckError::Font(e.to_string()))?;
    let color = Rgb::from_hex(color_hex)?;

    let scale = PxScale::from(font_size);
    let scaled_font = font.as_scaled(scale);

    let lines: Vec<&str> = text.split('\n').collect();
    let line_height = scaled_font.height();
    let total_height = line_height * lines.len() as f32;
    let start_y = ((BUTTON_SIZE as f32 - total_height) / 2.0).max(2.0);

    let width = pixmap.width() as i32;
    let height = pixmap.height() as i32;
    let mut canvas = Canvas {
        data: pixmap.data_mut(),
        width,
        height,
    };

    for (line_idx, line) in lines.iter().enumerate() {
        let line_width = measure_line(&scaled_font, line);
        let x_offset = ((BUTTON_SIZE as f32 - line_width) / 2.0).max(1.0);
        let y_baseline = line_height.mul_add(line_idx as f32 + 0.8, start_y);

        rasterize_glyphs(
            &mut canvas,
            line,
            &scaled_font,
            scale,
            x_offset,
            y_baseline,
            &color,
        );
    }

    Ok(())
}

/// Rasterize text anchored to the bottom of the canvas (for icon+label buttons).
///
/// # Errors
/// Returns `DeckError::Font` if the embedded font fails to load,
/// or `DeckError::Render` if the color is invalid.
pub fn render_text_at_bottom(
    pixmap: &mut Pixmap,
    text: &str,
    color_hex: &str,
    font_size: f32,
) -> Result<()> {
    let font =
        FontRef::try_from_slice(FALLBACK_FONT).map_err(|e| DeckError::Font(e.to_string()))?;
    let color = Rgb::from_hex(color_hex)?;

    let scale = PxScale::from(font_size);
    let scaled_font = font.as_scaled(scale);

    let y_baseline = BUTTON_SIZE as f32 - 4.0;
    let line_width = measure_line(&scaled_font, text);
    let x_offset = ((BUTTON_SIZE as f32 - line_width) / 2.0).max(1.0);

    let width = pixmap.width() as i32;
    let height = pixmap.height() as i32;
    let mut canvas = Canvas {
        data: pixmap.data_mut(),
        width,
        height,
    };

    rasterize_glyphs(
        &mut canvas,
        text,
        &scaled_font,
        scale,
        x_offset,
        y_baseline,
        &color,
    );

    Ok(())
}

fn measure_line(font: &ab_glyph::PxScaleFont<&FontRef<'_>>, text: &str) -> f32 {
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
