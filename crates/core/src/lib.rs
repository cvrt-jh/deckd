//! Platform-neutral core of deckd: config schema, parsing, validation,
//! variable expansion, key layout and key rendering.
//!
//! This crate does no I/O. It has no filesystem, network, environment or
//! async runtime access, so the Stream Deck daemon and the touch-panel
//! firmware can share it unchanged. Callers read the config text, supply
//! variable values and load icons themselves. Fonts are embedded.

pub mod config;
pub mod error;
pub mod layout;
pub mod render;
pub mod schema;
pub mod view;

pub use config::{expand_vars, parse, validate};
pub use error::CoreError;
pub use layout::{Layout, Rect, Size, KEY_GAP_PX};
