use crate::error::{DeckError, Result};
use crate::render::canvas::{parse_hex_color, BUTTON_SIZE};
use ab_glyph::{Font, FontRef, PxScale, ScaleFont};
use tiny_skia::Pixmap;

/// Embedded fonts.
const FONT_INTER: &[u8] = include_bytes!("../../assets/fonts/Inter-Regular.ttf");
const FONT_ROBOTO_SLAB: &[u8] = include_bytes!("../../assets/fonts/RobotoSlab-Bold.ttf");
const FONT_JB_THIN: &[u8] = include_bytes!("../../assets/fonts/JetBrainsMonoNerdFont-Thin.ttf");
const FONT_JB_EXTRALIGHT: &[u8] = include_bytes!("../../assets/fonts/JetBrainsMonoNerdFont-ExtraLight.ttf");
const FONT_JB_LIGHT: &[u8] = include_bytes!("../../assets/fonts/JetBrainsMonoNerdFont-Light.ttf");
const FONT_JB_REGULAR: &[u8] = include_bytes!("../../assets/fonts/JetBrainsMonoNerdFont-Regular.ttf");
const FONT_JB_MEDIUM: &[u8] = include_bytes!("../../assets/fonts/JetBrainsMonoNerdFont-Medium.ttf");
const FONT_JB_SEMIBOLD: &[u8] = include_bytes!("../../assets/fonts/JetBrainsMonoNerdFont-SemiBold.ttf");
const FONT_JB_BOLD: &[u8] = include_bytes!("../../assets/fonts/JetBrainsMonoNerdFont-Bold.ttf");
const FONT_JB_EXTRABOLD: &[u8] = include_bytes!("../../assets/fonts/JetBrainsMonoNerdFont-ExtraBold.ttf");

/// Get font bytes by name. Falls back to Inter.
///
/// JetBrains Mono Nerd Font weights:
///   "jb-thin", "jb-extralight", "jb-light", "jb-regular",
///   "jb-medium", "jb-semibold", "jb-bold", "jb-extrabold"
fn font_data(name: &str) -> &'static [u8] {
    match name {
        "roboto-slab" => FONT_ROBOTO_SLAB,
        "jb-thin" => FONT_JB_THIN,
        "jb-extralight" => FONT_JB_EXTRALIGHT,
        "jb-light" => FONT_JB_LIGHT,
        "jb-regular" => FONT_JB_REGULAR,
        "jb-medium" => FONT_JB_MEDIUM,
        "jb-semibold" => FONT_JB_SEMIBOLD,
        "jb-bold" => FONT_JB_BOLD,
        "jb-extrabold" => FONT_JB_EXTRABOLD,
        // Legacy aliases
        "jetbrains-mono" => FONT_JB_EXTRABOLD,
        "jetbrains-bold" => FONT_JB_BOLD,
        _ => FONT_INTER,
    }
}

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
pub fn render_text(pixmap: &mut Pixmap, text: &str, color_hex: &str, font_size: f32, font_name: &str) -> Result<()> {
    let font =
        FontRef::try_from_slice(font_data(font_name)).map_err(|e| DeckError::Font(e.to_string()))?;
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
        let visual_width = measure_line_visual(&scaled_font, scale, line);
        let x_offset = ((BUTTON_SIZE as f32 - visual_width) / 2.0).max(1.0);
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
    font_name: &str,
) -> Result<()> {
    let font =
        FontRef::try_from_slice(font_data(font_name)).map_err(|e| DeckError::Font(e.to_string()))?;
    let color = Rgb::from_hex(color_hex)?;

    let scale = PxScale::from(font_size);
    let scaled_font = font.as_scaled(scale);

    let y_baseline = BUTTON_SIZE as f32 - 4.0;
    let visual_width = measure_line_visual(&scaled_font, scale, text);
    let x_offset = ((BUTTON_SIZE as f32 - visual_width) / 2.0).max(1.0);

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

/// Measure visual width of a line using glyph outline bounds.
/// Falls back to advance-based measurement if outlines aren't available.
/// This produces better centering for icon font glyphs whose advance width
/// is much wider than their visual shape.
fn measure_line_visual(
    font: &ab_glyph::PxScaleFont<&FontRef<'_>>,
    scale: PxScale,
    text: &str,
) -> f32 {
    let mut min_x = f32::MAX;
    let mut max_x = f32::MIN;
    let mut cursor_x = 0.0f32;
    let mut prev = None;
    let mut has_bounds = false;

    for ch in text.chars() {
        let glyph_id = font.glyph_id(ch);
        if let Some(prev_id) = prev {
            cursor_x += font.kern(prev_id, glyph_id);
        }
        if let Some(outlined) = font.outline_glyph(
            glyph_id.with_scale_and_position(scale, ab_glyph::point(cursor_x, 0.0)),
        ) {
            let bounds = outlined.px_bounds();
            min_x = min_x.min(bounds.min.x);
            max_x = max_x.max(bounds.max.x);
            has_bounds = true;
        }
        cursor_x += font.h_advance(glyph_id);
        prev = Some(glyph_id);
    }

    if has_bounds { max_x - min_x } else { cursor_x }
}
