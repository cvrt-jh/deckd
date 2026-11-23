//! Key rendering: background, optional icon and label text into a square
//! RGBA pixmap of any key size.
//!
//! Callers own all I/O. An icon arrives as an already-loaded `Pixmap`,
//! scaled to fit `icon_max_px(key_px)`; the result is a `Pixmap` the caller
//! sends to its display.

pub mod canvas;
pub mod text;

use std::collections::HashMap;
use std::hash::BuildHasher;

use tiny_skia::Pixmap;

use crate::error::CoreError;
use crate::schema::{ButtonConfig, ButtonDefaults};
use crate::view::{count_active, resolve_label, threshold_color};
use canvas::create_canvas;

/// Gap in pixels above an icon that shares the key with a label.
const ICON_TOP_PAD: i64 = 4;

/// Largest font size for a label drawn below an icon.
const ICON_LABEL_MAX_FONT_SIZE: f32 = 12.0;

/// Largest edge length of an icon on a `key_px` key: two thirds of the key,
/// leaving room for a label below (48 px on a 72 px Stream Deck key).
#[must_use]
pub const fn icon_max_px(key_px: u32) -> u32 {
    key_px / 3 * 2 + key_px % 3 * 2 / 3
}

/// Horizontal position that centers an icon `icon_width` wide on the key.
///
/// # Errors
/// Returns `CoreError::Render` if the position does not fit `i32`.
pub fn icon_x(key_px: u32, icon_width: u32) -> Result<i32, CoreError> {
    let x = (i64::from(key_px) - i64::from(icon_width)) / 2;
    i32::try_from(x).map_err(|_| CoreError::Render(format!("icon x position {x} out of range")))
}

/// Vertical icon position: near the top when a label follows below, else
/// centered on the icon box of `icon_max_px(key_px)`.
///
/// # Errors
/// Returns `CoreError::Render` if the position does not fit `i32`.
pub fn icon_y(key_px: u32, has_label: bool) -> Result<i32, CoreError> {
    let y = if has_label {
        ICON_TOP_PAD
    } else {
        (i64::from(key_px) - i64::from(icon_max_px(key_px))) / 2
    };
    i32::try_from(y).map_err(|_| CoreError::Render(format!("icon y position {y} out of range")))
}

/// Render a single button to a `key_px` x `key_px` pixmap.
///
/// `states` maps HA entity IDs to their current state string.
/// When a button has `state_entity` and the state is "on", the `on_background`
/// and `on_text_color` overrides are used. `icon` is drawn when given; the
/// caller loads it and scales it to fit `icon_max_px(key_px)`.
///
/// # Errors
/// Returns `CoreError::Render` if canvas creation or a color fails, or
/// `CoreError::Font` if the embedded font fails to load.
pub fn render_button<S: BuildHasher>(
    button: &ButtonConfig,
    defaults: &ButtonDefaults,
    states: &HashMap<String, String, S>,
    key_px: u32,
    icon: Option<&Pixmap>,
) -> Result<Pixmap, CoreError> {
    // Check if entity is "on" for stateful color swapping.
    let entity_on = button
        .state_entity
        .as_ref()
        .and_then(|eid| states.get(eid))
        .is_some_and(|s| s == "on");

    // A folder button counts the devices inside it; any active device lights
    // the folder up via `active_background` / `active_text_color`.
    let any_active =
        !button.count_entities.is_empty() && count_active(&button.count_entities, states) > 0;

    let bg = if entity_on {
        button
            .on_background
            .as_deref()
            .or(button.background.as_deref())
            .unwrap_or(&defaults.background)
    } else if any_active {
        button
            .active_background
            .as_deref()
            .or(button.background.as_deref())
            .unwrap_or(&defaults.background)
    } else {
        button.background.as_deref().unwrap_or(&defaults.background)
    };

    // Resolve the live label value before choosing colors, so thresholds are
    // evaluated against exactly the value that gets drawn.
    let (label, label_value) = resolve_label(button, states);

    let base_text_color = if entity_on {
        button
            .on_text_color
            .as_deref()
            .or(button.text_color.as_deref())
            .unwrap_or(&defaults.text_color)
    } else if any_active {
        button
            .active_text_color
            .as_deref()
            .or(button.text_color.as_deref())
            .unwrap_or(&defaults.text_color)
    } else {
        button.text_color.as_deref().unwrap_or(&defaults.text_color)
    };

    // A matching threshold band overrides the text color.
    let threshold = label_value
        .as_deref()
        .and_then(|v| threshold_color(&button.thresholds, v));
    let text_color = threshold.as_deref().unwrap_or(base_text_color);

    let font_size = button.font_size.unwrap_or(defaults.font_size);
    let font_name = button.font.as_deref().unwrap_or(&defaults.font);

    let mut pm = create_canvas(bg, key_px)?;

    if let Some(icon_pm) = icon {
        let x = icon_x(key_px, icon_pm.width())?;
        let y = icon_y(key_px, label.is_some())?;
        canvas::composite(&mut pm, icon_pm, x, y);
    }

    if let Some(ref label) = label {
        if icon.is_some() {
            // Icon present: render text in the bottom portion.
            let label_font_size = font_size.min(ICON_LABEL_MAX_FONT_SIZE);
            text::render_text_at_bottom(&mut pm, label, text_color, label_font_size, font_name)?;
        } else {
            text::render_text(&mut pm, label, text_color, font_size, font_name)?;
        }
    }

    Ok(pm)
}

/// Render a blank (black) key.
///
/// # Errors
/// Returns `CoreError::Render` if canvas creation fails (`key_px` of 0).
pub fn render_blank(key_px: u32) -> Result<Pixmap, CoreError> {
    create_canvas("#000000", key_px)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::view::fixtures::{button_with_label, comfort_bands, plant_plugs};

    /// Sum of one RGBA channel across a pixmap.
    fn channel_sum(pm: &Pixmap, channel: usize) -> u64 {
        let (pixels, rest) = pm.data().as_chunks::<4>();
        assert!(rest.is_empty(), "RGBA data is a whole number of pixels");
        pixels.iter().map(|px| u64::from(px[channel])).sum()
    }

    #[test]
    fn icon_max_is_two_thirds_of_the_key() {
        assert_eq!(icon_max_px(72), 48);
        assert_eq!(icon_max_px(120), 80);
    }

    #[test]
    fn icon_position_at_72_px_matches_the_stream_deck_layout() {
        assert_eq!(icon_x(72, 48).unwrap(), 12);
        assert_eq!(icon_x(72, 31).unwrap(), 20);
        assert_eq!(icon_y(72, true).unwrap(), 4);
        assert_eq!(icon_y(72, false).unwrap(), 12);
    }

    #[test]
    fn icon_wider_than_the_key_is_centred_off_the_edge() {
        assert_eq!(icon_x(72, 80).unwrap(), -4);
    }

    #[test]
    fn icon_is_composited_and_label_moves_to_the_bottom() {
        let mut button = button_with_label("A", None);
        button.background = Some("#000000".into());
        let defaults = ButtonDefaults::default();
        let states: HashMap<String, String> = HashMap::new();

        let mut icon = Pixmap::new(10, 10).unwrap();
        icon.fill(tiny_skia::Color::from_rgba8(0, 0, 255, 255));

        let pm = render_button(&button, &defaults, &states, 72, Some(&icon)).unwrap();
        // Icon box: x 31..41, y 4..14 (label present), fully blue.
        let px = pm.pixel(35, 8).unwrap();
        assert_eq!((px.red(), px.green(), px.blue()), (0, 0, 255));
        let without = render_button(&button, &defaults, &states, 72, None).unwrap();
        assert_eq!(channel_sum(&without, 2), channel_sum(&without, 0));
    }

    #[test]
    fn folder_active_color_applies_only_when_something_is_on() {
        let mut button = button_with_label("{count}", None);
        button.count_entities = plant_plugs();
        button.background = Some("#000000".into());
        button.text_color = Some("#333333".into());
        button.active_text_color = Some("#4CAF50".into());
        button.font_size = Some(40.0);

        let defaults = ButtonDefaults::default();

        let mut on_states = HashMap::new();
        on_states.insert("switch.shellyplugsg3_1".to_string(), "on".to_string());
        let active = render_button(&button, &defaults, &on_states, 72, None).unwrap();

        let off_states: HashMap<String, String> = plant_plugs()
            .into_iter()
            .map(|e| (e, "off".to_string()))
            .collect();
        let idle = render_button(&button, &defaults, &off_states, 72, None).unwrap();

        assert!(
            channel_sum(&active, 1) > channel_sum(&idle, 1),
            "active folder must render greener than idle"
        );
    }

    #[test]
    fn render_button_applies_threshold_color_to_pixels() {
        // End-to-end: a hot value must actually paint red pixels, a cold value blue.
        let mut button = button_with_label("{value}", Some("sensor.keller_temperature"));
        button.thresholds = comfort_bands();
        button.background = Some("#000000".into());
        button.font_size = Some(30.0);

        let defaults = ButtonDefaults::default();

        let render_at = |temp: &str| -> Pixmap {
            let mut states = HashMap::new();
            states.insert("sensor.keller_temperature".to_string(), temp.to_string());
            render_button(&button, &defaults, &states, 72, None).unwrap()
        };

        let hot = render_at("27.21");
        let cold = render_at("18.0");
        let (hot_red, hot_blue) = (channel_sum(&hot, 0), channel_sum(&hot, 2));
        let (cold_red, cold_blue) = (channel_sum(&cold, 0), channel_sum(&cold, 2));

        assert!(
            hot_red > hot_blue,
            "27.2C should render red-dominant, got r={hot_red} b={hot_blue}"
        );
        assert!(
            cold_blue > cold_red,
            "18.0C should render blue-dominant, got r={cold_red} b={cold_blue}"
        );
    }
}
