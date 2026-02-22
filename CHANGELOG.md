# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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
