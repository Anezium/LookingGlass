//! Gameplay bindings and Windows XInput. Menu controls remain fixed and usable
//! even when every gameplay action has been rebound. No original DLL is loaded.
use crate::preferences::Preferences;
use macroquad::prelude::*;
use std::collections::HashSet;

pub const KEYS: &[(&str, KeyCode)] = &[
    ("W", KeyCode::W),
    ("S", KeyCode::S),
    ("A", KeyCode::A),
    ("D", KeyCode::D),
    ("Space", KeyCode::Space),
    ("E", KeyCode::E),
    ("Shift", KeyCode::LeftShift),
    ("C", KeyCode::C),
    ("B", KeyCode::B),
    ("G", KeyCode::G),
    ("J", KeyCode::J),
    ("K", KeyCode::K),
    ("L", KeyCode::L),
    ("N", KeyCode::N),
    ("O", KeyCode::O),
    ("T", KeyCode::T),
    ("U", KeyCode::U),
    ("Y", KeyCode::Y),
    ("Z", KeyCode::Z),
    ("Q", KeyCode::Q),
    ("R", KeyCode::R),
    ("F", KeyCode::F),
    ("H", KeyCode::H),
    ("I", KeyCode::I),
    ("M", KeyCode::M),
    ("P", KeyCode::P),
    ("V", KeyCode::V),
    ("X", KeyCode::X),
    ("Ctrl", KeyCode::LeftControl),
    ("Alt", KeyCode::LeftAlt),
    ("R Shift", KeyCode::RightShift),
    ("R Ctrl", KeyCode::RightControl),
    ("R Alt", KeyCode::RightAlt),
    ("Tab", KeyCode::Tab),
    ("Enter", KeyCode::Enter),
    ("Backspace", KeyCode::Backspace),
    ("Delete", KeyCode::Delete),
    ("Insert", KeyCode::Insert),
    ("Home", KeyCode::Home),
    ("End", KeyCode::End),
    ("Page Up", KeyCode::PageUp),
    ("Page Down", KeyCode::PageDown),
    ("Up", KeyCode::Up),
    ("Down", KeyCode::Down),
    ("Left", KeyCode::Left),
    ("Right", KeyCode::Right),
    ("[", KeyCode::LeftBracket),
    ("]", KeyCode::RightBracket),
    ("-", KeyCode::Minus),
    ("=", KeyCode::Equal),
    (",", KeyCode::Comma),
    (".", KeyCode::Period),
    ("/", KeyCode::Slash),
    (";", KeyCode::Semicolon),
    ("'", KeyCode::Apostrophe),
    ("\\", KeyCode::Backslash),
    ("1", KeyCode::Key1),
    ("2", KeyCode::Key2),
    ("3", KeyCode::Key3),
    ("4", KeyCode::Key4),
    ("5", KeyCode::Key5),
    ("6", KeyCode::Key6),
    ("7", KeyCode::Key7),
    ("8", KeyCode::Key8),
    ("9", KeyCode::Key9),
    ("0", KeyCode::Key0),
    ("F1", KeyCode::F1),
    ("F2", KeyCode::F2),
    ("F3", KeyCode::F3),
    ("F4", KeyCode::F4),
    ("F5", KeyCode::F5),
    ("F6", KeyCode::F6),
    ("F7", KeyCode::F7),
    ("F8", KeyCode::F8),
    ("F9", KeyCode::F9),
    ("F10", KeyCode::F10),
    ("F11", KeyCode::F11),
    ("Num 0", KeyCode::Kp0),
    ("Num 1", KeyCode::Kp1),
    ("Num 2", KeyCode::Kp2),
    ("Num 3", KeyCode::Kp3),
    ("Num 4", KeyCode::Kp4),
    ("Num 5", KeyCode::Kp5),
    ("Num 6", KeyCode::Kp6),
    ("Num 7", KeyCode::Kp7),
    ("Num 8", KeyCode::Kp8),
    ("Num 9", KeyCode::Kp9),
    ("Num +", KeyCode::KpAdd),
    ("Num -", KeyCode::KpSubtract),
    ("Num *", KeyCode::KpMultiply),
    ("Num /", KeyCode::KpDivide),
    ("Num .", KeyCode::KpDecimal),
    ("Num Enter", KeyCode::KpEnter),
];
pub fn key_name(key: KeyCode) -> Option<&'static str> {
    KEYS.iter().find(|(_, k)| *k == key).map(|(s, _)| *s)
}
pub fn key_code(name: &str) -> Option<KeyCode> {
    KEYS.iter().find(|(s, _)| *s == name).map(|(_, k)| *k)
}
pub fn valid_binding(name: &str) -> bool {
    !matches!(name, "F1" | "F2" | "F3" | "F4")
        && (key_code(name).is_some()
            || matches!(
                name,
                "None" | "Mouse 1" | "Mouse 2" | "Mouse 3" | "Wheel Up" | "Wheel Down"
            ))
}
fn mouse_button(name: &str) -> Option<MouseButton> {
    match name {
        "Mouse 1" => Some(MouseButton::Left),
        "Mouse 2" => Some(MouseButton::Right),
        "Mouse 3" => Some(MouseButton::Middle),
        _ => None,
    }
}
fn physical(name: &str, edge: bool) -> bool {
    if let Some(k) = key_code(name) {
        return if edge {
            is_key_pressed(k)
        } else {
            is_key_down(k)
        };
    }
    if let Some(b) = mouse_button(name) {
        return if edge {
            is_mouse_button_pressed(b)
        } else {
            is_mouse_button_down(b)
        };
    }
    match name {
        "Wheel Up" => mouse_wheel().1 > 0.,
        "Wheel Down" => mouse_wheel().1 < 0.,
        _ => false,
    }
}
pub fn capture_binding() -> Option<String> {
    KEYS.iter()
        .find(|(_, k)| is_key_pressed(*k))
        .map(|(s, _)| (*s).to_owned())
        .or_else(|| {
            ["Mouse 1", "Mouse 2", "Mouse 3", "Wheel Up", "Wheel Down"]
                .into_iter()
                .find(|s| physical(s, true))
                .map(str::to_owned)
        })
}

pub const PAD_BUTTONS: &[(&str, u32)] = &[
    ("A", 0x1000),
    ("B", 0x2000),
    ("X", 0x4000),
    ("Y", 0x8000),
    ("LB", 0x100),
    ("RB", 0x200),
    ("Back", 0x20),
    ("L Stick", 0x40),
    ("R Stick", 0x80),
    ("D Up", 1),
    ("D Down", 2),
    ("D Left", 4),
    ("D Right", 8),
    ("LT", 0x10000),
    ("RT", 0x20000),
];
fn pad_mask(name: &str) -> u32 {
    if name == "Start" {
        0x10
    } else {
        PAD_BUTTONS
            .iter()
            .find(|(s, _)| *s == name)
            .map_or(0, |(_, m)| *m)
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
struct RawPad {
    buttons: u16,
    lt: u8,
    rt: u8,
    lx: i16,
    ly: i16,
    rx: i16,
    ry: i16,
}
#[repr(C)]
#[derive(Default)]
struct RawState {
    packet: u32,
    pad: RawPad,
}
#[cfg(windows)]
#[link(name = "xinput")]
extern "system" {
    fn XInputGetState(index: u32, state: *mut RawState) -> u32;
}
fn read_pad(index: u32) -> Option<RawPad> {
    #[cfg(windows)]
    {
        let mut state = RawState::default();
        // Fixed C layout, writable state, index checked by XInput; Windows 10+ system API.
        (unsafe { XInputGetState(index, &mut state) } == 0).then_some(state.pad)
    }
    #[cfg(not(windows))]
    {
        let _ = index;
        None
    }
}
fn stick(x: i16, y: i16, deadzone: f32) -> Vec2 {
    let v = vec2(x as f32 / 32767., y as f32 / 32767.);
    let length = v.length();
    if length <= deadzone {
        Vec2::ZERO
    } else {
        v / length * ((length.min(1.) - deadzone) / (1. - deadzone))
    }
}

// Read the analog magnitude without changing the pinned miniquad backend.
// Backend W/A/S/D still serves menus/host emulation, but not Vita gameplay movement.
#[cfg(target_os = "vita")]
fn vita_sticks(deadzone: f32) -> (Vec2, Vec2) {
    // VitaSDK psp2common/ctrl.h SceCtrlData (0x20 bytes).
    #[repr(C)]
    struct CtrlData { timestamp: u64, buttons: u32, lx: u8, ly: u8, rx: u8, ry: u8, rest: [u8; 16] }
    const _: [(); 0x20] = [(); std::mem::size_of::<CtrlData>()];
    #[link(name = "SceCtrl_stub")]
    extern "C" { fn sceCtrlPeekBufferPositive(port: i32, data: *mut CtrlData, count: i32) -> i32; }
    let mut data: CtrlData = unsafe { std::mem::zeroed() };
    let rc = unsafe { sceCtrlPeekBufferPositive(0, &mut data, 1) };
    if rc <= 0 || data.buttons & 0x10000 != 0 { return (Vec2::ZERO, Vec2::ZERO); }
    let axis = |v: u8| (((v as f32 - 128.) / 127.).clamp(-1., 1.) * 32767.) as i16;
    (stick(axis(data.lx), -axis(data.ly), deadzone),
     stick(axis(data.rx), -axis(data.ry), deadzone))
}

#[cfg(any(target_os = "vita", test))]
fn resumed_axis(movement: Vec2, needs_neutral: &mut bool) -> Vec2 {
    if movement == Vec2::ZERO { *needs_neutral = false; }
    if *needs_neutral { Vec2::ZERO } else { movement }
}

// Threshold is on the radial magnitude after deadzone remapping.
const RUN_THRESHOLD: f32 = 0.65;
fn analog_run(movement: Vec2) -> bool { movement.length() >= RUN_THRESHOLD }
pub(crate) fn vita_label(key: &str) -> &str {
    match key {
        "Enter" => "Cross", "Space" => "Square", "E" => "Triangle",
        "Backspace" => "Circle", "Shift" => "L", "Ctrl" => "R",
        "Escape" => "Start", "Tab" => "Select", "Up" => "D-pad Up",
        "Down" => "D-pad Down", "Left" => "D-pad Left", "Right" => "D-pad Right",
        "W" | "S" | "A" | "D" => "Left stick",
        _ => key,
    }
}

#[derive(Default)]
struct Pad {
    connected: bool,
    down: u32,
    pressed: u32,
    blocked: u32,
    move_axis: Vec2,
    look_axis: Vec2,
    neutral_sticks: bool,
    nav_down: u32,
    nav_pressed: u32,
    nav_delay: f32,
}
impl Pad {
    fn update(&mut self, raw: Option<RawPad>, focused: bool, deadzone: f32, dt: f32) {
        let connected = raw.is_some();
        let raw = raw.unwrap_or_default();
        let down = raw.buttons as u32
            | if raw.lt > 30 { 0x10000 } else { 0 }
            | if raw.rt > 30 { 0x20000 } else { 0 };
        if !focused || !self.connected {
            self.blocked |= down;
            self.neutral_sticks = true;
        }
        self.blocked &= down;
        let available = if focused { down & !self.blocked } else { 0 };
        self.pressed = available & !self.down;
        self.down = available;
        self.connected = connected;
        let movement = stick(raw.lx, raw.ly, deadzone);
        let look = stick(raw.rx, raw.ry, deadzone);
        if movement == Vec2::ZERO && look == Vec2::ZERO {
            self.neutral_sticks = false;
        }
        (self.move_axis, self.look_axis) = if focused && !self.neutral_sticks {
            (movement, look)
        } else {
            (Vec2::ZERO, Vec2::ZERO)
        };
        let nav = self.down & 15
            | if self.move_axis.y > 0.55 { 1 } else { 0 }
            | if self.move_axis.y < -0.55 { 2 } else { 0 }
            | if self.move_axis.x < -0.55 { 4 } else { 0 }
            | if self.move_axis.x > 0.55 { 8 } else { 0 };
        self.nav_pressed = nav & !self.nav_down;
        if nav != self.nav_down {
            self.nav_delay = 0.38;
        } else if nav != 0 {
            self.nav_delay -= dt;
            if self.nav_delay <= 0. {
                self.nav_pressed |= nav;
                self.nav_delay = 0.12;
            }
        }
        self.nav_down = nav;
    }
    fn suppress(&mut self) {
        self.blocked |= self.down;
        self.down = 0;
        self.pressed = 0;
        self.nav_down = 0;
        self.nav_pressed = 0;
        self.neutral_sticks = true;
        self.move_axis = Vec2::ZERO;
        self.look_axis = Vec2::ZERO;
    }
}

#[derive(Default)]
pub struct Input {
    pad: Pad,
    slot: Option<u32>,
    retry: f64,
    blocked: HashSet<String>,
    pub using_pad: bool,
    pub disconnected: bool,
    focused: bool,
    #[cfg(target_os = "vita")]
    vita_neutral: bool,
    #[cfg(target_os = "vita")]
    vita_look_neutral: bool,
    #[cfg(target_os = "vita")]
    vita_look_log: std::cell::RefCell<crate::look::stick::RateLog>,
}
impl Input {
    pub fn update(&mut self, prefs: &Preferences, focused: bool) {
        let mut raw = self.slot.and_then(read_pad);
        if raw.is_none() && get_time() >= self.retry {
            self.slot = (0..4).find(|i| {
                if let Some(p) = read_pad(*i) {
                    raw = Some(p);
                    true
                } else {
                    false
                }
            });
            self.retry = get_time() + 1.;
        }
        self.disconnected = self.pad.connected && raw.is_none() && self.using_pad;
        self.pad
            .update(raw, focused, prefs.pad_deadzone, get_frame_time().min(0.1));
        #[cfg(target_os = "vita")]
        {
            if !self.focused {
                self.vita_neutral = true;
                self.vita_look_neutral = true;
            }
            let (movement, look) = if focused { vita_sticks(prefs.pad_deadzone) }
                else { (Vec2::ZERO, Vec2::ZERO) };
            self.pad.move_axis = resumed_axis(movement, &mut self.vita_neutral);
            self.pad.look_axis = resumed_axis(look, &mut self.vita_look_neutral);
        }
        if !focused {
            self.suppress();
        }
        self.blocked
            .retain(|s| physical(s, false) && !physical(s, true));
        if focused
            && (self.pad.pressed != 0
                || self.pad.move_axis != Vec2::ZERO
                || self.pad.look_axis != Vec2::ZERO)
        {
            self.using_pad = true;
        }
        if focused
            && (KEYS.iter().any(|(_, k)| is_key_pressed(*k))
                || [MouseButton::Left, MouseButton::Right, MouseButton::Middle]
                    .iter()
                    .any(|b| is_mouse_button_pressed(*b))
                || mouse_wheel().1 != 0.)
        {
            self.using_pad = false;
        }
        self.focused = focused;
    }
    /// On menus/load/focus boundaries, held inputs must be released before reuse.
    pub fn suppress(&mut self) {
        #[cfg(target_os = "vita")]
        {
            self.vita_neutral = true;
            self.vita_look_neutral = true;
            self.vita_look_log = Default::default();
        }
        self.pad.suppress();
        for &(s, _) in KEYS {
            if physical(s, false) {
                self.blocked.insert(s.into());
            }
        }
        for s in ["Mouse 1", "Mouse 2", "Mouse 3"] {
            if physical(s, false) {
                self.blocked.insert(s.into());
            }
        }
    }
    pub fn capture_pad(&self) -> Option<&'static str> {
        PAD_BUTTONS
            .iter()
            .find(|(_, m)| self.pad.pressed & m != 0)
            .map(|(s, _)| *s)
    }
    #[cfg(feature="native_timing")]
    pub fn diagnostic_activity(&self)->bool {
        self.pad.down!=0 || self.pad.move_axis!=Vec2::ZERO || self.pad.look_axis!=Vec2::ZERO
            || mouse_delta_position()!=Vec2::ZERO || mouse_wheel().1!=0.
            || KEYS.iter().any(|(_,k)|is_key_down(*k))
            || [MouseButton::Left,MouseButton::Right,MouseButton::Middle].iter().any(|b|is_mouse_button_down(*b))
    }
    pub fn pad_pressed(&self, name: &str) -> bool {
        self.focused && self.pad.pressed & pad_mask(name) != 0
    }
    pub fn pad_held(&self, name: &str) -> bool {
        self.focused && self.pad.down & pad_mask(name) != 0
    }
    pub fn ui(&self, key: KeyCode) -> bool {
        #[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
        if crate::transition_memory::host_recovery_key() == Some(key) {
            crate::transition_memory::host_recovery_clear_key();
            return true;
        }
        let mask = match key {
            KeyCode::Up => 1,
            KeyCode::Down => 2,
            KeyCode::Left => 4,
            KeyCode::Right => 8,
            _ => 0,
        };
        self.focused
            && (is_key_pressed(key)
                || (key == KeyCode::Escape && Preferences::vita_emulation()
                    && is_key_pressed(KeyCode::Backspace))
                || self.pad.nav_pressed & mask != 0
                || match key {
                    KeyCode::Enter => self.pad_pressed("A"),
                    KeyCode::Escape => self.pad_pressed("B") || self.pad_pressed("Start"),
                    _ => false,
                })
    }
    pub fn action(&self, prefs: &Preferences, original: &str, edge: bool) -> bool {
        self.action_with(prefs, original, edge, physical)
    }
    fn action_with(&self, prefs: &Preferences, original: &str, edge: bool,
        physical: impl Fn(&str, bool) -> bool) -> bool {
        if !self.focused {
            return false;
        }
        let row = crate::preferences::BINDINGS
            .iter()
            .position(|(_, key)| *key == original);
        let name = row.map_or(original, |i| prefs.bindings[i].as_str());
        // The backend quantizes the Vita stick to W/A/S/D; use its analog read instead.
        if cfg!(target_os = "vita") && matches!(name, "W" | "S" | "A" | "D") { return false; }
        (!self.blocked.contains(name) && physical(name, edge))
            || row.is_some_and(|i| {
                let mask = pad_mask(&prefs.pad_bindings[i]);
                (if edge {
                    self.pad.pressed
                } else {
                    self.pad.down
                }) & mask
                    != 0
            })
    }
    pub fn key(&self, prefs: &Preferences, key: KeyCode, edge: bool) -> bool {
        key_name(key).map_or_else(
            || {
                self.focused
                    && if edge {
                        is_key_pressed(key)
                    } else {
                        is_key_down(key)
                    }
            },
            |name| self.action(prefs, name, edge),
        )
    }
    pub fn film_skip(&self, prefs: &Preferences) -> bool {
        self.film_skip_with(prefs, Preferences::vita_emulation(), physical)
    }
    fn film_skip_with(&self, prefs: &Preferences, handheld: bool, physical: impl Fn(&str, bool) -> bool) -> bool {
        self.action_with(prefs, "Enter", false, &physical) || self.pad_held("A")
            || (handheld && self.focused
                && !self.blocked.contains("Enter") && physical("Enter", false))
    }
    pub fn film_skip_label(&self, prefs: &Preferences) -> String {
        if Preferences::vita_emulation() { format!("{} / Cross", self.label(prefs, "Enter")) }
        else if self.desktop_pad() { "A".into() }
        else { self.label(prefs, "Enter") }
    }
    pub fn running(&self, prefs: &Preferences, shift: bool) -> bool {
        self.running_with(prefs, shift, Preferences::vita_emulation())
    }
    fn running_with(&self, prefs: &Preferences, shift: bool, handheld: bool) -> bool {
        if handheld && (cfg!(target_os = "vita") || self.pad.move_axis != Vec2::ZERO) {
            analog_run(self.pad.move_axis)
        } else { prefs.run(shift) }
    }
    pub fn movement(&self) -> Vec2 {
        self.pad.move_axis
    }
    pub fn look(&self, prefs: &Preferences) -> Vec2 {
        self.pad.look_axis
            * vec2(1., if prefs.invert_pad { -1. } else { 1. })
            * prefs.pad_sensitivity
    }
    #[cfg(target_os = "vita")]
    pub fn vita_look(&self, prefs: &Preferences, elapsed: f32, yaw: &mut f32, pitch: &mut f32) {
        let axis = self.pad.look_axis;
        let before = *yaw;
        crate::look::stick::turn(axis, prefs.pad_sensitivity, prefs.invert_pad, elapsed, yaw, pitch);
        // Measure the applied, wrapped yaw change rather than printing the configured rate.
        self.vita_look_log.borrow_mut().sample(axis, elapsed, before, *yaw);
    }
    pub fn desktop_pad(&self) -> bool {
        self.using_pad && !Preferences::vita_emulation()
    }
    pub fn label(&self, prefs: &Preferences, original: &str) -> String {
        self.label_with(prefs, original, Preferences::vita_emulation())
    }
    fn label_with(&self, prefs: &Preferences, original: &str, handheld: bool) -> String {
        let label = crate::preferences::BINDINGS
            .iter()
            .position(|(_, key)| *key == original)
            .map_or_else(
                || original.to_owned(),
                |i| {
                    if self.using_pad && !handheld && prefs.pad_bindings[i] != "None" {
                        prefs.pad_bindings[i].clone()
                    } else {
                        prefs.bindings[i].clone()
                    }
                },
            );
        if handheld { vita_label(&label).to_owned() } else { label }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cinematic_cross_alias_preserves_gameplay_bindings_and_suppression() {
        let mut prefs = Preferences::default(); prefs.apply_vita_preset();
        let mut input = Input { focused: true, ..Default::default() };
        for key in ["Enter", "E"] {
            assert!(input.film_skip_with(&prefs, true, |k, _| k == key));
        }
        assert!(input.action_with(&prefs, "Space", false, |k, _| k == "Enter"));
        assert!(!input.action_with(&prefs, "Enter", false, |k, _| k == "Enter"));
        input.blocked.insert("Enter".into());
        assert!(!input.film_skip_with(&prefs, true, |k, _| k == "Enter"));
        input.focused = false;
        assert!(!input.film_skip_with(&prefs, true, |_, _| true));
    }
    #[test]
    fn radial_sticks_are_quiet_at_centre_and_preserve_analog_speed() {
        assert_eq!(stick(3000, -4000, 0.24), Vec2::ZERO);
        let half = stick(0, 20000, 0.24);
        assert!(half.y > 0.4 && half.y < 0.6);
        assert!((stick(i16::MAX, i16::MIN, 0.24).length() - 1.).abs() < 0.0001);
    }
    #[test]
    fn vita_look_deadzone_rescales_cardinal_diagonal_and_endpoints() {
        for dz in [0.1, 0.24, 0.4] {
            assert_eq!(stick(0, 0, dz), Vec2::ZERO);
            assert_eq!(stick((dz * 32767.) as i16, 0, dz), Vec2::ZERO);
            let diagonal = (dz * 32767. / 2_f32.sqrt()) as i16;
            assert_eq!(stick(diagonal, diagonal, dz), Vec2::ZERO);
            let just_outside = stick(((dz + 0.001) * 32767.) as i16, 0, dz);
            assert!(just_outside.x > 0. && just_outside.x < 0.002);
            assert_eq!(stick(i16::MAX, 0, dz), Vec2::X);
            assert_eq!(stick(i16::MIN, 0, dz), -Vec2::X);
            let half = stick(((dz + (1. - dz) * 0.5) * 32767.) as i16, 0, dz);
            assert!((half.x - 0.5).abs() < 0.0001);
            let diagonal = stick(i16::MAX, i16::MAX, dz);
            assert!((diagonal.length() - 1.).abs() < 1e-6);
            assert_eq!(diagonal.x, diagonal.y);
        }
    }
    #[test]
    fn reconnect_focus_and_modal_boundaries_require_release() {
        let mut p = Pad::default();
        let held = RawPad {
            buttons: 0x1000,
            rt: 255,
            lx: 20000,
            ..Default::default()
        };
        p.update(Some(held), true, 0.24, 0.016);
        assert_eq!(p.pressed, 0);
        assert_eq!(p.move_axis, Vec2::ZERO);
        p.update(Some(RawPad::default()), true, 0.24, 0.016);
        p.update(Some(held), true, 0.24, 0.016);
        assert_eq!(p.pressed, 0x21000);
        assert!(p.move_axis.x > 0.);
        p.update(Some(held), true, 0.24, 0.016);
        assert_eq!(p.pressed, 0);
        p.suppress();
        p.update(Some(held), true, 0.24, 0.016);
        assert_eq!(p.down, 0);
        p.update(None, true, 0.24, 0.016);
        assert!(!p.connected);
        p.update(Some(held), true, 0.24, 0.016);
        assert_eq!(p.down, 0);
        p.update(Some(held), false, 0.24, 0.016);
        assert_eq!(p.move_axis, Vec2::ZERO);
    }
    #[test]
    fn menu_stick_repeats_without_repeating_accept_or_trigger() {
        let mut p = Pad::default();
        p.update(Some(RawPad::default()), true, 0.24, 0.016);
        let raw = RawPad {
            ly: 32767,
            buttons: 0x1000,
            rt: 31,
            ..Default::default()
        };
        p.update(Some(raw), true, 0.24, 0.016);
        assert_eq!(p.nav_pressed, 1);
        assert_eq!(p.pressed, 0x21000);
        p.update(Some(raw), true, 0.24, 0.2);
        assert_eq!(p.nav_pressed, 0);
        assert_eq!(p.pressed, 0);
        p.update(Some(raw), true, 0.24, 0.2);
        assert_eq!(p.nav_pressed, 1);
        assert_eq!(p.pressed, 0);
    }
}

#[cfg(test)]
mod vita_preset_tests {
    use super::*;
    #[test]
    fn vita_sticks_require_neutral_independently_after_modal_boundaries() {
        let (mut movement_neutral, mut look_neutral) = (true, true);
        assert_eq!(resumed_axis(Vec2::X, &mut look_neutral), Vec2::ZERO);
        resumed_axis(Vec2::ZERO, &mut movement_neutral);
        assert_eq!(resumed_axis(Vec2::X, &mut movement_neutral), Vec2::X);
        assert_eq!(resumed_axis(Vec2::X, &mut look_neutral), Vec2::ZERO);
        resumed_axis(Vec2::ZERO, &mut look_neutral);
        assert_eq!(resumed_axis(Vec2::X, &mut look_neutral), Vec2::X);
    }
    #[test]
    fn analog_walk_run_threshold_and_physical_prompt_names() {
        let mut needs_neutral = true;
        assert_eq!(resumed_axis(vec2(0., 1.), &mut needs_neutral), Vec2::ZERO);
        assert_eq!(resumed_axis(Vec2::ZERO, &mut needs_neutral), Vec2::ZERO);
        assert_eq!(resumed_axis(vec2(0.3, 0.), &mut needs_neutral), vec2(0.3, 0.));
        assert!(!analog_run(Vec2::ZERO));
        assert!(!analog_run(vec2(0.64, 0.)));
        assert!(analog_run(vec2(0.65, 0.)));
        assert!(analog_run(vec2(0.5, 0.5)));
        let p = Preferences::default();
        let mut input = Input::default();
        input.pad.move_axis = vec2(0.35, 0.);
        assert!(!input.running_with(&p, false, true));
        input.pad.move_axis = vec2(0., 1.);
        assert!(input.running_with(&p, false, true));
        assert!(!input.running_with(&p, false, false)); // Desktop preference behavior survives.
        let mut p = p;
        p.apply_vita_preset();
        input.using_pad = true; // Analog use must not switch prompts to XInput labels.
        for (action, label) in [("Space", "Cross"), ("E", "Square"), ("Enter", "Triangle"),
            ("Ctrl", "Circle"), ("Mouse 1", "R"), ("Mouse 2", "L"),
            ("Escape", "Start"), ("Tab", "Select"), ("I", "D-pad Down")] {
            assert_eq!(input.label_with(&p, action, true), label);
        }
        for (key, label) in [("Enter", "Cross"), ("Space", "Square"), ("E", "Triangle"),
            ("Backspace", "Circle"), ("Shift", "L"), ("Ctrl", "R"),
            ("Escape", "Start"), ("Tab", "Select"), ("Down", "D-pad Down")] {
            assert_eq!(vita_label(key), label);
        }
    }
    #[test]
    fn handheld_events_dispatch_only_the_expected_actions_and_respect_suppression() {
        let mut prefs = Preferences::default();
        prefs.apply_vita_preset();
        let mut input = Input { focused: true, ..Default::default() };
        let cases = [("Ctrl", "Mouse 1"), ("Shift", "Mouse 2"),
            ("Left", "Wheel Up"), ("Right", "Wheel Down"),
            ("Enter", "Space"), ("Space", "E"), ("Backspace", "Ctrl"),
            ("Up", "C"), ("Down", "I"), ("Tab", "Tab"), ("E", "Enter")];
        for (event, expected) in cases {
            for (_, original) in crate::preferences::BINDINGS {
                for edge in [true, false] {
                    assert_eq!(input.action_with(&prefs, original, edge, |key, _| key == event),
                        *original == expected, "event={event} action={original} edge={edge}");
                }
            }
            input.blocked.insert(event.into());
            assert!(!input.action_with(&prefs, expected, true, |key, _| key == event));
            input.blocked.clear();
        }
        // Cross also emits left click; jumping/confirming must not fire a toy.
        for original in ["Mouse 1", "Mouse 2"] {
            assert!(!input.action_with(&prefs, original, true,
                |key, _| key == "Enter" || key == "Mouse 1"));
        }
        let mut stats = crate::inventory::Stats::weapon_preview();
        let mut visited = std::collections::BTreeSet::new();
        for _ in 0..10 {
            visited.insert(stats.selected());
            assert!(input.action_with(&prefs, "Wheel Down", true, |key, _| key == "Right"));
            stats.cycle(1);
        }
        assert_eq!(visited.len(), 10);
        for _ in 0..10 {
            assert!(input.action_with(&prefs, "Wheel Up", true, |key, _| key == "Left"));
            stats.cycle(-1);
        }
        input.focused = false;
        assert!(!input.action_with(&prefs, "Mouse 1", true, |_, _| true));
    }
}
