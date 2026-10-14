// Raw register access, GPIO pins and the board pin map (docs/hardware-reference.md).

use core::ptr::{read_volatile, write_volatile};

#[inline]
pub fn read(addr: u32) -> u32 {
    unsafe { read_volatile(addr as *const u32) }
}

#[inline]
pub fn write(addr: u32, v: u32) {
    unsafe { write_volatile(addr as *mut u32, v) }
}

#[inline]
pub fn modify(addr: u32, clear: u32, set: u32) {
    write(addr, (read(addr) & !clear) | set);
}

pub fn iwdg_feed() {
    write(0x4000_3000, 0xAAAA);
}

// ---- RCC ----

pub const RCC_APB2RSTR: u32 = 0x4002_100C;
pub const RCC_AHBENR: u32 = 0x4002_1014;
pub const RCC_APB2ENR: u32 = 0x4002_1018;
pub const RCC_APB1ENR: u32 = 0x4002_101C;

pub mod ahb {
    pub const DMA1: u32 = 1 << 0;
    pub const FSMC: u32 = 1 << 8;
}

pub mod apb2 {
    pub const AFIO: u32 = 1 << 0;
    pub const IOPA: u32 = 1 << 2;
    pub const IOPB: u32 = 1 << 3;
    pub const IOPC: u32 = 1 << 4;
    pub const IOPD: u32 = 1 << 5;
    pub const IOPE: u32 = 1 << 6;
    pub const ADC1: u32 = 1 << 9;
    pub const TIM1: u32 = 1 << 11;
    pub const USART1: u32 = 1 << 14;
}

pub mod apb1 {
    pub const USART2: u32 = 1 << 17;
    pub const USB: u32 = 1 << 23;
    pub const BKP: u32 = 1 << 27;
    pub const PWR: u32 = 1 << 28;
}

pub fn rcc_enable(reg: u32, bits: u32) {
    modify(reg, 0, bits);
}

// ---- USART register offsets ----

pub const USART_SR: u32 = 0x00;
pub const USART_DR: u32 = 0x04;
pub const USART_BRR: u32 = 0x08;
pub const USART_CR1: u32 = 0x0C;
pub const USART_CR2: u32 = 0x10;
pub const USART_CR3: u32 = 0x14;

pub const SR_ORE: u32 = 1 << 3;
pub const SR_RXNE: u32 = 1 << 5;
pub const SR_TC: u32 = 1 << 6;
pub const SR_TXE: u32 = 1 << 7;

pub const CR1_UE_TE_RE: u32 = 0x200C;

// ---- NVIC / SCB ----

/// Disable and un-pend every interrupt line.
pub fn nvic_disable_all() {
    for i in 0..3u32 {
        write(0xE000_E180 + i * 4, 0xFFFF_FFFF); // ICER
        write(0xE000_E280 + i * 4, 0xFFFF_FFFF); // ICPR
    }
}

pub const SCB_VTOR: u32 = 0xE000_ED08;

// ---- GPIO ----

pub const GPIOA: u32 = 0x4001_0800;
pub const GPIOB: u32 = 0x4001_0C00;
pub const GPIOC: u32 = 0x4001_1000;
pub const GPIOD: u32 = 0x4001_1400;
pub const GPIOE: u32 = 0x4001_1800;

// CRL/CRH nibbles (CNF:MODE)
pub const ANALOG: u32 = 0x0;
pub const OUT_PP: u32 = 0x3; // push-pull, 50 MHz
pub const IN_FLOAT: u32 = 0x4;
pub const OUT_OD: u32 = 0x7; // open-drain, 50 MHz
pub const IN_PULL: u32 = 0x8; // pull direction from ODR
pub const AF_PP: u32 = 0xB; // alternate function push-pull, 50 MHz

#[derive(Clone, Copy)]
pub struct Pin {
    port: u32,
    n: u32,
}

impl Pin {
    pub const fn new(port: u32, n: u32) -> Self {
        Pin { port, n }
    }

    pub fn config(self, mode: u32) {
        let (reg, shift) = if self.n < 8 { (self.port, self.n * 4) } else { (self.port + 0x04, (self.n - 8) * 4) };
        modify(reg, 0xF << shift, mode << shift);
    }

    pub fn is_high(self) -> bool {
        read(self.port + 0x08) & (1 << self.n) != 0
    }

    /// Output HIGH (or pull-up in IN_PULL mode)
    pub fn set(self) {
        write(self.port + 0x10, 1 << self.n); // BSRR
    }

    /// Output LOW (or pull-down in IN_PULL mode)
    pub fn clr(self) {
        write(self.port + 0x14, 1 << self.n); // BRR
    }
}

pub mod pins {
    use super::{Pin, GPIOA, GPIOB, GPIOC, GPIOE};

    // Power
    pub const PWR_LATCH: Pin = Pin::new(GPIOC, 0); // LOW = keep power on
    pub const PM_PWR: Pin = Pin::new(GPIOB, 1); // PM sensor power gate
    pub const CO2_PWR: Pin = Pin::new(GPIOB, 0);
    pub const AMP_PWR: Pin = Pin::new(GPIOA, 8); // HCHO analog front end
    pub const VBUS: Pin = Pin::new(GPIOE, 3); // HIGH = USB power present

    // Buttons, all active LOW
    pub const BTN_POWER: Pin = Pin::new(GPIOA, 0);
    pub const BTN_MENU: Pin = Pin::new(GPIOA, 1);
    pub const BTN_OK: Pin = Pin::new(GPIOC, 1);
    pub const BTN_UP: Pin = Pin::new(GPIOC, 2);
    pub const BTN_DOWN: Pin = Pin::new(GPIOC, 3);

    // LCD
    pub const LCD_RST: Pin = Pin::new(GPIOE, 1);
    pub const BACKLIGHT: Pin = Pin::new(GPIOE, 2);

    // Peripherals
    pub const PM_TX: Pin = Pin::new(GPIOA, 2); // USART2
    pub const PM_RX: Pin = Pin::new(GPIOA, 3);
    pub const CO2_TX: Pin = Pin::new(GPIOA, 9); // USART1
    pub const CO2_RX: Pin = Pin::new(GPIOA, 10);
    pub const USB_DP: Pin = Pin::new(GPIOA, 12);
    pub const BAT_SENSE: Pin = Pin::new(GPIOC, 4); // ADC ch14
    pub const HCHO_SENSE: Pin = Pin::new(GPIOC, 5); // ADC ch15
    pub const I2C_SCL: Pin = Pin::new(GPIOB, 8); // SHT21, bit-banged
    pub const I2C_SDA: Pin = Pin::new(GPIOB, 9);
    pub const BUZZER: Pin = Pin::new(GPIOB, 15); // TIM1_CH3N
}
