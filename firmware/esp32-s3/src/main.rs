//! deckd panel firmware spike for the Waveshare ESP32-S3-Touch-LCD-4:
//! draws a 4x4 grid of deckd-core keys, logs the key under every tap and
//! joins Wi-Fi from NVS credentials.

mod board;
mod display;
mod touch;
mod wifi;

use std::collections::HashMap;

use anyhow::{Context as _, Result};
use deckd_core::layout::Layout;
use deckd_core::render::render_button;
use deckd_core::schema::{ButtonConfig, ButtonDefaults};
use esp_idf_svc::eventloop::EspSystemEventLoop;
use esp_idf_svc::hal::delay::FreeRtos;
use esp_idf_svc::hal::peripherals::Peripherals;
use esp_idf_svc::nvs::EspDefaultNvsPartition;
use esp_idf_svc::sys;

use crate::board::{Expander, I2cBus};
use crate::display::{Display, SCREEN};
use crate::touch::Touch;

const LAYOUT: Layout = Layout::Grid4x4;
/// Touch poll interval.
const TOUCH_POLL_MS: u32 = 20;

fn main() {
    sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();

    if let Err(err) = run() {
        log::error!("fatal: {err:#}");
    }
    // Park instead of returning or panicking, so a fault never turns into a
    // reboot loop and the log stays readable.
    loop {
        FreeRtos::delay_ms(1000);
    }
}

fn run() -> Result<()> {
    log_psram();

    let peripherals = Peripherals::take().context("take peripherals")?;
    let sysloop = EspSystemEventLoop::take().context("take system event loop")?;
    let nvs = EspDefaultNvsPartition::take().context("take NVS partition")?;
    let pins = peripherals.pins;

    // Control bus: SDA GPIO15, SCL GPIO7 (BSP `BSP_I2C_SDA`/`BSP_I2C_SCL`).
    let bus = I2cBus::new(peripherals.i2c0, pins.gpio15, pins.gpio7)?;
    let expander = Expander::new(bus.clone()).context("expander: CH32V003 at 0x24")?;
    let found: Vec<String> = bus
        .borrow_mut()
        .scan()
        .iter()
        .map(|addr| format!("{addr:#04x}"))
        .collect();
    log::info!("i2c: devices {}", found.join(" "));

    // ST7701S 3-wire SPI: CS GPIO42, SCL GPIO2, SDA GPIO1 (BSP
    // `BSP_LCD_IO_SPI_CS`/`_SCL`/`_SDA`).
    let mut display = Display::new(expander, pins.gpio42, pins.gpio2, pins.gpio1)?;
    draw_grid(&mut display)?;

    let mut touch = match Touch::new(bus, SCREEN) {
        Ok(touch) => Some(touch),
        Err(err) => {
            log::error!("{err:#}");
            None
        }
    };

    // Keep the driver alive for the whole run; dropping it disconnects.
    let _wifi = match wifi::connect(peripherals.modem, sysloop, nvs) {
        Ok(wifi) => Some(wifi),
        Err(err) => {
            log::warn!("wifi: {err:#}");
            None
        }
    };

    let mut was_down = false;
    loop {
        if let Some(touch) = touch.as_mut() {
            match touch.poll() {
                Ok(Some((x, y))) => {
                    if !was_down {
                        match LAYOUT.key_at(x, y, SCREEN) {
                            Some(key) => log::info!("touch: ({x}, {y}) key {key}"),
                            None => log::info!("touch: ({x}, {y}) no key"),
                        }
                    }
                    was_down = true;
                }
                Ok(None) => was_down = false,
                Err(err) => log::warn!("{err:#}"),
            }
        }
        FreeRtos::delay_ms(TOUCH_POLL_MS);
    }
}

/// Render the 16 spike keys (amber, black `jb-bold` 22 px label = key index)
/// with deckd-core and draw each into its grid cell.
fn draw_grid(display: &mut Display) -> Result<()> {
    let defaults = ButtonDefaults::default();
    let states: HashMap<String, String> = HashMap::new();
    for index in 0..LAYOUT.key_count() {
        let rect = LAYOUT
            .key_rect(index, SCREEN)
            .with_context(|| format!("no rect for key {index}"))?;
        let button = spike_button(index)?;
        // Keys render at their on-screen size (120 px cell minus the gap).
        let pixmap = render_button(&button, &defaults, &states, u32::from(rect.w), None)
            .with_context(|| format!("render key {index}"))?;
        display
            .draw_key(rect, pixmap.data())
            .with_context(|| format!("draw key {index}"))?;
        if index == 0 {
            log::info!("display: key 0 at {rect:?}");
        }
    }
    log::info!("display: drew {} keys", LAYOUT.key_count());
    Ok(())
}

fn spike_button(index: u8) -> Result<ButtonConfig> {
    let text = format!(
        "key = {index}\nlabel = \"{index}\"\nbackground = \"#FFC107\"\n\
         text_color = \"#000000\"\nfont = \"jb-bold\"\nfont_size = 22.0\n"
    );
    toml::from_str(&text).with_context(|| format!("spike button {index}"))
}

/// Log the PSRAM the heap sees. The 480x480 RGB framebuffer lives there.
fn log_psram() {
    // SAFETY: plain query of the heap allocator.
    let total = unsafe { sys::heap_caps_get_total_size(sys::MALLOC_CAP_SPIRAM) };
    // SAFETY: as above.
    let free = unsafe { sys::heap_caps_get_free_size(sys::MALLOC_CAP_SPIRAM) };
    if total == 0 {
        log::error!("psram: none found (heap_caps MALLOC_CAP_SPIRAM total 0)");
    } else {
        log::info!("psram: {total} bytes in heap, {free} free");
    }
}
