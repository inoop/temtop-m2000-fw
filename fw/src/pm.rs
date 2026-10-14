// PM sensor on USART2 (115200 baud), received by DMA1 ch6 into a ring.

use crate::avg::RingAvg;
use crate::hw::{self, *};

const USART2: u32 = 0x4000_4400;
const DMA1_IFCR: u32 = 0x4002_0004;
const DMA1_CCR6: u32 = 0x4002_006C;
const DMA1_CNDTR6: u32 = 0x4002_0070;
const DMA1_CPAR6: u32 = 0x4002_0074;
const DMA1_CMAR6: u32 = 0x4002_0078;

const BUF_LEN: usize = 100;
const FRAME_LEN: usize = 20;
static mut DMA_BUF: [u8; BUF_LEN] = [0; BUF_LEN];

pub fn init_usart2() {
    hw::rcc_enable(RCC_APB1ENR, apb1::USART2);
    hw::rcc_enable(RCC_APB2ENR, apb2::IOPA);
    hw::rcc_enable(RCC_AHBENR, ahb::DMA1);

    pins::PM_TX.config(AF_PP);
    pins::PM_RX.config(IN_FLOAT);

    hw::write(DMA1_CCR6, 0);
    hw::write(DMA1_IFCR, 0xF << 20); // clear ch6 flags

    hw::write(USART2 + USART_CR1, 0);
    let _ = hw::read(USART2 + USART_SR);
    let _ = hw::read(USART2 + USART_DR);

    hw::write(USART2 + USART_BRR, 0x0138); // 36 MHz / 115200
    hw::write(USART2 + USART_CR2, 0);
    hw::write(USART2 + USART_CR3, 0);

    hw::write(DMA1_CNDTR6, BUF_LEN as u32);
    hw::write(DMA1_CPAR6, USART2 + USART_DR);
    hw::write(DMA1_CMAR6, core::ptr::addr_of!(DMA_BUF) as u32);
    hw::write(DMA1_CCR6, 0x10A1); // EN, CIRC, MINC, 8-bit, priority medium

    hw::write(USART2 + USART_CR1, CR1_UE_TE_RE);
    hw::write(USART2 + USART_CR3, 0x0040); // DMAR
}

/// Stock PM_ParseJCPacket @0x080130A0 state (docs/sensor-processing.md §2).
pub struct PmState {
    rd: usize,
    r25: RingAvg<10>,
    r10: RingAvg<10>,
    rcnt: RingAvg<10>,
    pub pm25: f32,
    pub pm10: f32,
    pub count: f32,
}

fn clamp_pm(v: f32) -> f32 {
    let v = if (v as f64) < 999.9 { v } else { 999.9f64 as f32 };
    if v > 0.0 { v } else { 0.0 }
}

fn clamp_cnt(v: f32) -> f32 {
    let v = if v < 1449855.0 { v } else { 1449855.0 };
    if v > 0.0 { v } else { 0.0 }
}

impl PmState {
    pub const fn new() -> Self {
        Self {
            rd: 0,
            r25: RingAvg::new(), r10: RingAvg::new(), rcnt: RingAvg::new(),
            pm25: 0.0, pm10: 0.0, count: 0.0,
        }
    }

    fn process(&mut self, pm25_raw: f32, pm10_raw: f32) {
        let k = crate::cal::get().k_pm;
        // Stock multiplies the unclamped averages, then clamps the products.
        // (No clean-air random filler: real value is shown, see docs.)
        self.pm25 = clamp_pm(self.r25.push(pm25_raw) * k);
        self.pm10 = clamp_pm(self.r10.push(pm10_raw) * k);
        self.count = clamp_cnt(self.rcnt.push(pm25_raw * 1450.0) * k);
    }

    /// Consume every complete packet the DMA has written since the last call.
    pub fn poll(&mut self) {
        let buf = unsafe { &*core::ptr::addr_of!(DMA_BUF) };
        let wr = (BUF_LEN - hw::read(DMA1_CNDTR6) as usize) % BUF_LEN;
        loop {
            let avail = (wr + BUF_LEN - self.rd) % BUF_LEN;
            if avail < FRAME_LEN { break; }
            let rd = self.rd;
            let at = |j: usize| buf[(rd + j) % BUF_LEN];
            if at(0) == b'J' && at(1) == b'C' && at(FRAME_LEN - 1) == 0x0D {
                let f32_at = |j: usize| f32::from_le_bytes([at(j), at(j + 1), at(j + 2), at(j + 3)]);
                self.process(f32_at(2), f32_at(6));
                self.rd = (rd + FRAME_LEN) % BUF_LEN;
            } else {
                self.rd = (rd + 1) % BUF_LEN;
            }
        }
    }

    /// Stock Format_Temperature: trunc(v*100) → tenths shown.
    pub fn pm25_tenths(&self) -> u32 { fmt_tenths(self.pm25) }
    pub fn pm10_tenths(&self) -> u32 { fmt_tenths(self.pm10) }
    /// Stock Format_LargeNumber: trunc(v*100)/100.
    pub fn count_int(&self) -> u32 { (self.count * 100.0) as u32 / 100 }
}

fn fmt_tenths(v: f32) -> u32 {
    let v = if v < 1000.0 { v } else { 999.9 };
    (v * 100.0) as u32 / 10
}
