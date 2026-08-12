pub mod canvas;
pub mod icon;
pub mod text;

use crate::config::schema::{ButtonConfig, ButtonDefaults, ThresholdConfig};
use crate::error::Result;
use canvas::create_canvas;
use std::collections::HashMap;
use std::path::Path;

/// Placeholder in a `label` replaced by the live `label_entity` value.
const VALUE_PLACEHOLDER: &str = "{value}";

/// Placeholder replaced by the number of `count_entities` currently "on".
const COUNT_PLACEHOLDER: &str = "{count}";

/// Placeholder replaced by the total number of `count_entities`.
const TOTAL_PLACEHOLDER: &str = "{total}";

/// Fallback shown for `{value}` when no value is known.
const DEFAULT_UNKNOWN_TEXT: &str = "--";

/// HA states that mean "no usable value".
fn is_unknown_state(state: &str) -> bool {
    matches!(state, "unknown" | "unavailable" | "none" | "")
}

/// Format an HA state string for display at the given precision.
///
/// Numeric states are rounded to `precision` decimals; anything else
/// (including "unavailable") is returned unchanged so it stays visible.
fn format_value(state: &str, precision: u8) -> String {
    match state.parse::<f64>() {
        Ok(n) => format!("{n:.*}", precision as usize),
        Err(_) => state.to_string(),
    }
}

/// Pick the threshold color for a numeric value.
///
/// The highest band whose `min` is <= `value` wins. Returns `None` when there
/// are no bands, the value is not numeric, or the value sits below every band.
fn threshold_color(thresholds: &[ThresholdConfig], value: &str) -> Option<String> {
    let n: f64 = value.parse().ok()?;
    thresholds
        .iter()
        .filter(|t| n >= t.min)
        .max_by(|a, b| a.min.total_cmp(&b.min))
        .map(|t| t.color.clone())
}

/// Count how many of `entities` are currently "on".
///
/// Entities missing from `entity_states` are counted as off. Because the daemon
/// holds last known values across failed fetches, a missing entity here means
/// it has genuinely never been seen rather than a transient HA stall.
fn count_active(entities: &[String], entity_states: &HashMap<String, String>) -> usize {
    entities
        .iter()
        .filter(|id| entity_states.get(*id).is_some_and(|s| s == "on"))
        .count()
}

/// Substitute the live entity value into a label.
///
/// Returns the resolved label plus the raw state string (if known) so callers
/// can apply threshold colors to the same value that was rendered.
fn resolve_label(
    button: &ButtonConfig,
    entity_states: &HashMap<String, String>,
) -> (Option<String>, Option<String>) {
    let Some(label) = button.label.as_deref() else {
        return (None, None);
    };

    // Folder counts are independent of `label_entity`, so a folder button can
    // show {count} without tracking a single value entity.
    let mut resolved = label.to_string();
    if !button.count_entities.is_empty() {
        let active = count_active(&button.count_entities, entity_states);
        resolved = resolved
            .replace(COUNT_PLACEHOLDER, &active.to_string())
            .replace(TOTAL_PLACEHOLDER, &button.count_entities.len().to_string());
    }

    let Some(entity_id) = button.label_entity.as_deref() else {
        return (Some(resolved), None);
    };

    let raw = entity_states
        .get(entity_id)
        .map(String::as_str)
        .filter(|s| !is_unknown_state(s));

    let shown = match raw {
        Some(state) => format_value(state, button.precision.unwrap_or(0)),
        None => button
            .unknown_text
            .clone()
            .unwrap_or_else(|| DEFAULT_UNKNOWN_TEXT.to_string()),
    };

    resolved = resolved.replace(VALUE_PLACEHOLDER, &shown);
    (Some(resolved), raw.map(str::to_string))
}

/// Render a single button to raw RGBA bytes (72x72).
///
/// `entity_states` maps HA entity IDs to their current state string.
/// When a button has `state_entity` and the state is "on", the `on_background`
/// and `on_text_color` overrides are used.
///
/// # Errors
/// Returns `DeckError::Render` if canvas creation, icon loading, or text rendering fails.
pub fn render_button(
    button: &ButtonConfig,
    defaults: &ButtonDefaults,
    config_dir: &Path,
    entity_states: &HashMap<String, String>,
) -> Result<Vec<u8>> {
    // Check if entity is "on" for stateful color swapping.
    let entity_on = button
        .state_entity
        .as_ref()
        .and_then(|eid| entity_states.get(eid))
        .is_some_and(|s| s == "on");

    // A folder button counts the devices inside it; any active device lights
    // the folder up via `active_background` / `active_text_color`.
    let any_active =
        !button.count_entities.is_empty() && count_active(&button.count_entities, entity_states) > 0;

    let bg = if entity_on {
        button.on_background.as_deref()
            .or(button.background.as_deref())
            .unwrap_or(&defaults.background)
    } else if any_active {
        button.active_background.as_deref()
            .or(button.background.as_deref())
            .unwrap_or(&defaults.background)
    } else {
        button.background.as_deref().unwrap_or(&defaults.background)
    };

    // Resolve the live label value before choosing colors, so thresholds are
    // evaluated against exactly the value that gets drawn.
    let (label, label_value) = resolve_label(button, entity_states);

    let base_text_color = if entity_on {
        button.on_text_color.as_deref()
            .or(button.text_color.as_deref())
            .unwrap_or(&defaults.text_color)
    } else if any_active {
        button.active_text_color.as_deref()
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

    let mut pm = create_canvas(bg)?;

    // Render icon if specified. Track whether it actually loaded.
    let mut icon_rendered = false;
    if let Some(ref icon_path) = button.icon {
        let full_path = if Path::new(icon_path).is_absolute() {
            std::path::PathBuf::from(icon_path)
        } else {
            config_dir.join(icon_path)
        };

        if full_path.exists() {
            match icon::load_icon(&full_path) {
                Ok(icon_pm) => {
                    let x = icon::center_x(icon_pm.width());
                    let y = icon::icon_y(label.is_some());
                    canvas::composite(&mut pm, &icon_pm, x, y);
                    icon_rendered = true;
                }
                Err(e) => {
                    tracing::warn!("failed to load icon {}: {e}", full_path.display());
                }
            }
        } else {
            tracing::warn!("icon not found: {}", full_path.display());
        }
    }

    // Render text label.
    if let Some(ref label) = label {
        if icon_rendered {
            // Icon present: render text in the bottom portion.
            let label_font_size = font_size.min(12.0);
            text::render_text_at_bottom(&mut pm, label, text_color, label_font_size, font_name)?;
        } else {
            // No icon: center text.
            text::render_text(&mut pm, label, text_color, font_size, font_name)?;
        }
    }

    Ok(pm.data().to_vec())
}

/// Render a blank (empty/black) button.
///
/// # Errors
/// Returns `DeckError::Render` if canvas creation fails.
pub fn render_blank() -> Result<Vec<u8>> {
    let pm = create_canvas("#000000")?;
    Ok(pm.data().to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Office comfort bands: blue below 21, white 21-25, red above 25.
    fn comfort_bands() -> Vec<ThresholdConfig> {
        vec![
            ThresholdConfig { min: f64::NEG_INFINITY, color: "#4FC3F7".into() },
            ThresholdConfig { min: 21.0, color: "#FFFFFF".into() },
            ThresholdConfig { min: 25.0, color: "#E53935".into() },
        ]
    }

    fn button_with_label(label: &str, entity: Option<&str>) -> ButtonConfig {
        ButtonConfig {
            key: 5,
            label: Some(label.to_string()),
            icon: None,
            background: None,
            text_color: None,
            font_size: None,
            font: None,
            on_press: None,
            state_entity: None,
            on_background: None,
            on_text_color: None,
            label_entity: entity.map(str::to_string),
            precision: Some(1),
            unknown_text: None,
            thresholds: Vec::new(),
            count_entities: Vec::new(),
            active_text_color: None,
            active_background: None,
        }
    }

    #[test]
    fn format_value_rounds_to_precision() {
        assert_eq!(format_value("27.21", 1), "27.2");
        assert_eq!(format_value("27.21", 0), "27");
        assert_eq!(format_value("5.89", 1), "5.9");
        // Rounds rather than truncates.
        assert_eq!(format_value("21.96", 1), "22.0");
    }

    #[test]
    fn format_value_passes_through_non_numeric() {
        assert_eq!(format_value("unavailable", 1), "unavailable");
        assert_eq!(format_value("on", 0), "on");
    }

    #[test]
    fn threshold_picks_correct_band() {
        let bands = comfort_bands();
        assert_eq!(threshold_color(&bands, "18.0").unwrap(), "#4FC3F7");
        assert_eq!(threshold_color(&bands, "22.5").unwrap(), "#FFFFFF");
        assert_eq!(threshold_color(&bands, "27.21").unwrap(), "#E53935");
    }

    #[test]
    fn threshold_boundaries_are_inclusive_lower_bounds() {
        let bands = comfort_bands();
        // Exactly 21 enters the comfort band; just under stays cold.
        assert_eq!(threshold_color(&bands, "21.0").unwrap(), "#FFFFFF");
        assert_eq!(threshold_color(&bands, "20.9").unwrap(), "#4FC3F7");
        // Exactly 25 is already hot.
        assert_eq!(threshold_color(&bands, "25.0").unwrap(), "#E53935");
        assert_eq!(threshold_color(&bands, "24.9").unwrap(), "#FFFFFF");
    }

    #[test]
    fn threshold_ignores_non_numeric_and_empty_bands() {
        assert!(threshold_color(&comfort_bands(), "unavailable").is_none());
        assert!(threshold_color(&[], "22.0").is_none());
    }

    #[test]
    fn threshold_order_in_config_does_not_matter() {
        // Descending order must resolve the same as ascending.
        let reversed = vec![
            ThresholdConfig { min: 25.0, color: "#E53935".into() },
            ThresholdConfig { min: 21.0, color: "#FFFFFF".into() },
        ];
        assert_eq!(threshold_color(&reversed, "27.0").unwrap(), "#E53935");
        assert_eq!(threshold_color(&reversed, "22.0").unwrap(), "#FFFFFF");
    }

    #[test]
    fn label_substitutes_live_value() {
        let button = button_with_label("Keller\n{value}\u{00b0}", Some("sensor.keller_temperature"));
        let mut states = HashMap::new();
        states.insert("sensor.keller_temperature".to_string(), "27.21".to_string());

        let (label, raw) = resolve_label(&button, &states);
        assert_eq!(label.unwrap(), "Keller\n27.2\u{00b0}");
        assert_eq!(raw.unwrap(), "27.21");
    }

    #[test]
    fn label_shows_unknown_text_when_value_missing() {
        let button = button_with_label("Keller\n{value}\u{00b0}", Some("sensor.keller_temperature"));
        // Entity absent from the map entirely (fetch failed).
        let (label, raw) = resolve_label(&button, &HashMap::new());
        assert_eq!(label.unwrap(), "Keller\n--\u{00b0}");
        assert!(raw.is_none(), "no raw value means thresholds must not apply");
    }

    #[test]
    fn label_treats_unavailable_as_unknown() {
        let button = button_with_label("Keller\n{value}\u{00b0}", Some("sensor.keller_temperature"));
        let mut states = HashMap::new();
        states.insert("sensor.keller_temperature".to_string(), "unavailable".to_string());

        let (label, raw) = resolve_label(&button, &states);
        assert_eq!(label.unwrap(), "Keller\n--\u{00b0}");
        assert!(raw.is_none());
    }

    #[test]
    fn label_without_entity_is_unchanged() {
        let button = button_with_label("Static", None);
        let (label, raw) = resolve_label(&button, &HashMap::new());
        assert_eq!(label.unwrap(), "Static");
        assert!(raw.is_none());
    }

    /// The three real plant plugs, as configured on the deck.
    fn plant_plugs() -> Vec<String> {
        vec![
            "switch.shellyplugsg3_1".to_string(),
            "switch.smart_plug_mini_4".to_string(),
            "switch.shellyplugsg3_90706943935c".to_string(),
        ]
    }

    #[test]
    fn count_active_counts_only_on_entities() {
        let mut states = HashMap::new();
        states.insert("switch.shellyplugsg3_1".to_string(), "on".to_string());
        states.insert("switch.smart_plug_mini_4".to_string(), "off".to_string());
        states.insert(
            "switch.shellyplugsg3_90706943935c".to_string(),
            "on".to_string(),
        );

        assert_eq!(count_active(&plant_plugs(), &states), 2);
    }

    #[test]
    fn count_active_treats_missing_and_unavailable_as_off() {
        let mut states = HashMap::new();
        states.insert("switch.shellyplugsg3_1".to_string(), "unavailable".to_string());
        // Other two absent entirely.
        assert_eq!(count_active(&plant_plugs(), &states), 0);
    }

    #[test]
    fn count_active_all_on_and_all_off() {
        let all_on: HashMap<String, String> = plant_plugs()
            .into_iter()
            .map(|e| (e, "on".to_string()))
            .collect();
        assert_eq!(count_active(&plant_plugs(), &all_on), 3);

        let all_off: HashMap<String, String> = plant_plugs()
            .into_iter()
            .map(|e| (e, "off".to_string()))
            .collect();
        assert_eq!(count_active(&plant_plugs(), &all_off), 0);
    }

    #[test]
    fn folder_label_substitutes_count_and_total() {
        let mut button = button_with_label("\u{f06c}\n{count}/{total}", None);
        button.count_entities = plant_plugs();

        let mut states = HashMap::new();
        states.insert("switch.shellyplugsg3_1".to_string(), "on".to_string());
        states.insert("switch.smart_plug_mini_4".to_string(), "off".to_string());
        states.insert(
            "switch.shellyplugsg3_90706943935c".to_string(),
            "on".to_string(),
        );

        let (label, raw) = resolve_label(&button, &states);
        assert_eq!(label.unwrap(), "\u{f06c}\n2/3");
        assert!(raw.is_none(), "a folder has no single value entity");
    }

    #[test]
    fn folder_count_works_without_label_entity() {
        // Regression: counts must not depend on label_entity being set.
        let mut button = button_with_label("{count}", None);
        button.count_entities = plant_plugs();
        button.label_entity = None;

        let mut states = HashMap::new();
        states.insert("switch.shellyplugsg3_1".to_string(), "on".to_string());

        let (label, _) = resolve_label(&button, &states);
        assert_eq!(label.unwrap(), "1");
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
        let dir = Path::new(".");

        let green_sum = |data: &[u8]| -> u64 {
            data.chunks_exact(4).map(|px| u64::from(px[1])).sum()
        };

        let mut on_states = HashMap::new();
        on_states.insert("switch.shellyplugsg3_1".to_string(), "on".to_string());
        let active = render_button(&button, &defaults, dir, &on_states).unwrap();

        let off_states: HashMap<String, String> = plant_plugs()
            .into_iter()
            .map(|e| (e, "off".to_string()))
            .collect();
        let idle = render_button(&button, &defaults, dir, &off_states).unwrap();

        assert!(
            green_sum(&active) > green_sum(&idle),
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
        let dir = Path::new(".");

        let render_at = |temp: &str| -> Vec<u8> {
            let mut states = HashMap::new();
            states.insert("sensor.keller_temperature".to_string(), temp.to_string());
            render_button(&button, &defaults, dir, &states).unwrap()
        };

        // Sum the red and blue channels across the button.
        let channel_sums = |data: &[u8]| -> (u64, u64) {
            let mut red = 0u64;
            let mut blue = 0u64;
            for px in data.chunks_exact(4) {
                red += u64::from(px[0]);
                blue += u64::from(px[2]);
            }
            (red, blue)
        };

        let (hot_red, hot_blue) = channel_sums(&render_at("27.21"));
        let (cold_red, cold_blue) = channel_sums(&render_at("18.0"));

        assert!(hot_red > hot_blue, "27.2C should render red-dominant, got r={hot_red} b={hot_blue}");
        assert!(cold_blue > cold_red, "18.0C should render blue-dominant, got r={cold_red} b={cold_blue}");
    }
}
