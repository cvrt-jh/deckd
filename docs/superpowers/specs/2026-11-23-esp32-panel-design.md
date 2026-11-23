# deckd panel: standalone ESP32-S3 touch panel - design

Date: 2026-11-23. Status: draft, awaiting Jay's approval.

## Intent

A second deckd "in another place": same purpose and look as the Pi + Stream Deck
setup, but on a Waveshare ESP32-S3-Touch-LCD-4 (4", 480x480 touch). It is a
standalone device: it talks to Home Assistant itself and does not depend on the
deckd Pi. If the first panel works, more panels follow, so the design must scale
to N panels without per-panel firmware builds.

## Goals

- 4x4 grid of touch keys (16 keys, 120x120 px each) on a 480x480 screen.
- Same TOML config format as deckd for the shared features, same palette rules
  (`configs/deckd/PALETTE.md`), same rendering (fonts, centering, colors).
- Shared features on the panel: `label`, `background`/`text_color`, fonts,
  `state_entity` + `on_*` colors, `label_entity` + `precision` + `unknown_text`
  + `thresholds`, `count_entities` + `{count}/{total}` + `active_*` colors,
  actions `http`, `navigate`, `back`, `home`; optimistic rendering on press.
- Live entity state pushed from HA (WebSocket subscription), not polled.
- Config fetched from HA, changed without reflashing (option A, Jay's pick).
- Firmware updates over Wi-Fi (OTA) after the first USB flash.

## Non-goals

- No remote-screen mode (panel as a dumb display for the Pi). Jay ruled it out.
- No `shell` action on the panel (no shell); a config using it fails the check.
- No PNG `icon` support in v1 (the deployed config uses none).
- No on-device config editor, no web UI on the panel.
- No change to deckd's behaviour on the Pi.

## Anti-goals

- deckd and the panel drift apart: two config dialects, two renderers. The
  shared crate exists to prevent this.
- A secret in the repo or in the public `/local/` config file.
- The Pi build or CI needing the Espressif toolchain.

## Constraints

- Hardware: ESP32-S3R8 (Xtensa, 8 MB PSRAM, 16 MB flash), ST7701S RGB panel,
  GT911 capacitive touch on I2C, IO expander for backlight/reset (exact parts and
  pins from the Waveshare wiki/schematic). Power: USB-C, 7-36 V DC terminal, or a
  3.7 V LiPo. Waveshare FAQ: on the wide-voltage input, I2C devices may not be
  detected (SW6106 issue) - test touch on DC power early.
- Rust on Xtensa needs the `esp` toolchain (`espup`), not stable.
- Network: IoT VLAN 9 (`cvrt-iot`), HA at 10.0.9.11:8123.
- HA has no scoped tokens. Each panel gets its own non-admin HA user + token.
- deckd's `feat/sensor-value-buttons` branch (5 commits not on `main`) carries
  `label_entity`/`thresholds`/`count_entities`; it is merged to `main` before the
  refactor starts.
- Project rules: tests for every change, strict clippy, no workarounds; a
  threat model file before credential work (global rule 4).

## Approaches considered

1. **Remote screen for deckd** - Pi renders, panel displays. One brain, minimal
   firmware, but dead when the Pi is down. Rejected: Jay wants standalone.
2. **Standalone firmware + shared `deckd-core` crate, config fetched from HA**
   (chosen). Works without the Pi, one firmware for N panels, button changes are
   a file edit. Costs a config fetch/cache path and a shared-crate refactor.
3. **Standalone, config compiled in.** Simplest firmware, but every button change
   is a build + OTA per panel. Rejected (option B): scales badly to more panels.

## Design

### Repo layout (in the deckd repo)

```
oss/deckd/
├── Cargo.toml              # workspace: crates/core + the deckd daemon (stable)
├── crates/core/            # deckd-core
├── src/ ...                # Pi daemon, uses deckd-core
└── firmware/esp32-s3/      # own workspace + rust-toolchain.toml (channel "esp"),
                            # path dependency on ../../crates/core
```

### deckd-core

Pure library, compiles on stable (host) and `esp` (Xtensa). No tokio, no
`notify`, no filesystem, no network.

- Config schema (serde) and validation (`check(&str) -> Result<AppConfig, ..>`),
  including grid size: 3x5 (Stream Deck MK.2) or 4x4 (panel), declared in
  `[deckd]` (e.g. `layout = "4x4"`, default `"3x5"` so existing configs parse
  unchanged). Keys outside the grid are a check error. `shell` is accepted by the
  schema; the panel's check rejects it.
- `${VAR}` expansion takes a lookup function instead of reading the process env.
- Button view-model: given config + entity states + pending optimistic flips,
  produce what a key shows (label text, colors, font, size).
- Rendering: tiny-skia + ab_glyph into an RGBA pixel buffer of a given key size,
  embedded fonts. deckd keeps 72x72, the panel uses 120x120. Font sizes in the
  config are absolute pixels for that device (no scaling).

deckd's existing tests plus `tests/config_consistency.rs` must pass unchanged
after the move; new core tests cover the 4x4 layout and the env lookup.

### Firmware (`firmware/esp32-s3/`)

- `esp-idf-svc` (std). Display through ESP-IDF's RGB LCD driver (ST7701S init
  sequence from Waveshare), touch via GT911 over I2C.
- Boot: Wi-Fi -> load cached config from flash -> render -> fetch fresh config ->
  connect HA WebSocket -> subscribe to the entities the config references.
- **Config source:** `http://<ha>/local/deckpanel/<panel-name>.toml` (HA `www/`
  folder). Fetched at boot and when the user long-presses a reserved corner (or a
  config-defined `reload` key; builder's call). Parsed with `deckd-core`; on
  success it replaces the flash cache, on failure the panel keeps the cached
  config and shows an error banner. The file holds no secrets: anything on VLAN 9
  can read `/local/`.
- **Device settings in NVS** (never in the repo or the TOML): panel name, Wi-Fi
  SSID/PSK, HA base URL, HA token. Written at first flash from 1Password
  (`[api] homeassistant-deckpanel-<name>`, vault Homelab) by a provisioning step.
  `${HA_TOKEN}` in action headers resolves from NVS.
- **State:** HA WebSocket API, `subscribe_entities` filtered to referenced
  entities; reconnect with backoff. While disconnected, every key holds its last
  known value, same as deckd (deckd fixed the "every key flicks to off on a short
  HA stall" bug this way; claude's call). `unknown_text` only shows for a
  `label_entity` that has never had a value. A small status dot (corner) shows
  that HA is disconnected.
- **Token scope (claude's call):** `${HA_TOKEN}` is only expanded for action URLs
  whose origin matches the HA base URL in NVS; any other origin gets no token and
  fails the panel's config check. This stops a spoofed `/local/` config from
  sending the token elsewhere. Transport stays plain HTTP on VLAN 9, the same as
  the Pi today (TLS would need HA itself to change; revisit if HA moves to TLS).
- **Actions:** `http` via ESP-IDF HTTP client (same URL/method/headers/body
  semantics as deckd), `navigate`/`back`/`home` locally. Optimistic flip on press,
  held 3 s like deckd.
- **Touch:** tap = `on_press`. Debounce; ignore touches that land between keys.
- **OTA:** ESP-IDF OTA with two app partitions; the firmware image is pushed from
  the Mac (mechanism is the builder's call). First flash over USB-C (`espflash`).
- **Screen:** brightness from `[deckd] brightness`; dim/off after an idle timeout
  (builder's call on default) and wake on touch.

### Docs and repo hygiene

- deckd `README`, `CLAUDE.md`, memory: new layout, the panel, its build/flash/OTA
  and provisioning commands.
- Homelab: `configs/deckpanel/<name>.toml` tracked in the homelab repo, deployed
  to HA `www/deckpanel/`; network table gets the panel's DHCP reservation.

### Physical security (Jay's decision)

No flash encryption, no secure boot, no NVS encryption. The panels are
internal-only; worst case is someone reading the IoT Wi-Fi PSK and a non-admin HA
token, both rotatable. No eFuses are burned, plain USB reflashing stays possible.

### First panel: `jh-desk`

- Name `jh-desk` (Jay's decision): HA user `deckpanel-jh-desk`, 1Password
  `[api] homeassistant-deckpanel-jh-desk` (vault Homelab), config
  `/local/deckpanel/jh-desk.toml`, DHCP reservation on VLAN 9.
- Power: USB-C from the desk (claude's call: the Mac or a USB charger is right
  there; the 7-36 V terminal and its SW6106 I2C issue are out of scope for v1).
- Keys: for testing, a copy of every button on every deckd page (home, plants and
  any other page in `configs/deckd/config.toml`), Jay's decision. The Stream Deck
  has 5 columns and the panel 4, so positions cannot be kept: keys keep their
  index 0-14 and flow row by row on the 4x4 grid, key 15 stays empty (claude's
  call). Font sizes get re-tuned for 120 px keys.
- Network: Wi-Fi `cvrt-iot` (VLAN 9), PSK from 1Password item `cvrt-iot`, DHCP
  reservation in UniFi, added to the homelab network table (Jay's decision).

## Delivery order

1. Merge `feat/sensor-value-buttons` into `main`.
2. Extract `deckd-core`; deckd unchanged in behaviour (its tests prove it).
3. Spike: firmware lights the screen and reads touch (validates the display/touch
   stack on this board before anything else is built on it).
4. Firmware: core rendering on device, config fetch/cache, WebSocket state,
   actions, OTA, provisioning.
5. Threat model file before step 4's credential work.

## Left to the builder

- Exact crates for the display/touch drivers and whether to use LVGL-free direct
  framebuffer drawing (expected: direct, since deckd-core renders keys itself).
- Reload gesture, idle timeout default, OTA push tooling, NVS provisioning tool.
- Whether optimistic-render and page-stack logic live in deckd-core (preferred if
  they fit without async) or are duplicated small.
