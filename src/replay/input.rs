//! Platform input as the game reads it, for recording and replaying a play.
//!
//! Off (the normal case): every function is the macroquad call of the same name, or the
//! SceCtrl read on the Vita. Active (recording or replaying a tape): each read answers from one
//! frame of state that the session installs at the start of a simulation step. A recording
//! captures that frame from the platform; a replay loads it from the tape. Both runs then read
//! the same values through the same code, whatever the real pad or clock does meanwhile.
//!
//! Files that the main loop reaches import these names explicitly, which shadows the macroquad
//! prelude glob for that file only.
use macroquad::prelude::{KeyCode, MouseButton};
use std::sync::{
    atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicU8, Ordering::Relaxed},
    Mutex,
};

/// Every miniquad key code, in declaration order. Bit i of a key mask is `KEYS[i]`.
/// The tape header stores these codes; a tape with another table is refused.
pub const KEYS: [KeyCode; 122] = {
    use KeyCode::*;
    [
        Space, Apostrophe, Comma, Minus, Period, Slash, Key0, Key1, Key2, Key3, Key4, Key5, Key6,
        Key7, Key8, Key9, Semicolon, Equal, A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R,
        S, T, U, V, W, X, Y, Z, LeftBracket, Backslash, RightBracket, GraveAccent, World1, World2,
        Escape, Enter, Tab, Backspace, Insert, Delete, Right, Left, Down, Up, PageUp, PageDown,
        Home, End, CapsLock, ScrollLock, NumLock, PrintScreen, Pause, F1, F2, F3, F4, F5, F6, F7,
        F8, F9, F10, F11, F12, F13, F14, F15, F16, F17, F18, F19, F20, F21, F22, F23, F24, F25,
        Kp0, Kp1, Kp2, Kp3, Kp4, Kp5, Kp6, Kp7, Kp8, Kp9, KpDecimal, KpDivide, KpMultiply,
        KpSubtract, KpAdd, KpEnter, KpEqual, LeftShift, LeftControl, LeftAlt, LeftSuper,
        RightShift, RightControl, RightAlt, RightSuper, Menu, Back, Unknown,
    ]
};
pub const BUTTONS: [MouseButton; 4] =
    [MouseButton::Left, MouseButton::Middle, MouseButton::Right, MouseButton::Unknown];

pub fn key_codes() -> Vec<u16> {
    KEYS.iter().map(|k| *k as u16).collect()
}
/// (code, bit) sorted by code, for a binary search on every key read while a tape runs.
fn key_bit(key: KeyCode) -> Option<u32> {
    static SORTED: std::sync::OnceLock<Vec<(u16, u32)>> = std::sync::OnceLock::new();
    let sorted = SORTED.get_or_init(|| {
        let mut v: Vec<(u16, u32)> = KEYS.iter().enumerate().map(|(i, k)| (*k as u16, i as u32)).collect();
        v.sort_unstable();
        v
    });
    sorted.binary_search_by_key(&(key as u16), |e| e.0).ok().map(|i| sorted[i].1)
}
fn button_bit(button: MouseButton) -> u32 {
    match button {
        MouseButton::Left => 0,
        MouseButton::Middle => 1,
        MouseButton::Right => 2,
        MouseButton::Unknown => 3,
    }
}

/// One simulation step of input. Chars are kept in the order macroquad pops them.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Frame {
    pub dt: f32,
    pub keys_down: u128,
    pub keys_pressed: u128,
    pub keys_released: u128,
    pub mouse_down: u8,
    pub mouse_pressed: u8,
    pub mouse_released: u8,
    pub mouse: (f32, f32),
    pub wheel: (f32, f32),
    /// Vita sticks as the game reads them (lx, ly, rx, ry); None when the read failed or the
    /// system holds the pad. Always None on the host.
    pub pad: Option<[u8; 4]>,
    pub chars: Vec<char>,
    pub quit: bool,
}

const OFF: u8 = 0;
const RECORD: u8 = 1;
const REPLAY: u8 = 2;
static MODE: AtomicU8 = AtomicU8::new(OFF);
// The main thread writes these at a step boundary and reads them during the step.
static DT: AtomicU32 = AtomicU32::new(0);
static KEYS_DOWN: [AtomicU64; 2] = [AtomicU64::new(0), AtomicU64::new(0)];
static KEYS_PRESSED: [AtomicU64; 2] = [AtomicU64::new(0), AtomicU64::new(0)];
static KEYS_RELEASED: [AtomicU64; 2] = [AtomicU64::new(0), AtomicU64::new(0)];
static MOUSE_BITS: AtomicU32 = AtomicU32::new(0);
static MOUSE_POS: AtomicU64 = AtomicU64::new(0);
static WHEEL: AtomicU64 = AtomicU64::new(0);
static QUIT: AtomicBool = AtomicBool::new(false);
/// Bit 32 set: a pad read; low 32 bits lx, ly, rx, ry.
static PAD: AtomicU64 = AtomicU64::new(0);
static CHARS: Mutex<Vec<char>> = Mutex::new(Vec::new());
/// Tape seconds, the sum of the frame times of every step so far, as f64 bits.
static CLOCK: AtomicU64 = AtomicU64::new(0);
static ORIGIN: Mutex<Option<std::time::Instant>> = Mutex::new(None);

fn mode() -> u8 {
    MODE.load(Relaxed)
}
pub fn active() -> bool {
    mode() != OFF
}
fn store_u128(cells: &[AtomicU64; 2], value: u128) {
    cells[0].store(value as u64, Relaxed);
    cells[1].store((value >> 64) as u64, Relaxed);
}
fn load_u128(cells: &[AtomicU64; 2]) -> u128 {
    u128::from(cells[0].load(Relaxed)) | (u128::from(cells[1].load(Relaxed)) << 64)
}
fn pair_bits(pair: (f32, f32)) -> u64 {
    u64::from(pair.0.to_bits()) | (u64::from(pair.1.to_bits()) << 32)
}
fn bits_pair(bits: u64) -> (f32, f32) {
    (f32::from_bits(bits as u32), f32::from_bits((bits >> 32) as u32))
}
fn pad_bits(pad: Option<[u8; 4]>) -> u64 {
    pad.map_or(0, |p| (1 << 32) | u64::from(u32::from_le_bytes(p)))
}
fn bits_pad(bits: u64) -> Option<[u8; 4]> {
    (bits >> 32 != 0).then(|| (bits as u32).to_le_bytes())
}

/// Start answering from installed frames. `replay` selects whether the Vita pad read comes
/// from the frame (replay) or from the console (record).
pub(super) fn activate(replay: bool) {
    CLOCK.store(0f64.to_bits(), Relaxed);
    *ORIGIN.lock().unwrap_or_else(|p| p.into_inner()) = Some(std::time::Instant::now());
    PAD.store(0, Relaxed);
    MODE.store(if replay { REPLAY } else { RECORD }, Relaxed);
}
pub(super) fn deactivate() {
    MODE.store(OFF, Relaxed);
    CHARS.lock().unwrap_or_else(|p| p.into_inner()).clear();
}
/// Install one step's input. The pad of a recording is filled in later by `vita_pad`.
pub(super) fn install(frame: &Frame) {
    DT.store(frame.dt.to_bits(), Relaxed);
    store_u128(&KEYS_DOWN, frame.keys_down);
    store_u128(&KEYS_PRESSED, frame.keys_pressed);
    store_u128(&KEYS_RELEASED, frame.keys_released);
    MOUSE_BITS.store(
        u32::from(frame.mouse_down) | u32::from(frame.mouse_pressed) << 8 | u32::from(frame.mouse_released) << 16,
        Relaxed,
    );
    MOUSE_POS.store(pair_bits(frame.mouse), Relaxed);
    WHEEL.store(pair_bits(frame.wheel), Relaxed);
    QUIT.store(frame.quit, Relaxed);
    if mode() == REPLAY {
        PAD.store(pad_bits(frame.pad), Relaxed);
    }
    let mut chars = CHARS.lock().unwrap_or_else(|p| p.into_inner());
    chars.clear();
    // Served by pop() from the end, like macroquad: store reversed.
    chars.extend(frame.chars.iter().rev());
    let clock = f64::from_bits(CLOCK.load(Relaxed)) + f64::from(frame.dt);
    CLOCK.store(clock.to_bits(), Relaxed);
}
/// The pad the game read during the step that is ending (record), for the tape.
pub(super) fn last_pad() -> Option<[u8; 4]> {
    bits_pad(PAD.load(Relaxed))
}

/// Everything macroquad reports for this frame. Chars are drained from its queue.
pub(super) fn capture_live() -> Frame {
    use macroquad::input as mq;
    let mut f = Frame { dt: macroquad::time::get_frame_time(), ..Default::default() };
    for (i, key) in KEYS.iter().enumerate() {
        let bit = 1u128 << i;
        if mq::is_key_down(*key) { f.keys_down |= bit; }
        if mq::is_key_pressed(*key) { f.keys_pressed |= bit; }
        if mq::is_key_released(*key) { f.keys_released |= bit; }
    }
    for button in BUTTONS {
        let bit = 1u8 << button_bit(button);
        if mq::is_mouse_button_down(button) { f.mouse_down |= bit; }
        if mq::is_mouse_button_pressed(button) { f.mouse_pressed |= bit; }
        if mq::is_mouse_button_released(button) { f.mouse_released |= bit; }
    }
    f.mouse = mq::mouse_position();
    f.wheel = mq::mouse_wheel();
    f.chars = std::iter::from_fn(mq::get_char_pressed).collect();
    f.quit = mq::is_quit_requested();
    f
}

pub fn is_key_down(key: KeyCode) -> bool {
    if !active() { return macroquad::input::is_key_down(key); }
    key_bit(key).is_some_and(|b| load_u128(&KEYS_DOWN) >> b & 1 != 0)
}
pub fn is_key_pressed(key: KeyCode) -> bool {
    if !active() { return macroquad::input::is_key_pressed(key); }
    key_bit(key).is_some_and(|b| load_u128(&KEYS_PRESSED) >> b & 1 != 0)
}
pub fn is_key_released(key: KeyCode) -> bool {
    if !active() { return macroquad::input::is_key_released(key); }
    key_bit(key).is_some_and(|b| load_u128(&KEYS_RELEASED) >> b & 1 != 0)
}
pub fn is_mouse_button_down(button: MouseButton) -> bool {
    if !active() { return macroquad::input::is_mouse_button_down(button); }
    MOUSE_BITS.load(Relaxed) >> button_bit(button) & 1 != 0
}
pub fn is_mouse_button_pressed(button: MouseButton) -> bool {
    if !active() { return macroquad::input::is_mouse_button_pressed(button); }
    MOUSE_BITS.load(Relaxed) >> (8 + button_bit(button)) & 1 != 0
}
#[allow(dead_code)] // No caller yet; kept so every macroquad mouse read has a tape form.
pub fn is_mouse_button_released(button: MouseButton) -> bool {
    if !active() { return macroquad::input::is_mouse_button_released(button); }
    MOUSE_BITS.load(Relaxed) >> (16 + button_bit(button)) & 1 != 0
}
pub fn mouse_position() -> (f32, f32) {
    if !active() { return macroquad::input::mouse_position(); }
    bits_pair(MOUSE_POS.load(Relaxed))
}
pub fn mouse_wheel() -> (f32, f32) {
    if !active() { return macroquad::input::mouse_wheel(); }
    bits_pair(WHEEL.load(Relaxed))
}
pub fn get_char_pressed() -> Option<char> {
    if !active() { return macroquad::input::get_char_pressed(); }
    CHARS.lock().unwrap_or_else(|p| p.into_inner()).pop()
}
pub fn is_quit_requested() -> bool {
    if !active() { return macroquad::input::is_quit_requested(); }
    QUIT.load(Relaxed)
}
pub fn get_frame_time() -> f32 {
    if !active() { return macroquad::time::get_frame_time(); }
    f32::from_bits(DT.load(Relaxed))
}
/// Seconds since the first step while active, so time differences match between runs.
pub fn get_time() -> f64 {
    if !active() { return macroquad::time::get_time(); }
    f64::from_bits(CLOCK.load(Relaxed))
}
/// Wall clock for gameplay timers that would otherwise read `Instant::now()`.
pub fn now() -> std::time::Instant {
    if !active() { return std::time::Instant::now(); }
    let origin = ORIGIN.lock().unwrap_or_else(|p| p.into_inner()).unwrap_or_else(std::time::Instant::now);
    origin + std::time::Duration::from_secs_f64(f64::from_bits(CLOCK.load(Relaxed)))
}

/// The Vita stick read used by gameplay (lx, ly, rx, ry). Recording reads the console and keeps
/// the value for the tape; replay returns the recorded value and never reads the pad.
#[cfg(target_os = "vita")]
pub fn vita_pad() -> Option<[u8; 4]> {
    if mode() == REPLAY {
        return bits_pad(PAD.load(Relaxed));
    }
    // VitaSDK psp2common/ctrl.h SceCtrlData (0x20 bytes).
    #[repr(C)]
    struct CtrlData { timestamp: u64, buttons: u32, lx: u8, ly: u8, rx: u8, ry: u8, rest: [u8; 16] }
    const _: [(); 0x20] = [(); std::mem::size_of::<CtrlData>()];
    #[link(name = "SceCtrl_stub")]
    extern "C" { fn sceCtrlPeekBufferPositive(port: i32, data: *mut CtrlData, count: i32) -> i32; }
    let mut data: CtrlData = unsafe { std::mem::zeroed() };
    let rc = unsafe { sceCtrlPeekBufferPositive(0, &mut data, 1) };
    // 0x10000: the system holds the pad (PS button menu, app in background).
    let pad = (rc > 0 && data.buttons & 0x10000 == 0).then_some([data.lx, data.ly, data.rx, data.ry]);
    if mode() == RECORD {
        PAD.store(pad_bits(pad), Relaxed);
    }
    pad
}

/// Host recordings without a person: keys, mouse buttons and mouse motion from a script, with
/// the real frame time. Selected by LOOKING_GLASS_REPLAY_SYNTH. `aim` is the turn (radians,
/// positive to the left) and distance to the nearest live hostile: the script then fights it.
/// Only the recording uses it; a replay reads the recorded keys.
#[cfg(not(target_os = "vita"))]
pub(super) fn synthetic(step: u64, previous: &Frame, aim: Option<(f32, f32)>) -> Frame {
    let mut f = Frame { dt: macroquad::time::get_frame_time(), ..Default::default() };
    let hold = |f: &mut Frame, key: KeyCode| f.keys_down |= 1 << key_bit(key).unwrap();
    let click = |f: &mut Frame, button: MouseButton| f.mouse_down |= 1 << button_bit(button);
    // Resume a loaded game, then a 360-step cycle: walk, turn, run, jump, attack, strafe.
    if (20..24).contains(&step) { click(&mut f, MouseButton::Left); }
    if step == 30 { hold(&mut f, KeyCode::Key2); }
    let t = step % 360;
    if let Some((turn, distance)) = aim.filter(|(_, d)| *d < 2500.).filter(|_| step >= 40) {
        if turn > 0.12 { hold(&mut f, KeyCode::Left); }
        if turn < -0.12 { hold(&mut f, KeyCode::Right); }
        if distance > 140. { hold(&mut f, KeyCode::W); }
        if distance > 600. { hold(&mut f, KeyCode::LeftShift); }
        if distance < 140. && (t / 40) % 3 == 1 { hold(&mut f, KeyCode::A); }
        if turn.abs() < 0.5 && distance < 900. && step % 24 < 3 {
            click(&mut f, if step % 96 < 3 { MouseButton::Right } else { MouseButton::Left });
        }
        if t == 200 { hold(&mut f, KeyCode::Space); }
        if step % 720 == 359 { hold(&mut f, KeyCode::Key1); }
        if step % 720 == 719 { hold(&mut f, KeyCode::Key2); }
    } else if step >= 40 {
        if t < 150 { hold(&mut f, KeyCode::W); }
        if t < 70 { hold(&mut f, KeyCode::LeftShift); }
        if (60..95).contains(&t) { hold(&mut f, KeyCode::Left); }
        if (110..113).contains(&t) || (300..303).contains(&t) { hold(&mut f, KeyCode::Space); }
        if [160, 180, 200, 220].iter().any(|s| (*s..*s + 4).contains(&t)) { click(&mut f, MouseButton::Left); }
        if (240..244).contains(&t) { click(&mut f, MouseButton::Right); }
        if (250..320).contains(&t) { hold(&mut f, KeyCode::D); }
        if (270..330).contains(&t) { hold(&mut f, KeyCode::Right); }
        if step % 720 == 359 { hold(&mut f, KeyCode::Key1); }
        if step % 720 == 719 { hold(&mut f, KeyCode::Key2); }
    }
    // Enter now and then: Retry after a death. Escape at LOOKING_GLASS_REPLAY_SYNTH_MENU: Start,
    // which ends the recording and opens the menu.
    if step % 240 < 2 && step >= 240 { hold(&mut f, KeyCode::Enter); }
    if std::env::var("LOOKING_GLASS_REPLAY_SYNTH_MENU").ok().and_then(|v| v.parse::<u64>().ok()) == Some(step) {
        hold(&mut f, KeyCode::Escape);
    }
    // Slow mouse motion for mouse look; integer pixels.
    let drift = if (330..350).contains(&t) { 3. } else { 0. };
    f.mouse = (previous.mouse.0 + drift, previous.mouse.1);
    f.keys_pressed = f.keys_down & !previous.keys_down;
    f.keys_released = previous.keys_down & !f.keys_down;
    f.mouse_pressed = f.mouse_down & !previous.mouse_down;
    f.mouse_released = previous.mouse_down & !f.mouse_down;
    f
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn key_table_is_complete_and_unique() {
        let codes = key_codes();
        let unique: std::collections::BTreeSet<_> = codes.iter().collect();
        assert_eq!(unique.len(), codes.len());
        for (i, key) in KEYS.iter().enumerate() {
            assert_eq!(key_bit(*key), Some(i as u32));
        }
    }
    #[test]
    fn pad_and_pairs_round_trip() {
        for pad in [None, Some([0, 255, 128, 127]), Some([0, 0, 0, 0])] {
            assert_eq!(bits_pad(pad_bits(pad)), pad);
        }
        let pair = (-0.0f32, f32::MIN_POSITIVE);
        let back = bits_pair(pair_bits(pair));
        assert_eq!((back.0.to_bits(), back.1.to_bits()), (pair.0.to_bits(), pair.1.to_bits()));
    }
}
