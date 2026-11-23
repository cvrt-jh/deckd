use crate::error::CoreError;
use tiny_skia::{Color, Pixmap, Transform};

/// Create a square `key_px` x `key_px` pixmap filled with a solid color.
///
/// # Errors
/// Returns `CoreError::Render` if the hex color is invalid or `key_px` is 0.
pub fn create_canvas(bg_hex: &str, key_px: u32) -> Result<Pixmap, CoreError> {
    let mut pixmap = Pixmap::new(key_px, key_px)
        .ok_or_else(|| CoreError::Render("failed to create pixmap".into()))?;

    let color = parse_hex_color(bg_hex)?;
    pixmap.fill(color);
    Ok(pixmap)
}

/// Composite a source pixmap onto the canvas at the given position.
pub fn composite(canvas: &mut Pixmap, src: &Pixmap, x: i32, y: i32) {
    canvas.draw_pixmap(
        x,
        y,
        src.as_ref(),
        &tiny_skia::PixmapPaint::default(),
        Transform::identity(),
        None,
    );
}

/// Parse a hex color string like "#1a1a2e" or "#fff" into a tiny-skia Color.
///
/// # Errors
/// Returns `CoreError::Render` if the hex string is malformed.
pub fn parse_hex_color(hex: &str) -> Result<Color, CoreError> {
    let hex = hex.trim_start_matches('#');
    let parse_err = || CoreError::Render(format!("invalid hex color: #{hex}"));

    let (r, g, b) = match hex.len() {
        3 => {
            let r = u8::from_str_radix(&hex[0..1].repeat(2), 16).map_err(|_| parse_err())?;
            let g = u8::from_str_radix(&hex[1..2].repeat(2), 16).map_err(|_| parse_err())?;
            let b = u8::from_str_radix(&hex[2..3].repeat(2), 16).map_err(|_| parse_err())?;
            (r, g, b)
        }
        6 => {
            let r = u8::from_str_radix(&hex[0..2], 16).map_err(|_| parse_err())?;
            let g = u8::from_str_radix(&hex[2..4], 16).map_err(|_| parse_err())?;
            let b = u8::from_str_radix(&hex[4..6], 16).map_err(|_| parse_err())?;
            (r, g, b)
        }
        _ => return Err(parse_err()),
    };

    Ok(Color::from_rgba8(r, g, b, 255))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_6_digit_hex() {
        let c = parse_hex_color("#1a1a2e").unwrap();
        assert!((c.red() - f32::from(0x1a_u8) / 255.0).abs() < f32::EPSILON);
    }

    #[test]
    fn parse_3_digit_hex() {
        let c = parse_hex_color("#fff").unwrap();
        assert!((c.red() - 1.0).abs() < f32::EPSILON);
        assert!((c.green() - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn parse_rejects_malformed_hex() {
        assert!(parse_hex_color("#12345").is_err());
        assert!(parse_hex_color("#zzz").is_err());
    }

    #[test]
    fn create_canvas_basic() {
        let pm = create_canvas("#000000", 72).unwrap();
        assert_eq!(pm.width(), 72);
        assert_eq!(pm.height(), 72);
    }

    #[test]
    fn create_canvas_rejects_zero_size() {
        assert!(create_canvas("#000000", 0).is_err());
    }
}
