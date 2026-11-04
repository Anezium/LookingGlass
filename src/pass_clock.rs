//! Whole-pass CPU clocks behind `world_pass_clock`, for console diagnostics. Off by default.
//!
//! The census splits a world draw into leaves, but its brackets inflate what they measure, and
//! the three laps per draw sit inside the pass it reports. This takes two clock reads per pass
//! and no read per draw, so a pass's time is its own. It keeps per-frame values for the survey's
//! window and prints one line after the window closes: no file, no allocation in a frame, no
//! lock, no thread-local storage. Off path: one relaxed load per scope.
#![allow(dead_code)]
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering::Relaxed};

pub(crate) const WORLD_OPAQUE: usize = 0;
pub(crate) const WORLD_SUBMIT: usize = 1;
pub(crate) const ALPHA_PREPARE: usize = 2;
pub(crate) const FX_FINISH: usize = 3;
const NAMES: [&str; 4] = ["world_opaque", "world_submit", "alpha_prepare", "fx_finish"];
const SLOTS: usize = NAMES.len();
/// The survey's window: 60 warmup frames, then 120 measured.
const WINDOW: usize = 180;
const WARMUP: usize = 60;

static ON: AtomicBool = AtomicBool::new(false);
static FRAME: AtomicUsize = AtomicUsize::new(0);
static US: [[AtomicU32; WINDOW]; SLOTS] = [const { [const { AtomicU32::new(0) }; WINDOW] }; SLOTS];
static DRAWS: [AtomicU32; WINDOW] = [const { AtomicU32::new(0) }; WINDOW];

#[cfg(target_os = "vita")]
#[inline] fn now() -> u32 {
    extern "C" { fn sceKernelGetProcessTimeLow() -> u32; }
    unsafe { sceKernelGetProcessTimeLow() }
}
#[cfg(not(target_os = "vita"))]
#[inline] fn now() -> u32 {
    static START: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
    START.get_or_init(std::time::Instant::now).elapsed().as_micros() as u32
}

/// The survey window opens. Runs before the window's first clock read.
pub(crate) fn arm() {
    #[cfg(feature = "vita_renderer")]
    if crate::render::vita_renderer::config().world_pass_clock { arm_now(); }
}
fn arm_now() { FRAME.store(0, Relaxed); ON.store(true, Relaxed); }
/// One frame of the window was presented.
#[inline] pub(crate) fn presented() {
    if !ON.load(Relaxed) { return; }
    let next = FRAME.load(Relaxed) + 1;
    FRAME.store(next, Relaxed);
    if next >= WINDOW { ON.store(false, Relaxed); }
}
pub(crate) struct Scope { slot: usize, start: u32, on: bool }
#[inline] pub(crate) fn scope(slot: usize) -> Scope {
    let on = ON.load(Relaxed);
    Scope { slot, start: if on { now() } else { 0 }, on }
}
impl Drop for Scope {
    #[inline] fn drop(&mut self) {
        if !self.on { return; }
        let elapsed = now().wrapping_sub(self.start);
        if let Some(cell) = US[self.slot].get(FRAME.load(Relaxed)) { cell.fetch_add(elapsed, Relaxed); }
    }
}
#[inline] pub(crate) fn draws(count: usize) {
    if !ON.load(Relaxed) { return; }
    if let Some(cell) = DRAWS.get(FRAME.load(Relaxed)) { cell.store(count as u32, Relaxed); }
}
fn median(values: &mut [u32]) -> u32 {
    values.sort_unstable();
    values.get(values.len() / 2).copied().unwrap_or(0)
}
/// The window's line. Warmup frames are left out, as the survey's own medians leave them out.
pub(crate) fn summary(map: &str) -> Option<String> {
    #[cfg(feature = "vita_renderer")]
    if !crate::render::vita_renderer::config().world_pass_clock { return None; }
    #[cfg(not(feature = "vita_renderer"))]
    { let _ = map; return None; }
    #[cfg(feature = "vita_renderer")]
    Some(line(map))
}
fn line(map: &str) -> String {
    let frames = FRAME.load(Relaxed).min(WINDOW);
    let measured = frames.saturating_sub(WARMUP);
    let mut text = format!("[lg/pass-clock] map={map} frames={frames} measured={measured}");
    for (slot, name) in NAMES.iter().enumerate() {
        let mut values: Vec<u32> = US[slot][WARMUP.min(frames)..frames].iter().map(|c| c.load(Relaxed)).collect();
        text.push_str(&format!(" {name}_median_us={}", median(&mut values)));
    }
    let mut values: Vec<u32> = DRAWS[WARMUP.min(frames)..frames].iter().map(|c| c.load(Relaxed)).collect();
    text.push_str(&format!(" world_draws_median={}", median(&mut values)));
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    /// Armed state is process-wide: everything is in one test on purpose.
    #[test]
    fn scopes_count_only_while_armed_and_the_line_leaves_out_warmup() {
        { let _s = scope(FX_FINISH); }
        assert_eq!(US[FX_FINISH][0].load(Relaxed), 0, "off path must not count");
        arm_now();
        for frame in 0..WINDOW {
            US[WORLD_OPAQUE][frame].store(if frame < WARMUP { 90_000 } else { 1000 + frame as u32 }, Relaxed);
            US[WORLD_SUBMIT][frame].store(frame as u32, Relaxed);
            draws(if frame < WARMUP { 9 } else { 503 });
            presented();
        }
        assert!(!ON.load(Relaxed), "the window ends by itself");
        presented();
        assert_eq!(FRAME.load(Relaxed), WINDOW);
        let line = line("skool2");
        assert!(line.contains("map=skool2 frames=180 measured=120"), "{line}");
        // Measured values are 1060..=1179, sorted index 60 is 1120. A warmup leak would show 90000.
        assert!(line.contains("world_opaque_median_us=1120"), "{line}");
        assert!(line.contains("world_submit_median_us=120"), "{line}");
        assert!(line.contains("world_draws_median=503"), "{line}");
        { let _s = scope(WORLD_OPAQUE); }
        assert_eq!(US[WORLD_OPAQUE][0].load(Relaxed), 90_000, "disarmed after the window");
        assert_eq!(median(&mut []), 0);
    }
}
