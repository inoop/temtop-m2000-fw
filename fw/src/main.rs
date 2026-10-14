#![no_std]
#![no_main]

use panic_halt as _;
use stm32f1xx_hal as _; // device interrupt vector table

use cortex_m_rt::entry;

mod adc;
mod avg;
mod battery;
mod buttons;
mod buzzer;
mod cal;
mod clock;
mod co2;
mod fmt;
mod fonts;
mod gfx;
mod hcho;
mod hw;
mod lcd;
mod menu;
mod pm;
mod power;
mod rtc;
mod sht21;
mod ui;
mod usb;

use clock::{Every, CYC_PER_SEC};
use fmt::StrBuf;
use menu::MenuEvent;
use ui::Readings;

mod boot {
    // The replacement for #[pre_init] is a global_asm! __pre_init, which
    // would mean writing this in assembly.
    #![allow(deprecated)]
    use crate::hw;

    /// Runs before RAM init: quiet interrupts and feed the watchdog left
    /// running by the bootloader as early as possible.
    #[cortex_m_rt::pre_init]
    unsafe fn pre_init() {
        core::arch::asm!("cpsid i");
        hw::iwdg_feed();
        hw::rcc_enable(hw::RCC_APB2ENR, hw::apb2::IOPA | hw::apb2::IOPB | hw::apb2::IOPC | hw::apb2::IOPE);
        let _ = hw::read(hw::RCC_APB2ENR);
        hw::nvic_disable_all();
        // Our vector table sits after the bootloader
        hw::write(hw::SCB_VTOR, 0x0800_5000);
    }
}

/// SWD only: frees PB3/PB4/PA15
fn release_jtag_pins() {
    hw::rcc_enable(hw::RCC_APB2ENR, hw::apb2::AFIO | hw::apb2::IOPB);
    hw::write(0x4001_0004, 0x0200_0000); // AFIO_MAPR.SWJ_CFG = 010
}

fn sensor_json(r: &Readings, ts: u32, vbus: bool) -> StrBuf<512> {
    let mut s = StrBuf::new();
    s.push_str("{\"ts\":");
    s.push_u32(ts);
    s.push_str(",\"pm25\":");
    s.push_dp1(r.pm25_tenths);
    s.push_str(",\"pm10\":");
    s.push_dp1(r.pm10_tenths);
    s.push_str(",\"pc\":");
    s.push_u32(r.count);
    s.push_str(",\"co2\":");
    s.push_opt(r.co2, "null");
    s.push_str(",\"hcho\":");
    s.push_dp3(r.hcho_ug);
    s.push_str(",\"t\":");
    s.push_opt(r.temp, "null");
    s.push_str(if r.celsius { ",\"tunit\":\"C\"" } else { ",\"tunit\":\"F\"" });
    s.push_str(",\"rh\":");
    s.push_opt(r.rh, "null");
    s.push_str(",\"bat\":");
    s.push_u32(r.bat_bars);
    s.push_str(",\"vbus\":");
    s.push_str(if vbus { "true" } else { "false" });
    s.push_str(if cal::from_flash() { ",\"cal\":\"flash\"" } else { ",\"cal\":\"default\"" });
    s.push_str("}\n");
    s
}

fn show_splash(theme: ui::Theme) {
    let p = theme.pal();
    gfx::fill(0, 0, 240, 320, p.bg);
    gfx::text(120, 150, gfx::Anchor::MM, "Temtop M2000", &fonts::NSI13, p.fg, p.bg);
}

#[entry]
fn main() -> ! {
    power::power_on_sequence();
    hw::iwdg_feed();
    clock::init_dwt();
    unsafe { clock::init_clock_tree(); }
    release_jtag_pins();
    buttons::init_buttons();
    power::wait_power_key_release();
    power::init_vbus_detect();
    lcd::init_fsmc_gpio();
    lcd::lcd_init();

    pm::init_usart2();
    co2::init_usart1();
    adc::init();
    sht21::init_sht21();
    buzzer::init_buzzer();
    unsafe { rtc::init_rtc(); }

    let theme = ui::Theme::load();
    show_splash(theme);

    usb::force_reenumerate();
    usb::init();

    unsafe { cortex_m::interrupt::enable(); }

    // Stock sets PB0 (CO2 power) HIGH once and never cycles it.
    hw::pins::CO2_PWR.set();
    cal::load();

    let mut pm = pm::PmState::new();
    let mut co2 = co2::Co2State::new();
    let mut co2_cal = co2::ZeroCal::new();
    let mut hcho = hcho::HchoState::new();
    let mut battery = battery::Battery::new();
    let mut temp_c: Option<f32> = None;
    let mut rh: Option<f32> = None;
    let mut secs: u32 = 0;
    let mut cal_msg: Option<&'static str> = None;

    let mut btn = buttons::ButtonState::new();
    let mut pending_keys: u8 = 0;
    let mut menu = menu::MenuState::new(theme);
    menu.load_alarms();
    let mut ui_cache = ui::UiCache::new();

    // Stock timer periods at 72 MHz: TIM2 (CO2 poll) 7101*10001, TIM3 (HCHO) 7101*20001
    let now = clock::cyccnt();
    let mut every_1s = Every::new(now, CYC_PER_SEC);
    let mut every_co2 = Every::new(now, 7101 * 10001);
    let mut every_hcho = Every::new(now, 7101 * 20001);
    let mut every_ui = Every::new(now, CYC_PER_SEC / 2);

    loop {
        clock::delay_ms(10);
        pm.poll();
        let now = clock::cyccnt();

        pending_keys |= btn.poll(now);
        if btn.power_long_press(now) {
            power::power_off();
        }

        if every_co2.due(now) {
            if let Some(ppm) = co2::request() {
                co2.process(ppm);
            }
        }

        if every_hcho.due(now) {
            hcho.process(adc::hcho());
        }

        if every_1s.due(now) {
            secs += 1;

            // RH first (0xE5), then T from that measurement (0xE0)
            if let Some(raw) = sht21::read_humidity_raw() {
                rh = Some(sht21::convert_humidity(raw));
                if let Some(raw) = sht21::read_temperature_raw() {
                    temp_c = Some(sht21::convert_temperature(raw));
                }
            }

            battery.update(adc::battery());

            if let Some(ok) = hcho.zero_cal_tick(adc::hcho()) {
                cal_msg = Some(if ok { "HCHO cal saved" } else { "HCHO cal FAILED" });
            }
            if co2_cal.tick() {
                cal_msg = Some("CO2 cal sent");
            }
        }

        // Redraw on the 500 ms tick, or immediately when a key was pressed
        if !every_ui.due(now) && pending_keys == 0 {
            continue;
        }
        hw::iwdg_feed();
        power::hold_latch();

        match menu.handle_keys(core::mem::take(&mut pending_keys)) {
            Some(MenuEvent::ThemeChanged) => menu.theme.save(),
            Some(MenuEvent::StartCo2Cal) => {
                co2_cal.start();
                cal_msg = None;
            }
            Some(MenuEvent::StartHchoCal) => {
                hcho.start_zero_cal();
                cal_msg = None;
            }
            None => {}
        }

        let c = cal::get();
        let celsius = c.temp_unit == 1;
        // Stock blanks CO2 until its warm-up counter (started at RTC second 4) reaches 183
        let co2_ready = secs >= 4 + 183 && co2.valid;
        let r = Readings {
            pm25_tenths: pm.pm25_tenths(),
            pm10_tenths: pm.pm10_tenths(),
            count: pm.count_int(),
            hcho_ug: hcho.ugm3(),
            co2: if co2_ready { Some(co2.ppm()) } else { None },
            temp: temp_c.map(|t| sht21::display_temperature(t, celsius)),
            celsius,
            rh: rh.map(sht21::display_humidity),
            bat_bars: battery.bars,
        };

        // Alarms: stock compares (int)value > threshold
        let alarm = menu.buzzer_on
            && ((pm.pm25 as i32) > menu.alarm_pm25 as i32
                || (hcho.value as i32) > menu.alarm_hcho as i32
                || (co2.value as i32) > menu.alarm_co2 as i32);
        buzzer::update(alarm);

        // Status text: calibration progress replaces the clock
        let mut msg = StrBuf::<20>::new();
        let progress = hcho.zero_cal_progress().map(|p| ("HCHO cal ", p))
            .or_else(|| co2_cal.progress().map(|p| ("CO2 cal ", p)));
        if let Some((label, p)) = progress {
            msg.push_str(label);
            msg.push_u32(p);
            msg.push(b'%');
        } else if let Some(m) = cal_msg {
            msg.push_str(m);
        }
        let (hh, mm, _) = rtc::counter_to_hms(rtc::read_counter());
        let status = ui::Status {
            time: [b'0' + hh / 10, b'0' + hh % 10, b':', b'0' + mm / 10, b'0' + mm % 10],
            msg: if msg.len() > 0 { Some(msg.as_str()) } else { None },
            alarm_on: alarm,
            alarm_pm25: menu.alarm_pm25,
            alarm_pm10: c.alarm_24,
            alarm_co2: menu.alarm_co2,
            alarm_hcho: menu.alarm_hcho,
        };

        let full = menu.take_full_redraw();
        if menu.on_dashboard() {
            if full { ui_cache.invalidate(); }
            ui::draw_dashboard(menu.theme, &mut ui_cache, &r, &status);
        } else {
            menu.render(full);
        }

        let json = sensor_json(&r, rtc::read_counter(), power::vbus_present());
        usb::write(json.as_bytes());
    }
}
