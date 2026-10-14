use core::ptr::{read_volatile, write_volatile};

const RCC_CR: *mut u32 = 0x4002_1000 as *mut u32;
const RCC_CFGR: *mut u32 = 0x4002_1004 as *mut u32;
const FLASH_ACR: *mut u32 = 0x4002_2000 as *mut u32;
const DWT_CYCCNT: *const u32 = 0xE000_1004 as *const u32;

pub const CYC_PER_SEC: u32 = 72_000_000;

pub unsafe fn init_clock_tree() {
    let cr = read_volatile(RCC_CR);

    // If PLL is already active (warm boot from stock FW), switch to HSI first
    if cr & (1 << 25) != 0 {
        // Switch SYSCLK to HSI
        let cfgr = read_volatile(RCC_CFGR);
        write_volatile(RCC_CFGR, cfgr & !0x3); // SW = HSI
        while read_volatile(RCC_CFGR) & 0xC != 0 {} // Wait SWS = HSI
        // Disable PLL
        write_volatile(RCC_CR, read_volatile(RCC_CR) & !(1 << 24));
        while read_volatile(RCC_CR) & (1 << 25) != 0 {} // Wait PLL off
    }

    // Flash: 2 wait states for 48-72 MHz, prefetch enabled
    write_volatile(FLASH_ACR, 0x32); // LATENCY=2, PRFTBE=1

    // Enable HSE (8 MHz crystal)
    write_volatile(RCC_CR, read_volatile(RCC_CR) | (1 << 16)); // HSEON
    while read_volatile(RCC_CR) & (1 << 17) == 0 {} // Wait HSERDY

    // Configure PLL and bus prescalers before enabling PLL
    let mut cfgr = read_volatile(RCC_CFGR);
    cfgr &= !(0xF << 18 | 1 << 17 | 1 << 16 | 0x7 << 8 | 0x7 << 11 | 0x3 << 14 | 1 << 22);
    cfgr |= (0b0111 << 18) // PLLMUL = ×9 → 72 MHz
          | (1 << 16)      // PLLSRC = HSE
          | (0b100 << 8)   // PPRE1 = /2 → APB1 = 36 MHz
                           // PPRE2 = /1 → APB2 = 72 MHz (bits already 0)
          | (0b11 << 14);  // ADCPRE = /8 → 9 MHz (stock)
                           // USBPRE = 0 → /1.5 → 48 MHz (bit already 0)
    write_volatile(RCC_CFGR, cfgr);

    // Enable PLL
    write_volatile(RCC_CR, read_volatile(RCC_CR) | (1 << 24)); // PLLON
    while read_volatile(RCC_CR) & (1 << 25) == 0 {} // Wait PLLRDY

    // Switch SYSCLK to PLL
    let cfgr = read_volatile(RCC_CFGR);
    write_volatile(RCC_CFGR, (cfgr & !0x3) | 0b10); // SW = PLL
    while read_volatile(RCC_CFGR) & 0xC != 0b1000 {} // Wait SWS = PLL
}

/// Start the DWT cycle counter used for all timing.
pub fn init_dwt() {
    crate::hw::modify(0xE000_EDFC, 0, 1 << 24); // DEMCR.TRCENA
    crate::hw::modify(0xE000_1000, 0, 1); // DWT_CTRL.CYCCNTENA
}

#[inline]
pub fn cyccnt() -> u32 {
    unsafe { read_volatile(DWT_CYCCNT) }
}

pub fn delay_us(us: u32) {
    let start = cyccnt();
    let ticks = us * (CYC_PER_SEC / 1_000_000);
    while cyccnt().wrapping_sub(start) < ticks {}
}

/// Busy-wait that keeps USB serviced and the watchdog fed.
pub fn delay_ms(ms: u32) {
    for _ in 0..ms {
        crate::usb::poll();
        crate::hw::iwdg_feed();
        delay_us(1000);
    }
}

/// Fixed-period timer on the cycle counter. Advances by whole periods, so
/// it does not drift when a pass runs late.
pub struct Every {
    last: u32,
    period: u32,
}

impl Every {
    pub fn new(now: u32, period: u32) -> Self {
        Every { last: now, period }
    }

    pub fn due(&mut self, now: u32) -> bool {
        if now.wrapping_sub(self.last) >= self.period {
            self.last = self.last.wrapping_add(self.period);
            true
        } else {
            false
        }
    }
}
