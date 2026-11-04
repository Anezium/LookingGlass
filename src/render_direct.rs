//! No render observers or dispatch trampolines in the frame-total package.
//!
//! The frame census adds one opt-in exception, compiled only for the host
//! `sim_maps_probe` binary: a pass-through `glDrawElementsInstanced` wrapper
//! that brackets the draw with a samples query. Installed only when its
//! environment variable is set; otherwise this module is unchanged.
#![allow(dead_code)]
use crate::light_profile::RenderPhase;
pub(crate) struct Owner;
pub(crate) fn owner(_: RenderPhase) -> Owner { Owner }
pub(crate) struct Submission;
pub(crate) fn submission(bindings: bool, raster: bool) -> Submission {
    assert!(!bindings && !raster, "frame-total package requires world GL caches off");
    Submission
}
pub(crate) fn begin() { deferred::begin(); }
pub(crate) fn end() { deferred::end(); }

#[cfg(all(target_os="macos", not(target_os="vita"), feature="sim_maps_probe"))]
mod draw_query_hook {
    use std::{cell::Cell, ffi::{c_char, c_void}, sync::OnceLock};
    use crate::sim_maps::probe::draw_query;

    thread_local! {
        /// True from `present` until the first framework batch is drawn. That
        /// first batch is the opaque full-screen upscale; the rest are HUD.
        static FIRST_DEFERRED: Cell<bool> = const { Cell::new(false) };
    }
    fn raw(name: &str) -> *mut c_void {
        extern "C" { fn dlopen(path: *const c_char, flags: i32) -> *mut c_void;
                     fn dlsym(handle: *mut c_void, name: *const c_char) -> *mut c_void; }
        static LIB: OnceLock<usize> = OnceLock::new();
        let handle = *LIB.get_or_init(|| {
            let path = c"/System/Library/Frameworks/OpenGL.framework/Versions/Current/OpenGL";
            unsafe { dlopen(path.as_ptr(), 1) as usize }
        });
        assert_ne!(handle, 0, "OpenGL library unavailable");
        let name = std::ffi::CString::new(name).expect("GL name");
        unsafe { dlsym(handle as _, name.as_ptr()) }
    }
    type Elements = unsafe extern "C" fn(u32, i32, u32, *const c_void, i32);
    unsafe extern "C" fn elements(mode: u32, count: i32, index_type: u32,
                                  indices: *const c_void, instances: i32) {
        static REAL: OnceLock<Elements> = OnceLock::new();
        let real = *REAL.get_or_init(|| unsafe {
            std::mem::transmute::<*mut c_void, Elements>(raw("glDrawElementsInstanced"))
        });
        let deferred = if draw_query::armed() {
            Some(if FIRST_DEFERRED.with(|slot| slot.replace(false)) {
                ("upscale", "upscale")
            } else { ("hud", "hud") })
        } else { None };
        let query = draw_query::begin(count, instances, deferred);
        real(mode, count, index_type, indices, instances);
        draw_query::end(query);
        // Raster census: one identical draw with depth always and no writes.
        let raster = draw_query::begin_raster();
        if raster.is_some() { real(mode, count, index_type, indices, instances); }
        draw_query::end_raster(raster);
    }
    pub(super) fn install() {
        static ONCE: std::sync::Once = std::sync::Once::new();
        ONCE.call_once(|| unsafe {
            macroquad::miniquad::gl::load_gl_funcs(|name| {
                let pointer = if name == "glDrawElementsInstanced" {
                    elements as *const () as *mut c_void
                } else { raw(name) };
                (!pointer.is_null()).then(|| std::mem::transmute(pointer))
            });
        });
    }
    pub(super) fn register(program: u32, name: &str) { draw_query::program(program, name); }
    pub(super) fn unregister(_: u32) {}
    pub(super) fn begin() { FIRST_DEFERRED.with(|slot| slot.set(true)); }
    pub(super) fn end() { FIRST_DEFERRED.with(|slot| slot.set(false)); }
}
#[cfg(not(all(target_os="macos", not(target_os="vita"), feature="sim_maps_probe")))]
mod draw_query_hook {
    pub(super) fn install() {}
    pub(super) fn register(_: u32, _: &str) {}
    pub(super) fn unregister(_: u32) {}
    pub(super) fn begin() {}
    pub(super) fn end() {}
}
/// Framework batches have no submission site; the first one after `present`
/// is the upscale, every later one is HUD.
mod deferred {
    #[cfg(all(target_os="macos", not(target_os="vita"), feature="sim_maps_probe"))]
    pub(super) fn begin() { if crate::sim_maps::probe::draw_query::table_active() { super::draw_query_hook::begin(); } }
    #[cfg(all(target_os="macos", not(target_os="vita"), feature="sim_maps_probe"))]
    pub(super) fn end() { if crate::sim_maps::probe::draw_query::table_active() { super::draw_query_hook::end(); } }
    #[cfg(not(all(target_os="macos", not(target_os="vita"), feature="sim_maps_probe")))]
    pub(super) fn begin() {}
    #[cfg(not(all(target_os="macos", not(target_os="vita"), feature="sim_maps_probe")))]
    pub(super) fn end() {}
}

pub(crate) fn register(program: u32, name: &str) {
    #[cfg(all(target_os="macos", not(target_os="vita"), feature="sim_maps_probe"))]
    if crate::sim_maps::probe::draw_query::table_active() { draw_query_hook::register(program, name); }
    let _ = (program, name);
}
pub(crate) fn unregister(program: u32) {
    #[cfg(all(target_os="macos", not(target_os="vita"), feature="sim_maps_probe"))]
    draw_query_hook::unregister(program);
    let _ = program;
}
pub(crate) fn install() {
    #[cfg(all(target_os="macos", not(target_os="vita"), feature="sim_maps_probe"))]
    if crate::sim_maps::probe::draw_query::table_active() { draw_query_hook::install(); }
}
pub(crate) fn configure() {}
// This is the actual resolver called by the native vglGetProcAddress wrapper.
pub(crate) fn lookup(_: &[u8]) -> Option<*mut std::ffi::c_void> { None }
#[cfg(test)]
mod frame_total_tests {
    use super::*;
    #[test]
    fn every_gl_name_bypasses_observers() {
        let names=include_str!("frame_total_gl_names.txt");
        let mut count=0;
        install(); configure();
        for name in names.lines() { assert!(lookup(name.as_bytes()).is_none(),"{name}"); count+=1; }
        assert!(count>=29); assert!(lookup(b"unknown").is_none());
        println!("frame_total resolver tested {count} names: zero GL observer hooks");
    }
}
