//! Pure helpers that turn a button config plus live entity states into what a
//! key shows: label text, counts and threshold colors. No drawing here.

use std::collections::HashMap;

use crate::schema::{ButtonConfig, ThresholdConfig};

/// Placeholder in a `label` replaced by the live `label_entity` value.
const VALUE_PLACEHOLDER: &str = "{value}";

/// Placeholder replaced by the number of `count_entities` currently "on".
const COUNT_PLACEHOLDER: &str = "{count}";

/// Placeholder replaced by the total number of `count_entities`.
const TOTAL_PLACEHOLDER: &str = "{total}";

/// Fallback shown for `{value}` when no value is known.
const DEFAULT_UNKNOWN_TEXT: &str = "--";

/// HA states that mean "no usable value".
#[must_use]
pub fn is_unknown_state(state: &str) -> bool {
    matches!(state, "unknown" | "unavailable" | "none" | "")
}

/// Format an HA state string for display at the given precision.
///
/// Numeric states are rounded to `precision` decimals; anything else
/// (including "unavailable") is returned unchanged so it stays visible.
#[must_use]
pub fn format_value(state: &str, precision: u8) -> String {
    match state.parse::<f64>() {
        Ok(n) => format!("{n:.*}", usize::from(precision)),
        Err(_) => state.to_string(),
    }
}

/// Pick the threshold color for a numeric value.
///
/// The highest band whose `min` is <= `value` wins. Returns `None` when there
/// are no bands, the value is not numeric, or the value sits below every band.
#[must_use]
pub fn threshold_color(thresholds: &[ThresholdConfig], value: &str) -> Option<String> {
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
#[must_use]
pub fn count_active<S: std::hash::BuildHasher>(
    entities: &[String],
    entity_states: &HashMap<String, String, S>,
) -> usize {
    entities
        .iter()
        .filter(|id| entity_states.get(*id).is_some_and(|s| s == "on"))
        .count()
}

/// Substitute the live entity value into a label.
///
/// Returns the resolved label plus the raw state string (if known) so callers
/// can apply threshold colors to the same value that was rendered.
#[must_use]
pub fn resolve_label<S: std::hash::BuildHasher>(
    button: &ButtonConfig,
    entity_states: &HashMap<String, String, S>,
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

/// Shared fixtures for the view and render tests.
#[cfg(test)]
pub(crate) mod fixtures {
    use crate::schema::{ButtonConfig, ThresholdConfig};

    /// Office comfort bands: blue below 21, white 21-25, red above 25.
    pub fn comfort_bands() -> Vec<ThresholdConfig> {
        vec![
            ThresholdConfig {
                min: f64::NEG_INFINITY,
                color: "#4FC3F7".into(),
            },
            ThresholdConfig {
                min: 21.0,
                color: "#FFFFFF".into(),
            },
            ThresholdConfig {
                min: 25.0,
                color: "#E53935".into(),
            },
        ]
    }

    pub fn button_with_label(label: &str, entity: Option<&str>) -> ButtonConfig {
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

    /// The three real plant plugs, as configured on the deck.
    pub fn plant_plugs() -> Vec<String> {
        vec![
            "switch.shellyplugsg3_1".to_string(),
            "switch.smart_plug_mini_4".to_string(),
            "switch.shellyplugsg3_90706943935c".to_string(),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::fixtures::{button_with_label, comfort_bands, plant_plugs};
    use super::*;

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
            ThresholdConfig {
                min: 25.0,
                color: "#E53935".into(),
            },
            ThresholdConfig {
                min: 21.0,
                color: "#FFFFFF".into(),
            },
        ];
        assert_eq!(threshold_color(&reversed, "27.0").unwrap(), "#E53935");
        assert_eq!(threshold_color(&reversed, "22.0").unwrap(), "#FFFFFF");
    }

    #[test]
    fn unknown_states_are_recognised() {
        for state in ["unknown", "unavailable", "none", ""] {
            assert!(is_unknown_state(state), "{state:?} must count as unknown");
        }
        assert!(!is_unknown_state("0"));
        assert!(!is_unknown_state("off"));
    }

    #[test]
    fn label_substitutes_live_value() {
        let button =
            button_with_label("Keller\n{value}\u{00b0}", Some("sensor.keller_temperature"));
        let mut states = HashMap::new();
        states.insert("sensor.keller_temperature".to_string(), "27.21".to_string());

        let (label, raw) = resolve_label(&button, &states);
        assert_eq!(label.unwrap(), "Keller\n27.2\u{00b0}");
        assert_eq!(raw.unwrap(), "27.21");
    }

    #[test]
    fn label_shows_unknown_text_when_value_missing() {
        let button =
            button_with_label("Keller\n{value}\u{00b0}", Some("sensor.keller_temperature"));
        // Entity absent from the map entirely (fetch failed).
        let (label, raw) = resolve_label(&button, &HashMap::new());
        assert_eq!(label.unwrap(), "Keller\n--\u{00b0}");
        assert!(
            raw.is_none(),
            "no raw value means thresholds must not apply"
        );
    }

    #[test]
    fn label_treats_unavailable_as_unknown() {
        let button =
            button_with_label("Keller\n{value}\u{00b0}", Some("sensor.keller_temperature"));
        let mut states = HashMap::new();
        states.insert(
            "sensor.keller_temperature".to_string(),
            "unavailable".to_string(),
        );

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
        states.insert(
            "switch.shellyplugsg3_1".to_string(),
            "unavailable".to_string(),
        );
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
}
