use crate::error::{DeckError, Result};
use tiny_skia::{Color, Pixmap, Transform};

/// Stream Deck MK.2 button size in pixels.
pub const BUTTON_SIZE: u32 = 72;

/// Create a new pixmap filled with a solid background color.
///
/// # Errors
/// Returns `DeckError::Render` if the hex color is invalid or pixmap creation fails.
pub fn create_canvas(bg_hex: &str) -> Result<Pixmap> {
    let mut pixmap = Pixmap::new(BUTTON_SIZE, BUTTON_SIZE)
        .ok_or_else(|| DeckError::Render("failed to create pixmap".into()))?;

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
/// Returns `DeckError::Render` if the hex string is malformed.
pub fn parse_hex_color(hex: &str) -> Result<Color> {
    let hex = hex.trim_start_matches('#');
    let parse_err = || DeckError::Render(format!("invalid hex color: #{hex}"));

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
        assert_eq!(c.red(), 0x1a as f32 / 255.0);
    }

    #[test]
    fn parse_3_digit_hex() {
        let c = parse_hex_color("#fff").unwrap();
        assert_eq!(c.red(), 1.0);
        assert_eq!(c.green(), 1.0);
    }

    #[test]
    fn create_canvas_basic() {
        let pm = create_canvas("#000000").unwrap();
        assert_eq!(pm.width(), BUTTON_SIZE);
        assert_eq!(pm.height(), BUTTON_SIZE);
    }
}
