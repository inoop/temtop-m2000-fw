use core::ptr::{read_volatile, write_volatile};

use crate::hw;

const TIM1_CR1: *mut u32 = 0x4001_2C00 as *mut u32;
const TIM1_PSC: *mut u32 = 0x4001_2C28 as *mut u32;
const TIM1_ARR: *mut u32 = 0x4001_2C2C as *mut u32;
const TIM1_CCR3: *mut u32 = 0x4001_2C3C as *mut u32;
const TIM1_CCMR2: *mut u32 = 0x4001_2C1C as *mut u32;
const TIM1_CCER: *mut u32 = 0x4001_2C20 as *mut u32;
const TIM1_BDTR: *mut u32 = 0x4001_2C44 as *mut u32;

const CCMR2_FORCE_INACTIVE: u32 = 0x0048; // OC3M = force inactive
const CCMR2_PWM1: u32 = 0x0068;

static mut STEP: u8 = 0;
static mut ENABLED: bool = false;

pub fn init_buzzer() {
    hw::rcc_enable(hw::RCC_APB2ENR, hw::apb2::TIM1);
    hw::pins::BUZZER.config(hw::AF_PP); // TIM1_CH3N

    unsafe {
        write_volatile(TIM1_CR1, 0);
        write_volatile(TIM1_PSC, 71);
        write_volatile(TIM1_ARR, 330);
        write_volatile(TIM1_CCR3, 50); // stock 15% duty
        write_volatile(TIM1_CCMR2, CCMR2_FORCE_INACTIVE);
        write_volatile(TIM1_CCER, 1 << 10); // CC3NE
        write_volatile(TIM1_BDTR, read_volatile(TIM1_BDTR) | (1 << 15)); // MOE
    }
}

fn tim1_enable() {
    unsafe {
        write_volatile(TIM1_CCMR2, CCMR2_PWM1);
        write_volatile(TIM1_CR1, read_volatile(TIM1_CR1) | 1);
    }
}

fn tim1_disable() {
    unsafe {
        write_volatile(TIM1_CR1, read_volatile(TIM1_CR1) & !1);
        write_volatile(TIM1_CCMR2, CCMR2_FORCE_INACTIVE); // PB15 rests LOW
    }
}

/// Call from the main loop each UI tick (~500 ms). While `alarm` holds, the
/// buzzer beeps on ticks 0 and 2 of every 8.
pub fn update(alarm: bool) {
    unsafe {
        if alarm != ENABLED {
            ENABLED = alarm;
            STEP = 0;
            if !alarm { tim1_disable(); }
        }
        if !ENABLED { return; }
        match STEP {
            0 | 2 => tim1_enable(),
            _ => tim1_disable(),
        }
        STEP = (STEP + 1) % 8;
    }
}
