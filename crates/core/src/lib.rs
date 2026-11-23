//! Platform-neutral core of deckd: config schema, parsing, validation,
//! variable expansion and key layout.
//!
//! This crate does no I/O. It has no filesystem, network, environment or
//! async runtime access, so the Stream Deck daemon and the touch-panel
//! firmware can share it unchanged. Callers read the config text and supply
//! variable values themselves.

pub mod config;
pub mod error;
pub mod layout;
pub mod schema;

pub use config::{expand_vars, parse, validate};
pub use error::CoreError;
pub use layout::{Layout, Rect, Size, KEY_GAP_PX};
