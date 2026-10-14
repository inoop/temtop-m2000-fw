// Dashboard skins and menu screens. Coordinates use PIL-style text anchors;
// the safe area keeps content clear of the case bezel.

use crate::fmt::StrBuf;
use crate::fonts::*;
use crate::gfx::{self, rgb, round_rect, text, text_box, fill, hline, vline, Anchor::*, Font};
use crate::rtc;

const L: i32 = 14;
const T: i32 = 16;
const R: i32 = 14;
const B: i32 = 16;
const X0: i32 = L;
const X1: i32 = 240 - R;
const Y0: i32 = T;
const Y1: i32 = 320 - B;
const CW: i32 = X1 - X0;

#[derive(Clone, Copy, PartialEq)]
pub enum Theme { Midnight, Hero, Tiles, Paper, Terminal }

pub const THEMES: [Theme; 5] = [Theme::Midnight, Theme::Hero, Theme::Tiles, Theme::Paper, Theme::Terminal];

impl Theme {
    pub fn name(self) -> &'static str {
        match self {
            Theme::Midnight => "Midnight",
            Theme::Hero => "Hero",
            Theme::Tiles => "Tiles",
            Theme::Paper => "Paper",
            Theme::Terminal => "Terminal",
        }
    }
    pub fn index(self) -> usize {
        THEMES.iter().position(|&t| t == self).unwrap_or(0)
    }
    pub fn from_index(i: usize) -> Theme {
        THEMES[i % THEMES.len()]
    }
    /// Last theme saved in battery-backed BKP_DR2, or Midnight
    pub fn load() -> Theme {
        let saved = rtc::bkp_read(BKP_THEME);
        if saved & 0xFF00 == THEME_MAGIC { Theme::from_index((saved & 0xFF) as usize) } else { Theme::Midnight }
    }

    pub fn save(self) {
        rtc::bkp_write(BKP_THEME, THEME_MAGIC | self.index() as u16);
    }

    pub fn pal(self) -> Pal {
        match self {
            Theme::Midnight => Pal {
                bg: rgb(10, 14, 26), panel: rgb(22, 28, 46), fg: rgb(235, 240, 250),
                mute: rgb(120, 132, 160), acc: rgb(250, 200, 70),
                title: &NSI13, item: &NS11, big: &NSI24, small: &NS9,
            },
            Theme::Hero => Pal {
                bg: rgb(12, 12, 14), panel: rgb(30, 30, 36), fg: rgb(245, 245, 245),
                mute: rgb(130, 130, 140), acc: rgb(245, 195, 60),
                title: &NSI12, item: &NS10, big: &NSI38, small: &NS8,
            },
            Theme::Tiles => Pal {
                bg: rgb(14, 16, 22), panel: rgb(32, 36, 48), fg: rgb(240, 242, 246),
                mute: rgb(150, 156, 170), acc: rgb(72, 210, 140),
                title: &ARB14, item: &AR12, big: &ARB22, small: &AR9,
            },
            Theme::Paper => Pal {
                bg: rgb(250, 250, 247), panel: rgb(232, 232, 226), fg: rgb(15, 15, 15),
                mute: rgb(125, 125, 120), acc: rgb(15, 15, 15),
                title: &WSB13, item: &WS11, big: &WSB54, small: &WS9,
            },
            Theme::Terminal => Pal {
                bg: rgb(8, 6, 2), panel: rgb(48, 32, 6), fg: rgb(255, 176, 40),
                mute: rgb(120, 80, 20), acc: rgb(255, 176, 40),
                title: &DMB10, item: &DM10, big: &DMB22, small: &DM10,
            },
        }
    }
}

const BKP_THEME: u32 = 2;
const THEME_MAGIC: u16 = 0x5A00;

pub struct Pal {
    /// Outer colour behind everything (also used for the bezel dead zone)
    pub bg: u16,
    pub panel: u16,
    pub fg: u16,
    pub mute: u16,
    pub acc: u16,
    title: &'static Font, item: &'static Font, big: &'static Font, small: &'static Font,
}

/// Values exactly as the stock firmware displays them.
pub struct Readings {
    pub pm25_tenths: u32,
    pub pm10_tenths: u32,
    pub count: u32,
    pub hcho_ug: u32,
    pub co2: Option<u32>,
    pub temp: Option<u32>,
    pub celsius: bool,
    pub rh: Option<u32>,
    pub bat_bars: u32,
}

pub struct Status<'a> {
    pub time: [u8; 5],
    pub msg: Option<&'a str>,
    pub alarm_on: bool,
    pub alarm_pm25: u32,
    pub alarm_pm10: u32,
    pub alarm_co2: u32,
    pub alarm_hcho: u32,
}

impl<'a> Status<'a> {
    fn top(&self) -> &str {
        match self.msg {
            Some(m) => m,
            None => core::str::from_utf8(&self.time).unwrap_or(""),
        }
    }
}

// ---- number formatting ----

pub type Num = StrBuf<32>;

impl Num {
    fn int(v: u32) -> Self { let mut n = Num::new(); n.push_u32(v); n }
    fn dp1(tenths: u32) -> Self { let mut n = Num::new(); n.push_dp1(tenths); n }
    fn dp3(thousandths: u32) -> Self { let mut n = Num::new(); n.push_dp3(thousandths); n }
    fn grouped(v: u32) -> Self { let mut n = Num::new(); n.push_grouped(v); n }
    fn opt(v: Option<u32>) -> Self { let mut n = Num::new(); n.push_opt(v, "--"); n }
    fn text(s: &str) -> Self { let mut n = Num::new(); n.push_str(s); n }
}

/// Every reading formatted once per frame
struct Fmt {
    pm25: Num,
    pm10: Num,
    co2: Num,
    hcho: Num,
    pc: Num,
    t: Num,
    h: Num,
}

impl Fmt {
    fn new(r: &Readings) -> Self {
        Fmt {
            pm25: Num::dp1(r.pm25_tenths),
            pm10: Num::dp1(r.pm10_tenths),
            co2: Num::opt(r.co2),
            hcho: Num::dp3(r.hcho_ug),
            pc: Num::int(r.count),
            t: Num::opt(r.temp),
            h: Num::opt(r.rh),
        }
    }
}

// ---- air quality levels (0 good, 1 moderate, 2 sensitive, 3 unhealthy) ----

fn lv_pm25(t: u32) -> u8 { if t < 121 { 0 } else if t < 355 { 1 } else if t < 555 { 2 } else { 3 } }
fn lv_pm10(t: u32) -> u8 { if t < 550 { 0 } else if t < 1550 { 1 } else if t < 2550 { 2 } else { 3 } }
fn lv_co2(v: Option<u32>) -> u8 {
    match v { None => 0, Some(v) => if v < 800 { 0 } else if v < 1000 { 1 } else if v < 2000 { 2 } else { 3 } }
}
fn lv_hcho(ug: u32) -> u8 { if ug < 50 { 0 } else if ug < 80 { 1 } else { 3 } }
const LEVEL_WORD: [&str; 4] = ["GOOD", "MODERATE", "SENSITIVE", "UNHEALTHY"];
const LEVEL_LOWER: [&str; 4] = ["good", "moderate", "sensitive", "unhealthy"];

fn unit_t(r: &Readings) -> &'static str { if r.celsius { "°C" } else { "°F" } }

/// Remembers what is on screen so only changed parts are redrawn
pub struct UiCache {
    levels: [u8; 6],
    gauge: i32,
    valid: bool,
}

impl UiCache {
    pub const fn new() -> Self { UiCache { levels: [0xFF; 6], gauge: -1, valid: false } }
    pub fn invalidate(&mut self) { self.valid = false; }
}

pub fn draw_dashboard(theme: Theme, c: &mut UiCache, r: &Readings, st: &Status) {
    let full = !c.valid;
    if full {
        fill(0, 0, 240, 320, theme.pal().bg);
        c.levels = [0xFF; 6];
        c.gauge = -1;
        c.valid = true;
    }
    let f = Fmt::new(r);
    let p = theme.pal();
    match theme {
        Theme::Midnight => midnight(&p, c, r, &f, st, full),
        Theme::Hero => hero(&p, c, r, &f, st, full),
        Theme::Tiles => tiles(&p, c, r, &f, st, full),
        Theme::Paper => paper(&p, c, r, &f, st, full),
        Theme::Terminal => terminal(&p, r, &f, st, full),
    }
}

fn changed(c: &mut UiCache, i: usize, lv: u8) -> bool {
    if c.levels[i] != lv { c.levels[i] = lv; true } else { false }
}

// ------------------------------------------------------------------ Midnight

fn midnight(p: &Pal, c: &mut UiCache, r: &Readings, f: &Fmt, st: &Status, full: bool) {
    let (bg, card, fg, mute) = (p.bg, p.panel, p.fg, p.mute);
    let acc = [rgb(72, 210, 140), rgb(250, 200, 70), rgb(250, 140, 60), rgb(240, 80, 90)];

    text_box(X0, Y0, X1 - 30 - X0, 20, X0 + 2, Y0 + 2, LA, st.top(), &NSI13, fg, bg);
    gfx::battery(X1 - 26, Y0 + 4, 20, 10, r.bat_bars, mute, bg);

    let rows: [(&str, &str, &str, Option<u8>); 5] = [
        ("PM2.5", f.pm25.as_str(), "µg/m³", Some(lv_pm25(r.pm25_tenths))),
        ("PM10", f.pm10.as_str(), "µg/m³", Some(lv_pm10(r.pm10_tenths))),
        ("CO2", f.co2.as_str(), "ppm", Some(lv_co2(r.co2))),
        ("HCHO", f.hcho.as_str(), "mg/m³", Some(lv_hcho(r.hcho_ug))),
        ("Particles", f.pc.as_str(), "/L", None),
    ];
    let rh = 38;
    let mut y = Y0 + 24;
    for (i, &(name, val, unit, lv)) in rows.iter().enumerate() {
        let ch = rh - 4;
        if full {
            round_rect(X0, y, CW, ch, 6, card, bg);
            text(X0 + 12, y + ch / 2, LM, name, &NS11, mute, card);
            text(X1 - 40, y + ch / 2 + 2, LM, unit, &NS9, mute, card);
        }
        if let Some(lv) = lv {
            if changed(c, i, lv) {
                round_rect(X0, y, 4, ch, 2, acc[lv as usize], bg);
            }
        }
        text_box(X0 + 70, y + 3, X1 - 43 - (X0 + 70), ch - 6, X1 - 44, y + ch / 2 + 1, RM, val, &NSI21, fg, card);
        y += rh;
    }
    let half = (CW - 4) / 2;
    let cells = [(f.t.as_str(), unit_t(r)), (f.h.as_str(), "%RH")];
    for (i, &(v, u)) in cells.iter().enumerate() {
        let x = X0 + i as i32 * (half + 4);
        let hh = Y1 - y;
        if full {
            round_rect(x, y, half, hh, 6, card, bg);
            text(x + half / 2 - 2, y + hh / 2 + 3, LM, u, &NS10, mute, card);
        }
        text_box(x + 4, y + hh / 2 - 16, half / 2 - 8, 32, x + half / 2 - 6, y + hh / 2, RM, v, &NSI24, fg, card);
    }
}

// ---------------------------------------------------------------------- Hero

fn hero(p: &Pal, c: &mut UiCache, r: &Readings, f: &Fmt, st: &Status, full: bool) {
    let (bg, fg, mute) = (p.bg, p.fg, p.mute);
    let line = rgb(40, 40, 46);
    let acc = [rgb(70, 205, 130), rgb(245, 195, 60), rgb(245, 135, 55), rgb(235, 75, 85)];
    let dark = rgb(20, 20, 20);

    text_box(X0, Y0, X1 - 30 - X0, 18, X0 + 2, Y0 + 2, LA, st.top(), &NSI12, mute, bg);
    gfx::battery(X1 - 26, Y0 + 3, 20, 10, r.bat_bars, mute, bg);

    let (cx, cy, rad) = (120, Y0 + 92, 66);
    let lv = lv_pm25(r.pm25_tenths);
    let frac = (r.pm25_tenths as f32 / 555.0).min(1.0);
    let g = (frac * 270.0) as i32;
    let lv_changed = changed(c, 0, lv);
    if g != c.gauge || lv_changed {
        c.gauge = g;
        gfx::ring_gauge(cx, cy, rad, 10, frac, acc[lv as usize], line, bg);
    }
    let v = f.pm25.as_str();
    let big: &Font = if NSI38.width(v) <= 98 { &NSI38 } else { &NSI24 };
    text_box(cx - 50, cy - 28, 100, 44, cx, cy - 6, MM, v, big, fg, bg);
    if full {
        text(cx, cy + 22, MM, "PM2.5 µg/m³", &NS10, mute, bg);
    }
    if lv_changed {
        round_rect(cx - 40, cy + 46, 80, 18, 9, acc[lv as usize], bg);
        text_box(cx - 36, cy + 48, 72, 14, cx, cy + 55, MM, LEVEL_WORD[lv as usize], &NSI10, dark, acc[lv as usize]);
    }

    let cells: [(&str, &str, &str); 6] = [
        ("PM10", f.pm10.as_str(), "µg/m³"), ("CO2", f.co2.as_str(), "ppm"),
        ("HCHO", f.hcho.as_str(), "mg/m³"), ("PARTICLES", f.pc.as_str(), "/L"),
        ("TEMP", f.t.as_str(), unit_t(r)), ("HUMIDITY", f.h.as_str(), "%"),
    ];
    let gy = Y0 + 178;
    let cw = CW / 3;
    let chh = (Y1 - gy) / 2;
    if full {
        hline(X0, X1, gy, line);
        hline(X0, X1, gy + chh, line);
        for i in 1..3 {
            vline(X0 + i * cw, gy + 4, Y1 - 4, line);
        }
    }
    for (i, &(n, v, u)) in cells.iter().enumerate() {
        let x = X0 + (i as i32 % 3) * cw;
        let y = gy + (i as i32 / 3) * chh;
        if full {
            text(x + cw / 2, y + 10, MM, n, &NSI8, mute, bg);
            text(x + cw / 2, y + 47, MM, u, &NS8, mute, bg);
        }
        let font: &Font = if NSI17.width(v) <= cw - 6 { &NSI17 } else { &NSI12 };
        text_box(x + 2, y + 19, cw - 3, 22, x + cw / 2, y + 30, MM, v, font, fg, bg);
    }
}

// --------------------------------------------------------------- Tiles (dark)

fn tiles(p: &Pal, c: &mut UiCache, r: &Readings, f: &Fmt, st: &Status, full: bool) {
    let (bg, neutral, ink, mute) = (p.bg, p.panel, p.fg, p.mute);
    let tint = [rgb(22, 70, 48), rgb(84, 70, 20), rgb(90, 50, 22), rgb(96, 30, 38)];
    let word = [rgb(110, 225, 160), rgb(250, 210, 90), rgb(250, 160, 90), rgb(250, 120, 130)];

    if full {
        text(X0 + 2, Y0 + 1, LA, "Air", &ARB14, ink, bg);
    }
    text_box(X0 + 40, Y0, X1 - 30 - (X0 + 40), 18, X1 - 30, Y0 + 2, RA, st.top(), &AR12, mute, bg);
    gfx::battery(X1 - 24, Y0 + 3, 20, 10, r.bat_bars, mute, bg);

    let items: [(&str, &str, &str, Option<u8>); 6] = [
        ("PM2.5", f.pm25.as_str(), "µg/m³", Some(lv_pm25(r.pm25_tenths))),
        ("PM10", f.pm10.as_str(), "µg/m³", Some(lv_pm10(r.pm10_tenths))),
        ("CO₂", f.co2.as_str(), "ppm", Some(lv_co2(r.co2))),
        ("HCHO", f.hcho.as_str(), "mg/m³", Some(lv_hcho(r.hcho_ug))),
        ("Temp", f.t.as_str(), unit_t(r), None),
        ("Humidity", f.h.as_str(), "%", None),
    ];
    let top = Y0 + 24;
    let gap = 6;
    let tw = (CW - gap) / 2;
    let th = (Y1 - top - 2 * gap - 26) / 3;
    for (i, &(n, v, u, lv)) in items.iter().enumerate() {
        let x = X0 + (i as i32 % 2) * (tw + gap);
        let y = top + (i as i32 / 2) * (th + gap);
        let lvv = lv.unwrap_or(0xFE);
        let col = match lv { Some(l) => tint[l as usize], None => neutral };
        if changed(c, i, lvv) || full {
            round_rect(x, y, tw, th, 10, col, bg);
            text(x + 9, y + 8, LA, n, &AR10, mute, col);
            text(x + tw - 8, y + 24, RA, u, &AR9, mute, col);
            if let Some(l) = lv {
                text(x + tw - 8, y + 9, RA, LEVEL_LOWER[l as usize], &AR8, word[l as usize], col);
            }
        }
        text_box(x + 5, y + th - 32, tw - 10, 26, x + 9, y + th - 10, LS, v, &ARB22, ink, col);
    }
    let y = Y1 - 22;
    if full {
        round_rect(X0, y, CW, 22, 8, neutral, bg);
        text(X0 + 9, y + 11, LM, "Particles", &AR10, mute, neutral);
        text(X1 - 8, y + 11, RM, "/L", &AR9, mute, neutral);
    }
    let pc = Num::grouped(r.count);
    text_box(X0 + 70, y + 3, X1 - 29 - (X0 + 70), 16, X1 - 30, y + 11, RM, pc.as_str(), &ARB13, ink, neutral);
}

// --------------------------------------------------------------------- Paper

fn paper(p: &Pal, c: &mut UiCache, r: &Readings, f: &Fmt, st: &Status, full: bool) {
    let (bg, ink, mute) = (p.bg, p.fg, p.mute);
    let rule = rgb(210, 210, 205);
    let (x0, x1) = (X0 + 6, X1 - 6);

    text_box(x0, Y0, x1 - 30 - x0, 16, x0, Y0 + 2, LA, st.top(), &WS11, mute, bg);
    gfx::battery(x1 - 24, Y0 + 3, 20, 9, r.bat_bars, mute, bg);
    if full {
        hline(x0, x1, Y0 + 18, ink);
        text(x0, Y0 + 26, LA, "PM2.5", &WS11, mute, bg);
        text(x1, Y0 + 82, RS, "µg/m³", &WS11, mute, bg);
    }
    text_box(x0 - 2, Y0 + 82 - WSB54.ascent as i32 + 6, x1 - 48 - x0, WSB54.ascent as i32 - 2, x0, Y0 + 82, LS, f.pm25.as_str(), &WSB54, ink, bg);
    let lv = lv_pm25(r.pm25_tenths);
    if changed(c, 0, lv) {
        for i in 0..4 {
            fill(x0 + i * 14, Y0 + 92, 10, 6, if i <= lv as i32 { ink } else { rule });
        }
        text_box(x0 + 60, Y0 + 88, x1 - x0 - 60, 14, x0 + 62, Y0 + 95, LM, LEVEL_LOWER[lv as usize], &WS9, ink, bg);
    }

    let rows: [(&str, &str, &str); 6] = [
        ("PM10", f.pm10.as_str(), "µg/m³"), ("CO₂", f.co2.as_str(), "ppm"),
        ("HCHO", f.hcho.as_str(), "mg/m³"), ("Particles", f.pc.as_str(), "per L"),
        ("Temperature", f.t.as_str(), unit_t(r)), ("Humidity", f.h.as_str(), "%"),
    ];
    let mut y = Y0 + 108;
    let rh = (Y1 - y) / 6;
    for &(n, v, u) in &rows {
        if full {
            hline(x0, x1, y, rule);
            text(x0, y + rh / 2, LM, n, &WS11, mute, bg);
            text(x1, y + rh / 2 + 1, RM, u, &WS9, mute, bg);
        }
        text_box(x0 + 80, y + 4, x1 - 33 - (x0 + 80), rh - 7, x1 - 34, y + rh / 2, RM, v, &WSB17, ink, bg);
        y += rh;
    }
}

// ------------------------------------------------------------------ Terminal

fn terminal(p: &Pal, r: &Readings, f: &Fmt, st: &Status, full: bool) {
    let (bg, amber, dim) = (p.bg, p.fg, p.mute);
    let red = rgb(255, 90, 60);
    let (x0, x1) = (X0 + 6, X1 - 6);
    let m = &DM10;

    if full {
        text(x0, Y0 + 2, LA, "TEMTOP M2000", m, amber, bg);
        text(x0, Y0 + 14, LA, "------------------------------", m, dim, bg);
    }
    // Status after the title; the clock is indented a little further
    let sx = x0 + m.width("TEMTOP M2000 ");
    let tx = if st.msg.is_some() { sx } else { x0 + m.width("TEMTOP M2000      ") };
    text_box(sx, Y0 + 2, x1 - sx, 13, tx, Y0 + 2, LA, st.top(), m, amber, bg);

    let items: [(&str, &str, &str, u32, u32); 4] = [
        ("PM2.5", f.pm25.as_str(), "ug/m3", r.pm25_tenths, st.alarm_pm25 * 10),
        ("PM10", f.pm10.as_str(), "ug/m3", r.pm10_tenths, st.alarm_pm10 * 10),
        ("CO2", f.co2.as_str(), "ppm", r.co2.unwrap_or(0), st.alarm_co2),
        ("HCHO", f.hcho.as_str(), "mg/m3", r.hcho_ug, st.alarm_hcho),
    ];
    let mut y = Y0 + 28;
    for &(n, v, u, raw, alarm) in &items {
        if full {
            text(x0, y, LA, n, &DMB10, amber, bg);
            text(x1, y + 1, RA, u, m, dim, bg);
        }
        text_box(x0 + 44, y - 3, x1 - 38 - (x0 + 44), 26, x1 - 40, y - 3, RA, v, &DMB22, amber, bg);
        let nb = 26u32;
        let on = if alarm == 0 { nb } else { ((raw * nb + alarm / 2) / alarm).min(nb) };
        let mut bar = [b'.'; 28];
        bar[0] = b'[';
        bar[27] = b']';
        for b in &mut bar[1..1 + on as usize] { *b = b'#'; }
        let s = core::str::from_utf8(&bar).unwrap_or("");
        text_box(x0, y + 22, x1 - x0, 13, x0, y + 22, LA, s, m, if on >= nb { red } else { amber }, bg);
        y += 44;
    }
    if full {
        text(x0, y, LA, "------------------------------", m, dim, bg);
    }
    let mut l1 = Num::new();
    l1.push_str("PARTICLES ");
    for _ in f.pc.len()..9 { l1.push(b' '); }
    l1.push_str(f.pc.as_str());
    l1.push_str(" /L");
    text_box(x0, y + 12, x1 - x0, 13, x0, y + 12, LA, l1.as_str(), m, amber, bg);

    let mut l2 = Num::new();
    l2.push_str("TEMP ");
    for _ in f.t.len()..3 { l2.push(b' '); }
    l2.push_str(f.t.as_str());
    l2.push(if r.celsius { b'C' } else { b'F' });
    l2.push_str("    RH ");
    for _ in f.h.len()..3 { l2.push(b' '); }
    l2.push_str(f.h.as_str());
    l2.push(b'%');
    text_box(x0, y + 26, x1 - x0, 13, x0, y + 26, LA, l2.as_str(), m, amber, bg);

    let mut l3 = [b' '; 20];
    l3[..5].copy_from_slice(b"BAT [");
    for i in 0..11 { l3[5 + i] = if (i as u32) < r.bat_bars { b'|' } else { b' ' }; }
    l3[16] = b']';
    text_box(x0, y + 40, x1 - x0, 13, x0, y + 40, LA, core::str::from_utf8(&l3).unwrap_or(""), m, dim, bg);
    text_box(x0, Y1 - 12, x1 - x0, 12, x0, Y1 - 12, LA,
             if st.alarm_on { "> ALARM: ON   menu=PA1" } else { "> alarm: off  menu=PA1" }, m,
             if st.alarm_on { red } else { dim }, bg);
}

// ---------------------------------------------------------------------- Menu

pub struct MenuRow<'a> {
    pub label: &'a str,
    pub value: Num,
}

impl<'a> MenuRow<'a> {
    pub fn with_int(label: &'a str, v: u32) -> Self { MenuRow { label, value: Num::int(v) } }
    pub fn with_str(label: &'a str, s: &str) -> Self { MenuRow { label, value: Num::text(s) } }
}

fn header(p: &Pal, title: &str) {
    fill(0, 0, 240, 320, p.bg);
    text(X0 + 2, Y0 + 2, LA, title, p.title, p.fg, p.bg);
    hline(X0, X1, Y0 + 20, p.mute);
}

fn footer(p: &Pal, hint: &str) {
    text(X0 + 2, Y1 - 2, LS, hint, p.small, p.mute, p.bg);
}

pub fn draw_menu(theme: Theme, rows: &[MenuRow], selected: usize) {
    let p = theme.pal();
    header(&p, "Settings");
    let rh = 30;
    for (i, row) in rows.iter().enumerate() {
        let y = Y0 + 26 + i as i32 * rh;
        let sel = i == selected;
        let bg = if sel { p.panel } else { p.bg };
        if sel {
            round_rect(X0, y, CW, rh - 4, 6, p.panel, p.bg);
            round_rect(X0, y, 4, rh - 4, 2, p.acc, p.bg);
        }
        text(X0 + 12, y + (rh - 4) / 2, LM, row.label, p.item, if sel { p.fg } else { p.mute }, bg);
        text(X1 - 8, y + (rh - 4) / 2, RM, row.value.as_str(), p.item, if sel { p.acc } else { p.mute }, bg);
    }
    footer(&p, "OK select   MENU back");
}

pub fn draw_editor(theme: Theme, title: &str, value: u32) {
    let p = theme.pal();
    header(&p, title);
    redraw_editor_value(theme, value);
    text(120, 220, MM, "UP / DOWN to change", p.item, p.mute, p.bg);
    footer(&p, "OK save   MENU cancel");
}

pub fn redraw_editor_value(theme: Theme, value: u32) {
    let p = theme.pal();
    let v = Num::int(value);
    text_box(X0, 110, CW, 80, 120, 150, MM, v.as_str(), p.big, p.fg, p.bg);
}

pub fn draw_confirm(theme: Theme, title: &str, question: &str) {
    let p = theme.pal();
    header(&p, title);
    round_rect(X0, 120, CW, 60, 8, p.panel, p.bg);
    text(120, 150, MM, question, p.item, p.fg, p.panel);
    footer(&p, "OK yes   MENU no");
}
