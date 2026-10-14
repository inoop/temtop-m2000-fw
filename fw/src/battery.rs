/// Battery gauge (hcho_bar_graph_draw @0x08022978): 0..=11 bars from the
/// ADC ch14 average, smoothed over the last 10 updates.
pub struct Battery {
    ring: [u16; 10],
    idx: usize,
    pub bars: u32,
}

impl Battery {
    pub const fn new() -> Self {
        Self { ring: [0; 10], idx: 0, bars: 0 }
    }

    /// Call once per second.
    pub fn update(&mut self, adc: u16) {
        let x = (adc as f32 - 2251.0) / 293.0;
        let x = if x < 0.0 { 0.0 } else { x };
        self.ring[self.idx] = ((x * 11.0) as u32).min(11) as u16;
        self.idx = (self.idx + 1) % self.ring.len();
        self.bars = self.ring.iter().map(|&v| v as u32).sum::<u32>() / 10;
    }
}
