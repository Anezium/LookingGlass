//! Constant-space gameplay timings; CSV export once per completed 60-frame window.
#![cfg_attr(not(target_os = "vita"), allow(dead_code))]
use crate::frame_profile::Mode;
use std::{
    cell::RefCell,
    io::Write,
    sync::atomic::{AtomicBool, Ordering},
    time::Instant,
};
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    Light,
    Full,
}
pub(crate) fn resolve(explicit: Option<Level>, mode: Mode) -> Level {
    explicit.unwrap_or(if mode == Mode::Full {
        Level::Light
    } else {
        Level::Full
    })
}
static FULL: AtomicBool = AtomicBool::new(true);
pub(crate) fn set_mode(mode: Mode) {
    #[cfg(feature = "vita_renderer")]
    FULL.store(
        resolve(crate::render::vita_renderer::config().profiler, mode) == Level::Full,
        Ordering::Relaxed,
    );
    #[cfg(not(feature = "vita_renderer"))]
    let _ = mode;
}
pub(crate) fn full() -> bool {
    FULL.load(Ordering::Relaxed)
}
fn active() -> bool {
    cfg!(target_os = "vita") && !full()
}
const WINDOW: usize = 60;
#[derive(Clone, Copy, Default, Debug)]
struct Row {
    frame: u64,
    wall_us: u64,
    cpu_us: u64,
    swap_us: u64,
    skin_collect_wait_us: u64,
    skin_gather_us: u64,
    skin_fallbacks: u64,
    skin_overflow: u64,
    fx: [u64; 8],
    actor_publication: [u64; 4],
    render: [RenderMetrics; 6],
}
/// Engine submission calls, counted at their owning phase. Texture binds count
/// image slots passed to apply_bindings; backend cache hits are not GL calls.
#[derive(Clone, Copy, Default, Debug, serde::Serialize)]
pub(crate) struct RenderMetrics {
    pub cpu_ns: u64,
    pub draws: u64,
    pub pipelines: u64,
    pub depth_states: u64,
    pub bindings: u64,
    pub uniform_uploads: u64,
    pub uniform_bytes: u64,
    pub buffer_updates: u64,
    pub buffer_bytes: u64,
    pub texture_bind_slots: u64,
    pub visibility_tests: u64,
    pub uniform_gl_calls: u64,
    pub visibility_cpu_ns: u64,
    pub parameter_cpu_ns: u64,
    pub deferred_cpu_ns: u64,
    pub texture_gl_binds: u64,
    pub state_gl_calls: u64,
    pub program_gl_changes: u64,
}
pub(crate) const PHASE_NAMES: [&str; 6] = ["world_opaque", "world_alpha", "actors", "fx", "hud", "present"];
#[derive(Clone, Copy)]
pub(crate) enum RenderPhase { WorldOpaque, WorldAlpha, Actors, Fx, Hud, Present }
pub(crate) fn render_active() -> bool {
    if cfg!(target_os = "vita") { return active(); }
    static HOST: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *HOST.get_or_init(|| std::env::var_os("LOOKING_GLASS_RENDER_METRICS").is_some())
}
pub(crate) fn render_add(phase: RenderPhase, value: RenderMetrics) {
    if !render_active() { return; }
    RING.with(|r| {
        let mut r = r.borrow_mut();
        let dst = &mut r.current.render[phase as usize];
        macro_rules! add { ($($field:ident),*) => { $(dst.$field += value.$field;)* }; }
        add!(cpu_ns, draws, pipelines, depth_states, bindings, uniform_uploads,
             uniform_bytes, buffer_updates, buffer_bytes, texture_bind_slots,
             visibility_tests, uniform_gl_calls, visibility_cpu_ns, parameter_cpu_ns,
             deferred_cpu_ns, texture_gl_binds, state_gl_calls, program_gl_changes);
    });
}
pub(crate) fn deferred_cpu(ns:u64) {
    if active() {RING.with(|r| {let mut r=r.borrow_mut();if r.cpu_start.is_none() {r.current.cpu_us += ns / 1000;} });}
}
pub(crate) fn cpu_running() -> bool {RING.with(|r|r.borrow().cpu_start.is_some())}
#[cfg(not(target_os="vita"))]
pub(crate) fn render_host_deferred_ns() -> u64 {
    RING.with(|r|r.borrow().current.render.iter().map(|p|p.deferred_cpu_ns).sum())
}
pub(crate) struct RenderTimer(RenderPhase, Option<Instant>);
pub(crate) fn render_timer(phase: RenderPhase) -> RenderTimer {
    RenderTimer(phase, render_active().then(Instant::now))
}
impl Drop for RenderTimer {
    fn drop(&mut self) {
        if let Some(start) = self.1 {
            render_add(self.0, RenderMetrics { cpu_ns: start.elapsed().as_nanos() as u64, ..Default::default() });
        }
    }
}
#[cfg(not(target_os = "vita"))]
pub(crate) fn render_host_reset() { RING.with(|r| r.borrow_mut().current.render = Default::default()); }
#[cfg(not(target_os = "vita"))]
pub(crate) fn render_host_report() -> serde_json::Value {
    RING.with(|r| {
        let row = r.borrow();
        serde_json::Value::Object(PHASE_NAMES.into_iter().zip(row.current.render)
            .map(|(name, value)| (name.to_owned(), serde_json::to_value(value).unwrap())).collect())
    })
}
struct Ring {
    rows: [Row; WINDOW],
    count: usize,
    exported: usize,
    sequence: u64,
    start: Option<Instant>,
    cpu_start: Option<Instant>,
    swap_start: Option<Instant>,
    current: Row,
    pending: bool,
}
impl Default for Ring {
    fn default() -> Self {
        Self {
            rows: [Row::default(); WINDOW],
            count: 0,
            exported: 0,
            sequence: 0,
            start: None,
            cpu_start: None,
            swap_start: None,
            current: Row::default(),
            pending: false,
        }
    }
}
impl Ring {
    fn begin(&mut self, now: Instant) {
        if self.pending {
            self.current.wall_us = now.duration_since(self.start.unwrap()).as_micros() as u64;
            self.sequence += 1;
            self.current.frame = self.sequence;
            self.rows[self.count % WINDOW] = self.current;
            self.count += 1;
        }
        self.pending = false;
        self.start = Some(now);
        self.current = Row::default();
    }
    fn write(&self, out: &mut impl Write) -> std::io::Result<()> {
        write!(out, "frame,wall_us,frame_cpu_us,swap_us,skin_collect_wait_us,skin_gather_us,skin_fallbacks,skin_overflow,particle_producer_us,fx_merge_us,fx_upload_cpu_us,fx_upload_gl_us,fx_upload_bytes,fx_finish_prepare_us,fx_worker_wait_us,fx_finish_recycle_us")?;
        for phase in PHASE_NAMES {
            for field in ["cpu_us", "draws", "pipelines", "depth_states", "bindings", "uniform_uploads", "uniform_bytes", "buffer_updates", "buffer_bytes", "texture_bind_slots", "visibility_tests", "uniform_gl_calls", "visibility_cpu_us", "parameter_cpu_us", "deferred_cpu_us", "texture_gl_binds", "state_gl_calls", "program_gl_changes"] {
                write!(out, ",{phase}_{field}")?;
            }
        }
        writeln!(out,",actor_direct_surfaces,actor_mutable_consumer_surfaces,actor_publication_banks,actor_publication_capacity_bytes")?;
        let first = self.count.saturating_sub(WINDOW);
        for i in first..self.count {
            let r = self.rows[i % WINDOW];
            write!(out, "{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{}", r.frame, r.wall_us, r.cpu_us, r.swap_us,
                r.skin_collect_wait_us, r.skin_gather_us, r.skin_fallbacks, r.skin_overflow,
                r.fx[0],r.fx[1],r.fx[2],r.fx[3],r.fx[4],r.fx[5],r.fx[6],r.fx[7])?;
            for p in r.render {
                write!(out, ",{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{}", p.cpu_ns / 1000,
                    p.draws, p.pipelines, p.depth_states, p.bindings, p.uniform_uploads,
                    p.uniform_bytes, p.buffer_updates, p.buffer_bytes, p.texture_bind_slots,
                    p.visibility_tests, p.uniform_gl_calls, p.visibility_cpu_ns / 1000, p.parameter_cpu_ns / 1000, p.deferred_cpu_ns / 1000, p.texture_gl_binds, p.state_gl_calls, p.program_gl_changes)?;
            }
            writeln!(out,",{},{},{},{}",r.actor_publication[0],r.actor_publication[1],r.actor_publication[2],r.actor_publication[3])?;
        }
        Ok(())
    }
}
thread_local! { static RING: RefCell<Ring> = RefCell::default(); }
pub(crate) fn reset() {
    RING.with(|r| *r.borrow_mut() = Ring::default());
}
pub(crate) fn begin(map: &str, mode: Mode) {
    if !active() {
        return;
    }
    RING.with(|r| r.borrow_mut().begin(Instant::now()));
    if let Err(e) = export_window(map, mode) {
        eprintln!("[lg/light] export failed: {e}");
    }
}
// Surface activity accumulates across views; bank/capacity values are gauges.
fn record_actor_count(row:&mut Row,name:&str,value:u64) {
    match name {
        "actor_direct_surfaces" => row.actor_publication[0]+=value,
        "actor_mutable_consumer_surfaces" => row.actor_publication[1]+=value,
        "actor_publication_banks" => row.actor_publication[2]=value,
        "actor_publication_capacity_bytes" => row.actor_publication[3]=value,
        _ => {}
    }
}
pub(crate) fn actor_count(name:&str,value:u64) {
    if active() {RING.with(|r|record_actor_count(&mut r.borrow_mut().current,name,value));}
}
pub(crate) fn skin_collect_wait(us: u64) {
    if active() { RING.with(|r| r.borrow_mut().current.skin_collect_wait_us += us); }
}
pub(crate) fn skin_overlap(gather_us: u64, fallbacks: u64, overflow: bool) {
    if active() {
        RING.with(|r| {
            let mut r = r.borrow_mut();
            r.current.skin_gather_us += gather_us;
            r.current.skin_fallbacks += fallbacks;
            r.current.skin_overflow |= u64::from(overflow);
        });
    }
}
#[derive(Clone, Copy)]
pub(crate) enum FxMetric { Producer, Merge, UploadCpu, UploadGl, UploadBytes, FinishPrepare, WorkerWait, FinishRecycle }
pub(crate) fn fx_add(metric: FxMetric, value: u64) {
    if active() { RING.with(|r| r.borrow_mut().current.fx[metric as usize] += value); }
    #[cfg(all(feature="vita_renderer", not(target_os="vita")))]
    crate::render_fx::retained::count(match metric {
        FxMetric::Producer => "vita_fx_producer_us", FxMetric::Merge => "vita_fx_merge_us",
        FxMetric::UploadCpu => "vita_fx_upload_cpu_phase_us", FxMetric::UploadGl => "vita_fx_upload_gl_phase_us",
        FxMetric::UploadBytes => "vita_fx_upload_phase_bytes", FxMetric::FinishPrepare => "vita_fx_finish_prepare_us",
        FxMetric::WorkerWait => "vita_fx_worker_wait_us", FxMetric::FinishRecycle => "vita_fx_finish_recycle_us",
    }, value);
}
pub(crate) struct FxTimer(FxMetric, Instant);
pub(crate) fn fx_timer(metric: FxMetric) -> FxTimer { FxTimer(metric, Instant::now()) }
impl Drop for FxTimer { fn drop(&mut self) { fx_add(self.0, self.1.elapsed().as_micros() as u64); } }

pub(crate) fn cpu_begin() {
    if active() || crate::actor_prep_profile::enabled() {
        RING.with(|r| r.borrow_mut().cpu_start = Some(Instant::now()));
    }
}
pub(crate) fn cpu_end() {
    if active() || crate::actor_prep_profile::enabled() {
        // Split a pending deferred packet at the actual CPU boundary.
        #[cfg(feature="vita_renderer")] if crate::actor_prep_profile::enabled() { crate::render_tail::cpu_boundary(Instant::now()); }
        RING.with(|r| {
            let mut r = r.borrow_mut();
            r.current.cpu_us = r
                .cpu_start
                .take()
                .map_or(0, |t| t.elapsed().as_micros() as u64);
        });
    }
}
pub(crate) fn swap_begin() {
    if active() {
        RING.with(|r| r.borrow_mut().swap_start = Some(Instant::now()));
    }
}
pub(crate) fn swap_end() {
    if active() {
        RING.with(|r| {
            let mut r = r.borrow_mut();
            r.current.swap_us += r
                .swap_start
                .take()
                .map_or(0, |t| t.elapsed().as_micros() as u64);
        });
    }
}
pub(crate) fn end(measured: bool) {
    if active() {
        RING.with(|r| r.borrow_mut().pending = measured);
    }
}
pub(crate) fn export(map: &str) -> anyhow::Result<()> {
    #[cfg(target_os = "vita")]
    return export_window(map, crate::frame_profile::mode());
    #[cfg(not(target_os = "vita"))]
    {
        let _ = map;
        Ok(())
    }
}
fn export_window(map: &str, mode: Mode) -> anyhow::Result<()> {
    RING.with(|r| {
        let mut r = r.borrow_mut();
        if r.count == r.exported || r.count % WINDOW != 0 {
            return Ok(());
        }
        #[cfg(target_os = "vita")]
        let dir = std::path::Path::new("ux0:data/alice/private/frame_profile");
        #[cfg(not(target_os = "vita"))]
        let dir = std::path::Path::new("private/frame_profile");
        std::fs::create_dir_all(dir)?;
        let name = format!("{map}-{}-light-{}.csv", mode_name(mode), r.count);
        let mut out = std::io::BufWriter::new(std::fs::File::create(dir.join(&name))?);
        r.write(&mut out)?;
        out.flush()?;
        r.exported = r.count;
        println!(
            "[lg/light] window mode={} frames_end={} rows={} file={name}",
            mode_name(mode),
            r.count,
            r.count.min(WINDOW)
        );
        Ok(())
    })
}
fn mode_name(mode: Mode) -> &'static str {
    #[cfg(any(target_os = "vita", test))]
    {
        mode.name()
    }
    #[cfg(not(any(target_os = "vita", test)))]
    {
        let _ = mode;
        "full"
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn defaults_and_explicit_levels_cover_every_mode() {
        for mode in [
            Mode::Full,
            Mode::NoPreview,
            Mode::NoPrep,
            Mode::NoWorld,
            Mode::Clk444,
            Mode::Bench,
            Mode::RenderBench,
            Mode::FullBench,
            Mode::Route,
            Mode::RouteContinue,
        ] {
            assert_eq!(
                resolve(None, mode),
                if mode == Mode::Full {
                    Level::Light
                } else {
                    Level::Full
                }
            );
            for level in [Level::Light, Level::Full] {
                assert_eq!(resolve(Some(level), mode), level);
            }
        }
    }
    #[test]
    fn ring_csv_is_chronological_bounded_and_attributes_previous_wall() {
        let mut r = Ring::default();
        let origin = Instant::now();
        r.begin(origin);
        for i in 1..=125 {
            r.current.cpu_us = 7;
            r.current.swap_us = 2;
            r.current.skin_collect_wait_us = i;
            r.current.skin_gather_us = i * 3;
            r.current.skin_fallbacks = i % 3;
            r.current.skin_overflow = i % 2;
            record_actor_count(&mut r.current,"actor_direct_surfaces",2);
            record_actor_count(&mut r.current,"actor_direct_surfaces",3);
            record_actor_count(&mut r.current,"actor_mutable_consumer_surfaces",1);
            record_actor_count(&mut r.current,"actor_publication_banks",99);
            record_actor_count(&mut r.current,"actor_publication_banks",7);
            record_actor_count(&mut r.current,"actor_publication_capacity_bytes",4096);
            r.pending = true;
            r.begin(origin + std::time::Duration::from_micros(i * 10));
        }
        let mut out = Vec::new();
        r.write(&mut out).unwrap();
        let s = String::from_utf8(out).unwrap();
        assert_eq!(s.lines().count(), 61);
        assert!(s.lines().next().unwrap().contains("world_opaque_draws"));
        assert_eq!(s.lines().next().unwrap().split(',').count(), 128);
        assert!(s.lines().nth(1).unwrap().starts_with("66,10,7,2,66,198,0,0,0,0,0,0,0,0,0,0,"));
        assert!(s.lines().last().unwrap().starts_with("125,10,7,2,125,375,2,1,0,0,0,0,0,0,0,0,"));
        assert!(s.lines().skip(1).all(|line| line.split(',').count() == 128));
        assert!(s.lines().skip(1).all(|line|line.ends_with(",5,1,7,4096")));
        assert_eq!(r.current.actor_publication,[0;4]);
        assert_eq!(r.current.skin_gather_us, 0);
        assert_eq!(r.current.skin_fallbacks, 0);
        assert_eq!(r.current.skin_overflow, 0);
        r.begin(origin + std::time::Duration::from_micros(2000));
        assert_eq!(r.count, 125);
    }
}
