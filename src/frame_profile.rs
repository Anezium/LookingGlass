//! Optional local CPU profiling; inactive during ordinary play.
use std::{cell::RefCell, collections::BTreeMap, time::Instant};

#[cfg(not(target_os = "vita"))]
thread_local! { static SAMPLES: RefCell<BTreeMap<&'static str, Vec<f64>>> = RefCell::default(); }

fn enabled() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    #[cfg(target_os = "vita")]
    let check = || std::path::Path::new(vita::PROFILE_DIR).exists();
    #[cfg(not(target_os = "vita"))]
    let check = || std::env::var_os("LOOKING_GLASS_FRAME_PROFILE").is_some();
    *ON.get_or_init(check)
}

pub struct Span {
    label: &'static str,
    start: Option<Instant>,
    #[cfg(target_os = "vita")]
    category: Option<&'static str>,
}
pub fn span(label: &'static str) -> Span {
    Span {
        label,
        start: enabled().then(Instant::now),
        #[cfg(target_os = "vita")]
        category: None,
    }
}
impl Span {
    pub fn cancel(&mut self) {
        self.start = None;
    }
}
impl Drop for Span {
    fn drop(&mut self) {
        if let Some(start) = self.start {
            let ms = start.elapsed().as_secs_f64() * 1000.;
            #[cfg(target_os = "vita")]
            vita::record(self.label, self.category, ms);
            #[cfg(not(target_os = "vita"))]
            SAMPLES.with(|s| {
                let mut s = s.borrow_mut();
                let v = s.entry(self.label).or_default();
                if v.len() < 10000 {
                    v.push(ms);
                }
            });
        }
    }
}

#[cfg(not(target_os = "vita"))]
pub fn report(map: &str) -> anyhow::Result<()> {
    if !enabled() {
        return Ok(());
    }
    let report = SAMPLES.with(|s| s.borrow().iter().map(|(name, v)| {
        let mut samples = v.iter().skip(30).copied().collect::<Vec<_>>();
        samples.sort_by(f64::total_cmp);
        let n = samples.len();
        (*name, serde_json::json!({"samples":n,"median_ms":samples.get(n/2),"p95_ms":samples.get(n*95/100)}))
    }).collect::<BTreeMap<_, _>>());
    let dir = std::path::PathBuf::from(std::env::var_os("LOOKING_GLASS_FRAME_PROFILE").unwrap());
    std::fs::create_dir_all(&dir)?;
    std::fs::write(
        dir.join(format!("{map}.json")),
        serde_json::to_vec_pretty(&report)?,
    )?;
    Ok(())
}

#[cfg(target_os = "vita")]
pub use vita::{
    begin_frame, end_frame, mode, record_batch, record_culled, record_geometry, record_layer,
    record_world_submission, report, world_span, Mode,
};

#[cfg(target_os = "vita")]
mod vita {
    use super::*;
    pub const PROFILE_DIR: &str = "ux0:data/alice/private/frame_profile";
    const MODE_FILE: &str = "ux0:data/alice/private/profile_mode";
    const LABELS: &[&str] = &[
        "frame_cpu",
        "world_draw",
        "world_prep",
        "world_submit",
        "skinning",
        "npcs",
        "movers",
        "particles",
        "transparent_sort",
        "gl_flush",
        "save_preview",
        "world_prep_static",
        "world_prep_dynamic",
        "world_submit_static",
        "world_submit_dynamic",
    ];
    #[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
    pub enum Mode {
        #[default]
        Full,
        NoPreview,
        NoPrep,
        NoWorld,
        Clk444,
    }
    impl Mode {
        fn read() -> Self {
            match std::fs::read_to_string(MODE_FILE)
                .unwrap_or_default()
                .trim()
            {
                "nopreview" => Self::NoPreview,
                "noprep" => Self::NoPrep,
                "noworld" => Self::NoWorld,
                "clk444" => Self::Clk444,
                _ => Self::Full,
            }
        }
        pub fn name(self) -> &'static str {
            match self {
                Self::Full => "full",
                Self::NoPreview => "nopreview",
                Self::NoPrep => "noprep",
                Self::NoWorld => "noworld",
                Self::Clk444 => "clk444",
            }
        }
        pub fn throttle_preview(self) -> bool {
            self != Self::Full
        }
        pub fn skip_static_prep(self) -> bool {
            self == Self::NoPrep
        }
        pub fn no_world(self) -> bool {
            self == Self::NoWorld
        }
    }
    #[derive(Default, Clone, serde::Serialize)]
    struct WorldCounts {
        batches_drawn: usize,
        batches_culled: usize,
        triangles: usize,
        vertices_submitted: usize,
        layers_drawn: usize,
        static_batches: usize,
        dynamic_batches: usize,
        retransformed_batches: usize,
        static_vertices: usize,
        dynamic_vertices: usize,
    }
    impl WorldCounts {
        fn add(&mut self, other: &Self) {
            self.batches_drawn += other.batches_drawn;
            self.batches_culled += other.batches_culled;
            self.triangles += other.triangles;
            self.vertices_submitted += other.vertices_submitted;
            self.layers_drawn += other.layers_drawn;
            self.static_batches += other.static_batches;
            self.dynamic_batches += other.dynamic_batches;
            self.retransformed_batches += other.retransformed_batches;
            self.static_vertices += other.static_vertices;
            self.dynamic_vertices += other.dynamic_vertices;
        }
    }
    #[derive(Default)]
    struct LabelSamples {
        frames: Vec<f64>,
        invocations: Vec<f64>,
        calls: usize,
    }
    #[derive(Default)]
    struct State {
        map: String,
        mode: Mode,
        poll_frames: usize,
        frames: usize,
        recording: bool,
        clocks_raised: bool,
        pending: Vec<(&'static str, f64)>,
        pending_world: WorldCounts,
        labels: BTreeMap<&'static str, LabelSamples>,
        world: WorldCounts,
        window_world: WorldCounts,
        window_cpu_ms: f64,
        windows: Vec<serde_json::Value>,
    }
    thread_local! { static STATE: RefCell<State> = RefCell::default(); }

    #[link(name = "ScePower_stub")]
    extern "C" {
        fn scePowerSetArmClockFrequency(freq: i32) -> i32;
        fn scePowerSetBusClockFrequency(freq: i32) -> i32;
        fn scePowerSetGpuClockFrequency(freq: i32) -> i32;
        fn scePowerSetGpuXbarClockFrequency(freq: i32) -> i32;
        fn scePowerGetArmClockFrequency() -> i32;
        fn scePowerGetBusClockFrequency() -> i32;
        fn scePowerGetGpuClockFrequency() -> i32;
        fn scePowerGetGpuXbarClockFrequency() -> i32;
    }
    fn clocks() -> [i32; 4] {
        unsafe {
            [
                scePowerGetArmClockFrequency(),
                scePowerGetBusClockFrequency(),
                scePowerGetGpuClockFrequency(),
                scePowerGetGpuXbarClockFrequency(),
            ]
        }
    }
    pub fn mode() -> Mode {
        STATE.with(|s| s.borrow().mode)
    }
    pub fn world_span(label: &'static str, static_batch: bool) -> Span {
        let mut s = super::span(label);
        s.category = Some(match (label, static_batch) {
            ("world_prep", true) => "world_prep_static",
            ("world_prep", false) => "world_prep_dynamic",
            ("world_submit", true) => "world_submit_static",
            _ => "world_submit_dynamic",
        });
        s
    }
    pub(super) fn record(label: &'static str, category: Option<&'static str>, ms: f64) {
        STATE.with(|s| {
            let mut s = s.borrow_mut();
            if s.recording {
                s.pending.push((label, ms));
                if let Some(category) = category {
                    s.pending.push((category, ms));
                }
            }
        });
    }
    pub fn record_culled() {
        STATE.with(|s| {
            let mut s = s.borrow_mut();
            if s.recording {
                s.pending_world.batches_culled += 1;
            }
        });
    }
    pub fn record_batch(static_batch: bool, retransformed: bool) {
        STATE.with(|s| {
            let mut s = s.borrow_mut();
            if !s.recording {
                return;
            }
            s.pending_world.batches_drawn += 1;
            if static_batch {
                s.pending_world.static_batches += 1;
            } else {
                s.pending_world.dynamic_batches += 1;
            }
            s.pending_world.retransformed_batches += usize::from(retransformed);
        });
    }
    pub fn record_geometry(triangles: usize, vertices: usize, static_batch: bool) {
        STATE.with(|s| {
            let mut s = s.borrow_mut();
            if !s.recording {
                return;
            }
            s.pending_world.triangles += triangles;
            s.pending_world.vertices_submitted += vertices;
            if static_batch {
                s.pending_world.static_vertices += vertices;
            } else {
                s.pending_world.dynamic_vertices += vertices;
            }
        });
    }
    pub fn record_world_submission() {
        STATE.with(|s| {
            let mut s = s.borrow_mut();
            if s.recording {
                s.pending_world.layers_drawn += 1;
            }
        });
    }
    pub fn record_layer(triangles: usize, vertices: usize, static_batch: bool) {
        record_world_submission();
        record_geometry(triangles, vertices, static_batch);
    }
    pub fn begin_frame(map: &str) {
        STATE.with(|s| {
            let mut s = s.borrow_mut();
            let next_mode = if s.poll_frames % 60 == 0 {
                Mode::read()
            } else {
                s.mode
            };
            s.poll_frames += 1;
            let mode_changed = next_mode != s.mode;
            if s.map != map || mode_changed {
                s.map = map.to_owned();
                s.mode = next_mode;
                s.frames = 0;
                s.labels.clear();
                s.world = WorldCounts::default();
                s.windows.clear();
                s.window_world = WorldCounts::default();
                s.window_cpu_ms = 0.;

                if next_mode == Mode::Clk444 && mode_changed {
                    let rc = unsafe {
                        [
                            scePowerSetArmClockFrequency(444),
                            scePowerSetBusClockFrequency(222),
                            scePowerSetGpuClockFrequency(222),
                            scePowerSetGpuXbarClockFrequency(166),
                        ]
                    };
                    s.clocks_raised = true;
                    println!("[lg/profile] clocks_set rc={rc:?} actual={:?}", clocks());
                }
                println!(
                    "[lg/profile] mode={} map={} clocks={:?} clocks_raised={}",
                    s.mode.name(),
                    s.map,
                    clocks(),
                    s.clocks_raised
                );
            }
            s.pending.clear();
            s.pending_world = WorldCounts::default();
            s.recording = enabled();
        });
    }
    pub fn end_frame(map: &str, gameplay: bool) -> bool {
        STATE.with(|s| {
            let mut s = s.borrow_mut(); s.recording = false;
            if !enabled() || s.map != map || !gameplay {
                s.pending.clear(); s.pending_world = WorldCounts::default();
                return false;
            }
            let pending = std::mem::take(&mut s.pending);
            let mut totals = BTreeMap::<&'static str, f64>::new();
            for (label, ms) in pending {
                let v = s.labels.entry(label).or_default(); v.calls += 1;
                if v.invocations.len() < 10000 { v.invocations.push(ms); }
                *totals.entry(label).or_default() += ms;
            }
            for &label in LABELS {
                let value = *totals.get(label).unwrap_or(&0.);
                let v = s.labels.entry(label).or_default();
                if v.frames.len() < 10000 { v.frames.push(value); }
            }
            let counts = std::mem::take(&mut s.pending_world);
            s.world.add(&counts); s.window_world.add(&counts);
            s.window_cpu_ms += totals.get("frame_cpu").copied().unwrap_or(0.);
            s.frames += 1;
            if s.frames % 10 == 0 {
                let window = serde_json::json!({"mode":s.mode.name(), "map":s.map, "gameplay_frames_end":s.frames,
                    "frame_cpu_mean_ms":s.window_cpu_ms/10., "counts":s.window_world});
                println!("[lg/profile] window {}", window);
                s.windows.push(window); s.window_world=WorldCounts::default(); s.window_cpu_ms=0.;
            }
            s.frames % 60 == 0
        })
    }
    fn stats(v: &[f64]) -> serde_json::Value {
        let mut sorted = v.to_vec();
        sorted.sort_by(f64::total_cmp);
        let n = sorted.len();
        serde_json::json!({"samples":n,"median_ms":sorted.get(n/2),"p95_ms":sorted.get(n*95/100),
            "mean_ms": if n>0 {Some(v.iter().sum::<f64>()/n as f64)} else {None}, "total_ms":v.iter().sum::<f64>()})
    }
    pub fn report(map: &str) -> anyhow::Result<()> {
        if !enabled() {
            return Ok(());
        }
        let (name, value) = STATE.with(|s| {
            let s=s.borrow();
            let spans=s.labels.iter().map(|(label,v)| {
                (*label, serde_json::json!({"per_frame":stats(&v.frames),"calls":v.calls,"per_invocation":stats(&v.invocations)}))
            }).collect::<BTreeMap<_,_>>();
            (format!("{map}-{}-{}.json",s.mode.name(),s.frames), serde_json::json!({
                "map":map,"mode":s.mode.name(),"gameplay_frames":s.frames,"clocks_raised":s.clocks_raised,
                "quantiles":"sorted[n/2] and sorted[floor(n*0.95)] (zero-based)",
                "population":"cumulative accepted gameplay frames since map/mode entry; zero for labels absent in a frame; max 10000 stored samples per series",
                "spans":spans,"world_counts":s.world,"windows":s.windows,
            }))
        });
        std::fs::create_dir_all(PROFILE_DIR)?;
        let path = std::path::Path::new(PROFILE_DIR).join(name);
        std::fs::write(&path, serde_json::to_vec_pretty(&value)?)?;
        println!("[lg/profile] report {}", path.display());
        Ok(())
    }
}
