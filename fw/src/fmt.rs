// Fixed-capacity string building for the display and the USB JSON stream.
// Pushes past the capacity are dropped.

pub struct StrBuf<const N: usize> {
    buf: [u8; N],
    len: usize,
}

impl<const N: usize> StrBuf<N> {
    pub const fn new() -> Self {
        StrBuf { buf: [0; N], len: 0 }
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.buf[..self.len]
    }

    pub fn as_str(&self) -> &str {
        core::str::from_utf8(self.as_bytes()).unwrap_or("")
    }

    pub fn push(&mut self, b: u8) {
        if self.len < N {
            self.buf[self.len] = b;
            self.len += 1;
        }
    }

    pub fn push_str(&mut self, s: &str) {
        for &b in s.as_bytes() { self.push(b); }
    }

    pub fn push_u32(&mut self, v: u32) {
        let mut tmp = [0u8; 10];
        let mut i = tmp.len();
        let mut v = v;
        loop {
            i -= 1;
            tmp[i] = b'0' + (v % 10) as u8;
            v /= 10;
            if v == 0 { break; }
        }
        for &b in &tmp[i..] { self.push(b); }
    }

    /// `v` with thousands separators: 12,345
    pub fn push_grouped(&mut self, v: u32) {
        let mut digits = StrBuf::<10>::new();
        digits.push_u32(v);
        let len = digits.len();
        for (i, &b) in digits.as_bytes().iter().enumerate() {
            if i > 0 && (len - i) % 3 == 0 { self.push(b','); }
            self.push(b);
        }
    }

    /// Tenths as d.d
    pub fn push_dp1(&mut self, tenths: u32) {
        self.push_u32(tenths / 10);
        self.push(b'.');
        self.push(b'0' + (tenths % 10) as u8);
    }

    /// Thousandths as d.ddd
    pub fn push_dp3(&mut self, thousandths: u32) {
        self.push_u32(thousandths / 1000);
        self.push(b'.');
        let f = thousandths % 1000;
        self.push(b'0' + (f / 100) as u8);
        self.push(b'0' + (f / 10 % 10) as u8);
        self.push(b'0' + (f % 10) as u8);
    }

    pub fn push_opt(&mut self, v: Option<u32>, none: &str) {
        match v {
            Some(v) => self.push_u32(v),
            None => self.push_str(none),
        }
    }
}
