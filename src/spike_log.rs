//! `spike_log_ms`: a console diagnostic for frames that stall in play. Default 0, which is off.
//!
//! With a threshold above zero, every gameplay frame slower than it prints one `[lg/spike]` line: the
//! main-thread time split into simulation (input to the end of particle placement), render (light
//! build, world, actors and effects submission), HUD and tail, and present (from the end of the
//! frame's CPU work to the start of the next frame, which holds the swap and the vblank wait). On the
//! Vita it adds the time the main thread spent inside `sceGxmBeginScene` and `sceGxmEndScene`, where
//! vitaGL waits for the GPU. Every asset read, texture upload, shader build and audio clip decode that
//! runs on the main thread outside a load phase prints its own `[lg/late-work]` line with its name and
//! time, and the frame's spike line counts them.
//!
//! Nothing here runs when the key is 0 beyond one config load per call site.
use std::{cell::RefCell, time::Instant};

pub(crate) const KINDS: [&str; 7] = ["texture_read", "texture_decode", "texture_upload", "shader", "asset_read",
    "material_parse", "audio_clip"];
pub(crate) const AUDIO_CLIP: usize = 6;

#[derive(Default)]
struct Frame {
    number: u64,
    begin: Option<Instant>,
    render: Option<Instant>,
    hud: Option<Instant>,
    cpu_end: Option<Instant>,
    calls: [u32; 7],
    ms: [f64; 7],
}
thread_local! { static FRAME: RefCell<Frame> = RefCell::default(); }

#[inline]
pub(crate) fn threshold_ms() -> f32 {
    #[cfg(feature = "vita_renderer")]
    { crate::render::vita_renderer::config().spike_log_ms }
    #[cfg(not(feature = "vita_renderer"))]
    { 0. }
}
#[inline]
pub(crate) fn on() -> bool { threshold_ms() > 0. }

/// Top of the viewer's gameplay loop. Closes the previous frame and prints it if it was slow.
pub(crate) fn begin_frame() {
    let limit = threshold_ms();
    if limit <= 0. { return; }
    #[cfg(target_os = "vita")]
    crate::gpu_timing::spike_enable();
    let now = Instant::now();
    #[cfg(target_os = "vita")]
    let gxm = crate::gpu_timing::spike_take();
    #[cfg(not(target_os = "vita"))]
    let gxm: Option<[u32; 2]> = None;
    FRAME.with(|f| {
        let mut f = f.borrow_mut();
        if let (Some(begin), Some(render), Some(hud), Some(cpu_end)) = (f.begin, f.render, f.hud, f.cpu_end) {
            let ms = |a: Instant, b: Instant| b.saturating_duration_since(a).as_secs_f64() * 1000.;
            let total = ms(begin, now);
            if total >= f64::from(limit) {
                let mut line = format!("[lg/spike] frame={} ms={total:.2} sim={:.2} render={:.2} hud={:.2} present={:.2}",
                    f.number, ms(begin, render), ms(render, hud), ms(hud, cpu_end), ms(cpu_end, now));
                if let Some([begin_us, end_us]) = gxm {
                    line.push_str(&format!(" gxm_begin={:.2} gxm_end={:.2}", f64::from(begin_us) / 1000., f64::from(end_us) / 1000.));
                }
                for (i, name) in KINDS.iter().enumerate() {
                    if f.calls[i] > 0 { line.push_str(&format!(" {name}={}/{:.2}", f.calls[i], f.ms[i])); }
                }
                println!("{line}");
            }
        }
        let number = f.number + 1;
        *f = Frame { number, begin: Some(now), ..Frame::default() };
    });
}
/// Simulation, camera and particle placement are done; light build and submission start.
pub(crate) fn render_begin() { mark(|f, now| f.render = Some(now)); }
/// The 3D frame is submitted; HUD, menus and the frame tail start.
pub(crate) fn hud_begin() { mark(|f, now| f.hud = Some(now)); }
/// The frame's CPU work ends; swap and vblank follow.
pub(crate) fn cpu_end() { mark(|f, now| f.cpu_end = Some(now)); }
fn mark(set: impl FnOnce(&mut Frame, Instant)) {
    if !on() { return; }
    let now = Instant::now();
    FRAME.with(|f| set(&mut f.borrow_mut(), now));
}
/// A loader call outside any load phase. `name` is printed when given.
pub(crate) fn late(kind: usize, ms: f64, name: Option<&str>, bytes: Option<usize>) {
    if !on() { return; }
    FRAME.with(|f| {
        let mut f = f.borrow_mut();
        f.calls[kind] += 1;
        f.ms[kind] += ms;
        let number = f.number;
        drop(f);
        let mut line = format!("[lg/late-work] frame={number} kind={} ms={ms:.2}", KINDS[kind]);
        if let Some(b) = bytes { line.push_str(&format!(" bytes={b}")); }
        if let Some(n) = name { line.push_str(&format!(" name={n}")); }
        println!("{line}");
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn spike_log_is_off_by_default_and_counts_nothing() {
        assert_eq!(crate::render::vita_renderer::Config::default().spike_log_ms, 0.);
        assert!(!on());
        begin_frame();
        late(AUDIO_CLIP, 5., Some("x.wav"), Some(4));
        FRAME.with(|f| { let f = f.borrow(); assert!(f.begin.is_none() && f.calls.iter().all(|&c| c == 0)); });
    }
}
