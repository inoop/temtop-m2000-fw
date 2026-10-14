// ILI9341 on FSMC bank 1 (16-bit, A16 selects command/data).

use core::ptr::write_volatile;

use crate::clock::delay_ms;
use crate::hw::{self, ahb, apb2, pins};

const LCD_CMD: *mut u16 = 0x6000_0000 as *mut u16;
const LCD_DATA: *mut u16 = 0x6002_0000 as *mut u16;

const SWRESET: u8 = 0x01;
const SLPOUT: u8 = 0x11;
const DISPON: u8 = 0x29;
const CASET: u8 = 0x2A;
const PASET: u8 = 0x2B;
const RAMWR: u8 = 0x2C;
const MADCTL: u8 = 0x36;

pub fn lcd_cmd(cmd: u8) {
    unsafe { write_volatile(LCD_CMD, cmd as u16) }
}

pub fn lcd_data8(data: u8) {
    unsafe { write_volatile(LCD_DATA, data as u16) }
}

pub fn lcd_data16(data: u16) {
    unsafe { write_volatile(LCD_DATA, data) }
}

pub fn init_fsmc_gpio() {
    hw::rcc_enable(hw::RCC_APB2ENR, apb2::IOPD | apb2::IOPE | apb2::AFIO);
    hw::rcc_enable(hw::RCC_AHBENR, ahb::FSMC);

    // FSMC data/control pins on ports D and E (AF push-pull), plus
    // PE1 (reset) and PE2 (backlight) as outputs
    hw::write(hw::GPIOD, 0xB4BB_44BB);
    hw::write(hw::GPIOD + 0x04, 0xBB44_BBBB);
    hw::write(hw::GPIOE, 0xB443_8334);
    hw::write(hw::GPIOE + 0x04, 0xBBBB_BBBB);
    pins::LCD_RST.set();
    pins::BACKLIGHT.set();
    hw::write(0xA000_0000, 0x0000_1059); // FSMC_BCR1
    hw::write(0xA000_0004, 0x1000_0101); // FSMC_BTR1
}

pub fn lcd_init() {
    pins::LCD_RST.clr();
    delay_ms(20);
    pins::LCD_RST.set();
    delay_ms(150);
    lcd_cmd(SWRESET); delay_ms(150);
    // Panel setup copied from stock ILI9341_Init (power, VCOM, gamma)
    const INIT: &[(u8, &[u8])] = &[
        (0xCF, &[0x00, 0xC1, 0x30]),
        (0xED, &[0x64, 0x03, 0x12, 0x81]),
        (0xE8, &[0x85, 0x10, 0x7A]),
        (0xCB, &[0x39, 0x2C, 0x00, 0x34, 0x02]),
        (0xF7, &[0x20]),
        (0xEA, &[0x00, 0x00]),
        (0xB1, &[0x00, 0x1A]),
        (0xB6, &[0x0A, 0xA2]),
        (0xC0, &[0x1B]),
        (0xC1, &[0x01]),
        (0xC5, &[0x30, 0x30]),
        (0xC7, &[0xB7]),
        (0xF2, &[0x00]),
        (0x26, &[0x01]),
        (0xE0, &[0x0F, 0x2A, 0x28, 0x08, 0x0E, 0x08, 0x54, 0xA9, 0x43, 0x0A, 0x0F, 0x00, 0x00, 0x00, 0x00]),
        (0xE1, &[0x00, 0x15, 0x17, 0x07, 0x11, 0x06, 0x2B, 0x56, 0x3C, 0x05, 0x10, 0x0F, 0x3F, 0x3F, 0x0F]),
        (MADCTL, &[0x48]),
        (CASET, &[0x00, 0x00, 0x00, 0xEF]),
        (PASET, &[0x00, 0x00, 0x01, 0x3F]),
        (0x3A, &[0x55]), // 16-bit pixels
    ];
    for &(cmd, data) in INIT {
        lcd_cmd(cmd);
        for &d in data { lcd_data8(d); }
        delay_ms(1);
    }
    lcd_cmd(SLPOUT); delay_ms(120);
    lcd_cmd(DISPON); delay_ms(20);
    // Stock LCD_SetOrientation(0): MADCTL = BGR only. 0xB6 above sets SS
    // (source scan reversed), so MX must be off or the image is mirrored.
    lcd_cmd(MADCTL); lcd_data8(0x08);
}

/// Set the window (inclusive corners) and start a pixel write; follow with
/// one lcd_data16() per pixel, row by row.
pub fn begin_write(x0: u16, y0: u16, x1: u16, y1: u16) {
    lcd_cmd(CASET);
    lcd_data8((x0 >> 8) as u8); lcd_data8((x0 & 0xFF) as u8);
    lcd_data8((x1 >> 8) as u8); lcd_data8((x1 & 0xFF) as u8);
    lcd_cmd(PASET);
    lcd_data8((y0 >> 8) as u8); lcd_data8((y0 & 0xFF) as u8);
    lcd_data8((y1 >> 8) as u8); lcd_data8((y1 & 0xFF) as u8);
    lcd_cmd(RAMWR);
}

pub fn lcd_fill_rect(x: u16, y: u16, w: u16, h: u16, color: u16) {
    begin_write(x, y, x + w - 1, y + h - 1);
    for _ in 0..(w as u32 * h as u32) { lcd_data16(color); }
}
