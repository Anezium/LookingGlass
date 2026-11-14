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
//! The line also splits the simulation by system (`sim_split`, the viewer's simulation blocks in
//! order), says in which of those blocks and of render, HUD and present the EndScene time fell
//! (`gxm_end_in`, milliseconds and calls), and gives the NPC update per actor kind (`npc`: actors
//! updated, total, then think, movement, animation and combat milliseconds).
//!
//! The lines are not written during play. On the Vita a write to stdout costs about 17.5 us per byte on
//! the calling thread (a2-08 log: a 450-byte spike line cost the next frame 7.5 ms, made that frame
//! slow in turn, and every frame printed). While a gameplay frame is open, `[lg/spike]` and
//! `[lg/late-work]` lines are appended to a buffer reserved once (`OUT_CAP`) and written at the next
//! flush point: a replay ending, a load phase opening, or the viewer loop ending. A frame's line then
//! costs only its formatting, which the next line reports as `log=` (milliseconds). Lines that do not
//! fit are counted and the flush says how many were dropped. Lines still buffered when the game
//! crashes are lost.
//!
//! Nothing here runs when the key is 0 beyond one config load per call site.
use std::{cell::RefCell, fmt::Write as _, time::Instant};

pub(crate) const KINDS: [&str; 7] = ["texture_read", "texture_decode", "texture_upload", "shader", "asset_read",
    "material_parse", "audio_clip"];
pub(crate) const AUDIO_CLIP: usize = 6;

/// The viewer's simulation blocks, in the order a frame runs them. A block that a frame skips
/// leaves its time with the block before it.
#[derive(Clone, Copy)]
pub(crate) enum Sim {
    Input, Search, Interactions, Movers, Movement, Story, Particles, Combat, Encounters, Levels, Npcs,
    CombatRest, Alice, Audio, Camera, AudioMix, Scene, Particles2,
}
const SIM: [&str; 18] = ["input", "search", "interactions", "movers", "movement", "story", "particles", "combat",
    "encounters", "levels", "npcs", "combat_rest", "alice", "audio", "camera", "audio_mix", "scene", "particles2"];
/// Regions after the simulation, for the EndScene attribution.
const RENDER: usize = SIM.len();
const HUD: usize = SIM.len() + 1;
const PRESENT: usize = SIM.len() + 2;
const REGIONS: usize = SIM.len() + 3;
const REGION_NAMES: [&str; 3] = ["render", "hud", "present"];
/// NPC kinds kept per frame; more kinds than this are summed into the last row.
const NPC_ROWS: usize = 24;

#[derive(Default)]
struct Frame {
    number: u64,
    flight_work: [u32;6],
    begin: Option<Instant>,
    render: Option<Instant>,
    hud: Option<Instant>,
    cpu_end: Option<Instant>,
    calls: [u32; 7],
    ms: [f64; 7],
    /// The open simulation block and when it started.
    block: Option<(usize, Instant)>,
    sim_ms: [f64; SIM.len()],
    /// GXM running totals at the frame start and when the open region started, then EndScene
    /// microseconds and calls per region.
    gxm_start: Option<[u32; 3]>,
    gxm_mark: Option<[u32; 3]>,
    region: usize,
    end_us: [u32; REGIONS],
    end_calls: [u32; REGIONS],
    npc: [Option<Npc>; NPC_ROWS],
    /// Path searches: Ant ground plans and flying routes, as (calls, milliseconds, expansions, clearance tests).
    planners: [(u32, f64, u32, u32); 2],
    /// Milliseconds this frame spent formatting the previous frame's line into the buffer.
    log_ms: f64,
}
#[derive(Clone, Copy)]
struct Npc { name: &'static str, updated: u64, ms: [f64; 4] }
thread_local! { static FRAME: RefCell<Frame> = RefCell::default(); }

/// Buffered lines, written at the next flush point.
#[derive(Default)]
struct Out { text: String, lines: u32, dropped: u32 }
/// Buffer size, reserved on the first buffered line. About 2,000 spike lines.
const OUT_CAP: usize = 1 << 20;
/// Room kept for the line being written, so the buffer never reallocates. Spike lines are under 2 KiB.
const LINE_MAX: usize = 4096;
thread_local! { static OUT: RefCell<Out> = RefCell::default(); }
impl Out {
    /// Appends one line written by `write`, or counts it as dropped when it does not fit.
    fn push(&mut self, write: impl FnOnce(&mut String)) {
        if self.text.capacity() == 0 { self.text.reserve_exact(OUT_CAP); }
        let mark = self.text.len();
        if mark + LINE_MAX > OUT_CAP { self.dropped += 1; return; }
        write(&mut self.text);
        self.text.push('\n');
        if self.text.len() - mark > LINE_MAX {
            self.text.truncate(mark);
            self.dropped += 1;
        } else {
            self.lines += 1;
        }
    }

}

#[inline]
pub(crate) fn threshold_ms() -> f32 {
    #[cfg(feature = "vita_renderer")]
    { crate::render::vita_renderer::config().spike_log_ms }
    #[cfg(not(feature = "vita_renderer"))]
    { 0. }
}
#[inline]
pub(crate) fn on() -> bool { threshold_ms() > 0. }

fn gxm_now() -> Option<[u32; 3]> {
    #[cfg(target_os = "vita")]
    { crate::gpu_timing::spike_peek() }
    #[cfg(not(target_os = "vita"))]
    { None }
}
impl Frame {
    /// Charges the GXM time since the last mark to the open region and opens `next`.
    fn gxm_switch(&mut self, next: usize) {
        let now = gxm_now();
        if let (Some(before), Some(after)) = (self.gxm_mark, now) {
            self.end_us[self.region] = self.end_us[self.region].wrapping_add(after[1].wrapping_sub(before[1]));
            self.end_calls[self.region] = self.end_calls[self.region].wrapping_add(after[2].wrapping_sub(before[2]));
        }
        self.gxm_mark = now;
        self.region = next;
    }
    fn close_block(&mut self, now: Instant) {
        if let Some((block, since)) = self.block.take() {
            self.sim_ms[block] += now.saturating_duration_since(since).as_secs_f64() * 1000.;
        }
    }
    #[cfg(test)]
    fn line(&self, now: Instant, marks: [Instant; 4]) -> String {
        let mut line = String::new();
        self.write_line(&mut line, now, marks);
        line
    }
    /// Appends the frame's line to `out` without allocating beyond `out` itself.
    fn write_line(&self, out: &mut String, now: Instant, [begin, render, hud, cpu_end]: [Instant; 4]) {
        let ms = |a: Instant, b: Instant| b.saturating_duration_since(a).as_secs_f64() * 1000.;
        let _ = write!(out, "[lg/spike] frame={} ms={:.2} sim={:.2} render={:.2} hud={:.2} present={:.2}",
            self.number, ms(begin, now), ms(begin, render), ms(render, hud), ms(hud, cpu_end), ms(cpu_end, now));
        if self.log_ms > 0. { let _ = write!(out, " log={:.2}", self.log_ms); }
        let gxm = self.gxm_start.zip(self.gxm_mark);
        if let Some((start, end)) = gxm {
            let _ = write!(out, " gxm_begin={:.2} gxm_end={:.2}", f64::from(end[0].wrapping_sub(start[0])) / 1000.,
                f64::from(end[1].wrapping_sub(start[1])) / 1000.);
        }
        for (i, name) in KINDS.iter().enumerate() {
            if self.calls[i] > 0 { let _ = write!(out, " {name}={}/{:.2}", self.calls[i], self.ms[i]); }
        }
        out.push_str(" sim_split=");
        let mut any = false;
        for (name, ms) in SIM.iter().zip(self.sim_ms).filter(|(_, ms)| *ms >= 0.05) {
            let _ = write!(out, "{}{name}:{ms:.2}", if any { "," } else { "" });
            any = true;
        }
        if !any { out.push('-'); }
        if gxm.is_some() {
            out.push_str(" gxm_end_in=");
            let mut any = false;
            for r in (0..REGIONS).filter(|&r| self.end_calls[r] > 0) {
                let _ = write!(out, "{}{}:{:.2}/{}", if any { "," } else { "" },
                    if r < SIM.len() { SIM[r] } else { REGION_NAMES[r - SIM.len()] },
                    f64::from(self.end_us[r]) / 1000., self.end_calls[r]);
                any = true;
            }
            if !any { out.push('-'); }
        }
        let mut any = false;
        for n in self.npc.iter().flatten().filter(|n| n.ms.iter().sum::<f64>() >= 0.05) {
            let _ = write!(out, "{}{}:{}/{:.2}/{:.2}/{:.2}/{:.2}/{:.2}", if any { "," } else { " npc=" }, n.name, n.updated,
                n.ms.iter().sum::<f64>(), n.ms[0], n.ms[1], n.ms[2], n.ms[3]);
            any = true;
        }
        if self.flight_work[0] > 0 {
            let [calls,tests,traversals,witnesses,short,free]=self.flight_work;
            let _=write!(out," flight_work={calls}/{tests}/{traversals}/{witnesses}/{short}/{free}");
        }
        for (name, (calls, ms, expanded, tests)) in PLANNERS.iter().zip(self.planners) {
            if calls > 0 { let _ = write!(out, " {name}={calls}/{ms:.2}/{expanded}/{tests}"); }
        }
    }
}

/// Top of the viewer's gameplay loop. Closes the previous frame and prints it if it was slow.
pub(crate) fn begin_frame() {
    let limit = threshold_ms();
    if limit <= 0. { return; }
    #[cfg(target_os = "vita")]
    crate::gpu_timing::spike_enable();
    let now = Instant::now();
    FRAME.with(|f| {
        let mut f = f.borrow_mut();
        f.close_block(now);
        f.gxm_switch(0);
        // The new frame starts at `now`, so formatting the previous line is charged to it (`log=`).
        let mut log_ms = 0.;
        if let (Some(begin), Some(render), Some(hud), Some(cpu_end)) = (f.begin, f.render, f.hud, f.cpu_end) {
            let in_window = {
                #[cfg(feature="vita_renderer")]
                { let c=crate::render::vita_renderer::config(); f.number >= u64::from(c.spike_log_first_frame) && (c.spike_log_last_frame == 0 || f.number <= u64::from(c.spike_log_last_frame)) }
                #[cfg(not(feature="vita_renderer"))]
                { true }
            };
            if in_window && now.saturating_duration_since(begin).as_secs_f64() * 1000. >= f64::from(limit) {
                OUT.with(|o| o.borrow_mut().push(|text| f.write_line(text, now, [begin, render, hud, cpu_end])));
                log_ms = now.elapsed().as_secs_f64() * 1000.;
            }
        }
        let number = f.number + 1;
        let mark = f.gxm_mark;
        *f = Frame { number, begin: Some(now), block: Some((Sim::Input as usize, now)), gxm_start: mark,
            gxm_mark: mark, log_ms, ..Frame::default() };
    });
}
/// Writes the buffered lines, one record each, then one `[lg/spike] flushed` line with the count,
/// the drops and the time the writes took. `at` names the flush point. Called outside play.
pub(crate) fn flush(at: &str) {
    if !on() { return; }
    let (mut text, lines, dropped) = OUT.with(|o| {
        let mut o = o.borrow_mut();
        (std::mem::take(&mut o.text), std::mem::take(&mut o.lines), std::mem::take(&mut o.dropped))
    });
    if lines > 0 || dropped > 0 {
        let start = Instant::now();
        for line in text.lines() { println!("{line}"); }
        println!("[lg/spike] flushed at={at} lines={lines} dropped={dropped} bytes={} ms={:.1}", text.len(),
            start.elapsed().as_secs_f64() * 1000.);
    }
    // Keep the reserved buffer for the next lines.
    text.clear();
    OUT.with(|o| { let mut o = o.borrow_mut(); if o.text.capacity() == 0 { o.text = text; } });
}
/// The viewer loop ended: flush, and print later lines at once until the next `begin_frame`.
pub(crate) fn end_play() {
    if !on() { return; }
    FRAME.with(|f| { let mut f = f.borrow_mut(); let number = f.number; *f = Frame { number, ..Frame::default() }; });
    flush("viewer-end");
}
/// The simulation enters `block`.
pub(crate) fn sim(block: Sim) {
    if !on() { return; }
    let now = Instant::now();
    FRAME.with(|f| {
        let mut f = f.borrow_mut();
        if f.begin.is_none() || f.render.is_some() { return; }
        f.close_block(now);
        f.block = Some((block as usize, now));
        f.gxm_switch(block as usize);
    });
}
/// Simulation, camera and particle placement are done; light build and submission start.
pub(crate) fn render_begin() { mark(RENDER, |f, now| f.render = Some(now)); }
/// The 3D frame is submitted; HUD, menus and the frame tail start.
pub(crate) fn hud_begin() { mark(HUD, |f, now| f.hud = Some(now)); }
/// The frame's CPU work ends; swap and vblank follow.
pub(crate) fn cpu_end() { mark(PRESENT, |f, now| f.cpu_end = Some(now)); }
fn mark(region: usize, set: impl FnOnce(&mut Frame, Instant)) {
    if !on() { return; }
    let now = Instant::now();
    FRAME.with(|f| {
        let mut f = f.borrow_mut();
        f.close_block(now);
        f.gxm_switch(region);
        set(&mut f, now);
    });
}
/// One NPC kind's update this frame, from `npc::profile`: actors updated and milliseconds in think,
/// movement, animation and combat.
pub(crate) fn npc(name: &'static str, updated: u64, ms: [f64; 4]) {
    if !on() { return; }
    FRAME.with(|f| {
        let mut f = f.borrow_mut();
        let row = f.npc.iter().position(|n| n.is_none_or(|n| n.name == name)).unwrap_or(NPC_ROWS - 1);
        let entry = f.npc[row].get_or_insert(Npc { name, updated: 0, ms: [0.; 4] });
        entry.updated += updated;
        for (a, b) in entry.ms.iter_mut().zip(ms) { *a += b; }
    });
}
/// Path searches the spike line counts: `nav_plan` is the ground detour of Ants
/// (`combat::navigation::plan`), `flight_route` the flying route of Ladybugs (`flight::route`).
pub(crate) fn flight_work(work: [u32;6]) {
    if !on() { return; }
    FRAME.with(|f| { for (a,b) in f.borrow_mut().flight_work.iter_mut().zip(work) { *a+=b; } });
}
pub(crate) const NAV_PLAN: usize = 0;
pub(crate) const FLIGHT_ROUTE: usize = 1;
const PLANNERS: [&str; 2] = ["nav_plan", "flight_route"];
/// One path search: its time, nodes expanded and clearance tests run.
pub(crate) fn planner(kind: usize, ms: f64, expanded: u32, tests: u32) {
    if !on() { return; }
    FRAME.with(|f| {
        let p = &mut f.borrow_mut().planners[kind];
        *p = (p.0 + 1, p.1 + ms, p.2 + expanded, p.3 + tests);
    });
}
/// A loader call outside any load phase. `name` is printed when given.
pub(crate) fn late(kind: usize, ms: f64, name: Option<&str>, bytes: Option<usize>) {
    if !on() { return; }
    let (number, playing) = FRAME.with(|f| {
        let mut f = f.borrow_mut();
        f.calls[kind] += 1;
        f.ms[kind] += ms;
        (f.number, f.begin.is_some())
    });
    let write = |line: &mut String| {
        let _ = write!(line, "[lg/late-work] frame={number} kind={} ms={ms:.2}", KINDS[kind]);
        if let Some(b) = bytes { let _ = write!(line, " bytes={b}"); }
        if let Some(n) = name { let _ = write!(line, " name={n}"); }
    };
    // Outside play (before the first frame, or after the viewer loop) the line is written at once.
    if playing {
        OUT.with(|o| o.borrow_mut().push(write));
    } else {
        let mut line = String::new();
        write(&mut line);
        println!("{line}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn spike_log_is_off_by_default_and_counts_nothing() {
        assert_eq!(crate::render::vita_renderer::Config::default().spike_log_ms, 0.);
        assert!(!on());
        begin_frame();
        sim(Sim::Npcs);
        npc("ant", 3, [1.; 4]);
        late(AUDIO_CLIP, 5., Some("x.wav"), Some(4));
        FRAME.with(|f| {
            let f = f.borrow();
            assert!(f.begin.is_none() && f.block.is_none() && f.calls.iter().all(|&c| c == 0) && f.npc[0].is_none());
        });
    }
    #[test]
    fn buffered_lines_stay_whole_and_overflow_is_counted() {
        let mut out = Out::default();
        out.push(|t| t.push_str("[lg/spike] frame=1"));
        out.push(|t| t.push_str("[lg/late-work] frame=1 kind=audio_clip"));
        assert_eq!(out.text, "[lg/spike] frame=1\n[lg/late-work] frame=1 kind=audio_clip\n");
        assert_eq!((out.lines, out.dropped), (2, 0));
        assert_eq!(out.text.capacity(), OUT_CAP);
        let long = "x".repeat(1000);
        while out.dropped == 0 { out.push(|t| t.push_str(&long)); }
        assert!(out.text.len() + LINE_MAX > OUT_CAP && out.text.ends_with('\n'), "a dropped line leaves no fragment");
        out.push(|t| t.push_str(&"y".repeat(LINE_MAX + 1)));
        assert_eq!(out.dropped, 2, "an over-long line is dropped too");
        assert_eq!(out.text.capacity(), OUT_CAP, "the buffer never grows during play");
        assert_eq!(out.text.lines().count() as u32, out.lines);
    }
    #[test]
    fn log_field_reports_the_previous_line_cost() {
        let at = Instant::now();
        let t = |ms: u64| at + std::time::Duration::from_millis(ms);
        let f = Frame { number: 8, begin: Some(t(0)), log_ms: 0.126, ..Frame::default() };
        assert!(f.line(t(40), [t(0), t(12), t(30), t(31)])
            .starts_with("[lg/spike] frame=8 ms=40.00 sim=12.00 render=18.00 hud=1.00 present=9.00 log=0.13 sim_split="));
    }
    #[test]
    fn spike_line_splits_simulation_blocks_and_npc_kinds() {
        let at = Instant::now();
        let t = |ms: u64| at + std::time::Duration::from_millis(ms);
        let mut f = Frame { number: 7, begin: Some(t(0)), block: Some((Sim::Input as usize, t(0))), ..Frame::default() };
        f.close_block(t(2));
        f.block = Some((Sim::Npcs as usize, t(2)));
        f.close_block(t(12));
        f.npc[0] = Some(Npc { name: "ant", updated: 11, ms: [1., 2., 3., 4.] });
        let line = f.line(t(40), [t(0), t(12), t(30), t(31)]);
        assert_eq!(line, "[lg/spike] frame=7 ms=40.00 sim=12.00 render=18.00 hud=1.00 present=9.00 \
            sim_split=input:2.00,npcs:10.00 npc=ant:11/10.00/1.00/2.00/3.00/4.00");
        f.gxm_start = Some([0, 1000, 1]);
        f.gxm_mark = Some([70, 46000, 3]);
        f.end_us[Sim::Scene as usize] = 44_500;
        f.end_calls[Sim::Scene as usize] = 1;
        f.end_us[PRESENT] = 500;
        f.end_calls[PRESENT] = 1;
        assert!(f.line(t(40), [t(0), t(12), t(30), t(31)]).contains(" gxm_begin=0.07 gxm_end=45.00 sim_split=input:2.00,npcs:10.00 gxm_end_in=scene:44.50/1,present:0.50/1 npc="));
    }
}
