// Power latch, power-off and VBUS detect (docs/hardware-reference.md §4).

use crate::hw::{self, iwdg_feed, pins::*, OUT_PP};

/// Stock power-on sequence (verified on hardware). Writing PC0 LOW alone does
/// not hold power; the stock order is reproduced here exactly:
/// PC0 HIGH (ILI9341_Init) + PE2/PB0/PA8 HIGH, ~200ms, PB1 LOW while waiting
/// for PA0 release (init_usart2_and_wait_ready), then PE2 HIGH, PC0 LOW
/// (App_Task_SHOT).
pub fn power_on_sequence() {
    // DWT/PLL not up yet: busy-wait at 8MHz HSI, as in the verified test build
    fn early_delay_ms(ms: u32) {
        for _ in 0..ms {
            cortex_m::asm::delay(8_000);
            iwdg_feed();
        }
    }

    PWR_LATCH.config(OUT_PP);
    PWR_LATCH.set();
    BACKLIGHT.config(OUT_PP);
    BACKLIGHT.set();
    CO2_PWR.config(OUT_PP);
    AMP_PWR.config(OUT_PP);
    CO2_PWR.set();
    AMP_PWR.set();
    // Buzzer pin silent
    BUZZER.config(OUT_PP);
    BUZZER.clr();

    early_delay_ms(200);

    PM_PWR.config(OUT_PP);
    PM_PWR.clr();
    let mut n = 0;
    loop {
        if BTN_POWER.is_high() {
            break;
        }
        early_delay_ms(1);
        if n > 300 {
            PM_PWR.set();
            break;
        }
        PM_PWR.clr();
        n += 1;
    }

    BACKLIGHT.set();
    PWR_LATCH.clr();
}

/// Re-assert the latch (stock App_Task_SHOT does this periodically).
pub fn hold_latch() {
    hw::rcc_enable(hw::RCC_APB2ENR, hw::apb2::IOPC);
    PWR_LATCH.config(OUT_PP);
    PWR_LATCH.clr();
}

pub fn wait_power_key_release() {
    while !BTN_POWER.is_high() {
        cortex_m::asm::nop();
    }
}

/// Mirrors stock Power_Off_Sequence @ 0x08021060: PC0 HIGH, poll PA0 until it
/// reads LOW twice 20ms apart, then PB1 LOW. Stock then keeps running until
/// the rail drops, so keep feeding the watchdog instead of halting.
pub fn power_off() -> ! {
    PWR_LATCH.set();
    loop {
        while BTN_POWER.is_high() {
            iwdg_feed();
        }
        for _ in 0..20 {
            crate::clock::delay_us(1000);
            iwdg_feed();
        }
        if !BTN_POWER.is_high() {
            break;
        }
    }
    PM_PWR.clr();
    loop {
        iwdg_feed();
    }
}

pub fn init_vbus_detect() {
    VBUS.config(hw::IN_PULL);
    VBUS.clr(); // pull-down
}

pub fn vbus_present() -> bool {
    VBUS.is_high()
}
