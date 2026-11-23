//! Shared control bus (I2C) and the CH32V003 helper controller.
//!
//! Pins, address, register map and reset sequence come from Waveshare's
//! official ESP-IDF BSP `waveshare/esp32_s3_touch_lcd_4` 3.0.0
//! (<https://components.espressif.com/components/waveshare/esp32_s3_touch_lcd_4>,
//! `include/bsp/esp32_s3_touch_lcd_4.h`, `bsp_display_new`) and its driver
//! `waveshare/custom_io_expander_ch32v003` 2.0.0
//! (`custom_io_expander_ch32v003.c`), as used by the demo
//! <https://github.com/waveshareteam/ESP32-S3-Touch-LCD-4> (board V4.0).

use std::cell::RefCell;
use std::rc::Rc;

use anyhow::{Context as _, Result};
use esp_idf_svc::hal::delay::FreeRtos;
use esp_idf_svc::hal::gpio::{InputPin, OutputPin};
use esp_idf_svc::hal::i2c::{I2c, I2cConfig, I2cDriver};
use esp_idf_svc::hal::units::Hertz;

// Bus pins: SCL GPIO7, SDA GPIO15. Source: BSP `BSP_I2C_SCL (GPIO_NUM_7)`,
// `BSP_I2C_SDA (GPIO_NUM_15)`. `main` hands the typed pins to `I2cBus::new`.

/// Bus clock. Source: the CH32V003 driver's `I2C_CLK_SPEED (400000)`.
const I2C_BAUD_HZ: u32 = 400_000;
/// Timeout for one expander transaction, in ticks (1 ms each).
const I2C_TIMEOUT_TICKS: u32 = 100;

/// CH32V003 I2C address. Source: `CUSTOM_IO_EXPANDER_I2C_CH32V003_ADDRESS`.
pub const CH32_ADDR: u8 = 0x24;
/// Direction register (bit set = output). Source: `DIRECTION_REG_ADDR`.
const CH32_REG_DIRECTION: u8 = 0x02;
/// Output level register. Source: `OUTPUT_REG_ADDR`.
const CH32_REG_OUTPUT: u8 = 0x03;
/// Backlight PWM register. Source: `PWM_REG_ADDR`.
const CH32_REG_PWM: u8 = 0x05;

// Expander pins as bit masks. Source: BSP header, `IO_EXPANDER_PIN_NUM_n`.
// Pin 6 is the buzzer enable (`BSP_BEE_EN`); it stays low (off).
/// Touch reset. Source: `BSP_LCD_TOUCH_RST (IO_EXPANDER_PIN_NUM_1)`.
const EXIO_TOUCH_RST: u8 = 1 << 1;
/// LCD reset. Source: `BSP_LCD_RST (IO_EXPANDER_PIN_NUM_3)`.
const EXIO_LCD_RST: u8 = 1 << 3;
/// System power enable. Source: `BSP_SYS_EN (IO_EXPANDER_PIN_NUM_5)`.
const EXIO_SYS_EN: u8 = 1 << 5;
/// RTC interrupt, the only input. Source: `BSP_RTC_INT (IO_EXPANDER_PIN_NUM_7)`.
const EXIO_RTC_INT: u8 = 1 << 7;

/// Reset pulse and settle time. Source: `bsp_display_new`,
/// `vTaskDelay(pdMS_TO_TICKS(200))` after each level change.
const RESET_DELAY_MS: u32 = 200;

/// The I2C master shared by the expander and the touch controller.
pub struct I2cBus {
    driver: I2cDriver<'static>,
}

/// The bus is used from the main task only.
pub type SharedI2c = Rc<RefCell<I2cBus>>;

impl I2cBus {
    /// Install the I2C master on the board's control bus pins.
    ///
    /// # Errors
    /// Fails if the ESP-IDF I2C driver cannot be installed.
    pub fn new(
        i2c: impl I2c + 'static,
        sda: impl InputPin + OutputPin + 'static,
        scl: impl InputPin + OutputPin + 'static,
    ) -> Result<SharedI2c> {
        let config = I2cConfig::new().baudrate(Hertz(I2C_BAUD_HZ));
        let driver = I2cDriver::new(i2c, sda, scl, &config).context("i2c: install driver")?;
        Ok(Rc::new(RefCell::new(Self { driver })))
    }

    /// Whether a device acknowledges `addr`.
    pub fn probe(&mut self, addr: u8) -> bool {
        self.driver.write(addr, &[], I2C_TIMEOUT_TICKS).is_ok()
    }

    /// Addresses of every device that acknowledges, for the boot log.
    pub fn scan(&mut self) -> Vec<u8> {
        (0x08..0x78).filter(|&addr| self.probe(addr)).collect()
    }

    /// Write `bytes` to `addr`.
    ///
    /// # Errors
    /// Fails on NACK or timeout.
    pub fn write(&mut self, addr: u8, bytes: &[u8], timeout_ticks: u32) -> Result<()> {
        self.driver
            .write(addr, bytes, timeout_ticks)
            .with_context(|| format!("i2c: write to {addr:#04x}"))
    }

    /// Write `bytes`, then read `buf.len()` bytes back (repeated start).
    ///
    /// # Errors
    /// Fails on NACK or timeout.
    pub fn write_read(
        &mut self,
        addr: u8,
        bytes: &[u8],
        buf: &mut [u8],
        timeout_ticks: u32,
    ) -> Result<()> {
        self.driver
            .write_read(addr, bytes, buf, timeout_ticks)
            .with_context(|| format!("i2c: write_read at {addr:#04x}"))
    }
}

/// The CH32V003 helper: an 8-pin IO expander plus backlight PWM.
pub struct Expander {
    bus: SharedI2c,
}

impl Expander {
    /// Bring the board up the way `bsp_display_new` does: expander reset to
    /// its power-up defaults, outputs configured, LCD and touch held in
    /// reset for 200 ms, then system power on and both released.
    ///
    /// # Errors
    /// Fails if the CH32V003 does not answer at 0x24.
    pub fn new(bus: SharedI2c) -> Result<Self> {
        let mut expander = Self { bus };
        // Driver `reset()`: direction 0xFF, output 0x00.
        expander.write_reg(CH32_REG_DIRECTION, 0xFF)?;
        expander.write_reg(CH32_REG_OUTPUT, 0x00)?;
        // Everything but RTC_INT is an output; buzzer, LCD and touch reset
        // low, then power on and release both resets.
        expander.write_reg(CH32_REG_DIRECTION, !EXIO_RTC_INT)?;
        expander.write_reg(CH32_REG_OUTPUT, 0x00)?;
        FreeRtos::delay_ms(RESET_DELAY_MS);
        expander.write_reg(CH32_REG_OUTPUT, EXIO_SYS_EN | EXIO_LCD_RST | EXIO_TOUCH_RST)?;
        FreeRtos::delay_ms(RESET_DELAY_MS);
        Ok(expander)
    }

    /// Backlight brightness in percent (clamped to 100).
    ///
    /// # Errors
    /// Fails on an I2C error.
    pub fn set_backlight(&mut self, pct: u8) -> Result<()> {
        self.write_reg(CH32_REG_PWM, backlight_pwm(pct)?)
    }

    fn write_reg(&mut self, reg: u8, value: u8) -> Result<()> {
        self.bus
            .borrow_mut()
            .write(CH32_ADDR, &[reg, value], I2C_TIMEOUT_TICKS)
            .with_context(|| format!("expander: write reg {reg:#04x}"))
    }
}

/// CH32V003 PWM register value for a brightness in percent (clamped to 100).
///
/// The PWM is inverted: the BSP's `bsp_display_brightness_set` writes
/// `(100 - pct) * 255 / 100`, so 100 % is PWM 0 and 0 % is PWM 255.
///
/// # Errors
/// Never in practice (the value is at most 255); the conversion is checked.
pub fn backlight_pwm(pct: u8) -> Result<u8> {
    let inverted = 100 - u16::from(pct.min(100));
    u8::try_from(inverted * 255 / 100).context("backlight: PWM value out of range")
}
