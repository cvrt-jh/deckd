#![allow(
    clippy::module_name_repetitions,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::ignored_unit_patterns
)]

pub mod action;
pub mod config;
pub mod daemon;
pub mod device;
pub mod error;
pub mod event;
pub mod page;
pub mod render;
pub mod state;
