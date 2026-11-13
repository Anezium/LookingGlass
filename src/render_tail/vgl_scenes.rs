//! Host-only model of the pinned vitaGL scene rule. That build (dca4b9d, no
//! `STORE_DEPTH_STENCIL`) ends the GXM scene whenever a draw or clear targets another
//! framebuffer, and on `glFlush`, `glFinish`, `glReadPixels` of the framebuffer in use and
//! `glBlitFramebuffer`. Depth is neither stored nor loaded, so the next scene on a
//! framebuffer object starts at the background depth, 1.0.
//!
//! `LOOKING_GLASS_VGL_SCENES=log` prints `VGL_SCENE_BREAK` when a render target is drawn
//! into again after such a break, with the call that ended its scene.
//! `LOOKING_GLASS_VGL_SCENES=emulate` also clears that target's depth at the restart, so
//! the host picture shows what the Vita draws. `LOOKING_GLASS_VGL_SCENES=trace` prints
//! every restart of frames 60 and 61 with its call site. Off unless the variable is set.
#![allow(dead_code)] // `active` and `install_raw` serve only the frame-total table.
use std::{cell::RefCell, collections::HashMap, ffi::c_void, sync::OnceLock};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode { Log, Emulate, Trace }
fn mode() -> Option<Mode> {
    static MODE: OnceLock<Option<Mode>> = OnceLock::new();
    *MODE.get_or_init(|| match std::env::var("LOOKING_GLASS_VGL_SCENES").as_deref() {
        Ok("log") => Some(Mode::Log),
        Ok("emulate") => Some(Mode::Emulate),
        Ok("trace") => Some(Mode::Trace),
        _ => None,
    })
}

/// Why a scene that already held draws ended on a framebuffer object.
struct Ended { frame: u64, cause: String, trace: String }
#[derive(Default)]
struct State {
    bound: u32,
    in_use: Option<u32>,
    not_flushed: bool,
    /// The scene in use has received at least one draw.
    drawn: bool,
    ended: HashMap<u32, Ended>,
    lost: Option<Ended>,
    sites: HashMap<String, u64>,
    breaks: u64,
    frame: u64,
    restarts: u64,
    draws: u64,
}
thread_local! { static STATE: RefCell<State> = RefCell::default(); }

fn frame() -> u64 { crate::sim_maps::probe::frame() }
fn trace() -> String {
    let text = std::backtrace::Backtrace::force_capture().to_string();
    text.lines().filter(|l| l.contains("looking_glass::") && !l.contains("vgl_scenes"))
        .map(|l| l.trim().trim_start_matches(|c: char| c.is_ascii_digit() || c == ':').trim())
        .take(12).collect::<Vec<_>>().join(" <- ")
}

macro_rules! next {
    ($($slot:ident: fn($($ty:ty),*)),* $(,)?) => {
        $(static $slot: OnceLock<unsafe extern "C" fn($($ty),*)> = OnceLock::new();)*
    };
}
next! {
    BIND: fn(u32, u32), CLEAR: fn(u32), ELEMENTS: fn(u32, i32, u32, *const c_void),
    ARRAYS: fn(u32, i32, i32), ELEMENTS_I: fn(u32, i32, u32, *const c_void, i32),
    ARRAYS_I: fn(u32, i32, i32, i32), FLUSH: fn(), FINISH: fn(),
    READ: fn(i32, i32, i32, i32, u32, u32, *mut c_void),
    BLIT: fn(i32, i32, i32, i32, i32, i32, i32, i32, u32, u32),
}
static GET_INTEGER: OnceLock<unsafe extern "C" fn(u32, *mut i32)> = OnceLock::new();
static GET_FLOAT: OnceLock<unsafe extern "C" fn(u32, *mut f32)> = OnceLock::new();
static IS_ENABLED: OnceLock<unsafe extern "C" fn(u32) -> u8> = OnceLock::new();
static ENABLE: OnceLock<unsafe extern "C" fn(u32)> = OnceLock::new();
static DISABLE: OnceLock<unsafe extern "C" fn(u32)> = OnceLock::new();
static DEPTH_MASK: OnceLock<unsafe extern "C" fn(u8)> = OnceLock::new();
static CLEAR_DEPTH: OnceLock<unsafe extern "C" fn(f64)> = OnceLock::new();

/// Background depth for a fresh scene on `fb`, outside the caller's scissor and mask.
unsafe fn clear_depth(fb: u32) {
    if fb == 0 { return; }
    let (get_i, get_f, is_on) = (GET_INTEGER.get().unwrap(), GET_FLOAT.get().unwrap(), IS_ENABLED.get().unwrap());
    let mut mask = 0i32; get_i(0x0B72, &mut mask);
    let mut value = 1f32; get_f(0x0B73, &mut value);
    let scissor = is_on(0x0C11) != 0;
    DISABLE.get().unwrap()(0x0C11);
    DEPTH_MASK.get().unwrap()(1);
    CLEAR_DEPTH.get().unwrap()(1.0);
    CLEAR.get().unwrap()(0x0100);
    CLEAR_DEPTH.get().unwrap()(f64::from(value));
    DEPTH_MASK.get().unwrap()(u8::from(mask != 0));
    if scissor { ENABLE.get().unwrap()(0x0C11); }
}

/// vitaGL `scene_reset`: `forced` is set by flush, finish, readback and blit.
fn scene(forced: Option<&str>, depth_cleared: bool, draws: bool) {
    let Some(m) = mode() else { return };
    let restart = STATE.with(|s| {
        let mut s = s.borrow_mut();
        let now = frame();
        if now != s.frame {
            println!("VGL_SCENES {}", serde_json::json!({"frame": s.frame, "restarts": s.restarts, "draws": s.draws}));
            (s.frame, s.restarts, s.draws) = (now, 0, 0);
        }
        s.draws += u64::from(draws);
        let target = s.bound;
        if s.in_use == Some(target) && forced.is_none() {
            if draws { s.drawn = true; report(&mut s, target); }
            return None;
        }
        let previous = s.in_use;
        s.restarts += 1;
        if m == Mode::Trace && (60..62).contains(&now) {
            println!("VGL_SCENE_TRACE {}", serde_json::json!({"frame": now, "from": previous, "to": target,
                "forced": forced, "depth_cleared": depth_cleared, "draw": draws, "trace": trace()}));
        }
        s.in_use = Some(target);
        s.not_flushed = true;
        // A scene with draws ended on an FBO: its depth is gone. Remember what ended it.
        if let Some(fb) = previous.filter(|&fb| fb != 0 && s.drawn) {
            let cause = match forced {
                Some(c) => c.to_string(),
                None => format!("draw or clear into framebuffer {target}"),
            };
            s.ended.insert(fb, Ended { frame: now, cause, trace: trace() });
        }
        s.drawn = false;
        if target == 0 { return None; }
        s.lost = None;
        if depth_cleared { s.ended.remove(&target); }
        else if let Some(e) = s.ended.remove(&target).filter(|e| e.frame == now) { s.lost = Some(e); }
        if draws { s.drawn = true; report(&mut s, target); }
        Some(target)
    });
    if let (Some(fb), Mode::Emulate) = (restart, m) {
        if !depth_cleared { unsafe { clear_depth(fb); } }
    }
}
fn report(s: &mut State, target: u32) {
    let Some(l) = s.lost.take() else { return };
    s.breaks += 1;
    let drawer = trace();
    let seen = s.sites.entry(format!("{}|{}", l.trace, drawer)).or_default();
    *seen += 1;
    let first = *seen == 1;
    println!("VGL_SCENE_BREAK {}", serde_json::json!({
        "frame": frame(), "target": target, "cause": l.cause, "ended_frame": l.frame, "breaks": s.breaks,
        "site_count": *seen, "ended_by": if first { l.trace } else { String::new() },
        "next_draw": if first { drawer } else { String::new() },
    }));
}

unsafe extern "C" fn bind(target: u32, fb: u32) {
    if target == 0x8D40 || target == 0x8CA9 { STATE.with(|s| s.borrow_mut().bound = fb); }
    BIND.get().unwrap()(target, fb);
}
unsafe extern "C" fn clear(mask: u32) {
    scene(None, mask & 0x0100 != 0, false);
    CLEAR.get().unwrap()(mask);
}
unsafe extern "C" fn elements(m: u32, n: i32, k: u32, p: *const c_void) { scene(None, false, true); ELEMENTS.get().unwrap()(m, n, k, p); }
unsafe extern "C" fn arrays(m: u32, f: i32, n: i32) { scene(None, false, true); ARRAYS.get().unwrap()(m, f, n); }
unsafe extern "C" fn elements_i(m: u32, n: i32, k: u32, p: *const c_void, i: i32) { scene(None, false, true); ELEMENTS_I.get().unwrap()(m, n, k, p, i); }
unsafe extern "C" fn arrays_i(m: u32, f: i32, n: i32, i: i32) { scene(None, false, true); ARRAYS_I.get().unwrap()(m, f, n, i); }
unsafe extern "C" fn flush() {
    if STATE.with(|s| s.borrow().not_flushed) {
        scene(Some("glFlush"), false, false);
        STATE.with(|s| s.borrow_mut().not_flushed = false);
    }
    FLUSH.get().unwrap()();
}
unsafe extern "C" fn finish() {
    if STATE.with(|s| s.borrow().not_flushed) {
        scene(Some("glFinish"), false, false);
        STATE.with(|s| s.borrow_mut().not_flushed = false);
    }
    FINISH.get().unwrap()();
}
unsafe extern "C" fn read(x: i32, y: i32, w: i32, h: i32, f: u32, t: u32, p: *mut c_void) {
    // The host model treats the draw binding as the read binding, as miniquad does.
    if STATE.with(|s| { let s = s.borrow(); s.in_use == Some(s.bound) }) { scene(Some("glReadPixels"), false, false); }
    READ.get().unwrap()(x, y, w, h, f, t, p);
}
unsafe extern "C" fn blit(a: i32, b: i32, c: i32, d: i32, e: i32, f: i32, g: i32, h: i32, m: u32, i: u32) {
    scene(Some("glBlitFramebuffer"), false, false);
    BLIT.get().unwrap()(a, b, c, d, e, f, g, h, m, i);
}

pub(crate) fn active() -> bool { mode().is_some() }
/// Frame-total builds have no observer table: wrap the OpenGL library entries directly.
pub(crate) fn install_raw() {
    fn raw(name: &str) -> *mut c_void {
        extern "C" { fn dlopen(path: *const std::ffi::c_char, flags: i32) -> *mut c_void;
                     fn dlsym(handle: *mut c_void, name: *const std::ffi::c_char) -> *mut c_void; }
        static LIB: OnceLock<usize> = OnceLock::new();
        let handle = *LIB.get_or_init(|| unsafe {
            dlopen(c"/System/Library/Frameworks/OpenGL.framework/Versions/Current/OpenGL".as_ptr(), 1) as usize
        });
        assert_ne!(handle, 0, "OpenGL library unavailable");
        let name = std::ffi::CString::new(name).expect("GL name");
        unsafe { dlsym(handle as _, name.as_ptr()) }
    }
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| macroquad::miniquad::gl::load_gl_funcs(|name| unsafe {
        let pointer = wrap(name, &raw).unwrap_or_else(|| raw(name));
        (!pointer.is_null()).then(|| std::mem::transmute(pointer))
    }));
}
/// Wraps `name` when the model is on; `base` resolves the next function in the chain.
pub(super) fn wrap(name: &str, base: &dyn Fn(&str) -> *mut c_void) -> Option<*mut c_void> {
    mode()?;
    unsafe fn set<T: Copy>(slot: &OnceLock<T>, p: *mut c_void) { let _ = slot.set(std::mem::transmute_copy(&p)); }
    unsafe {
        set(&GET_INTEGER, base("glGetIntegerv")); set(&GET_FLOAT, base("glGetFloatv"));
        set(&IS_ENABLED, base("glIsEnabled")); set(&ENABLE, base("glEnable")); set(&DISABLE, base("glDisable"));
        set(&DEPTH_MASK, base("glDepthMask")); set(&CLEAR_DEPTH, base("glClearDepth"));
        Some(match name {
            "glBindFramebuffer" => { set(&BIND, base(name)); bind as *const () as _ }
            "glClear" => { set(&CLEAR, base(name)); clear as *const () as _ }
            "glDrawElements" => { set(&ELEMENTS, base(name)); elements as *const () as _ }
            "glDrawArrays" => { set(&ARRAYS, base(name)); arrays as *const () as _ }
            "glDrawElementsInstanced" => { set(&ELEMENTS_I, base(name)); elements_i as *const () as _ }
            "glDrawArraysInstanced" => { set(&ARRAYS_I, base(name)); arrays_i as *const () as _ }
            "glFlush" => { set(&FLUSH, base(name)); flush as *const () as _ }
            "glFinish" => { set(&FINISH, base(name)); finish as *const () as _ }
            "glReadPixels" => { set(&READ, base(name)); read as *const () as _ }
            "glBlitFramebuffer" => { set(&BLIT, base(name)); blit as *const () as _ }
            _ => return None,
        })
    }
}
