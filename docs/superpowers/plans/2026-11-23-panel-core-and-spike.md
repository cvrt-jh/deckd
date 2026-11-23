# deckd panel, plan 1: deckd-core + hardware spike - Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Extract deckd's config and key rendering into a shared `deckd-core` crate without changing deckd's behaviour, then prove the Waveshare ESP32-S3-Touch-LCD-4 can show a deckd-core key, report touches per key, and join `cvrt-iot`.

**Architecture:** Cargo workspace with `crates/core` (pure std library: schema, validation, var expansion, layout geometry, rendering into a `tiny_skia::Pixmap`) and the existing deckd daemon. `firmware/esp32-s3/` is a separate Cargo workspace on the Espressif `esp` toolchain that depends on core by path. Plan 2 (written after this spike) builds the full firmware: config fetch/cache, HA WebSocket, actions, OTA.

**Tech Stack:** Rust stable (deckd, core), `esp` toolchain via `espup`, `esp-idf-svc`, `espflash`, tiny-skia, ab_glyph, serde/toml, `num-traits`.

**Spec:** `docs/superpowers/specs/2026-11-23-esp32-panel-design.md`

## Global Constraints

- deckd behaviour on the Pi must not change: same config format, same rendered pixels.
- Configs without a `layout` key are 3x5 (`[deckd] layout = "3x5"` default); the panel uses `"4x4"`.
- deckd-core: no tokio, no `notify`, no filesystem, no network, no `std::env`.
- No `#[allow(...)]` / `#![allow(...)]` in new code (global rule). Float-to-int via `num_traits::ToPrimitive` (`to_u32()` etc.) or `try_from`, never `as` that clippy flags.
- `cargo clippy --all-targets -- -D warnings` and `cargo fmt --check` clean for the workspace.
- Cargo target dirs on `/Volumes/SANDISK-4TB/cargo-target/<project>` (`deckd`, `deckd-esp`) - hook 20.
- Secrets (Wi-Fi PSK) only from `~/Git/.env` via `with-env`, never printed, never committed, never in the TOML.
- No `Co-Authored-By` lines in commits. Only hyphen-minus dashes.
- Board: `/dev/cu.usbmodem103401`, native USB serial/JTAG `303a:1001`, powered by USB-C.

## Review Focus

1. The deployed Pi config (no `layout` key, keys 0-14) must parse and render byte-identically after the move - Task 2 golden test.
2. A touch in the gap between keys or outside the grid must trigger nothing - Task 3 `key_at` tests.
3. Touch axes rotated or mirrored against the display - Task 7 corner-tap check.
4. Wrong or missing Wi-Fi credentials in NVS must log a clear error and keep the screen alive, never a panic/reboot loop - Task 7 check with an empty NVS.
5. A `${VAR}` the lookup does not know stays literally in the text (deckd's current behaviour) - Task 3 test.

---

### Task 1: Merge `feat/sensor-value-buttons` into `main`

**Files:** none changed; git only.

- [ ] **Step 1:** Commit this plan (`docs: plan 1 for deckd-core and panel spike`), then `git push origin feat/sensor-value-buttons` (carries spec + plan).
- [ ] **Step 2:** `gh pr create --base main --head feat/sensor-value-buttons --title "feat: sensor value, threshold and folder-count buttons" --body "<summary of the 5 feature commits + spec/plan docs>"`.
- [ ] **Step 3:** Wait for CI (background `gh pr checks --watch`). Expected: Check job green.
- [ ] **Step 4:** `gh pr merge --merge --delete-branch`, then `git switch main && git pull && git switch -c feat/deckd-core`.

### Task 2: Golden render test on today's code (known-answer control)

Locks today's output before anything moves. Must be committed and green BEFORE Task 3.

**Files:**
- Create: `tests/render_golden.rs`
- Create: `tests/golden/example_config.sha256` (generated)

- [ ] **Step 1: Write the test.** `render_golden_matches_example_config`: load `config.example.toml` via `deckd::config::load`, render every button on every page (sorted by page id, then key) with `render_button(.., &states)` for three fixed state maps: empty; all referenced entities `"on"` and sensors `"23.4"`; all `"unavailable"`. Hash all bytes in order (sha256 via the `sha2` dev-dependency) and compare with the hex in `tests/golden/example_config.sha256`. If the file is missing and `DECKD_BLESS=1`, write it and pass; otherwise fail.
- [ ] **Step 2:** `DECKD_BLESS=1 cargo test --test render_golden` creates the file; `cargo test --test render_golden` PASS.
- [ ] **Step 3: Prove it can go red.** Change one threshold color in a temp copy path the test reads via `DECKD_GOLDEN_CONFIG` env override → FAIL; remove override → PASS.
- [ ] **Step 4:** Commit `test: golden hash of rendered example-config keys`.

### Task 3: `deckd-core` crate: schema, validation, expansion, layout

**Files:**
- Modify: `Cargo.toml` (add `[workspace] members = [".", "crates/core"]`, `exclude = ["firmware"]`)
- Create: `crates/core/Cargo.toml` (name `deckd-core`, deps serde, toml, thiserror, num-traits)
- Create: `crates/core/src/lib.rs`, `crates/core/src/error.rs`, `crates/core/src/schema.rs` (moved from `src/config/schema.rs`), `crates/core/src/config.rs`, `crates/core/src/layout.rs`
- Modify: `src/config/mod.rs` (wraps core), `src/config/schema.rs` (becomes `pub use deckd_core::schema::*;`), `src/error.rs` (`#[from] deckd_core::CoreError`)

**Interfaces (Produces):**
- `pub enum Layout { Grid3x5, Grid4x4 }` serde `"3x5"`/`"4x4"`, `#[default] Grid3x5`; field `DeckdConfig::layout: Layout`.
- `impl Layout { pub const fn rows(self) -> u8; pub const fn cols(self) -> u8; pub const fn key_count(self) -> u8; pub fn key_rect(self, index: u8, screen: Size) -> Option<Rect>; pub fn key_at(self, x: u16, y: u16, screen: Size) -> Option<u8> }`
- `pub struct Size { pub w: u16, pub h: u16 }`, `pub struct Rect { pub x: u16, pub y: u16, pub w: u16, pub h: u16 }`. Keys are square cells `screen.w / cols` wide with a fixed `KEY_GAP_PX: u16 = 6` inset (3 px each side); touches in the gap return `None`.
- `pub fn expand_vars(input: &str, lookup: impl Fn(&str) -> Option<String>) -> String` (same `${VAR}`/`$VAR` rules as today; unknown kept literally).
- `pub fn parse(toml_text: &str) -> Result<AppConfig, CoreError>` (parse + `validate`, no expansion).
- `pub fn validate(cfg: &AppConfig) -> Result<(), CoreError>`: brightness <= 100; every key < `layout.key_count()`.
- `pub enum CoreError { Toml(toml::de::Error), Config(String), Render(String), Font(String) }`.
- deckd: `config::load(path)` = read file → `expand_vars(&s, |k| std::env::var(k).ok())` → `deckd_core::config::parse`.

- [ ] **Step 1: Failing tests** in `crates/core/src/{layout,config}.rs`:
  - `default_layout_is_3x5` (`[deckd]` without `layout` parses to `Grid3x5`).
  - `grid_4x4_accepts_key_15_rejects_16`; `grid_3x5_rejects_key_15` (error text contains `out of range (0-14)`).
  - `key_at_4x4_480`: screen 480x480 → `(60,60)=Some(0)`, `(420,420)=Some(15)`, `(179,60)=Some(1)` boundary checks, `(120,60)=None` (gap), `(480,10)=None` (off-screen).
  - `key_rect_4x4_480_index_5 == Rect{x:123,y:123,w:114,h:114}`.
  - `expand_vars_uses_lookup` (`${A}` → `x`), `expand_vars_unknown_kept` (`${NOPE}` stays), `expand_vars_bare_dollar` (`$` alone stays).
- [ ] **Step 2:** `cargo test -p deckd-core` → FAIL (not defined).
- [ ] **Step 3:** Implement; move schema; existing deckd config tests move to core where they only touch parsing (env-reading test stays in deckd).
- [ ] **Step 4:** `cargo test -p deckd-core && cargo test -p deckd` PASS, golden test unchanged PASS.
- [ ] **Step 5:** Commit `refactor: deckd-core crate with schema, validation and grid layout`.

### Task 4: Move rendering into `deckd-core`

**Files:**
- Move: `assets/fonts/*` → `crates/core/assets/fonts/` (update `include_bytes!` paths; `README` font section)
- Create: `crates/core/src/render/{mod,canvas,text}.rs` (from `src/render/`), `crates/core/src/view.rs` (pure helpers `format_value`, `threshold_color`, `count_active`, `resolve_label`, `is_unknown_state` + their tests)
- Modify: `src/render/mod.rs` (thin wrapper: loads icon from disk, calls core, returns `Vec<u8>`), keep `src/render/icon.rs` in deckd (uses `image` + fs)

**Interfaces (Produces):**
- `pub fn render_button(button: &ButtonConfig, defaults: &ButtonDefaults, states: &HashMap<String, String>, key_px: u32, icon: Option<&Pixmap>) -> Result<Pixmap, CoreError>`
- `pub fn render_blank(key_px: u32) -> Result<Pixmap, CoreError>`
- `BUTTON_SIZE` disappears from core; text layout uses `pixmap.width()`. deckd passes `72`.
- Icons: core takes an already-loaded, already-sized `Pixmap`; deckd's `icon.rs` keeps loading and scaling.

- [ ] **Step 1:** Move the 17 render tests with the code; add `render_button_120px_has_120px_pixmap` and `render_text_centered_at_120` (non-background pixels' bounding box center within 2 px of (60,60) for label `"X"`).
- [ ] **Step 2:** `cargo test -p deckd-core` FAIL until moved, then implement the move.
- [ ] **Step 3:** `cargo test -p deckd-core && cargo test -p deckd` PASS. Golden test PASS **without re-blessing** - a changed hash means behaviour changed; fix the code, never the hash.
- [ ] **Step 4:** Remove the crate-level `#![allow(...)]` in `src/lib.rs`: fix each flagged cast with `ToPrimitive`/`try_from`. `cargo clippy --workspace --all-targets -- -D warnings` clean.
- [ ] **Step 5:** Commit `refactor: move key rendering and fonts into deckd-core`.

### Task 5: CI + docs for the workspace, merge point

**Files:** `.github/workflows/ci.yml`, `README.md`, `CLAUDE.md`, `CHANGELOG.md`

- [ ] **Step 1:** CI runs `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace` (firmware excluded by the workspace `exclude`).
- [ ] **Step 2:** `CLAUDE.md` structure section: `crates/core/`, `firmware/` (coming), font path; `CHANGELOG` "Unreleased: deckd-core".
- [ ] **Step 3:** Merge point: `full-suite --reason "merge: deckd-core" -- cargo test --workspace` PASS; `deploy/build-pi.sh` builds; `--check` the deployed homelab config with the new binary locally (`target/.../deckd --check --config ~/Git/cvrt-jh/tech/homelab/configs/deckd/config.toml`, `HA_TOKEN` via `with-env`) exits 0.
- [ ] **Step 4:** Commit, push, PR `feat/deckd-core` → main, CI green, merge. Pi deploy is NOT part of this plan (no behaviour change; next regular deploy picks it up).

### Task 6: Espressif toolchain on the Mac

**Files:** home memory `cli-tools.md` (record the install), deckd `CLAUDE.md` (firmware commands).

- [ ] **Step 1:** Read home memory `cli-tools.md` and `techstack.md` first.
- [ ] **Step 2:** `cargo install espup espflash ldproxy --locked`; `espup install --targets esp32s3` (writes `~/export-esp.sh`).
- [ ] **Step 3:** Verify: `espflash board-info --port /dev/cu.usbmodem103401` prints `Chip type: esp32s3`, flash `16MB`, PSRAM present. Record the output in deckd memory.
- [ ] **Step 4:** Add `CVRT_IOT_WIFI_SSID` / `CVRT_IOT_WIFI_PSK` to `~/Git/.env` from 1Password item `cvrt-iot` (vault Homelab, `/1pw` skill) if missing; update `credentials.md` per its onboarding steps.

### Task 7: Firmware spike: screen, touch, Wi-Fi

Proves the hardware stack. Minimal tests (spike), but it lives in the repo as the base for plan 2.

**Files:**
- Create: `firmware/esp32-s3/` from `cargo generate esp-rs/esp-idf-template` (std, esp32s3, ESP-IDF v5.x), `rust-toolchain.toml` (`channel = "esp"`), `sdkconfig.defaults` (PSRAM octal on, 16 MB flash, custom partition table with `nvs`, `otadata`, `ota_0`, `ota_1`)
- Create: `firmware/esp32-s3/src/{main,display,touch,wifi}.rs`, `firmware/esp32-s3/tools/provision.sh`
- Dependency: `deckd-core = { path = "../../crates/core" }`

**Interfaces (Produces, for plan 2):**
- `display::Display::new(peripherals...) -> anyhow::Result<Display>`; `Display::draw_key(&mut self, rect: Rect, rgba: &[u8]) -> anyhow::Result<()>` (RGBA8888 → RGB565); `Display::set_brightness(&mut self, pct: u8)`.
- `touch::Touch::poll(&mut self) -> anyhow::Result<Option<(u16, u16)>>` in screen coordinates (rotation/mirroring fixed here).
- `wifi::connect(nvs) -> anyhow::Result<EspWifi>` reading NVS namespace `deckpanel` keys `wifi_ssid`, `wifi_psk`.
- Pin map and ST7701S init sequence: from the Waveshare wiki/demo for ESP32-S3-Touch-LCD-4 (RGB panel, GT911 on I2C, IO expander drives LCD reset/CS and backlight), recorded as named constants in `display.rs`/`touch.rs` with a source link comment.

- [ ] **Step 1:** Build: `. ~/export-esp.sh && CARGO_TARGET_DIR=/Volumes/SANDISK-4TB/cargo-target/deckd-esp cargo build --release` in `firmware/esp32-s3` (this also proves deckd-core compiles for `xtensa-esp32s3-espidf`).
- [ ] **Step 2: Screen.** `main` renders a key with `deckd_core::render::render_button` (label `"deckd\npanel"`, bg `#FFC107`, text `#000000`, `jb-bold`, 22) at 120 px into every `layout.key_rect(i)` of `Grid4x4`, label = index. Flash `espflash flash --monitor --port /dev/cu.usbmodem103401`. Expected: Jay sees 16 amber keys numbered 0-15, row-major from top-left.
- [ ] **Step 3: Touch.** Log `key_at` for each touch. Jay taps the four corner keys; expected log `key 0`, `key 3`, `key 12`, `key 15`; a tap on a gap logs `no key`. If axes are swapped/mirrored, fix in `touch.rs`, re-run.
- [ ] **Step 4: Wi-Fi, error path.** Flash with empty NVS. Expected: log `wifi: no credentials in NVS (deckpanel/wifi_ssid)`, screen stays up, no reboot loop.
- [ ] **Step 5: Provision.** `tools/provision.sh /dev/cu.usbmodem103401 jh-desk`: generates the NVS CSV in `$TMPDIR` from `with-env CVRT_IOT_WIFI_SSID CVRT_IOT_WIFI_PSK`, builds the partition with ESP-IDF's `nvs_partition_gen.py`, writes it with `espflash write-bin <nvs offset>`, deletes the CSV and bin. Never echoes the PSK. Expected log: `wifi: connected, ip 10.0.9.x`, MAC printed.
- [ ] **Step 6: DHCP reservation.** In UniFi (read `~/Git/cvrt-jh/tech/unifi/CLAUDE.md` first) reserve a VLAN 9 IP for the panel's MAC, name `deckpanel-jh-desk`; reboot the board, confirm it gets that IP. Add the row to homelab `CLAUDE.md` network table and homelab memory `network-and-dns`.
- [ ] **Step 7:** Commit `feat(firmware): ESP32-S3 panel spike (display, touch, wifi, provisioning)`; push; PR; merge after CI.
- [ ] **Step 8:** Write findings (driver choice, pin map, touch orientation, measured current draw from a USB power meter if available) into deckd memory, then write plan 2 from the spec.
