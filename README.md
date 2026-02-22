# deckd

Headless Stream Deck daemon for Raspberry Pi. Config-driven, no GUI required.

## Features

- USB device auto-discovery and reconnection
- TOML configuration with hot reload
- Button rendering: solid background + PNG icon + text label (72x72)
- Multi-page navigation with page stack
- Actions: HTTP/webhook, shell commands, page navigation
- systemd service with udev rules for plug-and-play
- Structured logging (JSON for journald)

## Supported Hardware

- Elgato Stream Deck MK.2 (15 LCD keys, 72x72 per key)
- Target platform: Raspberry Pi (64-bit OS, aarch64)

## Quick Start

```bash
# Build
cargo build --release

# Validate config
./target/release/deckd --config config.example.toml --check

# Run
./target/release/deckd --config config.example.toml
```

## Configuration

See [config.example.toml](config.example.toml) for a full example.

```toml
[deckd]
brightness = 80

[deckd.defaults]
background = "#1a1a2e"
text_color = "#e0e0e0"
font_size = 14

[pages.home]
name = "Home"

[[pages.home.buttons]]
key = 0
label = "Deploy"
icon = "icons/rocket.png"
on_press = { action = "http", method = "POST", url = "https://example.com/webhook" }
```

### Actions

| Action | Fields | Description |
|--------|--------|-------------|
| `http` | `method`, `url`, `headers`, `body` | HTTP request |
| `shell` | `command` | Shell command via `/bin/sh -c` |
| `navigate` | `page` | Push a page onto the stack |
| `back` | — | Pop the page stack |
| `home` | — | Reset to home page |

### Environment Variables

Config values support `${VAR}` expansion:

```toml
url = "http://localhost:8123/api/services/light/toggle"
headers = { "Authorization" = "Bearer ${HA_TOKEN}" }
```

## Raspberry Pi Deployment

### Prerequisites

- Raspberry Pi OS (64-bit, Bookworm)
- Elgato Stream Deck connected via USB

### Install

```bash
# Copy binary
sudo cp target/aarch64-unknown-linux-gnu/release/deckd /usr/local/bin/

# Create config directory
sudo mkdir -p /etc/deckd
sudo cp config.example.toml /etc/deckd/config.toml

# Set up udev rules (allows non-root USB access)
sudo cp udev/40-streamdeck.rules /etc/udev/rules.d/
sudo udevadm control --reload-rules

# Create service user
sudo useradd -r -s /usr/sbin/nologin -G plugdev deckd

# Install systemd service
sudo cp systemd/deckd.service /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable --now deckd
```

### Logs

```bash
journalctl -u deckd -f
```

## Cross-Compilation

Build for Raspberry Pi from macOS/Linux:

```bash
# Install cross
cargo install cross

# Build
cross build --target aarch64-unknown-linux-gnu --release
```

## License

MIT OR Apache-2.0
