# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- `deckd-core` crate (`crates/core`): config schema, parsing, validation and env
  expansion, layout grids (3x5 and 4x4), and key rendering (tiny-skia, ab_glyph,
  embedded fonts) extracted from the daemon into a no-I/O library; the repo is
  now a Cargo workspace
- CI runs fmt, clippy and tests across the whole workspace
- Sensor value buttons: `label_entity` substitutes a live Home Assistant value
  into a `{value}` placeholder in the label, with `precision` for rounding and
  `unknown_text` for unavailable values
- Threshold colors: `thresholds` defines `{ min, color }` bands that override
  `text_color` based on the numeric value. The highest matching band wins, so
  the order in the config does not matter
- Folder active counts: `count_entities` populates `{count}` and `{total}`
  placeholders on a folder button, with `active_text_color` and
  `active_background` applied while any counted device is on
- Config consistency test rejecting a stateful button whose `on_press` acts on a
  different entity than its `state_entity` displays
- `deploy/build-pi.sh` builds in a Bookworm arm64 container so the binary's glibc
  matches the Pi, and verifies architecture and glibc before producing an
  artifact

### Fixed

- Buttons no longer drop to their off state when a Home Assistant fetch fails or
  times out. Previously a missing state was indistinguishable from "off", so a
  brief HA or DNS stall made every stateful button flick off and back on the
  next 5 second poll. Last known values are now held, and the number of held
  entities is logged
- The 5 second state poll now also triggers for buttons that only use
  `label_entity` or `count_entities`, which previously never refreshed
- `handle_event` no longer takes a redundant second broadcast sender; `tx` and
  `event_tx` were clones of the same channel

## [0.1.0] - 2025-02-22

### Added

- USB device auto-discovery and reconnection for Elgato Stream Deck MK.2
- TOML configuration with serde parsing and validation
- Environment variable expansion (`${VAR}`) in config values
- Button rendering with solid background, PNG icons, and text labels (72x72, tiny-skia)
- Multi-page navigation with page stack (navigate, back, home)
- HTTP/webhook action support (GET, POST, PUT, DELETE, PATCH)
- Shell command action support
- Hot config reload via filesystem watcher (notify, debounced)
- Broadcast channel event system connecting all subsystems
- CLI with clap (--config, --check, --json flags)
- Structured logging with tracing (text and JSON output)
- systemd service unit with hardening
- udev rules for non-root USB access
- Inter font embedded for text rendering
