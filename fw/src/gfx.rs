// Anti-aliased drawing on the ILI9341 for the UI themes. Everything is drawn
// in a single pass with its final pixels (text is composited onto a known
// background colour) so updates never flicker.

use crate::lcd::{begin_write, lcd_data16, lcd_fill_rect};

pub const W: i32 = 240;
pub const H: i32 = 320;

pub struct Glyph {
    pub ch: char,
    pub w: u8,
    pub h: u8,
    pub x: i8,
    pub y: i8,
    pub adv: u8,
    pub ofs: u32, // nibble offset into data
}

pub struct Font {
    pub ascent: u8,
    pub descent: u8,
    pub glyphs: &'static [Glyph],
    pub data: &'static [u8],
}

impl Font {
    fn glyph(&self, ch: char) -> Option<&Glyph> {
        self.glyphs.iter().find(|g| g.ch == ch)
    }

    pub fn width(&self, s: &str) -> i32 {
        s.chars().map(|c| self.glyph(c).map_or(0, |g| g.adv as i32)).sum()
    }

    pub fn height(&self) -> i32 {
        self.ascent as i32 + self.descent as i32
    }

    #[inline]
    fn alpha(&self, g: &Glyph, x: i32, y: i32) -> u8 {
        let n = g.ofs as usize + (y as usize) * g.w as usize + x as usize;
        let b = self.data[n >> 1];
        if n & 1 == 0 { b & 0xF } else { b >> 4 }
    }
}

pub const fn rgb(r: u8, g: u8, b: u8) -> u16 {
    ((r as u16 & 0xF8) << 8) | ((g as u16 & 0xFC) << 3) | (b as u16 >> 3)
}

/// Blend fg over bg, alpha 0..=15
#[inline]
pub fn blend(fg: u16, bg: u16, a: u32) -> u16 {
    if a == 0 { return bg; }
    if a >= 15 { return fg; }
    let (fr, fgc, fb) = ((fg >> 11) as u32, ((fg >> 5) & 0x3F) as u32, (fg & 0x1F) as u32);
    let (br, bgc, bb) = ((bg >> 11) as u32, ((bg >> 5) & 0x3F) as u32, (bg & 0x1F) as u32);
    let r = (fr * a + br * (15 - a) + 7) / 15;
    let g = (fgc * a + bgc * (15 - a) + 7) / 15;
    let b = (fb * a + bb * (15 - a) + 7) / 15;
    ((r << 11) | (g << 5) | b) as u16
}

pub fn fill(x: i32, y: i32, w: i32, h: i32, c: u16) {
    let (x0, y0) = (x.max(0), y.max(0));
    let (x1, y1) = ((x + w).min(W), (y + h).min(H));
    if x1 > x0 && y1 > y0 {
        lcd_fill_rect(x0 as u16, y0 as u16, (x1 - x0) as u16, (y1 - y0) as u16, c);
    }
}

fn window(x: i32, y: i32, w: i32, h: i32) {
    begin_write(x as u16, y as u16, (x + w - 1) as u16, (y + h - 1) as u16);
}

/// Fast sqrt for small positive values
pub fn sqrtf(v: f32) -> f32 {
    if v <= 0.0 { return 0.0; }
    let mut x = f32::from_bits((v.to_bits() >> 1) + 0x1FC0_0000);
    x = 0.5 * (x + v / x);
    x = 0.5 * (x + v / x);
    x
}

/// Rounded rectangle, corners anti-aliased against `outside`
pub fn round_rect(x: i32, y: i32, w: i32, h: i32, r: i32, c: u16, outside: u16) {
    if r <= 0 {
        fill(x, y, w, h, c);
        return;
    }
    let r = r.min(w / 2).min(h / 2);
    // middle band
    fill(x, y + r, w, h - 2 * r, c);
    let rf = r as f32;
    for row in 0..r {
        // top row `row` and mirrored bottom row
        for &(yy, dy) in &[(y + row, rf - row as f32 - 0.5), (y + h - 1 - row, rf - row as f32 - 0.5)] {
            if yy < 0 || yy >= H { continue; }
            window(x, yy, w, 1);
            for col in 0..w {
                let dx = if col < r { rf - col as f32 - 0.5 }
                    else if col >= w - r { (col - (w - r)) as f32 + 0.5 } else { 0.0 };
                let px = if dx <= 0.0 {
                    c
                } else {
                    let d = sqrtf(dx * dx + dy * dy);
                    let cov = (rf - d + 0.5).clamp(0.0, 1.0);
                    blend(c, outside, (cov * 15.0 + 0.5) as u32)
                };
                lcd_data16(px);
            }
        }
    }
}

/// PIL text anchors: horizontal (Left/Middle/Right) + vertical (Ascender/Middle/baSeline)
#[allow(dead_code)] // full PIL-style anchor set
#[derive(Clone, Copy, PartialEq)]
pub enum Anchor {
    LA, LM, LS, MA, MM, MS, RA, RM, RS,
}

/// Top-left of the text cell for a PIL-style anchor
fn origin(font: &Font, x: i32, y: i32, a: Anchor, w: i32) -> (i32, i32) {
    use Anchor::*;
    let asc = font.ascent as i32;
    let ox = match a { LA | LM | LS => x, MA | MM | MS => x - w / 2, RA | RM | RS => x - w };
    let oy = match a {
        LA | MA | RA => y,
        LM | MM | RM => y - (asc + font.descent as i32) / 2,
        LS | MS | RS => y - asc,
    };
    (ox, oy)
}

/// Clear the box (bx,by,bw,bh) to `bg` and draw `s` anchored at (x,y), all in
/// one pass. Text outside the box is clipped.
pub fn text_box(bx: i32, by: i32, bw: i32, bh: i32, x: i32, y: i32, a: Anchor,
                s: &str, font: &Font, fg: u16, bg: u16) {
    let bx0 = bx.max(0);
    let by0 = by.max(0);
    let bx1 = (bx + bw).min(W);
    let by1 = (by + bh).min(H);
    if bx1 <= bx0 || by1 <= by0 { return; }
    let tw = font.width(s);
    let (ox, oy) = origin(font, x, y, a, tw);

    // glyph placements
    let mut placed: [(i32, i32, Option<&Glyph>); 40] = [(0, 0, None); 40];
    let mut n = 0;
    let mut pen = ox;
    for ch in s.chars() {
        if n >= placed.len() { break; }
        if let Some(g) = font.glyph(ch) {
            placed[n] = (pen + g.x as i32, oy + g.y as i32, Some(g));
            n += 1;
            pen += g.adv as i32;
        }
    }

    let mut line = [0u16; 240];
    let lw = (bx1 - bx0) as usize;
    window(bx0, by0, bx1 - bx0, by1 - by0);
    for yy in by0..by1 {
        for v in &mut line[..lw] { *v = bg; }
        for &(gx, gy, g) in &placed[..n] {
            let g = g.unwrap();
            let gyr = yy - gy;
            if gyr < 0 || gyr >= g.h as i32 { continue; }
            for gxr in 0..g.w as i32 {
                let px = gx + gxr;
                if px < bx0 || px >= bx1 { continue; }
                let a = font.alpha(g, gxr, gyr) as u32;
                if a == 0 { continue; }
                let slot = &mut line[(px - bx0) as usize];
                // overlapping italic glyphs: keep the stronger coverage
                *slot = blend(fg, *slot, a);
            }
        }
        for &v in &line[..lw] { lcd_data16(v); }
    }
}

/// Draw text with its own tight cell as the box
pub fn text(x: i32, y: i32, a: Anchor, s: &str, font: &Font, fg: u16, bg: u16) {
    let tw = font.width(s);
    let (ox, oy) = origin(font, x, y, a, tw);
    // italic overhang: pad 2 px each side
    text_box(ox - 2, oy, tw + 4, font.height(), x, y, a, s, font, fg, bg);
}

pub fn hline(x0: i32, x1: i32, y: i32, c: u16) {
    fill(x0, y, x1 - x0 + 1, 1, c);
}

pub fn vline(x: i32, y0: i32, y1: i32, c: u16) {
    fill(x, y0, 1, y1 - y0 + 1, c);
}

pub fn outline(x: i32, y: i32, w: i32, h: i32, c: u16) {
    hline(x, x + w - 1, y, c);
    hline(x, x + w - 1, y + h - 1, c);
    vline(x, y, y + h - 1, c);
    vline(x + w - 1, y, y + h - 1, c);
}

/// Battery icon, `bars` of 0..=11
pub fn battery(x: i32, y: i32, w: i32, h: i32, bars: u32, c: u16, bg: u16) {
    outline(x, y, w, h, c);
    fill(x + w, y + 3, 2, h - 6, c);
    let inner = w - 4;
    let fw = (inner * bars.min(11) as i32 / 11).max(1);
    fill(x + 2, y + 2, fw, h - 4, c);
    fill(x + 2 + fw, y + 2, inner - fw, h - 4, bg);
}

/// atan2 approximation, result in degrees 0..360 measured clockwise from +x
/// (screen coordinates, y down) — same convention as PIL's arc().
fn angle_deg(dx: f32, dy: f32) -> f32 {
    let ax = if dx < 0.0 { -dx } else { dx };
    let ay = if dy < 0.0 { -dy } else { dy };
    let (mn, mx) = if ax < ay { (ax, ay) } else { (ay, ax) };
    if mx == 0.0 { return 0.0; }
    let t = mn / mx;
    let s = t * t;
    let mut r = ((-0.0464964749 * s + 0.15931422) * s - 0.327622764) * s * t + t;
    if ay > ax { r = 1.57079637 - r; }
    if dx < 0.0 { r = 3.14159274 - r; }
    if dy < 0.0 { r = -r; }
    let d = r * 57.2957795;
    if d < 0.0 { d + 360.0 } else { d }
}

/// Ring gauge from 135° to 405° (gap at the bottom). The arc up to `frac`
/// is drawn in `on`, the rest in `track`. Only ring pixels are touched.
pub fn ring_gauge(cx: i32, cy: i32, r: i32, width: i32, frac: f32, on: u16, track: u16, bg: u16) {
    let ro = r as f32 + 0.5;
    let ri = (r - width) as f32 + 0.5;
    let end = 135.0 + 270.0 * frac.clamp(0.0, 1.0);
    for yy in (cy - r - 1)..=(cy + r + 1) {
        let dy = yy as f32 + 0.5 - cy as f32;
        let ox = sqrtf((ro * ro - dy * dy).max(0.0)) as i32 + 1;
        let ix = if dy.abs() < ri { sqrtf(ri * ri - dy * dy) as i32 - 1 } else { -1 };
        // Below the centre the gap (45°..135°) is |dx| < dy: leave it alone
        // so whatever sits inside it (the Hero level pill) survives redraws.
        let ix = if dy > 0.0 { ix.max(dy as i32 - 1) } else { ix };
        let segs = if ix > 0 { [(cx - ox, cx - ix), (cx + ix, cx + ox)] } else { [(cx - ox, cx + ox), (0, -1)] };
        for &(x0, x1) in &segs {
            if x1 < x0 { continue; }
            window(x0, yy, x1 - x0 + 1, 1);
            for xx in x0..=x1 {
                let dx = xx as f32 + 0.5 - cx as f32;
                let d = sqrtf(dx * dx + dy * dy);
                let cov = (ro - d).min(d - ri).clamp(0.0, 1.0);
                let mut ang = angle_deg(dx, dy);
                if ang < 135.0 { ang += 360.0; }
                let px = if cov <= 0.0 || ang > 405.0 {
                    bg
                } else {
                    let c = if ang <= end { on } else { track };
                    blend(c, bg, (cov * 15.0 + 0.5) as u32)
                };
                lcd_data16(px);
            }
        }
    }
}
