//! 480x480 ST7701S RGB panel through ESP-IDF's RGB LCD driver.
//!
//! Every constant is taken from Waveshare's official ESP-IDF BSP
//! `waveshare/esp32_s3_touch_lcd_4` 3.0.0
//! (<https://components.espressif.com/components/waveshare/esp32_s3_touch_lcd_4>,
//! `include/bsp/esp32_s3_touch_lcd_4.h`, `include/bsp/display.h`,
//! `esp32_s3_touch_lcd_4.c`), the BSP used by the demo
//! <https://github.com/waveshareteam/ESP32-S3-Touch-LCD-4> (board V4.0), and
//! from the Espressif components it builds on: `espressif/esp_lcd_st7701`
//! 2.0.3 (timing macro, init framing) and
//! `espressif/esp_lcd_panel_io_additions` 1.0.1 (3-wire SPI framing).
//!
//! Bring-up order (as `bsp_display_new` + `esp_lcd_new_panel_st7701`):
//! expander reset pulse, ST7701S init over bit-banged 3-wire SPI, then the
//! RGB panel with its framebuffer in PSRAM.

use core::ptr;

use anyhow::{bail, ensure, Context as _, Result};
use deckd_core::layout::{Rect, Size};
use esp_idf_svc::hal::delay::{Ets, FreeRtos};
use esp_idf_svc::hal::gpio::{Output, OutputPin, PinDriver};
use esp_idf_svc::sys;

use crate::board::Expander;

/// Panel resolution. Source: `display.h` `BSP_LCD_H_RES`/`BSP_LCD_V_RES`.
pub const SCREEN: Size = Size { w: 480, h: 480 };

// RGB interface pins. Source: BSP header `BSP_LCD_*`.
/// `BSP_LCD_VSYNC (GPIO_NUM_39)`.
const LCD_VSYNC_GPIO: i32 = 39;
/// `BSP_LCD_HSYNC (GPIO_NUM_38)`.
const LCD_HSYNC_GPIO: i32 = 38;
/// `BSP_LCD_DE (GPIO_NUM_40)`.
const LCD_DE_GPIO: i32 = 40;
/// `BSP_LCD_PCLK (GPIO_NUM_41)`.
const LCD_PCLK_GPIO: i32 = 41;
/// `BSP_LCD_DISP (GPIO_NUM_NC)`: display on/off goes by command.
const LCD_DISP_GPIO: i32 = -1;
/// `BSP_LCD_DATA0` .. `BSP_LCD_DATA15` in order (B0-B4, G0-G5, R0-R4).
const LCD_DATA_GPIOS: [i32; 16] = [5, 45, 48, 47, 21, 14, 13, 12, 11, 10, 9, 46, 3, 8, 18, 17];

// ST7701S 3-wire SPI (init only). Source: BSP header `BSP_LCD_IO_SPI_*`.
// GPIO1/2 are shared with the microSD slot (CMD/CLK); the SPI lines are
// released after init. Pins: CS GPIO42, SCL GPIO2, SDA GPIO1; `main` hands
// the typed pins to `Display::new`.

/// 3-wire SPI half clock period. Source: `PANEL_IO_3WIRE_SPI_CLK_MAX`
/// (500 kHz) -> `1_000_000 / (500_000 * 2)` = 1 us.
const SPI_HALF_PERIOD_US: u32 = 1;

/// RGB pixel clock. Source: `ST7701_480_480_PANEL_60HZ_RGB_TIMING()` and
/// `BSP_LCD_PIXEL_CLOCK_HZ`, both 16 MHz.
const PCLK_HZ: u32 = 16_000_000;
/// Sync timings. Source: `ST7701_480_480_PANEL_60HZ_RGB_TIMING()`.
const HSYNC_PULSE_WIDTH: u32 = 10;
const HSYNC_BACK_PORCH: u32 = 10;
const HSYNC_FRONT_PORCH: u32 = 20;
const VSYNC_PULSE_WIDTH: u32 = 10;
const VSYNC_BACK_PORCH: u32 = 10;
const VSYNC_FRONT_PORCH: u32 = 10;
/// Data bus width and framebuffer depth (RGB565). Source: `display.h`
/// `BSP_RGB_DATA_WIDTH (16)`, `BSP_LCD_BITS_PER_PIXEL (16)`.
const RGB_DATA_WIDTH: usize = 16;
const FB_BITS_PER_PIXEL: usize = 16;
/// Bounce buffer: one screen width times 20 lines. Source:
/// `BSP_LCD_DRAW_BUFF_SIZE`, Kconfig `BSP_LCD_RGB_BOUNCE_BUFFER_HEIGHT` = 20.
const BOUNCE_BUFFER_LINES: usize = 20;
/// PSRAM DMA alignment. Source: BSP `rgb_config.psram_trans_align = 64`.
const PSRAM_TRANS_ALIGN: usize = 64;

/// ST7701S Command2 bank select (`ST7701_CMD_CND2BKxSEL`) with Command2
/// off, sent first by `panel_st7701_send_init_cmds`.
const CMD_CND2BKXSEL: u8 = 0xFF;
const CND2_OFF: [u8; 5] = [0x77, 0x01, 0x00, 0x00, 0x00];
/// MADCTL / COLMOD preamble of `panel_st7701_send_init_cmds`: RGB order
/// (`LCD_RGB_ELEMENT_ORDER_RGB` -> 0x00) and the BSP's
/// `BSP_LCD_BIT_PER_PIXEL (18)` -> 0x60.
const CMD_MADCTL: u8 = 0x36;
const CMD_COLMOD: u8 = 0x3A;
const MADCTL_RGB: u8 = 0x00;
const COLMOD_RGB666: u8 = 0x60;

/// One init command: opcode, parameters, delay after it in ms.
struct InitCmd {
    cmd: u8,
    data: &'static [u8],
    delay_ms: u32,
}

const fn ic(cmd: u8, data: &'static [u8], delay_ms: u32) -> InitCmd {
    InitCmd {
        cmd,
        data,
        delay_ms,
    }
}

/// Vendor init sequence, verbatim from the BSP's `lcd_init_cmds[]`
/// (`esp32_s3_touch_lcd_4.c`, `waveshare/esp32_s3_touch_lcd_4` 3.0.0).
const LCD_INIT_CMDS: &[InitCmd] = &[
    ic(0x11, &[], 120),
    ic(0xFF, &[0x77, 0x01, 0x00, 0x00, 0x10], 0),
    ic(0xC0, &[0x3B, 0x00], 0),
    ic(0xC1, &[0x0D, 0x02], 0),
    ic(0xC2, &[0x21, 0x08], 0),
    ic(0xCD, &[0x08], 0),
    ic(
        0xB0,
        &[
            0x00, 0x11, 0x18, 0x0E, 0x11, 0x06, 0x07, 0x08, 0x07, 0x22, 0x04, 0x12, 0x0F, 0xAA,
            0x31, 0x18,
        ],
        0,
    ),
    ic(
        0xB1,
        &[
            0x00, 0x11, 0x19, 0x0E, 0x12, 0x07, 0x08, 0x08, 0x08, 0x22, 0x04, 0x11, 0x11, 0xA9,
            0x32, 0x18,
        ],
        0,
    ),
    ic(0xFF, &[0x77, 0x01, 0x00, 0x00, 0x11], 0),
    ic(0xB0, &[0x60], 0),
    ic(0xB1, &[0x30], 0),
    ic(0xB2, &[0x87], 0),
    ic(0xB3, &[0x80], 0),
    ic(0xB5, &[0x49], 0),
    ic(0xB7, &[0x85], 0),
    ic(0xB8, &[0x21], 0),
    ic(0xC1, &[0x78], 0),
    ic(0xC2, &[0x78], 20),
    ic(0xE0, &[0x00, 0x1B, 0x02], 0),
    ic(
        0xE1,
        &[
            0x08, 0xA0, 0x00, 0x00, 0x07, 0xA0, 0x00, 0x00, 0x00, 0x44, 0x44,
        ],
        0,
    ),
    ic(
        0xE2,
        &[
            0x11, 0x11, 0x44, 0x44, 0xED, 0xA0, 0x00, 0x00, 0xEC, 0xA0, 0x00, 0x00,
        ],
        0,
    ),
    ic(0xE3, &[0x00, 0x00, 0x11, 0x11], 0),
    ic(0xE4, &[0x44, 0x44], 0),
    ic(
        0xE5,
        &[
            0x0A, 0xE9, 0xD8, 0xA0, 0x0C, 0xEB, 0xD8, 0xA0, 0x0E, 0xED, 0xD8, 0xA0, 0x10, 0xEF,
            0xD8, 0xA0,
        ],
        0,
    ),
    ic(0xE6, &[0x00, 0x00, 0x11, 0x11], 0),
    ic(0xE7, &[0x44, 0x44], 0),
    ic(
        0xE8,
        &[
            0x09, 0xE8, 0xD8, 0xA0, 0x0B, 0xEA, 0xD8, 0xA0, 0x0D, 0xEC, 0xD8, 0xA0, 0x0F, 0xEE,
            0xD8, 0xA0,
        ],
        0,
    ),
    ic(0xEB, &[0x02, 0x00, 0xE4, 0xE4, 0x88, 0x00, 0x40], 0),
    ic(0xEC, &[0x3C, 0x00], 0),
    ic(
        0xED,
        &[
            0xAB, 0x89, 0x76, 0x54, 0x02, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x20, 0x45, 0x67,
            0x98, 0xBA,
        ],
        0,
    ),
    ic(0xFF, &[0x77, 0x01, 0x00, 0x00, 0x00], 0),
    ic(0x36, &[0x00], 0),
    ic(0x3A, &[0x66], 0),
    ic(0x21, &[], 120),
    ic(0x29, &[], 0),
];

/// Bit-banged 9-bit 3-wire SPI to the ST7701S, mode 0, MSB first, CS
/// active low, D/C bit first (0 = command, 1 = data), as
/// `ST7701_PANEL_IO_3WIRE_SPI_CONFIG(line_config, 0)` configures
/// `esp_lcd_panel_io_3wire_spi`.
struct St7701Spi<'d> {
    cs: PinDriver<'d, Output>,
    scl: PinDriver<'d, Output>,
    sda: PinDriver<'d, Output>,
}

impl<'d> St7701Spi<'d> {
    fn new(
        cs: impl OutputPin + 'd,
        scl: impl OutputPin + 'd,
        sda: impl OutputPin + 'd,
    ) -> Result<Self> {
        let mut spi = Self {
            cs: PinDriver::output(cs).context("lcd spi: CS pin")?,
            scl: PinDriver::output(scl).context("lcd spi: SCL pin")?,
            sda: PinDriver::output(sda).context("lcd spi: SDA pin")?,
        };
        spi.cs.set_high().context("lcd spi: CS idle")?;
        spi.scl.set_low().context("lcd spi: SCL idle")?;
        spi.sda.set_low().context("lcd spi: SDA idle")?;
        Ok(spi)
    }

    fn command(&mut self, cmd: u8, params: &[u8]) -> Result<()> {
        self.package(false, cmd)?;
        for &param in params {
            self.package(true, param)?;
        }
        Ok(())
    }

    /// One CS-framed 9-bit word (`spi_write_package` with 1-byte payload).
    fn package(&mut self, is_data: bool, byte: u8) -> Result<()> {
        self.cs.set_low()?;
        Ets::delay_us(SPI_HALF_PERIOD_US);
        let word = (u16::from(is_data) << 8) | u16::from(byte);
        for bit in (0..9).rev() {
            if word & (1 << bit) == 0 {
                self.sda.set_low()?;
            } else {
                self.sda.set_high()?;
            }
            // Mode 0: data is sampled on the rising edge.
            self.scl.set_low()?;
            Ets::delay_us(SPI_HALF_PERIOD_US);
            self.scl.set_high()?;
            Ets::delay_us(SPI_HALF_PERIOD_US);
        }
        self.scl.set_low()?;
        self.sda.set_low()?;
        Ets::delay_us(SPI_HALF_PERIOD_US);
        self.cs.set_high()?;
        Ets::delay_us(SPI_HALF_PERIOD_US);
        Ok(())
    }
}

/// The panel: RGB framebuffer in PSRAM plus the backlight on the expander.
pub struct Display {
    panel: sys::esp_lcd_panel_handle_t,
    expander: Expander,
    /// Scratch RGB565 buffer for one key, reused between draws.
    scratch: Vec<u16>,
}

impl Display {
    /// Initialise the ST7701S and start the RGB panel. `expander` must have
    /// just pulsed the LCD reset (see `Expander::new`).
    ///
    /// # Errors
    /// Fails if a pin cannot be driven, the RGB panel cannot be created
    /// (no PSRAM for the framebuffer) or the panel init fails.
    pub fn new(
        expander: Expander,
        spi_cs: impl OutputPin,
        spi_scl: impl OutputPin,
        spi_sda: impl OutputPin,
    ) -> Result<Self> {
        {
            let mut spi = St7701Spi::new(spi_cs, spi_scl, spi_sda)?;
            spi.command(CMD_CND2BKXSEL, &CND2_OFF)
                .context("st7701: bank select")?;
            spi.command(CMD_MADCTL, &[MADCTL_RGB])
                .context("st7701: MADCTL")?;
            spi.command(CMD_COLMOD, &[COLMOD_RGB666])
                .context("st7701: COLMOD")?;
            for init in LCD_INIT_CMDS {
                spi.command(init.cmd, init.data)
                    .with_context(|| format!("st7701: init command {:#04x}", init.cmd))?;
                if init.delay_ms > 0 {
                    FreeRtos::delay_ms(init.delay_ms);
                }
            }
            // Dropping the pin drivers resets GPIO1/2/42 for the SD slot.
        }
        log::info!(
            "display: ST7701S init sent ({} commands)",
            LCD_INIT_CMDS.len() + 3
        );

        let panel = new_rgb_panel()?;
        let mut display = Self {
            panel,
            expander,
            scratch: Vec::new(),
        };
        // SAFETY: `panel` is a live handle from `esp_lcd_new_rgb_panel`.
        sys::esp!(unsafe { sys::esp_lcd_panel_reset(display.panel) })
            .context("rgb panel: reset")?;
        // SAFETY: as above.
        sys::esp!(unsafe { sys::esp_lcd_panel_init(display.panel) }).context("rgb panel: init")?;
        display.clear(0x0000)?;
        display.set_brightness(100);
        log::info!(
            "display: RGB panel up, {}x{} RGB565, framebuffer in PSRAM",
            SCREEN.w,
            SCREEN.h
        );
        // The last init command (0x29, display on) already turned the panel
        // on, so no separate disp_on_off call is needed.
        Ok(display)
    }

    /// Draw one key: `rgba` holds `rect.w * rect.h` RGBA8888 pixels
    /// (tiny-skia premultiplied; keys are opaque), row-major.
    ///
    /// # Errors
    /// Fails if `rgba` has the wrong length, the rect leaves the screen or
    /// the driver rejects the copy.
    pub fn draw_key(&mut self, rect: Rect, rgba: &[u8]) -> Result<()> {
        let pixels = usize::from(rect.w) * usize::from(rect.h);
        ensure!(
            rgba.len() == pixels * 4,
            "display: key {}x{} needs {} RGBA bytes, got {}",
            rect.w,
            rect.h,
            pixels * 4,
            rgba.len()
        );
        let x_end = rect
            .x
            .checked_add(rect.w)
            .context("display: rect x overflow")?;
        let y_end = rect
            .y
            .checked_add(rect.h)
            .context("display: rect y overflow")?;
        ensure!(
            x_end <= SCREEN.w && y_end <= SCREEN.h,
            "display: rect {rect:?} leaves the {}x{} screen",
            SCREEN.w,
            SCREEN.h
        );
        self.scratch.clear();
        self.scratch
            .extend(rgba.chunks_exact(4).map(|px| rgb565(px[0], px[1], px[2])));
        self.blit(rect.x, rect.y, x_end, y_end)
    }

    /// Fill the whole screen with one RGB565 color, line by line.
    ///
    /// # Errors
    /// Fails if the driver rejects a copy.
    pub fn clear(&mut self, color: u16) -> Result<()> {
        self.scratch.clear();
        self.scratch.resize(usize::from(SCREEN.w), color);
        for y in 0..SCREEN.h {
            self.blit(0, y, SCREEN.w, y + 1)?;
        }
        Ok(())
    }

    /// Backlight brightness in percent (clamped to 100). An I2C failure is
    /// logged, not returned: brightness is cosmetic.
    pub fn set_brightness(&mut self, pct: u8) {
        if let Err(err) = self.expander.set_backlight(pct) {
            log::warn!("display: set brightness {pct}%: {err:#}");
        }
    }

    /// Copy `self.scratch` into the framebuffer window `[x0, x1) x [y0, y1)`.
    fn blit(&mut self, x0: u16, y0: u16, x1: u16, y1: u16) -> Result<()> {
        let expected = usize::from(x1 - x0) * usize::from(y1 - y0);
        if self.scratch.len() != expected {
            bail!(
                "display: blit buffer has {} pixels, window needs {expected}",
                self.scratch.len()
            );
        }
        // SAFETY: `panel` is live; the buffer holds exactly the window's
        // pixels and outlives the call (the RGB driver copies synchronously
        // into its framebuffer).
        sys::esp!(unsafe {
            sys::esp_lcd_panel_draw_bitmap(
                self.panel,
                i32::from(x0),
                i32::from(y0),
                i32::from(x1),
                i32::from(y1),
                self.scratch.as_ptr().cast(),
            )
        })
        .context("display: draw_bitmap")
    }
}

/// Create the RGB panel with the BSP's configuration.
fn new_rgb_panel() -> Result<sys::esp_lcd_panel_handle_t> {
    // SAFETY: an all-zero config is the documented starting point
    // (`= {0}` in C); every field the driver reads is set below.
    let mut config: sys::esp_lcd_rgb_panel_config_t = unsafe { core::mem::zeroed() };
    config.clk_src = sys::soc_periph_lcd_clk_src_t_LCD_CLK_SRC_DEFAULT;
    config.timings.pclk_hz = PCLK_HZ;
    config.timings.h_res = u32::from(SCREEN.w);
    config.timings.v_res = u32::from(SCREEN.h);
    config.timings.hsync_pulse_width = HSYNC_PULSE_WIDTH;
    config.timings.hsync_back_porch = HSYNC_BACK_PORCH;
    config.timings.hsync_front_porch = HSYNC_FRONT_PORCH;
    config.timings.vsync_pulse_width = VSYNC_PULSE_WIDTH;
    config.timings.vsync_back_porch = VSYNC_BACK_PORCH;
    config.timings.vsync_front_porch = VSYNC_FRONT_PORCH;
    config.timings.flags.set_pclk_active_neg(0);
    config.data_width = RGB_DATA_WIDTH;
    config.bits_per_pixel = FB_BITS_PER_PIXEL;
    config.num_fbs = 1;
    config.bounce_buffer_size_px = usize::from(SCREEN.w) * BOUNCE_BUFFER_LINES;
    config.__bindgen_anon_1.psram_trans_align = PSRAM_TRANS_ALIGN;
    config.hsync_gpio_num = LCD_HSYNC_GPIO;
    config.vsync_gpio_num = LCD_VSYNC_GPIO;
    config.de_gpio_num = LCD_DE_GPIO;
    config.pclk_gpio_num = LCD_PCLK_GPIO;
    config.disp_gpio_num = LCD_DISP_GPIO;
    config.data_gpio_nums[..LCD_DATA_GPIOS.len()].copy_from_slice(&LCD_DATA_GPIOS);
    for unused in &mut config.data_gpio_nums[LCD_DATA_GPIOS.len()..] {
        *unused = -1;
    }
    config.flags.set_fb_in_psram(1);

    let mut panel: sys::esp_lcd_panel_handle_t = ptr::null_mut();
    // SAFETY: `config` is fully initialised and `panel` is a valid out
    // pointer.
    sys::esp!(unsafe { sys::esp_lcd_new_rgb_panel(&raw const config, &raw mut panel) })
        .context("rgb panel: create (framebuffer needs PSRAM)")?;
    Ok(panel)
}

/// RGBA8888 channel values to RGB565.
#[must_use]
pub fn rgb565(r: u8, g: u8, b: u8) -> u16 {
    (u16::from(r >> 3) << 11) | (u16::from(g >> 2) << 5) | u16::from(b >> 3)
}
