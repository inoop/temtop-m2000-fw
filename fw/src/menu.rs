use crate::buttons::{KEY_MENU, KEY_OK, KEY_UP, KEY_DOWN};
use crate::hw::pins::BACKLIGHT;
use crate::ui::{self, MenuRow, Theme};

#[derive(Clone, Copy, PartialEq)]
enum Screen {
    Dashboard,
    MenuList,
    EditValue(Item),
    Confirm(Item),
}

#[derive(Clone, Copy, PartialEq)]
enum Item {
    AlarmPm25,
    AlarmHcho,
    AlarmCo2,
    Buzzer,
    Theme,
    Co2Cal,
    HchoCal,
    Backlight,
}

const ITEMS: [Item; 8] = [
    Item::AlarmPm25, Item::AlarmHcho, Item::AlarmCo2, Item::Buzzer,
    Item::Theme, Item::Co2Cal, Item::HchoCal, Item::Backlight,
];

enum Kind {
    Value { max: u32, step: u32 },
    Toggle,
    Choice,
    Action { question: &'static str },
}

impl Item {
    fn label(self) -> &'static str {
        match self {
            Item::AlarmPm25 => "PM2.5 alarm",
            Item::AlarmHcho => "HCHO alarm",
            Item::AlarmCo2 => "CO2 alarm",
            Item::Buzzer => "Buzzer",
            Item::Theme => "Theme",
            Item::Co2Cal => "CO2 zero cal",
            Item::HchoCal => "HCHO zero cal",
            Item::Backlight => "Backlight",
        }
    }

    fn kind(self) -> Kind {
        match self {
            Item::AlarmPm25 | Item::AlarmHcho => Kind::Value { max: 999, step: 10 },
            Item::AlarmCo2 => Kind::Value { max: 5000, step: 100 },
            Item::Buzzer | Item::Backlight => Kind::Toggle,
            Item::Theme => Kind::Choice,
            Item::Co2Cal => Kind::Action { question: "Start 30 min CO2 cal?" },
            Item::HchoCal => Kind::Action { question: "Start 10 min HCHO cal?" },
        }
    }
}

/// Something the main loop has to act on
pub enum MenuEvent {
    ThemeChanged,
    StartCo2Cal,
    StartHchoCal,
}

pub struct MenuState {
    screen: Screen,
    selected: usize,
    edit_val: u32,
    pub alarm_pm25: u32,
    pub alarm_hcho: u32,
    pub alarm_co2: u32,
    pub buzzer_on: bool,
    backlight_on: bool,
    pub theme: Theme,
    need_full: bool,
    need_value: bool,
}

impl MenuState {
    pub const fn new(theme: Theme) -> Self {
        Self {
            screen: Screen::Dashboard,
            selected: 0,
            edit_val: 0,
            alarm_pm25: 100,
            alarm_hcho: 80,
            alarm_co2: 1000,
            buzzer_on: true,
            backlight_on: true,
            theme,
            need_full: true,
            need_value: false,
        }
    }

    /// Alarm thresholds come from CalData (+0x1C/+0x20/+0x28).
    pub fn load_alarms(&mut self) {
        let c = crate::cal::get();
        self.alarm_pm25 = c.alarm_pm25;
        self.alarm_hcho = c.alarm_hcho;
        self.alarm_co2 = c.alarm_co2;
    }

    pub fn on_dashboard(&self) -> bool {
        self.screen == Screen::Dashboard
    }

    fn value(&self, item: Item) -> u32 {
        match item {
            Item::AlarmPm25 => self.alarm_pm25,
            Item::AlarmHcho => self.alarm_hcho,
            Item::AlarmCo2 => self.alarm_co2,
            _ => 0,
        }
    }

    fn set_value(&mut self, item: Item, v: u32) {
        match item {
            Item::AlarmPm25 => self.alarm_pm25 = v,
            Item::AlarmHcho => self.alarm_hcho = v,
            Item::AlarmCo2 => self.alarm_co2 = v,
            _ => {}
        }
    }

    fn is_on(&self, item: Item) -> bool {
        match item {
            Item::Buzzer => self.buzzer_on,
            Item::Backlight => self.backlight_on,
            _ => false,
        }
    }

    fn toggle(&mut self, item: Item) {
        match item {
            Item::Buzzer => self.buzzer_on = !self.buzzer_on,
            Item::Backlight => {
                self.backlight_on = !self.backlight_on;
                if self.backlight_on { BACKLIGHT.set(); } else { BACKLIGHT.clr(); }
            }
            _ => {}
        }
    }

    fn go(&mut self, s: Screen) {
        self.screen = s;
        self.need_full = true;
    }

    pub fn handle_keys(&mut self, keys: u8) -> Option<MenuEvent> {
        if keys == 0 { return None; }
        match self.screen {
            Screen::Dashboard => {
                if keys & KEY_MENU != 0 {
                    self.selected = 0;
                    self.go(Screen::MenuList);
                }
            }
            Screen::MenuList => {
                if keys & KEY_MENU != 0 {
                    self.go(Screen::Dashboard);
                } else if keys & KEY_UP != 0 {
                    if self.selected > 0 { self.selected -= 1; self.need_full = true; }
                } else if keys & KEY_DOWN != 0 {
                    if self.selected + 1 < ITEMS.len() { self.selected += 1; self.need_full = true; }
                } else if keys & KEY_OK != 0 {
                    return self.activate(ITEMS[self.selected]);
                }
            }
            Screen::EditValue(item) => {
                if let Kind::Value { max, step } = item.kind() {
                    if keys & KEY_UP != 0 {
                        self.edit_val = (self.edit_val + step).min(max);
                        self.need_value = true;
                    } else if keys & KEY_DOWN != 0 {
                        self.edit_val = self.edit_val.saturating_sub(step);
                        self.need_value = true;
                    } else if keys & KEY_OK != 0 {
                        self.set_value(item, self.edit_val);
                        self.go(Screen::MenuList);
                    } else if keys & KEY_MENU != 0 {
                        self.go(Screen::MenuList);
                    }
                }
            }
            Screen::Confirm(item) => {
                if keys & KEY_OK != 0 {
                    self.go(Screen::Dashboard);
                    return match item {
                        Item::Co2Cal => Some(MenuEvent::StartCo2Cal),
                        Item::HchoCal => Some(MenuEvent::StartHchoCal),
                        _ => None,
                    };
                } else if keys & KEY_MENU != 0 {
                    self.go(Screen::MenuList);
                }
            }
        }
        None
    }

    /// OK pressed on a menu row
    fn activate(&mut self, item: Item) -> Option<MenuEvent> {
        match item.kind() {
            Kind::Value { .. } => {
                self.edit_val = self.value(item);
                self.go(Screen::EditValue(item));
            }
            Kind::Toggle => {
                self.toggle(item);
                self.need_full = true;
            }
            Kind::Choice => {
                self.theme = Theme::from_index(self.theme.index() + 1);
                self.need_full = true;
                return Some(MenuEvent::ThemeChanged);
            }
            Kind::Action { .. } => self.go(Screen::Confirm(item)),
        }
        None
    }

    /// Returns true (once) when the current screen must be fully redrawn.
    pub fn take_full_redraw(&mut self) -> bool {
        let f = self.need_full;
        self.need_full = false;
        f
    }

    /// Draw the menu screens. The dashboard is drawn by the caller.
    pub fn render(&mut self, full: bool) {
        match self.screen {
            Screen::Dashboard => {}
            Screen::MenuList => {
                if full { self.draw_list(); }
            }
            Screen::EditValue(item) => {
                if full {
                    ui::draw_editor(self.theme, item.label(), self.edit_val);
                } else if self.need_value {
                    ui::redraw_editor_value(self.theme, self.edit_val);
                }
                self.need_value = false;
            }
            Screen::Confirm(item) => {
                if full {
                    let q = match item.kind() { Kind::Action { question } => question, _ => "" };
                    ui::draw_confirm(self.theme, item.label(), q);
                }
            }
        }
    }

    fn draw_list(&self) {
        let rows: [MenuRow; ITEMS.len()] = core::array::from_fn(|i| {
            let item = ITEMS[i];
            match item.kind() {
                Kind::Value { .. } => MenuRow::with_int(item.label(), self.value(item)),
                Kind::Toggle => MenuRow::with_str(item.label(), if self.is_on(item) { "ON" } else { "OFF" }),
                Kind::Choice => MenuRow::with_str(item.label(), self.theme.name()),
                Kind::Action { .. } => MenuRow::with_str(item.label(), ">"),
            }
        });
        ui::draw_menu(self.theme, &rows, self.selected);
    }
}
