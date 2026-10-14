// Factory calibration block, 68 bytes at flash 0x0807FC00 (stock CalData).
// Layout and semantics: docs/sensor-processing.md §1.

use core::ptr::{read_volatile, write_volatile};

const CAL_ADDR: u32 = 0x0807_FC00;
const PAGE_ADDR: u32 = 0x0807_F800; // 2 KB page containing CAL_ADDR
const PAGE_SIZE: usize = 2048;
const CAL_SIZE: usize = 0x44;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct CalData {
    pub magic: [u8; 3],      // 0x00: 01 03 01
    pub _r03: u8,            // 0x03
    pub k_pm: f32,           // 0x04: PM2.5 / PM10 / count factor
    pub _r08: u32,           // 0x08
    pub k_hcho: f32,         // 0x0C: HCHO factor
    pub _r10: [u8; 3],       // 0x10
    pub temp_unit: u8,       // 0x13: 1 = °C, otherwise °F
    pub k_co2: f32,          // 0x14: CO2 factor
    pub _r18: u32,           // 0x18
    pub alarm_pm25: u32,     // 0x1C
    pub alarm_hcho: u32,     // 0x20
    pub alarm_24: u32,       // 0x24 (never read by stock)
    pub alarm_co2: u32,      // 0x28
    pub alarm_2c: u32,       // 0x2C (never read by stock)
    pub hcho_adc: [u16; 5],  // 0x30
    pub hcho_conc: [u16; 5], // 0x3A
}

const _: () = assert!(core::mem::size_of::<CalData>() == CAL_SIZE);

// Settings_SetDefaults @0x08020198
const DEFAULTS: CalData = CalData {
    magic: [1, 3, 1],
    _r03: 0xFF,
    k_pm: 1.0,
    _r08: 0xFFFF_FFFF,
    k_hcho: 1.0,
    _r10: [0xFF; 3],
    temp_unit: 0,
    k_co2: 1.0,
    _r18: 0xFFFF_FFFF,
    alarm_pm25: 100,
    alarm_hcho: 80,
    alarm_24: 100,
    alarm_co2: 1000,
    alarm_2c: 100,
    hcho_adc: [1526, 1636, 1771, 2303, 4096],
    hcho_conc: [0, 90, 230, 839, 5000],
};

static mut CAL: CalData = DEFAULTS;
static mut FROM_FLASH: bool = false;

/// Copy the block from flash. Unlike stock, an invalid block is NOT rewritten
/// with defaults; defaults are only used in RAM.
pub fn load() {
    unsafe {
        let mut raw = [0u8; CAL_SIZE];
        for (i, b) in raw.iter_mut().enumerate() {
            *b = read_volatile((CAL_ADDR as usize + i) as *const u8);
        }
        if raw[0] == 1 && raw[1] == 3 && raw[2] == 1 {
            CAL = core::ptr::read_unaligned(raw.as_ptr() as *const CalData);
            FROM_FLASH = true;
        } else {
            CAL = DEFAULTS;
            FROM_FLASH = false;
        }
    }
}

pub fn get() -> &'static CalData {
    unsafe { &*core::ptr::addr_of!(CAL) }
}

pub fn get_mut() -> &'static mut CalData {
    unsafe { &mut *core::ptr::addr_of_mut!(CAL) }
}

pub fn from_flash() -> bool {
    unsafe { FROM_FLASH }
}

// ---- Flash write (read-modify-write of the 2 KB page, like stock Flash_WriteHalfwords) ----

const FLASH_KEYR: *mut u32 = 0x4002_2004 as *mut u32;
const FLASH_SR: *mut u32 = 0x4002_200C as *mut u32;
const FLASH_CR: *mut u32 = 0x4002_2010 as *mut u32;
const FLASH_AR: *mut u32 = 0x4002_2014 as *mut u32;

const SR_BSY: u32 = 1 << 0;
const SR_ERR: u32 = (1 << 2) | (1 << 4); // PGERR | WRPRTERR
const SR_EOP: u32 = 1 << 5;
const CR_PG: u32 = 1 << 0;
const CR_PER: u32 = 1 << 1;
const CR_STRT: u32 = 1 << 6;
const CR_LOCK: u32 = 1 << 7;

static mut PAGE_BUF: [u16; PAGE_SIZE / 2] = [0; PAGE_SIZE / 2];

unsafe fn wait_idle() -> bool {
    while read_volatile(FLASH_SR) & SR_BSY != 0 {
        crate::hw::iwdg_feed();
    }
    let sr = read_volatile(FLASH_SR);
    write_volatile(FLASH_SR, SR_ERR | SR_EOP);
    sr & SR_ERR == 0
}

/// Write the current RAM calibration back to 0x0807FC00. Every other byte of
/// the page is preserved. Returns false on a flash error.
pub fn save() -> bool {
    unsafe {
        let buf = &mut *core::ptr::addr_of_mut!(PAGE_BUF);
        for (i, hw) in buf.iter_mut().enumerate() {
            *hw = read_volatile((PAGE_ADDR as usize + i * 2) as *const u16);
        }
        let src = core::ptr::addr_of!(CAL) as *const u16;
        let off = ((CAL_ADDR - PAGE_ADDR) / 2) as usize;
        for i in 0..CAL_SIZE / 2 {
            buf[off + i] = read_volatile(src.add(i));
        }

        cortex_m::interrupt::free(|_| {
            if read_volatile(FLASH_CR) & CR_LOCK != 0 {
                write_volatile(FLASH_KEYR, 0x4567_0123);
                write_volatile(FLASH_KEYR, 0xCDEF_89AB);
            }
            let mut ok = wait_idle();

            write_volatile(FLASH_CR, CR_PER);
            write_volatile(FLASH_AR, PAGE_ADDR);
            write_volatile(FLASH_CR, CR_PER | CR_STRT);
            ok &= wait_idle();
            write_volatile(FLASH_CR, 0);

            if ok {
                write_volatile(FLASH_CR, CR_PG);
                for (i, &hw) in buf.iter().enumerate() {
                    if hw == 0xFFFF { continue; }
                    write_volatile((PAGE_ADDR as usize + i * 2) as *mut u16, hw);
                    if !wait_idle() { ok = false; break; }
                }
                write_volatile(FLASH_CR, 0);
            }
            write_volatile(FLASH_CR, CR_LOCK);
            ok
        })
    }
}
