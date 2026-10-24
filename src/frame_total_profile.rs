//! Frame-boundary-only instrumentation for a controlled native A/B build.
#![allow(dead_code)]
use crate::frame_profile::Mode;
use std::{io::Write, sync::Mutex, time::Instant};
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    Off,
    Light,
    Full,
}
pub(crate) fn resolve(explicit: Option<Level>, mode: Mode) -> Level {
    explicit.unwrap_or(if mode == Mode::Full {
        Level::Off
    } else {
        Level::Full
    })
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

pub(crate) fn full() -> bool { false }
pub(crate) fn set_mode(_: Mode) {}
pub(crate) fn capture_enabled() -> bool {
    crate::render::vita_renderer::config().profile_capture
}
pub(crate) fn render_active() -> bool { false }
pub(crate) fn render_add(_: RenderPhase, _: RenderMetrics) {}
pub(crate) fn deferred_cpu(_: u64) {}
pub(crate) fn cpu_running() -> bool { false }
pub(crate) fn cpu_begin() {}
pub(crate) fn cpu_end() {}
pub(crate) fn swap_begin() {}
pub(crate) fn swap_end() {}
pub(crate) fn skin_collect_wait(_: u64) {}
pub(crate) fn skin_overlap(_: u64, _: u64, _: bool) {}
pub(crate) fn door_counts(_: u64, _: u64, _: u64) {}
pub(crate) struct RenderTimer;
pub(crate) fn render_timer(_: RenderPhase) -> RenderTimer { RenderTimer }
#[derive(Clone, Copy)]
pub(crate) enum FxMetric { Producer, Merge, UploadCpu, UploadGl, UploadBytes, FinishPrepare, WorkerWait, FinishRecycle }
pub(crate) struct FxTimer;
pub(crate) fn fx_timer(_: FxMetric) -> FxTimer { FxTimer }
pub(crate) fn fx_add(_: FxMetric, _: u64) {}
#[cfg(not(target_os="vita"))]
pub(crate) fn render_host_reset() {}
#[cfg(not(target_os="vita"))]
pub(crate) fn render_host_report() -> serde_json::Value { serde_json::Value::Null }
#[cfg(not(target_os="vita"))]
pub(crate) fn render_host_deferred_ns() -> u64 { 0 }

// Metric-only clocks at existing render call sites become zero-cost values.
// Never use this type for simulation, waiting, deadlines or audio scheduling.
#[derive(Clone, Copy)]
pub(crate) struct InactiveInstant;
impl InactiveInstant {
    pub(crate) const fn now() -> Self { Self }
    pub(crate) const fn elapsed(self) -> std::time::Duration { std::time::Duration::ZERO }
}

const CAPACITY: usize = 120;
struct Ring {
    wall_us: [u64; CAPACITY],
    count: usize,
    start: Option<Instant>,
    pending: bool,
    armed: bool,
    overflow: bool,
}
impl Default for Ring {
    fn default() -> Self { Self { wall_us: [0; CAPACITY], count: 0,
        start: None, pending: false, armed: false, overflow: false } }
}
impl Ring {
    // Pause and level-transition reports in full mode have no bench window.
    // Keep this decision outside target cfgs so host tests exercise the exact
    // eligibility and validation path used by Vita export before any file I/O.
    fn report_due(&self, mode: Mode) -> anyhow::Result<bool> {
        if !self.armed || !mode.is_bench() { return Ok(false); }
        anyhow::ensure!(self.count == CAPACITY && !self.overflow, "incomplete frame-total capture");
        anyhow::ensure!(self.wall_us.iter().all(|v| *v > 0), "invalid wall interval");
        Ok(true)
    }
    // Attribute the whole begin-to-begin interval to the previous accepted frame.
    // In particular, the final report-only begin closes frame 120 before export.
    #[inline(never)]
    #[export_name="alice_frame_total_sample"]
    fn begin(&mut self, now: Instant) {
        if self.pending {
            if self.count < CAPACITY {
                self.wall_us[self.count] = now.duration_since(self.start.unwrap()).as_micros() as u64;
                self.count += 1;
            } else { self.overflow = true; }
        }
        self.pending = false;
        self.start = Some(now);
    }
    fn write(&self, out: &mut impl Write) -> std::io::Result<()> {
        write!(out, "frame,wall_us,frame_cpu_us,swap_us,skin_collect_wait_us,skin_gather_us,skin_fallbacks,skin_overflow,particle_producer_us,fx_merge_us,fx_upload_cpu_us,fx_upload_gl_us,fx_upload_bytes,fx_finish_prepare_us,fx_worker_wait_us,fx_finish_recycle_us")?;
        for phase in PHASE_NAMES {
            for field in ["cpu_us", "draws", "pipelines", "depth_states", "bindings", "uniform_uploads", "uniform_bytes", "buffer_updates", "buffer_bytes", "texture_bind_slots", "visibility_tests", "uniform_gl_calls", "visibility_cpu_us", "parameter_cpu_us", "deferred_cpu_us", "texture_gl_binds", "state_gl_calls", "program_gl_changes"] {
                write!(out, ",{phase}_{field}")?;
            }
        }
        writeln!(out, ",gl_observers")?;
        for (i, wall) in self.wall_us[..self.count].iter().enumerate() {
            write!(out, "{},{}", i+1, wall)?;
            for _ in 2..124 { write!(out, ",")?; }
            writeln!(out, ",direct")?;
        }
        Ok(())
    }
}
// Only begin/end/reset/export touch this fixed buffer. Worker metric callbacks
// above are empty. The lock is uncontended and costs the same in both builds.
// Exported, aligned storage also prevents LLVM global merging from changing
// the timer's access sequence according to unrelated renderer Config layouts.
#[repr(align(64))]
struct Storage(Mutex<Ring>);
#[export_name="alice_frame_total_ring"]
static RING: Storage = Storage(Mutex::new(Ring {
    wall_us: [0; CAPACITY], count: 0, start: None, pending: false,
    armed: false, overflow: false,
}));
pub(crate) fn reset() { *RING.0.lock().unwrap() = Ring::default(); crate::phase_census::reset(); }
#[inline(never)]
#[export_name="alice_frame_total_begin"]
pub(crate) fn begin(_: &str, mode: Mode) {
    // Mode::read already requires profile_capture; avoid per-frame Config
    // layout access here. Only explicitly armed benchmark modes are sampled.
    if !mode.is_bench() { return; }
    let mut r=RING.0.lock().unwrap(); r.armed=true; r.begin(Instant::now());
    crate::phase_census::begin_configured();
}
pub(crate) fn end(measured: bool) {
    let mut r=RING.0.lock().unwrap(); r.pending=r.armed && measured;
}
pub(crate) fn export(map: &str) -> anyhow::Result<()> {
    #[cfg(target_os="vita")]
    {
        use std::path::Path;
        if !capture_enabled() { return Ok(()); }
        let r=RING.0.lock().unwrap();
        if !r.report_due(crate::frame_profile::mode())? { return Ok(()); }
            let dir=Path::new("ux0:data/alice/private/frame_profile");
            anyhow::ensure!(dir.is_dir(), "existing frame_profile directory required");
            let stem=format!("{map}-{}-light-{}", crate::frame_profile::mode().name(), r.count);
            let mut out=std::io::BufWriter::new(std::fs::File::create(dir.join(format!("{stem}.csv")))?);
            r.write(&mut out)?; out.flush()?;
            let mut doors=std::io::BufWriter::new(std::fs::File::create(dir.join(format!("{stem}-doors.csv")))?);
            writeln!(doors,"frame_first,frame_last,frames,hatter_door_world_traces,hatter_door_own_traces,hatter_door_trace_pairs_deferred")?;
            writeln!(doors,"1,60,60,,,")?; writeln!(doors,"61,120,60,,,")?; doors.flush()?;
            crate::phase_census::export(map)?;
            println!("[lg/frame-total] measured=120 gl_hooks=0 phase_timers=0 clock=begin-to-begin");
            Ok(())
    }
    #[cfg(not(target_os="vita"))]
    { let _=map; Ok(()) }
}

#[cfg(test)]
mod frame_total_tests {
    use super::*;
    #[test]
    fn full_mode_pause_and_transition_reports_are_noops() {
        let mut r=Ring::default();
        // Actual pre-arm full-mode state: no captured frames, no interval.
        assert_eq!(r.count, 0);
        assert!(!r.report_due(Mode::Full).unwrap());
        assert!(!r.report_due(Mode::FullBench).unwrap());
        // A mode change away from an armed bench also must not export.
        r.armed=true;
        assert!(!r.report_due(Mode::Full).unwrap());
        assert!(r.report_due(Mode::FullBench).is_err());
        r.count=CAPACITY; r.wall_us=[200; CAPACITY];
        assert!(r.report_due(Mode::FullBench).unwrap());
        r.overflow=true;
        assert!(r.report_due(Mode::FullBench).is_err());
        r.overflow=false; r.wall_us[119]=0;
        assert!(r.report_due(Mode::FullBench).is_err());
    }
    #[test]
    fn measured_intervals_exclude_warmup_and_close_final_frame() {
        let origin=Instant::now(); let mut r=Ring::default();
        r.begin(origin);
        for i in 1..=60 { r.pending=false; r.begin(origin+std::time::Duration::from_micros(i*100)); }
        assert_eq!(r.count,0);
        for i in 1..=120 { r.pending=true; r.begin(origin+std::time::Duration::from_micros(6000+i*200)); }
        assert_eq!(r.count,120); assert_eq!(r.wall_us,[200;120]);
        let mut output=Vec::new(); r.write(&mut output).unwrap();
        let output=String::from_utf8(output).unwrap();
        assert_eq!(output.lines().count(),121);
        for row in output.lines().skip(1) {
            let cells:Vec<_>=row.split(',').collect();
            assert_eq!(cells.len(),125); assert_eq!(cells[1],"200");
            assert!(cells[2..124].iter().all(|v|v.is_empty())); assert_eq!(cells[124],"direct");
        }
        r.pending=false; r.begin(origin+std::time::Duration::from_secs(1));
        assert_eq!(r.count,120); assert!(!r.overflow);
        r.pending=true; r.begin(origin+std::time::Duration::from_secs(2)); assert!(r.overflow);
    }
}
