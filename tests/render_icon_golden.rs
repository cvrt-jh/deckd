//! Golden hash of keys that draw an icon.
//!
//! `config.example.toml` names an icon file that does not exist, so the main
//! golden test never draws one. This test writes a known PNG (larger than the
//! icon box, with partial alpha) to a temp dir and locks the
//! 72 px output of an icon+label key and an icon-only key. The expected hash
//! was recorded from the renderer before it moved into `deckd-core`.

use std::collections::HashMap;
use std::path::PathBuf;

use deckd::config::schema::{ButtonConfig, ButtonDefaults};
use sha2::{Digest, Sha256};

const EXPECTED: &str = "39d943946f800d6b23c957edd4c94bc4d81c32ab311a9129de5b07e98be86f1d";

/// Temp dir unique to this test process, removed on drop.
struct TempDir(PathBuf);

impl Drop for TempDir {
    fn drop(&mut self) {
        // Best effort: a leftover dir in the OS temp dir is harmless, and a
        // panic inside drop would hide the real test failure.
        if let Err(e) = std::fs::remove_dir_all(&self.0) {
            eprintln!("could not remove {}: {e}", self.0.display());
        }
    }
}

fn write_icon() -> TempDir {
    let dir = std::env::temp_dir().join(format!("deckd-icon-golden-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("create temp dir");
    let img = image::RgbaImage::from_fn(100, 100, |x, y| {
        let r = u8::try_from(x * 255 / 99).expect("x fits");
        let g = u8::try_from(y * 255 / 99).expect("y fits");
        let a = if (x / 10 + y / 10) % 2 == 0 { 255 } else { 128 };
        image::Rgba([r, g, 200, a])
    });
    img.save(dir.join("icon.png")).expect("write icon png");
    TempDir(dir)
}

fn icon_button(label: Option<&str>) -> ButtonConfig {
    ButtonConfig {
        key: 0,
        label: label.map(str::to_string),
        icon: Some("icon.png".to_string()),
        background: Some("#1a1a2e".to_string()),
        text_color: Some("#FFFFFF".to_string()),
        font_size: Some(18.0),
        font: None,
        on_press: None,
        state_entity: None,
        on_background: None,
        on_text_color: None,
        label_entity: None,
        precision: None,
        unknown_text: None,
        thresholds: Vec::new(),
        count_entities: Vec::new(),
        active_text_color: None,
        active_background: None,
    }
}

#[test]
fn icon_keys_match_golden_hash() {
    let dir = write_icon();
    let defaults = ButtonDefaults::default();
    let states: HashMap<String, String> = HashMap::new();

    let mut hasher = Sha256::new();
    for label in [Some("Deploy"), None] {
        let bytes = deckd::render::render_button(&icon_button(label), &defaults, &dir.0, &states)
            .expect("render icon key");
        let without_icon = deckd::render::render_button(
            &ButtonConfig {
                icon: None,
                ..icon_button(label)
            },
            &defaults,
            &dir.0,
            &states,
        )
        .expect("render key without icon");
        assert_ne!(
            bytes, without_icon,
            "icon was not drawn; the hash would not cover the icon path"
        );
        hasher.update(&bytes);
    }
    assert_eq!(format!("{:x}", hasher.finalize()), EXPECTED);
}
