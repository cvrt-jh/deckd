//! GT911 capacitive touch over I2C.
//!
//! Register map and read sequence follow Espressif's `esp_lcd_touch_gt911`
//! component (used by the Waveshare BSP `waveshare/esp32_s3_touch_lcd_4`
//! 3.0.0, `bsp_touch_new`):
//! <https://components.espressif.com/components/espressif/esp_lcd_touch_gt911>.
//! The GT911 reset line is driven by the board's IO expander and is pulsed in
//! `display::Display::new`, so the controller is ready once the display is up.

use anyhow::{Context as _, Result};
use deckd_core::layout::Size;

use crate::board::SharedI2c;

/// GT911 I2C address (INT low during reset). Source: `esp_lcd_touch_gt911.h`
/// `ESP_LCD_TOUCH_IO_I2C_GT911_ADDRESS`.
const GT911_ADDR: u8 = 0x5D;
/// Backup GT911 address (INT high during reset). The board leaves INT
/// unconnected (`BSP_LCD_TOUCH_INT = GPIO_NUM_NC`), so the BSP probes both.
const GT911_ADDR_BACKUP: u8 = 0x14;

/// Status register: bit 7 = buffer ready, bits 0-3 = number of points.
/// Source: `ESP_LCD_TOUCH_GT911_READ_XY_REG`.
const REG_STATUS: u16 = 0x814E;
/// First point record (track id, x lo/hi, y lo/hi, size lo/hi, reserved).
const REG_POINT1: u16 = REG_STATUS + 1;
/// Product id register ("911" in ASCII). Source:
/// `ESP_LCD_TOUCH_GT911_PRODUCT_ID_REG`.
const REG_PRODUCT_ID: u16 = 0x8140;

const STATUS_READY: u8 = 0x80;
const STATUS_COUNT_MASK: u8 = 0x0F;
/// The GT911 reports at most 5 points; a larger count is a glitch.
const MAX_POINTS: u8 = 5;

/// Orientation of raw GT911 coordinates relative to the screen.
///
/// The Waveshare BSP configures the touch with `swap_xy = 0, mirror_x = 0,
/// mirror_y = 0` and the display without rotation. If the corner-tap check
/// shows swapped or mirrored axes, flip these three constants (and only
/// these).
pub const TOUCH_SWAP_XY: bool = false;
/// Mirror the X axis after the optional swap.
pub const TOUCH_MIRROR_X: bool = false;
/// Mirror the Y axis after the optional swap.
pub const TOUCH_MIRROR_Y: bool = false;

/// I2C timeout for one touch transaction, in `FreeRTOS` ticks (1 ms each at
/// `CONFIG_FREERTOS_HZ=1000`).
const I2C_TIMEOUT_TICKS: u32 = 50;

/// GT911 touch controller.
pub struct Touch {
    bus: SharedI2c,
    addr: u8,
    screen: Size,
    /// Last reported point. The GT911 only flags fresh data once per scan,
    /// so between scans the finger state stays what it last was.
    last: Option<(u16, u16)>,
}

impl Touch {
    /// Find the GT911 on the shared bus and check its product id.
    ///
    /// # Errors
    /// Fails if neither GT911 address answers or the product id read fails.
    pub fn new(bus: SharedI2c, screen: Size) -> Result<Self> {
        let addr = [GT911_ADDR, GT911_ADDR_BACKUP]
            .into_iter()
            .find(|&addr| bus.borrow_mut().probe(addr))
            .context("touch: no GT911 at 0x5D or 0x14")?;
        let mut touch = Self {
            bus,
            addr,
            screen,
            last: None,
        };
        let mut id = [0u8; 4];
        touch
            .read(REG_PRODUCT_ID, &mut id)
            .context("touch: read GT911 product id")?;
        log::info!(
            "touch: GT911 at {addr:#04x}, product id {:?}",
            String::from_utf8_lossy(&id)
        );
        Ok(touch)
    }

    /// Current touch point in screen coordinates, `None` while nothing
    /// touches the screen. Only the first point of a multi-touch is used.
    ///
    /// # Errors
    /// Fails on an I2C error.
    pub fn poll(&mut self) -> Result<Option<(u16, u16)>> {
        let mut status = [0u8; 1];
        self.read(REG_STATUS, &mut status)
            .context("touch: read status")?;
        let status = status[0];
        if status & STATUS_READY == 0 {
            return Ok(self.last);
        }
        let count = status & STATUS_COUNT_MASK;
        if count == 0 || count > MAX_POINTS {
            self.last = None;
        } else {
            let mut point = [0u8; 8];
            self.read(REG_POINT1, &mut point)
                .context("touch: read point")?;
            let raw_x = u16::from_le_bytes([point[1], point[2]]);
            let raw_y = u16::from_le_bytes([point[3], point[4]]);
            self.last = Some(to_screen(raw_x, raw_y, self.screen));
        }
        self.write(REG_STATUS, 0).context("touch: clear status")?;
        Ok(self.last)
    }

    fn read(&mut self, reg: u16, buf: &mut [u8]) -> Result<()> {
        self.bus
            .borrow_mut()
            .write_read(self.addr, &reg.to_be_bytes(), buf, I2C_TIMEOUT_TICKS)
    }

    fn write(&mut self, reg: u16, value: u8) -> Result<()> {
        let [hi, lo] = reg.to_be_bytes();
        self.bus
            .borrow_mut()
            .write(self.addr, &[hi, lo, value], I2C_TIMEOUT_TICKS)
    }
}

/// Map a raw GT911 point to screen coordinates using the orientation
/// constants, clamped to the screen.
#[must_use]
pub fn to_screen(raw_x: u16, raw_y: u16, screen: Size) -> (u16, u16) {
    let (x, y) = if TOUCH_SWAP_XY {
        (raw_y, raw_x)
    } else {
        (raw_x, raw_y)
    };
    let max_x = screen.w.saturating_sub(1);
    let max_y = screen.h.saturating_sub(1);
    let x = x.min(max_x);
    let y = y.min(max_y);
    let x = if TOUCH_MIRROR_X { max_x - x } else { x };
    let y = if TOUCH_MIRROR_Y { max_y - y } else { y };
    (x, y)
}
