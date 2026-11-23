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

**Holding state:** If an HA fetch fails or times out, the last known value is held rather than rendered as "off". Without this, a brief HA or DNS stall makes every stateful button flick to its off state and back on the next poll.

### Sensor Value Buttons

A button can render a live sensor reading instead of a static label. Set `label_entity` and put a `{value}` placeholder in the `label`:

```toml
[[pages.home.buttons]]
key = 5
label = "Keller\n{value}°"
label_entity = "sensor.keller_temperature"
precision = 1                    # Decimal places (default 0)
unknown_text = "--"              # Shown when the value is unavailable
background = "#000000"
text_color = "#FFFFFF"

# Office comfort bands: blue below 21, white 21-25, red above 25.
# The highest band whose `min` is <= the value wins; order does not matter.
thresholds = [
  { min = -100.0, color = "#25AFF4" },
  { min = 21.0,   color = "#FFFFFF" },
  { min = 25.0,   color = "#E02B1D" },
]
```

| Field | Description |
|-------|-------------|
| `label_entity` | HA entity whose state replaces `{value}` in the label |
| `precision` | Decimal places for numeric values (default 0) |
| `unknown_text` | Replacement for `{value}` when unknown/unavailable (default `--`) |
| `thresholds` | Numeric bands overriding `text_color`; each is `{ min, color }` |

**Picking colors:** a deck is read at a glance, so color carries meaning. Keep one hue per device category, give idle states a darker variant of the same hue rather than a different hue, and keep saturated colors in a consistent lightness band so they match in apparent brightness on black. Do not paste a Material or Tailwind default from memory: those are tuned for white backgrounds and come out too light. If a deployment keeps a documented palette, take the color from there instead of inventing one.

Non-numeric states (`unavailable`, `unknown`) fall back to `unknown_text` and never trigger a threshold color. `label_entity` and `state_entity` can be combined on one button, and both are polled on the same 5 second interval.

### Folders (Sub-pages with an Active Count)

A folder is a button that navigates to a sub-page. Add `count_entities` to show how many devices inside are currently on, and `active_*` colors to light the folder up while any of them is active:

```toml
[[pages.home.buttons]]
key = 4
label = "\n{count}/{total}"
text_color = "#2E5233"           # Dimmed when everything is off
active_text_color = "#4CAF50"    # Lit while any device is on
count_entities = [
  "switch.plug_a",
  "switch.plug_b",
  "switch.plug_c",
]
on_press = { action = "navigate", page = "plants" }

[pages.plants]
name = "Plants"

# ... one stateful button per device ...

[[pages.plants.buttons]]
key = 14
label = "\nBack"
background = "#4CAF50"       # the folder's own colour, as background
text_color = "#000000"
on_press = { action = "back" }
```

Giving Back the folder's colour as its **background** (rather than its text colour) makes it read as an activated key: being inside the folder is the active state, so it matches the filled look every toggle uses for "on". Use the folder's steady category colour and do not vary it with the devices inside, since Back indicates location rather than status.

| Field | Description |
|-------|-------------|
| `count_entities` | Entities counted for `{count}`; `{total}` is how many are listed |
| `active_text_color` | Text color while `{count}` > 0 |
| `active_background` | Background color while `{count}` > 0 |

`{count}` and `{total}` work independently of `label_entity`, so a folder needs no value entity of its own. Navigating back re-fetches state, so the badge is current as soon as the home page reappears. Page nesting is arbitrary depth: `back` pops one level, `home` resets to the top.

### Actions

| Action | Fields | Description |
|--------|--------|-------------|
| `http` | `method`, `url`, `headers`, `body` | HTTP request (GET/POST/PUT/DELETE/PATCH) |
| `shell` | `command` | Shell command via `/bin/sh -c` |
| `navigate` | `page` | Push a page onto the navigation stack |
| `back` | — | Pop the page stack |
| `home` | — | Reset to home page |

### Fonts

All fonts are embedded in the binary from `crates/core/assets/fonts/`, so no runtime font files are needed.

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

### Building for the Pi

```bash
deploy/build-pi.sh          # artifact at target/pi/deckd
```

This builds inside a `debian:bookworm-slim` arm64 container, matching the Pi's glibc exactly, and verifies the architecture and glibc requirement before producing an artifact.

Two traps it avoids, both of which produce a binary that only fails at service start:

- **`cross` cannot be used from an arm64 host.** Its images are published for linux/amd64 only, so on an Apple Silicon Mac every tag fails with `no matching manifest for linux/arm64/v8`. `Cross.toml` is retained for x86_64 CI.
- **glibc must not be newer than the target's.** The Pi (Debian Bookworm) provides glibc 2.36. A binary built against 2.39 fails with `/lib/aarch64-linux-gnu/libc.so.6: version 'GLIBC_2.39' not found`. Check any binary with `objdump -T deckd | grep -oE 'GLIBC_2\.[0-9]+' | sort -uV | tail -1`.

Always validate on the target before restarting the service. `--check` exits non-zero on a bad config and also proves the binary can run at all:

```bash
scp target/pi/deckd pi:/tmp/deckd.new
scp config.toml pi:/tmp/config.toml.new
ssh pi '/tmp/deckd.new --check --config /tmp/config.toml.new'
```

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
