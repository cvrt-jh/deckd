# deckd panel firmware (ESP32-S3-Touch-LCD-4)

Firmware for the Waveshare ESP32-S3-Touch-LCD-4 (480x480 ST7701S RGB panel, GT911 touch, CH32V003 helper, 16 MB flash, 8 MB octal PSRAM). It renders keys with `deckd-core` on a 4x4 grid. Status: spike (screen, touch, Wi-Fi).

Pins, the ST7701S init sequence and the helper protocol come from Waveshare's official BSP `waveshare/esp32_s3_touch_lcd_4` 3.0.0 (demo repo <https://github.com/waveshareteam/ESP32-S3-Touch-LCD-4>); each constant carries its source in `src/board.rs`, `src/display.rs`, `src/touch.rs`.

## Toolchain

The Xtensa Rust toolchain is pinned to **1.93.0.0** (`rust-toolchain.toml`, channel `esp-1.93`):

- 1.94.0.2 to 1.98.0.0 (LLVM 21) abort compiling `tiny-skia` with `Cannot select: XtensaISD::PCREL_WRAPPER` (esp-rs/rust#277, #282; espressif/llvm-project#127).
- 1.98.1.0 and later fix that, but silently miscompile bytes of a static's address (esp-rs/rust#283, open).

Move to the plain `esp` channel once #283 is fixed. Install the pinned toolchain next to any other:

```sh
espup install --toolchain-version 1.93.0.0 --name esp-1.93 --std --targets esp32s3 \
  --export-file ~/export-esp-1.93.sh
```

ESP-IDF v5.5.3 and its tools are fetched by `esp-idf-sys` into `~/.espressif` on the first build (about 4 GB, about 11 minutes).

## Build

```sh
cd firmware/esp32-s3
. ~/export-esp-1.93.sh
CARGO_TARGET_DIR=/path/to/target cargo build --release
CARGO_TARGET_DIR=/path/to/target cargo clippy --release
```

`.cargo/config.toml` sets `CARGO_WORKSPACE_DIR`, so `sdkconfig.defaults` is found even with a moved target dir (without it the defaults are silently ignored: no PSRAM, 160 MHz).

Only the fonts the panel uses are embedded (`deckd-core` features `font-jb-bold`, `font-jb-extrabold`); all weights would not fit the flash.

## Flash

```sh
ESPFLASH_PORT=/dev/cu.usbmodemXXXX CARGO_TARGET_DIR=/path/to/target cargo run --release
# or, for a built ELF, with extra espflash args:
ESPFLASH_PORT=/dev/cu.usbmodemXXXX tools/flash.sh \
  $CARGO_TARGET_DIR/xtensa-esp32s3-espidf/release/deckd-panel --erase-parts nvs
```

`tools/flash.sh` (the cargo runner) runs `espflash flash --monitor --chip esp32s3 --bootloader <bootloader.bin> --partition-table partitions.csv --flash-size 16mb`. The bootloader is the one esp-idf-sys built from `sdkconfig.defaults` (in its hashed build dir, which a static runner line cannot name); espflash's bundled one boots in DIO at 40 MHz. `--erase-parts nvs` wipes the device settings.

Partitions (`partitions.csv`): `nvs` 0x9000 (24 KB), `otadata` 0xf000, `ota_0` 0x20000 and `ota_1` 0x810000, 0x7F0000 (7.94 MiB) each.

## Provision

Writes namespace `deckpanel` (`wifi_ssid`, `wifi_psk`, `panel_name`) into the `nvs` partition. Reads `CVRT_IOT_WIFI_SSID` / `CVRT_IOT_WIFI_PSK` from the environment (re-runs itself under `with-env` when they are unset); the PSK is never printed.

```sh
tools/provision.sh /dev/cu.usbmodemXXXX jh-desk
tools/provision.sh --dry-run /dev/cu.usbmodemXXXX jh-desk   # build the image only
```

The SSID must be 1-32 bytes, the PSK 8-63 printable ASCII characters or 64 hex digits. The shell never expands the PSK (safe under `bash -x`), and both variables are unset before `nvs_partition_gen.py` and `espflash` run.

Without credentials the panel logs `wifi: no credentials in NVS (deckpanel/wifi_ssid)` and keeps the screen up.

## Touch orientation

`TOUCH_SWAP_XY`, `TOUCH_MIRROR_X`, `TOUCH_MIRROR_Y` in `src/touch.rs` (all false, as the BSP). Flip them if the corner keys log the wrong index.
