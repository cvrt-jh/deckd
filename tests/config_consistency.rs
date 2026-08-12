//! Guards against config mistakes that render a button silently useless.
//!
//! The motivating defect: a plant-plug button tracked `switch.shellyplugsg3_1`
//! but its toggle body named `switch.smart_plug_mini`, an entity that does not
//! exist. The button showed one plug's state and switched nothing, with no
//! error anywhere. Nothing in the type system prevents that, so it is checked
//! here instead.

use deckd::config::schema::{ActionConfig, AppConfig};

/// A stateful toggle button must act on the same entity it displays.
fn assert_state_matches_action(cfg: &AppConfig) {
    for (page_id, page) in &cfg.pages {
        for button in &page.buttons {
            let (Some(entity), Some(ActionConfig::Http { body: Some(body), .. })) =
                (button.state_entity.as_deref(), button.on_press.as_ref())
            else {
                continue;
            };

            // Only enforce this when the action targets an entity at all;
            // a button may legitimately POST something unrelated.
            if !body.contains("entity_id") {
                continue;
            }

            assert!(
                body.contains(entity),
                "page '{page_id}' key {}: tracks `{entity}` but its action body does not \
                 reference it ({body}). The button would show one entity's state while \
                 switching another.",
                button.key
            );
        }
    }
}

#[test]
fn example_config_is_self_consistent() {
    let raw = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/config.example.toml"
    ))
    .expect("read config.example.toml");
    let cfg: AppConfig = toml::from_str(&raw).expect("parse config.example.toml");
    assert_state_matches_action(&cfg);
}

#[test]
fn mismatched_state_and_action_is_rejected() {
    // Known-answer control: this is the real defect found on the deck, so the
    // check above must fail on it. If this test stops panicking, the guard has
    // silently stopped guarding.
    let toml_str = r#"
[deckd]

[pages.home]
name = "Home"

[[pages.home.buttons]]
key = 3
label = "Jay"
state_entity = "switch.shellyplugsg3_1"
on_press = { action = "http", method = "POST", url = "http://ha.local/api/services/switch/toggle", body = "{\"entity_id\": \"switch.smart_plug_mini\"}" }
"#;
    let cfg: AppConfig = toml::from_str(toml_str).expect("parse");

    let result = std::panic::catch_unwind(|| assert_state_matches_action(&cfg));
    assert!(
        result.is_err(),
        "the consistency check must reject a button that tracks one entity and toggles another"
    );
}

#[test]
fn matching_state_and_action_is_accepted() {
    let toml_str = r#"
[deckd]

[pages.home]
name = "Home"

[[pages.home.buttons]]
key = 3
label = "Jay"
state_entity = "switch.shellyplugsg3_1"
on_press = { action = "http", method = "POST", url = "http://ha.local/api/services/switch/toggle", body = "{\"entity_id\": \"switch.shellyplugsg3_1\"}" }
"#;
    let cfg: AppConfig = toml::from_str(toml_str).expect("parse");
    assert_state_matches_action(&cfg);
}
