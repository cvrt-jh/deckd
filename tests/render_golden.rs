//! Golden hash of every rendered key in `config.example.toml`.
//!
//! Locks the rendered output so a refactor of config or rendering code can be
//! proven byte-identical. Bless a new hash with `DECKD_BLESS=1`. The config
//! path can be overridden with `DECKD_GOLDEN_CONFIG` (used to prove the test
//! goes red when the output changes).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use deckd::config::schema::AppConfig;
use sha2::{Digest, Sha256};

fn golden_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden/example_config.sha256")
}

fn config_path() -> PathBuf {
    std::env::var_os("DECKD_GOLDEN_CONFIG").map_or_else(
        || Path::new(env!("CARGO_MANIFEST_DIR")).join("config.example.toml"),
        PathBuf::from,
    )
}

/// Three fixed state maps: empty, everything on, everything unavailable.
fn state_maps(cfg: &AppConfig) -> Vec<HashMap<String, String>> {
    let mut on = HashMap::new();
    let mut unavailable = HashMap::new();
    for page in cfg.pages.values() {
        for button in &page.buttons {
            let toggles = button
                .state_entity
                .iter()
                .chain(button.count_entities.iter());
            for entity in toggles {
                on.insert(entity.clone(), "on".to_string());
                unavailable.insert(entity.clone(), "unavailable".to_string());
            }
            if let Some(entity) = &button.label_entity {
                on.insert(entity.clone(), "23.4".to_string());
                unavailable.insert(entity.clone(), "unavailable".to_string());
            }
        }
    }
    vec![HashMap::new(), on, unavailable]
}

fn render_hash() -> String {
    let path = config_path();
    let cfg = deckd::config::load(&path).expect("load config");
    let config_dir = path.parent().expect("config has a parent dir");
    let defaults = &cfg.deckd.defaults;

    let mut page_ids: Vec<&String> = cfg.pages.keys().collect();
    page_ids.sort();

    let mut hasher = Sha256::new();
    let mut rendered = 0usize;
    for states in state_maps(&cfg) {
        for page_id in &page_ids {
            let mut buttons: Vec<_> = cfg.pages[*page_id].buttons.iter().collect();
            buttons.sort_by_key(|b| b.key);
            for button in buttons {
                let bytes = deckd::render::render_button(button, defaults, config_dir, &states)
                    .unwrap_or_else(|e| panic!("render page '{page_id}' key {}: {e}", button.key));
                hasher.update(&bytes);
                rendered += 1;
            }
        }
    }
    assert!(
        rendered > 0,
        "no buttons rendered; the hash would be vacuous"
    );
    format!("{:x}", hasher.finalize())
}

#[test]
fn render_golden_matches_example_config() {
    let actual = render_hash();
    let golden = golden_path();

    if !golden.exists() {
        assert!(
            std::env::var("DECKD_BLESS").is_ok_and(|v| v == "1"),
            "{} is missing; run with DECKD_BLESS=1 to create it",
            golden.display()
        );
        std::fs::write(&golden, format!("{actual}\n")).expect("write golden hash");
        return;
    }

    let expected = std::fs::read_to_string(&golden).expect("read golden hash");
    assert_eq!(
        actual,
        expected.trim(),
        "rendered output of the example config changed"
    );
}
