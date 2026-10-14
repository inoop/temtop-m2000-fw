use crate::hw::{self, pins::*, Pin};

pub const KEY_UP: u8 = 0x01;
pub const KEY_DOWN: u8 = 0x02;
pub const KEY_MENU: u8 = 0x04;
pub const KEY_OK: u8 = 0x08;
pub const KEY_POWER: u8 = 0x10;

/// All active LOW with pull-up
const KEYS: [(Pin, u8); 5] = [
    (BTN_POWER, KEY_POWER), // PA0
    (BTN_MENU, KEY_MENU),   // PA1
    (BTN_OK, KEY_OK),       // PC1
    (BTN_UP, KEY_UP),       // PC2
    (BTN_DOWN, KEY_DOWN),   // PC3
];

/// Polled every main-loop pass (~10 ms). A key counts as pressed once it has
/// read the same for 3 consecutive polls; each press is reported once.
pub struct ButtonState {
    last_raw: u8,
    same: u8,
    stable: u8,
    power_since: Option<u32>,
}

const DEBOUNCE_POLLS: u8 = 3;
const LONG_PRESS_CYC: u32 = 3 * crate::clock::CYC_PER_SEC;

impl ButtonState {
    pub const fn new() -> Self {
        Self { last_raw: 0, same: 0, stable: 0, power_since: None }
    }

    /// Returns newly pressed keys (press edges).
    pub fn poll(&mut self, now: u32) -> u8 {
        let raw = read_raw();
        if raw == self.last_raw {
            if self.same < DEBOUNCE_POLLS { self.same += 1; }
        } else {
            self.same = 0;
            self.last_raw = raw;
        }
        let mut pressed = 0;
        if self.same >= DEBOUNCE_POLLS && raw != self.stable {
            pressed = raw & !self.stable;
            self.stable = raw;
        }
        if self.stable & KEY_POWER != 0 {
            if self.power_since.is_none() { self.power_since = Some(now); }
        } else {
            self.power_since = None;
        }
        pressed
    }

    pub fn power_long_press(&self, now: u32) -> bool {
        matches!(self.power_since, Some(t) if now.wrapping_sub(t) >= LONG_PRESS_CYC)
    }
}

fn read_raw() -> u8 {
    KEYS.iter().filter(|(pin, _)| !pin.is_high()).fold(0, |keys, &(_, k)| keys | k)
}

pub fn init_buttons() {
    for (pin, _) in KEYS {
        pin.config(hw::IN_PULL);
        pin.set(); // pull-up
    }
}
