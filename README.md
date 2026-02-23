# deckd

Headless Stream Deck daemon for Raspberry Pi. Config-driven, no GUI required.

Turns an Elgato Stream Deck MK.2 into a standalone control surface for Home Assistant, webhooks, and shell commands — no laptop needed.

## Features

- USB device auto-discovery and reconnection
- TOML configuration with **hot reload** (edit config, buttons update instantly)
- Button rendering: solid background + PNG icon + text label (72x72 per key)
- **Home Assistant integration** with live state-based button colors and **optimistic rendering**
- Multi-page navigation with page stack (push/pop/home)
- Actions: HTTP/webhook, shell commands, page navigation
- **11 embedded fonts** including JetBrains Mono Nerd Font (8 weights) with icon glyphs
- Environment variable expansion in config (`${HA_TOKEN}`)
- systemd service with udev rules for plug-and-play
- Structured JSON logging for journald

## Supported Hardware

| Device | Keys | Resolution | Status |
|--------|------|------------|--------|
| Elgato Stream Deck MK.2 | 15 LCD keys (3x5) | 72x72 per key | Supported |

**Target platform:** Raspberry Pi 3B+ or newer (64-bit OS, aarch64)

## Quick Start

```bash
# Build
cargo build --release

# Run
./target/release/deckd --config config.toml

# Run with JSON logging (for journald)
./target/release/deckd --config config.toml --json
```

## Configuration

See [config.example.toml](config.example.toml) for a full example.

### Minimal Config

```toml
[deckd]
brightness = 80

[deckd.defaults]
background = "#1a1a2e"
text_color = "#e0e0e0"
font_size = 14
font = "jb-regular"

[pages.home]
name = "Home"

[[pages.home.buttons]]
key = 0
label = "Deploy"
on_press = { action = "http", method = "POST", url = "https://example.com/webhook" }
```

### Button Layout (Stream Deck MK.2)

```
 0   1   2   3   4
 5   6   7   8   9
10  11  12  13  14
```

### Stateful Buttons (Home Assistant)

Buttons can reflect live HA entity state with automatic color swapping:

```toml
[[pages.home.buttons]]
key = 10
label = "XL"
background = "#000000"           # Color when OFF
text_color = "#FA6831"
on_background = "#FA6831"        # Color when ON
on_text_color = "#000000"
state_entity = "switch.printer"  # HA entity to track
font_size = 56
font = "jb-extrabold"
on_press = { action = "http", method = "POST", url = "http://homeassistant.local:8123/api/services/switch/toggle", headers = { "Authorization" = "Bearer ${HA_TOKEN}", "Content-Type" = "application/json" }, body = "{\"entity_id\": \"switch.printer\"}" }
```

**Optimistic rendering:** On button press, the button color flips instantly (~50ms) without waiting for the network. The daemon then syncs with the real HA state after 3 seconds. Background polling every 5 seconds keeps buttons in sync with external changes.

### Actions

| Action | Fields | Description |
|--------|--------|-------------|
| `http` | `method`, `url`, `headers`, `body` | HTTP request (GET/POST/PUT/DELETE/PATCH) |
| `shell` | `command` | Shell command via `/bin/sh -c` |
| `navigate` | `page` | Push a page onto the navigation stack |
| `back` | — | Pop the page stack |
| `home` | — | Reset to home page |

### Fonts

All fonts are embedded in the binary — no runtime font files needed.

| Config Value | Font | Best For |
|-------------|------|----------|
| `inter` | Inter Regular | Default, body text |
| `roboto-slab` | Roboto Slab Bold | Serif headings |
| `jb-thin` | JetBrains Mono NF Thin | Subtle labels |
| `jb-extralight` | JetBrains Mono NF ExtraLight | Light labels |
| `jb-light` | JetBrains Mono NF Light | Light labels |
| `jb-regular` | JetBrains Mono NF Regular | Monospace text |
| `jb-medium` | JetBrains Mono NF Medium | Medium emphasis |
| `jb-semibold` | JetBrains Mono NF SemiBold | Semi-bold labels |
| `jb-bold` | JetBrains Mono NF Bold | Bold labels |
| `jb-extrabold` | JetBrains Mono NF ExtraBold | Maximum impact |

JetBrains Mono Nerd Font includes **icon glyphs** (Nerd Font icons). Use Unicode escapes in TOML labels:

```toml
label = "\uF06C\nPlants"   # Leaf icon + "Plants" on second line
```

### Environment Variables

Config values support `${VAR}` expansion from the process environment:

```toml
headers = { "Authorization" = "Bearer ${HA_TOKEN}" }
```

Set them in the systemd service file:
```ini
Environment="HA_TOKEN=your-token-here"
```

### Icons

- Format: PNG, 72x72 recommended (auto-scaled to fit 48x48)
- Paths: relative to config directory or absolute
- When icon + label: icon on top, label at bottom (max 12px font)

## Raspberry Pi Deployment

### Prerequisites

```bash
# Install dependencies
sudo apt install -y libudev-dev libusb-1.0-0-dev libhidapi-dev

# Install Rust (if building on Pi)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

### Install

```bash
# Copy binary
sudo cp target/release/deckd /usr/local/bin/

# Create config directory
sudo mkdir -p /etc/deckd
sudo cp config.example.toml /etc/deckd/config.toml
# Edit config.toml with your buttons and HA token

# Set up udev rules (non-root USB access)
sudo cp udev/40-streamdeck.rules /etc/udev/rules.d/
sudo udevadm control --reload-rules

# Install and enable systemd service
sudo cp systemd/deckd.service /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable --now deckd
```

### Logs

```bash
# Follow live logs
journalctl -u deckd -f

# Last 50 lines
journalctl -u deckd -n 50 --no-pager
```

### Hot Reload

Edit `/etc/deckd/config.toml` — buttons update automatically within 500ms. No restart needed for:
- Adding/removing/changing buttons
- Changing colors, fonts, font sizes
- Changing button actions
- Adding/removing pages

A restart IS needed for:
- Adding new embedded fonts (requires rebuild)
- Changing brightness (reads on device connect)

## Architecture

```
Stream Deck MK.2 <--USB--> Raspberry Pi
                            |
                            deckd daemon (Rust, tokio)
                            ├── Device manager (USB HID polling)
                            ├── Config loader (TOML + hot reload)
                            ├── Render engine (tiny-skia + ab_glyph)
                            ├── Action executor (HTTP, shell, navigate)
                            ├── Page manager (stack-based navigation)
                            └── State poller (Home Assistant API)
```

All subsystems communicate via a **broadcast channel** (`DeckEvent` enum). Lock-free config via `ArcSwap`. Cooperative shutdown via `CancellationToken`.

## License

MIT OR Apache-2.0
