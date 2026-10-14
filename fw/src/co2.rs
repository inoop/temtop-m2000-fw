// MH-Z19 CO2 sensor on USART1 (9600 baud), polled.

use crate::avg::RingAvg;
use crate::hw::{self, *};

const USART1: u32 = 0x4001_3800;

pub fn init_usart1() {
    hw::rcc_enable(RCC_APB2ENR, apb2::USART1 | apb2::IOPA);
    pins::CO2_TX.config(AF_PP);
    pins::CO2_RX.config(IN_FLOAT);

    hw::rcc_enable(RCC_APB2ENR, apb2::IOPB);
    pins::CO2_PWR.config(OUT_PP);

    hw::write(USART1 + USART_CR1, 0);
    let _ = hw::read(USART1 + USART_SR);
    let _ = hw::read(USART1 + USART_DR);

    hw::write(USART1 + USART_BRR, 0x1D4C); // 72 MHz / 9600
    hw::write(USART1 + USART_CR2, 0);
    hw::write(USART1 + USART_CR3, 0);
    hw::write(USART1 + USART_CR1, CR1_UE_TE_RE);
}

fn sr() -> u32 { hw::read(USART1 + USART_SR) }
fn dr() -> u8 { hw::read(USART1 + USART_DR) as u8 }

fn wait_flag(flag: u32, tries: u32) -> bool {
    (0..tries).any(|_| sr() & flag != 0)
}

/// Flush stale RX, send `cmd` and wait for transmission complete.
fn send(cmd: &[u8]) {
    while sr() & SR_RXNE != 0 {
        let _ = dr();
    }
    let _ = sr();
    let _ = dr();
    for &b in cmd {
        if wait_flag(SR_TXE, 100_000) {
            hw::write(USART1 + USART_DR, b as u32);
        }
    }
    wait_flag(SR_TC, 100_000);
}

/// Receive up to buf.len() bytes; long timeout for the first byte, short
/// between bytes. Returns the number received.
fn recv(buf: &mut [u8]) -> usize {
    let mut got = 0usize;
    for (i, slot) in buf.iter_mut().enumerate() {
        let timeout = if i == 0 { 1_500_000u32 } else { 150_000u32 };
        let mut found = false;
        for _ in 0..timeout {
            let s = sr();
            if s & SR_ORE != 0 {
                let _ = dr();
                continue;
            }
            if s & SR_RXNE != 0 {
                *slot = dr();
                got += 1;
                found = true;
                break;
            }
        }
        if !found { break; }
    }
    got
}

/// Stock poll command (TIM2 ISR, 0x080286E8).
pub fn request() -> Option<u16> {
    send(&[0xFF, 0x01, 0x86, 0x00, 0x00, 0x00, 0x00, 0x00, 0x79]);
    let mut resp = [0u8; 9];
    let got = recv(&mut resp);
    // CO2_MHZ19_ProcessPacket @0x080070F4: header check only, no checksum.
    if got >= 9 && resp[0] == 0xFF && resp[1] == 0x86 {
        return Some(((resp[2] as u16) << 8) | (resp[3] as u16));
    }
    None
}

/// Stock zero-point command (0x080286F1).
fn zero_calibrate() {
    send(&[0xFF, 0x01, 0x87, 0x00, 0x00, 0x00, 0x00, 0x00, 0x78]);
}

/// 10-sample average × CalData+0x14 (docs/sensor-processing.md §4).
pub struct Co2State {
    avg: RingAvg<10>,
    pub value: f32,
    pub valid: bool,
}

impl Co2State {
    pub const fn new() -> Self {
        Self { avg: RingAvg::new(), value: 0.0, valid: false }
    }

    pub fn process(&mut self, ppm: u16) {
        self.value = self.avg.push(ppm as f32) * crate::cal::get().k_co2;
        self.valid = true;
    }

    /// Stock Format_IntegerValue: clamp 5000, trunc(v*100)/100.
    pub fn ppm(&self) -> u32 {
        let v = if self.value >= 5000.0 { 5000.0 } else { self.value };
        (v * 100.0) as u32 / 100
    }
}

/// User-started zero calibration: progress counts seconds/18 up to 100
/// (~30 min), then the zero-point command is sent.
pub struct ZeroCal {
    secs: Option<u32>,
}

impl ZeroCal {
    pub const fn new() -> Self {
        Self { secs: None }
    }

    pub fn start(&mut self) {
        self.secs = Some(0);
    }

    /// Progress 0..=100 while running
    pub fn progress(&self) -> Option<u32> {
        self.secs.map(|s| (s / 18).min(100))
    }

    /// Call once per second. Returns true when the command was sent.
    pub fn tick(&mut self) -> bool {
        let Some(s) = self.secs.as_mut() else { return false };
        *s += 1;
        if *s / 18 < 100 { return false; }
        zero_calibrate();
        self.secs = None;
        true
    }
}
