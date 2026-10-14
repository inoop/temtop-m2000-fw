// ADC1, stock ADC_DMA_Config @0x08005C4C: ADCCLK = PCLK2/8 (clock.rs), scan
// ch14 (rank 1, battery) + ch15 (rank 2, HCHO), continuous, 55.5-cycle
// sample time, DMA1 ch1 circular into 20 halfwords.

use crate::hw::{self, ahb, apb2, pins};

const ADC1_CR1: u32 = 0x4001_2404;
const ADC1_CR2: u32 = 0x4001_2408;
const ADC1_SMPR1: u32 = 0x4001_240C;
const ADC1_SQR1: u32 = 0x4001_242C;
const ADC1_SQR3: u32 = 0x4001_2434;
const ADC1_DR: u32 = 0x4001_244C;

const CR2_ADON: u32 = 1 << 0;
const CR2_CONT: u32 = 1 << 1;
const CR2_CAL: u32 = 1 << 2;
const CR2_RSTCAL: u32 = 1 << 3;
const CR2_DMA: u32 = 1 << 8;
const CR2_EXTSEL_SWSTART: u32 = 7 << 17;
const CR2_EXTTRIG: u32 = 1 << 20;
const CR2_SWSTART: u32 = 1 << 22;

const DMA1_IFCR: u32 = 0x4002_0004;
const DMA1_CCR1: u32 = 0x4002_0008;
const DMA1_CNDTR1: u32 = 0x4002_000C;
const DMA1_CPAR1: u32 = 0x4002_0010;
const DMA1_CMAR1: u32 = 0x4002_0014;

const LEN: usize = 20;
static mut BUF: [u16; LEN] = [0; LEN];

pub fn init() {
    hw::rcc_enable(hw::RCC_AHBENR, ahb::DMA1);
    hw::rcc_enable(hw::RCC_APB2ENR, apb2::ADC1 | apb2::IOPC);

    let rst = hw::read(hw::RCC_APB2RSTR);
    hw::write(hw::RCC_APB2RSTR, rst | apb2::ADC1);
    cortex_m::asm::delay(720);
    hw::write(hw::RCC_APB2RSTR, rst & !apb2::ADC1);

    pins::BAT_SENSE.config(hw::ANALOG);
    pins::HCHO_SENSE.config(hw::ANALOG);

    // DMA1 ch1: ADC1_DR -> BUF
    hw::write(DMA1_CCR1, 0);
    hw::write(DMA1_IFCR, 0xF);
    hw::write(DMA1_CNDTR1, LEN as u32);
    hw::write(DMA1_CPAR1, ADC1_DR);
    hw::write(DMA1_CMAR1, core::ptr::addr_of!(BUF) as u32);

    hw::write(ADC1_CR1, 1 << 8); // SCAN
    hw::write(ADC1_SMPR1, 5 << 12 | 5 << 15); // ch14/15 = 55.5 cycles
    hw::write(ADC1_SQR1, 1 << 20); // L = 2 conversions
    hw::write(ADC1_SQR3, 14 | 15 << 5);
    let cr2 = CR2_ADON | CR2_CONT | CR2_DMA | CR2_EXTSEL_SWSTART | CR2_EXTTRIG;
    hw::write(ADC1_CR2, cr2);
    crate::clock::delay_ms(2);
    hw::write(ADC1_CR2, cr2 | CR2_RSTCAL);
    wait_cr2_clear(CR2_RSTCAL);
    hw::write(ADC1_CR2, cr2 | CR2_CAL);
    wait_cr2_clear(CR2_CAL);
    // Enable DMA only after calibration so BUF stays aligned (even = ch14).
    // EN, CIRC, MINC, 16-bit periph/mem, priority high
    hw::write(DMA1_CCR1, 0x25A1);
    hw::write(ADC1_CR2, cr2 | CR2_SWSTART);
}

fn wait_cr2_clear(bit: u32) {
    for _ in 0..100_000u32 {
        if hw::read(ADC1_CR2) & bit == 0 { break; }
    }
}

fn avg(first: usize) -> u16 {
    let mut sum = 0u32;
    for i in (first..LEN).step_by(2) {
        sum += unsafe { core::ptr::read_volatile(core::ptr::addr_of!(BUF[i])) } as u32;
    }
    (sum / (LEN as u32 / 2)) as u16
}

/// ch14 10-sample average, ADC_AverageEvenSamples @0x0801E9EC
pub fn battery() -> u16 { avg(0) }

/// ch15 10-sample average, ADC_AverageOddSamples @0x0801FD48
pub fn hcho() -> u16 { avg(1) }
