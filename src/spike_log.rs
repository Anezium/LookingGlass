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
//! Nothing here runs when the key is 0 beyond one config load per call site.
use std::{cell::RefCell, time::Instant};

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
}
#[derive(Clone, Copy)]
struct Npc { name: &'static str, updated: u64, ms: [f64; 4] }
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
    fn line(&self, now: Instant, [begin, render, hud, cpu_end]: [Instant; 4]) -> String {
        let ms = |a: Instant, b: Instant| b.saturating_duration_since(a).as_secs_f64() * 1000.;
        let mut line = format!("[lg/spike] frame={} ms={:.2} sim={:.2} render={:.2} hud={:.2} present={:.2}",
            self.number, ms(begin, now), ms(begin, render), ms(render, hud), ms(hud, cpu_end), ms(cpu_end, now));
        let gxm = self.gxm_start.zip(self.gxm_mark);
        if let Some((start, end)) = gxm {
            line.push_str(&format!(" gxm_begin={:.2} gxm_end={:.2}", f64::from(end[0].wrapping_sub(start[0])) / 1000.,
                f64::from(end[1].wrapping_sub(start[1])) / 1000.));
        }
        for (i, name) in KINDS.iter().enumerate() {
            if self.calls[i] > 0 { line.push_str(&format!(" {name}={}/{:.2}", self.calls[i], self.ms[i])); }
        }
        let split: Vec<String> = SIM.iter().zip(self.sim_ms).filter(|(_, ms)| *ms >= 0.05)
            .map(|(name, ms)| format!("{name}:{ms:.2}")).collect();
        line.push_str(&format!(" sim_split={}", if split.is_empty() { "-".into() } else { split.join(",") }));
        if gxm.is_some() {
            let places: Vec<String> = (0..REGIONS).filter(|&r| self.end_calls[r] > 0)
                .map(|r| format!("{}:{:.2}/{}", if r < SIM.len() { SIM[r] } else { REGION_NAMES[r - SIM.len()] },
                    f64::from(self.end_us[r]) / 1000., self.end_calls[r]))
                .collect();
            line.push_str(&format!(" gxm_end_in={}", if places.is_empty() { "-".into() } else { places.join(",") }));
        }
        let npcs: Vec<String> = self.npc.iter().flatten().filter(|n| n.ms.iter().sum::<f64>() >= 0.05)
            .map(|n| format!("{}:{}/{:.2}/{:.2}/{:.2}/{:.2}/{:.2}", n.name, n.updated, n.ms.iter().sum::<f64>(),
                n.ms[0], n.ms[1], n.ms[2], n.ms[3]))
            .collect();
        if !npcs.is_empty() { line.push_str(&format!(" npc={}", npcs.join(","))); }
        for (name, (calls, ms, expanded, tests)) in PLANNERS.iter().zip(self.planners) {
            if calls > 0 { line.push_str(&format!(" {name}={calls}/{ms:.2}/{expanded}/{tests}")); }
        }
        line
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
        if let (Some(begin), Some(render), Some(hud), Some(cpu_end)) = (f.begin, f.render, f.hud, f.cpu_end) {
            if now.saturating_duration_since(begin).as_secs_f64() * 1000. >= f64::from(limit) {
                println!("{}", f.line(now, [begin, render, hud, cpu_end]));
            }
        }
        let number = f.number + 1;
        let mark = f.gxm_mark;
        *f = Frame { number, begin: Some(now), block: Some((Sim::Input as usize, now)), gxm_start: mark,
            gxm_mark: mark, ..Frame::default() };
    });
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
        sim(Sim::Npcs);
        npc("ant", 3, [1.; 4]);
        late(AUDIO_CLIP, 5., Some("x.wav"), Some(4));
        FRAME.with(|f| {
            let f = f.borrow();
            assert!(f.begin.is_none() && f.block.is_none() && f.calls.iter().all(|&c| c == 0) && f.npc[0].is_none());
        });
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
