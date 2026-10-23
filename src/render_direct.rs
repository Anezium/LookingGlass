//! No render observers or dispatch trampolines in the frame-total package.
#![allow(dead_code)]
use crate::light_profile::RenderPhase;
pub(crate) struct Owner;
pub(crate) fn owner(_: RenderPhase) -> Owner { Owner }
pub(crate) struct Submission;
pub(crate) fn submission(bindings: bool, raster: bool) -> Submission {
    assert!(!bindings && !raster, "frame-total package requires world GL caches off");
    Submission
}
pub(crate) fn begin() {}
pub(crate) fn end() {}
pub(crate) fn register(_: u32, _: &str) {}
pub(crate) fn unregister(_: u32) {}
pub(crate) fn install() {}
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
