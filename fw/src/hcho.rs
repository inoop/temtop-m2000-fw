// HCHO pipeline, identical to stock HCHO_ProcessReading @0x0800CDC4
// (docs/sensor-processing.md §3). No automatic baseline drift: breakpoints
// only change through the user-started zero calibration.

use crate::avg::RingAvg;
use crate::cal;

pub struct HchoState {
    avg: RingAvg<3>,
    pub value: f32, // µg/m³ after K
    // Zero calibration (HCHOWarmup_Tick @0x08006FBC)
    zc_ring: [u16; 30],
    zc_count: u32,
    zc_active: bool,
}

impl HchoState {
    pub const fn new() -> Self {
        Self { avg: RingAvg::new(), value: 0.0, zc_ring: [0; 30], zc_count: 0, zc_active: false }
    }

    /// One TIM3-period update (~1.972 s) with the ch15 10-sample average.
    pub fn process(&mut self, adc: u16) {
        self.value = self.avg.push(interpolate(adc) as f32) * cal::get().k_hcho;
    }

    /// Stock Format_FixedPoint3: clamp 5000, trunc → µg/m³ shown as d.ddd mg/m³.
    pub fn ugm3(&self) -> u32 {
        let v = if self.value < 5000.0 { self.value } else { 5000.0 };
        v as u32
    }

    pub fn start_zero_cal(&mut self) {
        self.zc_ring = [0; 30];
        self.zc_count = 0;
        self.zc_active = true;
    }

    /// Progress 0..=100 while running (stock shows counter/6).
    pub fn zero_cal_progress(&self) -> Option<u32> {
        self.zc_active.then(|| (self.zc_count / 6).min(100))
    }

    /// Call once per second while zero calibration runs. Returns Some(ok)
    /// when it finished and the result was written to flash.
    pub fn zero_cal_tick(&mut self, adc: u16) -> Option<bool> {
        if !self.zc_active { return None; }
        self.zc_count += 1;
        for i in (1..30).rev() { self.zc_ring[i] = self.zc_ring[i - 1]; }
        self.zc_ring[0] = adc;
        if self.zc_count / 6 < 100 { return None; }

        // hcho_calibration_update @0x080277FC
        self.zc_active = false;
        let mut sum = 0u32;
        for &v in &self.zc_ring { sum += v as u32; }
        let avg = (sum / 30) as u16;
        let a = &mut cal::get_mut().hcho_adc;
        if a[0] < avg {
            let d = avg - a[0];
            a[0] = avg;
            for v in &mut a[1..] { *v = v.wrapping_add(d); }
        } else {
            let d = a[0] - avg;
            a[0] = avg;
            for v in &mut a[1..] { *v = v.wrapping_sub(d); }
        }
        Some(cal::save())
    }
}

/// HCHO_PiecewiseInterp @0x0801F63C.
fn interpolate(adc: u16) -> u16 {
    let c = cal::get();
    let a = &c.hcho_adc;
    let k = &c.hcho_conc;
    let val = if (adc as i32) < a[0] as i32 { a[0] } else { adc };
    let mut r: u16 = 0; // stock leaves this uninitialised for non-monotonic tables
    for i in 0..4 {
        if a[i] as i32 <= val as i32 && (val as i32) < a[i + 1] as i32 {
            let x = (val as f32 - a[i] as f32) * (k[i + 1] as f32 - k[i] as f32)
                / (a[i + 1] as f32 - a[i] as f32)
                + k[i] as f32;
            r = x as u32 as u16;
        }
    }
    if val >= a[4] { r = k[4]; }
    r
}
