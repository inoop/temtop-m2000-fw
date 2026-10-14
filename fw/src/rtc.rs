use core::ptr::{read_volatile, write_volatile};

use crate::hw;

const RCC_BDCR: *mut u32 = 0x4002_1020 as *mut u32;
const PWR_CR: *mut u32 = 0x4000_7000 as *mut u32;
const RTC_CRL: *mut u32 = 0x4000_2804 as *mut u32;
const RTC_PRLL: *mut u32 = 0x4000_280C as *mut u32;
const RTC_CNTH: *const u32 = 0x4000_2818 as *const u32;
const RTC_CNTL: *const u32 = 0x4000_281C as *const u32;
const BKP_DR1: *mut u32 = 0x4000_6C04 as *mut u32;

const MAGIC: u32 = 0xA5A5;

pub unsafe fn init_rtc() {
    hw::rcc_enable(hw::RCC_APB1ENR, hw::apb1::PWR | hw::apb1::BKP);

    // Enable backup domain write access
    let pwr = read_volatile(PWR_CR);
    write_volatile(PWR_CR, pwr | (1 << 8));

    if read_volatile(BKP_DR1) == MAGIC {
        wait_rsf();
        return;
    }

    // First-time init: enable LSE, configure RTC
    let bdcr = read_volatile(RCC_BDCR);
    write_volatile(RCC_BDCR, bdcr | 1); // LSEON
    for _ in 0..2_000_000u32 {
        if read_volatile(RCC_BDCR) & 2 != 0 { break; } // LSERDY
    }

    // Select LSE as RTC clock, enable RTC
    let bdcr = read_volatile(RCC_BDCR);
    write_volatile(RCC_BDCR, (bdcr & !(3 << 8)) | (1 << 8) | (1 << 15));

    wait_rsf();
    wait_rtoff();

    // Enter config mode
    write_volatile(RTC_CRL, read_volatile(RTC_CRL) | (1 << 4)); // CNF=1

    // Prescaler: 32768 Hz / 32768 = 1 Hz
    write_volatile(RTC_PRLL, 0x7FFF);

    // Exit config mode
    write_volatile(RTC_CRL, read_volatile(RTC_CRL) & !(1 << 4)); // CNF=0
    wait_rtoff();

    // Write magic to BKP_DR1
    write_volatile(BKP_DR1, MAGIC);
}

fn wait_rsf() {
    unsafe {
        // Clear RSF
        write_volatile(RTC_CRL, read_volatile(RTC_CRL) & !(1 << 3));
        for _ in 0..1_000_000u32 {
            if read_volatile(RTC_CRL) & (1 << 3) != 0 { break; }
        }
    }
}

fn wait_rtoff() {
    unsafe {
        for _ in 0..1_000_000u32 {
            if read_volatile(RTC_CRL) & (1 << 5) != 0 { break; }
        }
    }
}

pub fn read_counter() -> u32 {
    unsafe {
        let h1 = read_volatile(RTC_CNTH) & 0xFFFF;
        let l = read_volatile(RTC_CNTL) & 0xFFFF;
        let h2 = read_volatile(RTC_CNTH) & 0xFFFF;
        if h1 != h2 {
            let l2 = read_volatile(RTC_CNTL) & 0xFFFF;
            (h2 << 16) | l2
        } else {
            (h1 << 16) | l
        }
    }
}

pub fn counter_to_hms(counter: u32) -> (u8, u8, u8) {
    let secs = counter % 86400;
    let h = (secs / 3600) as u8;
    let m = ((secs % 3600) / 60) as u8;
    let s = (secs % 60) as u8;
    (h, m, s)
}

/// Battery-backed backup data register n (1..=10)
pub fn bkp_read(n: u32) -> u16 {
    unsafe { read_volatile((0x4000_6C00 + 4 * n) as *const u32) as u16 }
}

pub fn bkp_write(n: u32, v: u16) {
    unsafe { write_volatile((0x4000_6C00 + 4 * n) as *mut u32, v as u32) }
}
