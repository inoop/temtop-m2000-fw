use core::ptr::{read_volatile, write_volatile};

use crate::hw::{self, pins};

// Bit-band aliases for atomic single-bit GPIO access
const SCL_OUT: *mut u32 = 0x4221_81A0 as *mut u32; // GPIOB ODR bit 8
const SDA_OUT: *mut u32 = 0x4221_81A4 as *mut u32; // GPIOB ODR bit 9
const SDA_IN: *const u32 = 0x4221_8124 as *const u32; // GPIOB IDR bit 9
const SCL_IN: *const u32 = 0x4221_8120 as *const u32; // GPIOB IDR bit 8

const ADDR_W: u8 = 0x80; // SHT21 write (0x40 << 1)
const ADDR_R: u8 = 0x81; // SHT21 read
const CMD_HUMIDITY: u8 = 0xE5; // Measure RH, hold master
const CMD_TEMPERATURE: u8 = 0xE0; // Read T from previous RH measurement (stock)
const CMD_SOFT_RESET: u8 = 0xFE;

fn i2c_delay() {
    cortex_m::asm::delay(360); // ~5µs at 72MHz → ~100kHz I2C
}

fn scl_low() { unsafe { write_volatile(SCL_OUT, 0); } }
fn sda_low() { unsafe { write_volatile(SDA_OUT, 0); } }
fn sda_read() -> bool { unsafe { read_volatile(SDA_IN) != 0 } }
fn scl_read() -> bool { unsafe { read_volatile(SCL_IN) != 0 } }

fn sda_release() {
    unsafe { write_volatile(SDA_OUT, 1); }
}
fn scl_release() {
    unsafe { write_volatile(SCL_OUT, 1); }
}

fn i2c_start() {
    sda_release();
    scl_release();
    i2c_delay();
    i2c_delay();
    sda_low();
    i2c_delay();
    i2c_delay();
    scl_low();
}

fn i2c_stop() {
    scl_low();
    sda_low();
    i2c_delay();
    i2c_delay();
    scl_release();
    i2c_delay();
    sda_release();
    i2c_delay();
    i2c_delay();
}

fn i2c_write_byte(byte: u8) -> bool {
    for i in (0..8).rev() {
        if byte & (1 << i) != 0 { sda_release(); } else { sda_low(); }
        i2c_delay();
        scl_release();
        i2c_delay();
        scl_low();
    }
    // Release SDA for ACK, with clock stretching support
    sda_release();
    i2c_delay();
    scl_release();
    let mut timeout = 10_000_000u32;
    while !scl_read() {
        timeout -= 1;
        if timeout == 0 { scl_low(); return false; }
    }
    i2c_delay();
    let ack = !sda_read();
    scl_low();
    ack
}

fn i2c_read_byte(ack: bool) -> u8 {
    sda_release();
    let mut byte: u8 = 0;
    for _ in 0..8 {
        i2c_delay();
        scl_release();
        let mut timeout = 10_000_000u32;
        while !scl_read() {
            timeout -= 1;
            if timeout == 0 { break; }
        }
        i2c_delay();
        byte = (byte << 1) | if sda_read() { 1 } else { 0 };
        scl_low();
    }
    if ack { sda_low(); } else { sda_release(); }
    i2c_delay();
    scl_release();
    i2c_delay();
    scl_low();
    byte
}

pub fn init_sht21() {
    hw::rcc_enable(hw::RCC_APB2ENR, hw::apb2::IOPB);
    // Open-drain so the sensor can stretch SCL
    pins::I2C_SCL.config(hw::OUT_OD);
    pins::I2C_SDA.config(hw::OUT_OD);

    // Both lines idle HIGH (released)
    scl_release();
    sda_release();

    // I2C bus recovery: clock out 9 bits to free stuck SDA
    for _ in 0..9 {
        scl_low();
        i2c_delay();
        i2c_delay();
        scl_release();
        i2c_delay();
        i2c_delay();
        if sda_read() { break; }
    }
    // Generate STOP to reset bus state
    scl_low();
    sda_low();
    i2c_delay();
    scl_release();
    i2c_delay();
    sda_release();
    i2c_delay();

    // Soft reset
    i2c_start();
    i2c_write_byte(ADDR_W);
    i2c_write_byte(CMD_SOFT_RESET);
    i2c_stop();
    crate::clock::delay_ms(20);
}

fn read_raw(cmd: u8) -> Option<u16> {
    i2c_start();
    if !i2c_write_byte(ADDR_W) { i2c_stop(); return None; }
    if !i2c_write_byte(cmd) { i2c_stop(); return None; }
    // Repeated start — sensor clock-stretches during measurement
    i2c_start();
    if !i2c_write_byte(ADDR_R) { i2c_stop(); return None; }
    let msb = i2c_read_byte(true);  // ACK
    let lsb = i2c_read_byte(false); // NACK (skip CRC byte)
    i2c_stop();
    Some(((msb as u16) << 8) | (lsb as u16))
}

pub fn read_humidity_raw() -> Option<u16> {
    read_raw(CMD_HUMIDITY)
}

pub fn read_temperature_raw() -> Option<u16> {
    read_raw(CMD_TEMPERATURE)
}

/// Stock SHT21_ReadHumidity @0x0800CEC0 (f32). Must be called before
/// read_temperature_raw(): command 0xE0 returns the T of that measurement.
pub fn convert_humidity(raw: u16) -> f32 {
    let rh = ((raw as u32) * 125) as f32 / 65536.0 - 6.0;
    let rh = if rh >= 90.0 { 90.0 } else { rh };
    if rh <= 0.0 { 0.0 } else { rh }
}

/// Stock SHT21_ReadTemperature @0x0800CFA4: f64 math, then the clamp chain
/// with the -1.0 self-heating compensation only for 1 < T < 85.
pub fn convert_temperature(raw: u16) -> f32 {
    let t = ((raw as f32 as f64) * 175.72 / 65536.0 - 46.83) as f32;
    if t <= 1.0 {
        0.0
    } else if t < 85.0 {
        t - 1.0
    } else {
        85.0
    }
}

/// Display_FormatTemperatureHumidity @0x0801FF70: integer, truncated.
/// CalData+0x13 == 1 → °C, otherwise °F.
pub fn display_temperature(t: f32, celsius: bool) -> u32 {
    if celsius {
        (t as u32) & 0xFF
    } else {
        (((t as f64) * 1.8 + 32.0) as f32 as u32) & 0xFF
    }
}

pub fn display_humidity(rh: f32) -> u32 {
    (rh as u32) & 0xFF
}
