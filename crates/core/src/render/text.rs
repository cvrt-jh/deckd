use crate::error::CoreError;
use crate::render::canvas::parse_hex_color;
use ab_glyph::{Font, FontRef, PxScale, ScaleFont};
use num_traits::ToPrimitive;
use tiny_skia::Pixmap;

#[cfg(not(feature = "font-inter"))]
compile_error!("deckd-core needs feature `font-inter`: Inter is the fallback for every font name");

// Embedded fonts. Each one is compiled in only with its cargo feature
// (`all-fonts`, the default, enables every one), so a small target such as
// the panel firmware links just the fonts it uses.
static FONT_INTER: &[u8] = include_bytes!("../../assets/fonts/Inter-Regular.ttf");
#[cfg(feature = "font-roboto-slab")]
static FONT_ROBOTO_SLAB: &[u8] = include_bytes!("../../assets/fonts/RobotoSlab-Bold.ttf");
#[cfg(feature = "font-jb-thin")]
static FONT_JB_THIN: &[u8] = include_bytes!("../../assets/fonts/JetBrainsMonoNerdFont-Thin.ttf");
#[cfg(feature = "font-jb-extralight")]
static FONT_JB_EXTRALIGHT: &[u8] =
    include_bytes!("../../assets/fonts/JetBrainsMonoNerdFont-ExtraLight.ttf");
#[cfg(feature = "font-jb-light")]
static FONT_JB_LIGHT: &[u8] = include_bytes!("../../assets/fonts/JetBrainsMonoNerdFont-Light.ttf");
#[cfg(feature = "font-jb-regular")]
static FONT_JB_REGULAR: &[u8] =
    include_bytes!("../../assets/fonts/JetBrainsMonoNerdFont-Regular.ttf");
#[cfg(feature = "font-jb-medium")]
static FONT_JB_MEDIUM: &[u8] =
    include_bytes!("../../assets/fonts/JetBrainsMonoNerdFont-Medium.ttf");
#[cfg(feature = "font-jb-semibold")]
static FONT_JB_SEMIBOLD: &[u8] =
    include_bytes!("../../assets/fonts/JetBrainsMonoNerdFont-SemiBold.ttf");
#[cfg(feature = "font-jb-bold")]
static FONT_JB_BOLD: &[u8] = include_bytes!("../../assets/fonts/JetBrainsMonoNerdFont-Bold.ttf");
#[cfg(feature = "font-jb-extrabold")]
static FONT_JB_EXTRABOLD: &[u8] =
    include_bytes!("../../assets/fonts/JetBrainsMonoNerdFont-ExtraBold.ttf");

/// Font names embedded in this build, aliases included.
///
/// Weights of the `JetBrains` Mono Nerd Font:
///   "jb-thin", "jb-extralight", "jb-light", "jb-regular",
///   "jb-medium", "jb-semibold", "jb-bold", "jb-extrabold"
///
/// Legacy aliases: "jetbrains-mono" (extra bold), "jetbrains-bold" (bold).
static FONTS: &[(&str, &[u8])] = &[
    ("inter", FONT_INTER),
    #[cfg(feature = "font-roboto-slab")]
    ("roboto-slab", FONT_ROBOTO_SLAB),
    #[cfg(feature = "font-jb-thin")]
    ("jb-thin", FONT_JB_THIN),
    #[cfg(feature = "font-jb-extralight")]
    ("jb-extralight", FONT_JB_EXTRALIGHT),
    #[cfg(feature = "font-jb-light")]
    ("jb-light", FONT_JB_LIGHT),
    #[cfg(feature = "font-jb-regular")]
    ("jb-regular", FONT_JB_REGULAR),
    #[cfg(feature = "font-jb-medium")]
    ("jb-medium", FONT_JB_MEDIUM),
    #[cfg(feature = "font-jb-semibold")]
    ("jb-semibold", FONT_JB_SEMIBOLD),
    #[cfg(feature = "font-jb-bold")]
    ("jb-bold", FONT_JB_BOLD),
    #[cfg(feature = "font-jb-bold")]
    ("jetbrains-bold", FONT_JB_BOLD),
    #[cfg(feature = "font-jb-extrabold")]
    ("jb-extrabold", FONT_JB_EXTRABOLD),
    #[cfg(feature = "font-jb-extrabold")]
    ("jetbrains-mono", FONT_JB_EXTRABOLD),
];

/// Gap in pixels between the bottom edge and the baseline of a bottom label.
const BOTTOM_BASELINE_PAD: f32 = 4.0;

/// Whether the font `name` (or alias) is embedded in this build. Names that
/// are unknown or whose feature is off render with the Inter fallback.
#[must_use]
pub fn font_available(name: &str) -> bool {
    FONTS.iter().any(|(font, _)| *font == name)
}

/// Get font bytes by name. Falls back to Inter for an unknown name or a
/// font that is not embedded in this build.
fn font_data(name: &str) -> &'static [u8] {
    FONTS
        .iter()
        .find(|(font, _)| *font == name)
        .map_or(FONT_INTER, |(_, data)| data)
}

/// Convert a count or pixel size to `f32` for layout math.
fn to_f32<T: ToPrimitive + Copy>(value: T, what: &str) -> Result<f32, CoreError> {
    value
        .to_f32()
        .ok_or_else(|| CoreError::Render(format!("{what} does not fit f32")))
}

/// Pixmap size for layout (`f32`) and pixel addressing (`i32`).
struct Dims {
    width_px: f32,
    height_px: f32,
    width: i32,
    height: i32,
}

impl Dims {
    fn of(pixmap: &Pixmap) -> Result<Self, CoreError> {
        let too_big = |_| CoreError::Render("pixmap does not fit i32".into());
        Ok(Self {
            width_px: to_f32(pixmap.width(), "pixmap width")?,
            height_px: to_f32(pixmap.height(), "pixmap height")?,
            width: i32::try_from(pixmap.width()).map_err(too_big)?,
            height: i32::try_from(pixmap.height()).map_err(too_big)?,
        })
    }
}

/// Channel value in `0.0..=1.0` scaled to `0..=255`, truncating.
fn unit_to_u8(value: f32) -> Result<u8, CoreError> {
    (value * 255.0)
        .to_u8()
        .ok_or_else(|| CoreError::Render(format!("color channel {value} out of range")))
}

/// Glyph coverage scaled to an alpha byte.
///
/// The rasterizer reports coverage as an accumulated absolute value that can
/// overshoot 1.0 slightly, so the result saturates at 255 (and at 0 below),
/// like a saturating float cast. A NaN coverage draws nothing.
fn coverage_alpha(coverage: f32) -> u8 {
    (coverage * 255.0).clamp(0.0, 255.0).to_u8().unwrap_or(0)
}

/// RGB color components for blending.
struct Rgb {
    red: u8,
    green: u8,
    blue: u8,
}

impl Rgb {
    fn from_hex(hex: &str) -> Result<Self, CoreError> {
        let color = parse_hex_color(hex)?;
        Ok(Self {
            red: unit_to_u8(color.red())?,
            green: unit_to_u8(color.green())?,
            blue: unit_to_u8(color.blue())?,
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
    // fg * a + bg * (255 - a) is at most 255 * 255, so the quotient is at
    // most 255 and the saturation never triggers.
    let blend = |fg: u8, bg: u8| -> u8 {
        let mixed = (u16::from(fg) * u16::from(alpha) + u16::from(bg) * u16::from(inv)) / 255;
        u8::try_from(mixed).unwrap_or(u8::MAX)
    };
    data[idx] = blend(color.red, data[idx]);
    data[idx + 1] = blend(color.green, data[idx + 1]);
    data[idx + 2] = blend(color.blue, data[idx + 2]);
    data[idx + 3] = 255;
}

/// Byte offset of pixel (x, y) in an RGBA buffer `width` pixels wide, or
/// `None` when the pixel lies outside the canvas.
fn pixel_offset(x: i32, y: i32, width: i32, height: i32) -> Option<usize> {
    if x < 0 || x >= width || y < 0 || y >= height {
        return None;
    }
    let index = usize::try_from(y.checked_mul(width)?.checked_add(x)?).ok()?;
    index.checked_mul(4)
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
            // Glyph origin on the canvas; a glyph whose bounds do not fit i32
            // lies far outside any key and is skipped.
            let origin = bounds.min.x.to_i32().zip(bounds.min.y.to_i32());
            let cw = canvas.width;
            let ch = canvas.height;
            if let Some((ox, oy)) = origin {
                outlined.draw(|px, py, coverage| {
                    let x = i32::try_from(px).ok().and_then(|px| px.checked_add(ox));
                    let y = i32::try_from(py).ok().and_then(|py| py.checked_add(oy));
                    if let Some(idx) = x.zip(y).and_then(|(x, y)| pixel_offset(x, y, cw, ch)) {
                        blend_pixel(canvas.data, idx, color, coverage_alpha(coverage));
                    }
                });
            }
        }

        cursor_x += font.h_advance(glyph_id);
        prev_glyph_id = Some(glyph_id);
    }
}

/// Rasterize text centered on the pixmap.
///
/// Layout uses the pixmap's own width and height, so the same call centers
/// text on a 72 px Stream Deck key and a 120 px panel key.
///
/// # Errors
/// Returns `CoreError::Font` if the embedded font fails to load,
/// or `CoreError::Render` if the color is invalid.
pub fn render_text(
    pixmap: &mut Pixmap,
    text: &str,
    color_hex: &str,
    font_size: f32,
    font_name: &str,
) -> Result<(), CoreError> {
    let font = FontRef::try_from_slice(font_data(font_name))
        .map_err(|e| CoreError::Font(e.to_string()))?;
    let color = Rgb::from_hex(color_hex)?;

    let scale = PxScale::from(font_size);
    let scaled_font = font.as_scaled(scale);

    let dims = Dims::of(pixmap)?;
    let lines: Vec<&str> = text.split('\n').collect();
    let line_height = scaled_font.height();
    let total_height = line_height * to_f32(lines.len(), "line count")?;
    let start_y = ((dims.height_px - total_height) / 2.0).max(2.0);

    let mut canvas = Canvas {
        data: pixmap.data_mut(),
        width: dims.width,
        height: dims.height,
    };

    for (line_idx, line) in lines.iter().enumerate() {
        let visual_width = measure_line_visual(&scaled_font, scale, line);
        let x_offset = ((dims.width_px - visual_width) / 2.0).max(1.0);
        let y_baseline = line_height.mul_add(to_f32(line_idx, "line index")? + 0.8, start_y);

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
/// Returns `CoreError::Font` if the embedded font fails to load,
/// or `CoreError::Render` if the color is invalid.
pub fn render_text_at_bottom(
    pixmap: &mut Pixmap,
    text: &str,
    color_hex: &str,
    font_size: f32,
    font_name: &str,
) -> Result<(), CoreError> {
    let font = FontRef::try_from_slice(font_data(font_name))
        .map_err(|e| CoreError::Font(e.to_string()))?;
    let color = Rgb::from_hex(color_hex)?;

    let scale = PxScale::from(font_size);
    let scaled_font = font.as_scaled(scale);

    let dims = Dims::of(pixmap)?;
    let y_baseline = dims.height_px - BOTTOM_BASELINE_PAD;
    let visual_width = measure_line_visual(&scaled_font, scale, text);
    let x_offset = ((dims.width_px - visual_width) / 2.0).max(1.0);

    let mut canvas = Canvas {
        data: pixmap.data_mut(),
        width: dims.width,
        height: dims.height,
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
        if let Some(outlined) = font
            .outline_glyph(glyph_id.with_scale_and_position(scale, ab_glyph::point(cursor_x, 0.0)))
        {
            let bounds = outlined.px_bounds();
            min_x = min_x.min(bounds.min.x);
            max_x = max_x.max(bounds.max.x);
            has_bounds = true;
        }
        cursor_x += font.h_advance(glyph_id);
        prev = Some(glyph_id);
    }

    if has_bounds {
        max_x - min_x
    } else {
        cursor_x
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "all-fonts")]
    #[test]
    fn font_available_with_all_fonts() {
        assert!(font_available("jb-thin"));
        assert!(font_available("inter"));
        assert!(font_available("jetbrains-mono"), "alias");
        assert!(!font_available("nope"));
        assert_eq!(font_data("jb-thin").as_ptr(), FONT_JB_THIN.as_ptr());
    }

    #[cfg(not(feature = "font-jb-thin"))]
    #[test]
    fn font_without_feature_is_unavailable_and_falls_back_to_inter() {
        assert!(!font_available("jb-thin"));
        assert_eq!(font_data("jb-thin").as_ptr(), FONT_INTER.as_ptr());
        assert!(font_available("inter"));
    }

    #[test]
    fn unknown_font_falls_back_to_inter() {
        assert!(!font_available("nope"));
        assert_eq!(font_data("nope").as_ptr(), FONT_INTER.as_ptr());
    }

    #[test]
    fn coverage_alpha_saturates_like_a_float_cast() {
        assert_eq!(coverage_alpha(0.0), 0);
        assert_eq!(coverage_alpha(0.5), 127);
        assert_eq!(coverage_alpha(1.0), 255);
        // The rasterizer can report slightly more than full coverage.
        assert_eq!(coverage_alpha(1.004), 255);
        assert_eq!(coverage_alpha(-0.1), 0);
        assert_eq!(coverage_alpha(f32::NAN), 0);
    }

    #[test]
    fn pixel_offset_rejects_outside_pixels() {
        assert_eq!(pixel_offset(0, 0, 72, 72), Some(0));
        assert_eq!(pixel_offset(71, 71, 72, 72), Some((71 * 72 + 71) * 4));
        assert_eq!(pixel_offset(-1, 0, 72, 72), None);
        assert_eq!(pixel_offset(72, 0, 72, 72), None);
        assert_eq!(pixel_offset(0, 72, 72, 72), None);
    }

    #[test]
    fn blend_pixel_full_and_zero_alpha() {
        let white = Rgb {
            red: 255,
            green: 255,
            blue: 255,
        };
        let mut data = [10, 20, 30, 0];
        blend_pixel(&mut data, 0, &white, 0);
        assert_eq!(data, [10, 20, 30, 255]);
        blend_pixel(&mut data, 0, &white, 255);
        assert_eq!(data, [255, 255, 255, 255]);
    }
}
