/// Mean of the last N samples, or of the samples so far until the ring has
/// filled once. Summation order (0.0 + r[0] + r[1] + ...) matches the stock
/// firmware bit for bit.
pub struct RingAvg<const N: usize> {
    ring: [f32; N],
    idx: usize,
    full: bool,
}

impl<const N: usize> RingAvg<N> {
    pub const fn new() -> Self {
        Self { ring: [0.0; N], idx: 0, full: false }
    }

    /// Add a sample and return the new mean.
    pub fn push(&mut self, v: f32) -> f32 {
        self.ring[self.idx] = v;
        self.idx += 1;
        if self.idx >= N {
            self.idx = 0;
            self.full = true;
        }
        let n = if self.full { N } else { self.idx };
        let mut s = 0.0f32;
        for &v in &self.ring[..n] { s += v; }
        s / n as f32
    }
}
