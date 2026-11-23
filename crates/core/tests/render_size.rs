//! Key size is a parameter of core rendering: the same button renders at the
//! Stream Deck's 72 px and the touch panel's 120 px, with text laid out
//! against the actual pixmap size.

use std::collections::HashMap;

use deckd_core::render::{render_blank, render_button};
use deckd_core::schema::{ButtonConfig, ButtonDefaults};

fn label_button(label: &str) -> ButtonConfig {
    ButtonConfig {
        key: 0,
        label: Some(label.to_string()),
        icon: None,
        background: Some("#000000".to_string()),
        text_color: Some("#FFFFFF".to_string()),
        font_size: None,
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
fn render_button_120px_has_120px_pixmap() {
    let pm = render_button(
        &label_button("X"),
        &ButtonDefaults::default(),
        &HashMap::new(),
        120,
        None,
    )
    .expect("render at 120 px");
    assert_eq!((pm.width(), pm.height()), (120, 120));
    assert_eq!(pm.data().len(), 120 * 120 * 4);
}

#[test]
fn render_blank_120px_has_120px_pixmap() {
    let pm = render_blank(120).expect("blank at 120 px");
    assert_eq!((pm.width(), pm.height()), (120, 120));
}

#[test]
fn render_text_centered_at_120() {
    let pm = render_button(
        &label_button("X"),
        &ButtonDefaults::default(),
        &HashMap::new(),
        120,
        None,
    )
    .expect("render at 120 px");

    // Bounding box of every pixel that is not the black background.
    let width = pm.width();
    let mut min = (u32::MAX, u32::MAX);
    let mut max = (0u32, 0u32);
    let mut lit = 0usize;
    for (i, px) in pm.pixels().iter().enumerate() {
        if px.red() == 0 && px.green() == 0 && px.blue() == 0 {
            continue;
        }
        let i = u32::try_from(i).expect("pixel index fits u32");
        let (x, y) = (i % width, i / width);
        min = (min.0.min(x), min.1.min(y));
        max = (max.0.max(x), max.1.max(y));
        lit += 1;
    }
    assert!(
        lit > 0,
        "label 'X' drew no pixels; the centre check would be vacuous"
    );

    // Centre of the box, doubled to stay in integers: (min + max) vs 2 * 60.
    let cx2 = i64::from(min.0 + max.0);
    let cy2 = i64::from(min.1 + max.1);
    assert!(
        (cx2 - 120).abs() <= 4 && (cy2 - 120).abs() <= 4,
        "'X' box {min:?}..{max:?} is not centred within 2 px of (60,60)"
    );
}
