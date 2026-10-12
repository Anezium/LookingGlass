//! Optional local CPU profiling; inactive during ordinary play.
use std::{cell::RefCell, collections::BTreeMap, time::Instant};

// Count LG-owned creation calls, including Macroquad material compilation.
// Framework startup shaders are outside the viewer frame window.
#[derive(Default)]
struct Creations { current: [u64; 2], #[cfg(not(target_os="vita"))] frames: Vec<[u64; 2]> }
thread_local! { static CREATIONS: RefCell<Creations> = RefCell::default(); }
pub(crate) fn shader_creation() {
    CREATIONS.with(|c| c.borrow_mut().current[0] += 1);
    #[cfg(target_os="vita")]
    counts(&[("shader_creations",1)]);
}
pub(crate) fn pipeline_creation() {
    CREATIONS.with(|c| c.borrow_mut().current[1] += 1);
    #[cfg(target_os="vita")]
    counts(&[("pipeline_creations",1)]);
}
pub(crate) fn begin_creation_frame() { CREATIONS.with(|c| c.borrow_mut().current=[0;2]); }
#[cfg(not(target_os="vita"))]
pub(crate) fn end_creation_frame() {
    CREATIONS.with(|c| { let mut c=c.borrow_mut(); let current=c.current; c.frames.push(current); });
}
#[cfg(not(target_os="vita"))]
pub(crate) fn creation_report() -> serde_json::Value {
    CREATIONS.with(|c| { let c=c.borrow(); let measured=c.frames.get(60..).unwrap_or_default();
        serde_json::json!({"warmup_frames":60,"frames":c.frames,"measured_frames":measured.len(),
          "shader_creations_after_warmup":measured.iter().map(|v|v[0]).sum::<u64>(),
          "pipeline_creations_after_warmup":measured.iter().map(|v|v[1]).sum::<u64>()}) })
}
#[cfg(feature="vita_renderer")]
pub(crate) fn current_creations() -> [u64;2] { CREATIONS.with(|c| c.borrow().current) }
pub(crate) fn load_material(shader: macroquad::miniquad::ShaderSource<'_>, params: macroquad::material::MaterialParams) -> Result<macroquad::material::Material, macroquad::Error> {
    shader_creation(); pipeline_creation();
    macroquad::material::load_material(shader,params)
}

#[cfg(any(target_os = "vita", test))]
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub enum Mode {
    #[default]
    Full,
    NoPreview,
    NoPrep,
    NoWorld,
    Clk444,
    Bench,
    RenderBench,
    FullBench,
    Route,
    RouteContinue,
}
#[cfg(any(target_os = "vita", test))]
impl Mode {
    #[cfg(target_os = "vita")]
    fn read() -> Self {
        Self::parse(&std::fs::read_to_string("ux0:data/alice/private/profile_mode").unwrap_or_default())
    }
    fn parse(value: &str) -> Self {
        match value.trim() {
            "nopreview" => Self::NoPreview,
            "noprep" => Self::NoPrep,
            "noworld" => Self::NoWorld,
            "clk444" => Self::Clk444,
            "bench" => Self::Bench,
            "renderbench" => Self::RenderBench,
            "fullbench" => Self::FullBench,
            "route" => Self::Route,
            "route-continue" => Self::RouteContinue,
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
            Self::Bench => "bench",
            Self::RenderBench => "renderbench",
            Self::FullBench => "fullbench",
            Self::Route => "route",
            Self::RouteContinue => "route-continue",
        }
    }
    pub fn is_bench(self) -> bool {
        matches!(self, Self::Bench | Self::RenderBench | Self::FullBench)
    }
    pub fn warmup_frames(self) -> usize {
        if matches!(self, Self::RenderBench | Self::FullBench) { 60 } else { 0 }
    }
    pub fn throttle_preview(self) -> bool {
        self != Self::Full
    }
    pub fn skip_static_prep(self) -> bool {
        self == Self::NoPrep
    }
    pub fn no_world(self) -> bool {
        matches!(self, Self::NoWorld | Self::Bench)
    }
}
// Advance once per viewer frame, independently of wall-clock duration.
#[cfg(any(target_os = "vita", test))]
pub struct PreviewThrottle {
    frames_since_capture: u32,
}
#[cfg(any(target_os = "vita", test))]
impl Default for PreviewThrottle {
    fn default() -> Self {
        Self {
            frames_since_capture: 120,
        }
    }
}
#[cfg(any(target_os = "vita", test))]
impl PreviewThrottle {
    pub fn advance_frame(&mut self) {
        self.frames_since_capture = (self.frames_since_capture + 1).min(120);
    }
    pub fn capture(&mut self, mode: Mode) -> bool {
        if mode.is_bench() || (mode.throttle_preview() && self.frames_since_capture < 120) {
            return false;
        }
        self.frames_since_capture = 0;
        true
    }
}

#[cfg(any(target_os = "vita", test))]
pub fn saves_writable(saves_enabled: bool, bench_session: bool) -> bool {
    saves_enabled && !bench_session
}

#[cfg(any(target_os = "vita", test))]
fn wall_sample(start: Option<Instant>, measured: bool, now: Instant) -> Option<f64> {
    measured.then(|| start.map(|start| now.duration_since(start).as_secs_f64() * 1000.)).flatten()
}

#[cfg(not(target_os = "vita"))]
thread_local! { static SAMPLES: RefCell<BTreeMap<&'static str, Vec<f64>>> = RefCell::default(); }

fn enabled() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    #[cfg(target_os = "vita")]
    let check = || std::path::Path::new(vita::PROFILE_DIR).exists() && !matches!(Mode::read(),Mode::Route|Mode::RouteContinue);
    #[cfg(not(target_os = "vita"))]
    let check = || std::env::var_os("LOOKING_GLASS_FRAME_PROFILE").is_some();
    *ON.get_or_init(check)
}

pub(crate) fn heap_used_bytes()->Option<u64> {
    #[cfg(target_os="vita")] {Some(vita::heap_used())}
    #[cfg(not(target_os="vita"))] {None}
}

pub struct Span {
    label: &'static str,
    start: Option<Instant>,
    #[cfg(target_os = "vita")]
    category: Option<&'static str>,
    #[cfg(target_os = "vita")]
    token: Option<u64>,
}
pub fn span(label: &'static str) -> Span {
    Span {
        label,
        start: enabled().then(Instant::now),
        #[cfg(target_os = "vita")]
        category: None,
        #[cfg(target_os = "vita")]
        token: vita::enter(label),
    }
}
impl Span {
    pub fn cancel(&mut self) {
        #[cfg(target_os = "vita")]
        vita::cancel(self.token.take());
        self.start = None;
    }
}
impl Drop for Span {
    fn drop(&mut self) {
        if let Some(start) = self.start {
            let ms = start.elapsed().as_secs_f64() * 1000.;
            #[cfg(target_os = "vita")]
            vita::record(self.label, self.category, self.token, ms);
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
    add_counter, begin_frame, bench_accepted_frames, bench_frames, simulation_frozen, counter_owner, counts, end_frame, flush_boundary, mode,
    record_batch, record_culled, record_geometry, record_layer, record_lg_submission, record_model,
    record_world_submission, report, set_owner, startup_bench, world_span, Counter, Owner,
};

#[cfg(target_os = "vita")]
mod vita {
    use super::*;
    pub const PROFILE_DIR: &str = "ux0:data/alice/private/frame_profile";
    use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU32, Ordering::Relaxed};

    // Hot paths only touch fixed integer atomics. Names and maps are used once
    // per accepted frame, never per sweep/hull/puff.
    #[derive(Clone, Copy)]
    #[repr(u8)]
    pub enum Counter {
        SweepCalls,
        SweepExceptCalls,
        CameraSweepCalls,
        SweepGeometryCalls,
        LedgeTraceCalls,
        ColliderTraceCalls,
        DynamicCollidersTested,
        StaticHullsTested,
        StaticHullsClipped,
        ColliderHullsTested,
        ColliderHullsClipped,
        SettledHullsClipped,
        PuffsBefore,
        PuffsAfter,
        PuffsSweeps,
        PuffsConstrained,
        PuffsFrozen,
        PuffsStationarySweeps,
        PuffsImpacts,
        PuffsDeaths,
        PuffsBirths,
    }
    const COUNTER_NAMES: &[&str] = &[
        "sweep_calls",
        "sweep_except_calls",
        "camera_sweep_calls",
        "sweep_geometry_calls",
        "ledge_trace_calls",
        "collider_trace_calls",
        "dynamic_colliders_tested",
        "static_hulls_tested",
        "static_hulls_clipped",
        "collider_hulls_tested",
        "collider_hulls_clipped",
        "settled_hulls_clipped",
        "puffs_before",
        "puffs_after",
        "puffs_sweeps",
        "puffs_constrained",
        "puffs_frozen",
        "puffs_stationary_sweeps",
        "puffs_impacts",
        "puffs_deaths",
        "puffs_births",
    ];
    #[derive(Clone, Copy)]
    #[repr(u8)]
    pub enum Owner {
        None,
        Movers,
        Particles,
        Movement,
        Camera,
        Scene,
        Interactions,
        Combat,
        Alice,
        InteractionSearch,
        Hud,
    }
    const OWNER_NAMES: &[&str] = &[
        "",
        "movers",
        "particles",
        "frame_movement",
        "frame_camera",
        "frame_scene",
        "frame_interactions",
        "frame_combat",
        "frame_alice",
        "frame_interaction_search",
        "frame_hud",
    ];
    // The viewer thread owns profiling, including begin/end_frame and owners.
    // Relaxed loads/stores avoid newlib TLS calls and atomic RMW loops; these
    // counters are not a cross-thread aggregation API.
    static HOT_RECORDING: AtomicBool = AtomicBool::new(false);
    static OWNER: AtomicU8 = AtomicU8::new(0);
    static HOT_COUNTS: [[AtomicU32; COUNTER_NAMES.len() + 1]; OWNER_NAMES.len()] =
        [const { [const { AtomicU32::new(0) }; COUNTER_NAMES.len() + 1] }; OWNER_NAMES.len()];
    pub fn set_owner(owner: Owner) {
        OWNER.store(owner as u8, Relaxed);
    }
    pub struct CounterOwner(u8);
    pub fn counter_owner(owner: Owner) -> CounterOwner {
        let previous = OWNER.load(Relaxed);
        OWNER.store(owner as u8, Relaxed);
        CounterOwner(previous)
    }
    impl Drop for CounterOwner {
        fn drop(&mut self) {
            OWNER.store(self.0, Relaxed);
        }
    }
    #[inline]
    pub fn add_counter(counter: Counter, value: u32) {
        if !HOT_RECORDING.load(Relaxed) {
            return;
        }
        let owner = OWNER.load(Relaxed) as usize;
        let cell = &HOT_COUNTS[0][counter as usize];
        cell.store(cell.load(Relaxed) + value, Relaxed);
        let seen = &HOT_COUNTS[0][COUNTER_NAMES.len()];
        seen.store(seen.load(Relaxed) | (1 << counter as u8), Relaxed);
        if owner != 0 {
            let cell = &HOT_COUNTS[owner][counter as usize];
            cell.store(cell.load(Relaxed) + value, Relaxed);
            let seen = &HOT_COUNTS[owner][COUNTER_NAMES.len()];
            seen.store(seen.load(Relaxed) | (1 << counter as u8), Relaxed);
        }
    }
    fn reset_hot_counts() {
        for row in &HOT_COUNTS {
            for cell in row {
                cell.store(0, Relaxed);
            }
        }
        set_owner(Owner::None);
    }

    const LABELS: &[&str] = &[
        "frame_cpu",
        "world_visibility",
        "world_visibility_bounds",
        "world_visibility_pvs",
        "world_visibility_indices",
        "interaction_search_after",
        "lights_build",
        "interaction_doors",
        "transparent_keys",
        "transparent_order",
        "transparent_pack",
        "transparent_submit",
        "transparent_compact",
        "save_grab_screen",
        "movers_duchess",
        "movers_pandemonium",
        "movers_pool",
        "movers_beyond",
        "movers_fortress",
        "movers_village",
        "movers_school",
        "movers_gym",
        "movers_school2",
        "village_fixed",
        "movers_levels",
        "movers_presentation",
        "village_cinema",
        "village_sample",
        "village_collider_build",
        "village_ground_probe",
        "village_dynamic_publish",
        "village_rider_contact",
        "village_final_publish",
        "particles_integrate",
        "particles_sweep",
        "particles_liquid",
        "particles_retain",
        "particles_emit",
        "collider_vertices",
        "collider_edges",
        "collider_transform",
        "collider_extract",
        "fixed_clock",
        "flush_atlas",
        "flush_depth_begin",
        "flush_depth_end",
        "flush_clear_depth",
        "flush_fx_begin",
        "flush_fx_finish",
        "flush_fx_check",
        "flush_skin_appearance",
        "flush_save_preview",
        "frame_input",
        "frame_interaction_search",
        "frame_interactions",
        "frame_movers",
        "frame_movement",
        "frame_story",
        "frame_particles",
        "frame_combat",
        "frame_alice",
        "frame_audio",
        "frame_camera",
        "frame_audio_mix",
        "frame_scene",
        "frame_world",
        "frame_characters",
        "frame_objects",
        "frame_world_particles",
        "frame_effects",
        "frame_first_person",
        "frame_capture",
        "frame_hud",
        "frame_save",
        "steam_animate",
        "steam_sync",
        "steam_collected",
        "village_particle_bindings",
        "lights_select",
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
        exclusive: Vec<f64>,
        frame_calls: Vec<f64>,
        invocation_total_ms: f64,
    }
    #[derive(Default)]
    struct CounterSamples {
        frames: Vec<u64>,
        total: u64,
    }
    #[derive(Default)]
    struct State {
        map: String,
        mode: Mode,
        poll_frames: usize,
        frames: usize,
        accepted_frames: usize,
        frame_start: Option<Instant>,
        wall_pending: bool,
        frame_wall_ms: Vec<f64>,
        heap_uordblks: Vec<u64>,
        heap_before: Option<u64>,
        recording: bool,
        clocks_raised: bool,
        pending: Vec<(&'static str, f64, f64)>,
        stack: Vec<(u64, &'static str, f64)>,
        next_token: u64,
        since_flush: u64,
        hierarchy_errors: usize,
        pending_counts: BTreeMap<(&'static str, &'static str), u64>,
        counters: BTreeMap<(&'static str, &'static str), CounterSamples>,
        pending_models: BTreeMap<usize, u64>,
        models: BTreeMap<usize, CounterSamples>,
        pending_world: WorldCounts,
        labels: BTreeMap<&'static str, LabelSamples>,
        world: WorldCounts,
        window_world: WorldCounts,
        window_cpu_ms: f64,
        windows: Vec<serde_json::Value>,
    }
    thread_local! { static STATE: RefCell<State> = RefCell::default(); }

    // ABI from VitaSDK malloc.h: ten size_t fields, 32-bit on Vita.
    #[repr(C)]
    struct MallInfo { arena:usize, ordblks:usize, smblks:usize, hblks:usize, hblkhd:usize, usmblks:usize, fsmblks:usize, uordblks:usize, fordblks:usize, keepcost:usize }
    extern "C" { fn mallinfo() -> MallInfo; }
    pub(super) fn heap_used()->u64 { unsafe { mallinfo().uordblks as u64 } }
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
    // Startup happens before the viewer's first begin_frame/poll.
    pub fn startup_bench() -> bool {
        Mode::read().is_bench() && enabled()
    }
    pub fn bench_frames() -> Option<usize> {
        STATE.with(|s| {
            let s = s.borrow();
            (enabled() && s.mode.is_bench()).then_some(s.frames)
        })
    }
    pub fn bench_accepted_frames() -> Option<usize> {
        STATE.with(|s| {
            let s = s.borrow();
            (enabled() && s.mode.is_bench()).then_some(s.accepted_frames)
        })
    }
    pub fn simulation_frozen() -> bool {
        STATE.with(|s| {
            let s = s.borrow();
            enabled() && s.mode == Mode::RenderBench && s.accepted_frames >= s.mode.warmup_frames()
        })
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
    // The stack tracks actual scoped nesting. Categories duplicate statistics,
    // but never become stack children. Tokens distinguish discarded/menu frames.
    pub(super) fn enter(label: &'static str) -> Option<u64> {
        STATE.with(|s| {
            let mut s = s.borrow_mut();
            if !s.recording {
                return None;
            }
            s.next_token += 1;
            let token = s.next_token;
            s.stack.push((token, label, 0.));
            Some(token)
        })
    }
    pub(super) fn cancel(token: Option<u64>) {
        STATE.with(|s| {
            let mut s = s.borrow_mut();
            if let Some(token) = token {
                if let Some(i) = s.stack.iter().position(|x| x.0 == token) {
                    s.stack.remove(i);
                }
            }
        });
    }
    pub(super) fn record(
        label: &'static str,
        category: Option<&'static str>,
        token: Option<u64>,
        ms: f64,
    ) {
        STATE.with(|s| {
            let mut s = s.borrow_mut();
            if !s.recording {
                return;
            }
            let Some(token) = token else {
                return;
            };
            let Some(i) = s.stack.iter().position(|x| x.0 == token) else {
                return;
            };
            if i + 1 != s.stack.len() {
                s.hierarchy_errors += 1;
            }
            let (_, _, children) = s.stack.remove(i);
            if let Some(parent) = s.stack.last_mut() {
                parent.2 += ms;
            }
            let exclusive = ms - children;
            s.pending.push((label, ms, exclusive));
            if let Some(category) = category {
                s.pending.push((category, ms, exclusive));
            }
        });
    }
    // Call once after local integer loops. No per-plan or per-triplet timing.
    pub fn counts(values: &[(&'static str, u64)]) {
        if !HOT_RECORDING.load(Relaxed) { return; }
        STATE.with(|s| {
            let mut s = s.borrow_mut();
            if !s.recording {
                return;
            }
            let owner = s.stack.iter().rev().find_map(|(_, label, _)| match *label {
                "movers"
                | "particles"
                | "frame_movement"
                | "frame_camera"
                | "frame_scene"
                | "frame_interactions"
                | "frame_combat"
                | "frame_alice"
                | "frame_interaction_search"
                | "frame_hud" => Some(*label),
                _ => None,
            });
            for &(name, value) in values {
                *s.pending_counts.entry(("", name)).or_default() += value;
                if let Some(owner) = owner {
                    *s.pending_counts.entry((owner, name)).or_default() += value;
                }
            }
        });
    }
    pub fn record_model(model: usize) {
        STATE.with(|s| {
            let mut s = s.borrow_mut();
            if s.recording {
                *s.pending_models.entry(model).or_default() += 1;
            }
        });
    }
    pub fn record_lg_submission() {
        STATE.with(|s| {
            let mut s = s.borrow_mut();
            if s.recording {
                s.since_flush += 1;
                *s.pending_counts
                    .entry(("", "lg_3d_submissions"))
                    .or_default() += 1;
            }
        });
    }
    pub fn flush_boundary(site: &'static str) {
        STATE.with(|s| {
            let mut s = s.borrow_mut();
            if s.recording {
                let n = s.since_flush;
                s.since_flush = 0;
                *s.pending_counts
                    .entry((site, "lg_3d_submissions_since_flush"))
                    .or_default() += n;
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
        let now = Instant::now();
        super::begin_creation_frame();
        reset_hot_counts();
        HOT_RECORDING.store(enabled(), Relaxed);
        STATE.with(|s| {
            let mut s = s.borrow_mut();
            let next_mode = if s.poll_frames % 60 == 0 {
                Mode::read()
            } else {
                s.mode
            };
            s.poll_frames += 1;
            // Attribute begin-to-begin wall time to the PREVIOUS measured frame.
            // This includes macroquad submission, swap and next_frame, and never
            // substitutes the last warmup frame for the first measured frame.
            if let Some(ms) = wall_sample(s.frame_start, s.wall_pending, now) {
                if s.frame_wall_ms.len() < 10000 { s.frame_wall_ms.push(ms); }
            }
            if s.wall_pending && s.heap_uordblks.len()<10000 { s.heap_uordblks.push(heap_used()); }
            s.wall_pending = false;
            s.frame_start = Some(now);
            let mode_changed = next_mode != s.mode;
            if s.map != map || mode_changed {
                s.map = map.to_owned();
                s.mode = next_mode;
                s.frames = 0;
                s.accepted_frames = 0;
                s.frame_wall_ms.clear();
                s.heap_uordblks.clear(); s.heap_before=None;
                s.labels.clear();
                s.counters.clear();
                s.models.clear();
                s.hierarchy_errors = 0;
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
            s.pending_counts.clear();
            s.pending_models.clear();
            s.stack.clear();
            s.since_flush = 0;
            s.pending_world = WorldCounts::default();
            s.recording = enabled();
            if s.recording && s.accepted_frames>0 && s.accepted_frames==s.mode.warmup_frames() && s.heap_before.is_none() { s.heap_before=Some(heap_used()); }
        });
    }
    pub fn end_frame(map: &str, gameplay: bool) -> bool {
        counts(&[("shader_creations",0),("pipeline_creations",0)]);
        HOT_RECORDING.store(false, Relaxed);
        STATE.with(|s| {
            let mut s = s.borrow_mut(); s.recording = false;
            if !enabled() || s.map != map || !gameplay {
                s.pending.clear(); s.pending_world = WorldCounts::default();
                return false;
            }
            s.accepted_frames += 1;
            if s.accepted_frames <= s.mode.warmup_frames() {
                s.pending.clear();
                s.pending_counts.clear();
                s.pending_models.clear();
                s.pending_world = WorldCounts::default();
                return false;
            }
            if s.heap_before.is_none() { s.heap_before=Some(heap_used()); }
            s.wall_pending = true;
            let pending = std::mem::take(&mut s.pending);
            let mut totals = BTreeMap::<&'static str, f64>::new();
            let mut exclusive_totals = BTreeMap::<&'static str, f64>::new();
            let mut calls = BTreeMap::<&'static str, f64>::new();
            for (label, ms, exclusive) in pending {
                let previous=s.frames.min(10000);
                let v = s.labels.entry(label).or_insert_with(|| LabelSamples { frames:vec![0.;previous], exclusive:vec![0.;previous], frame_calls:vec![0.;previous], ..Default::default() }); v.calls += 1;
                if v.invocations.len() < 1024 { v.invocations.push(ms); }
                v.invocation_total_ms += ms;
                *exclusive_totals.entry(label).or_default() += exclusive;
                *calls.entry(label).or_default() += 1.;
                *totals.entry(label).or_default() += ms;
            }
            for &label in LABELS { s.labels.entry(label).or_default(); }
            for (label, v) in &mut s.labels {
                let value = *totals.get(label).unwrap_or(&0.);
                if v.frames.len() < 10000 {
                    v.frames.push(value);
                    v.exclusive.push(*exclusive_totals.get(label).unwrap_or(&0.));
                    v.frame_calls.push(*calls.get(label).unwrap_or(&0.));
                }
            }
            for (owner, row) in OWNER_NAMES.iter().zip(&HOT_COUNTS) {
                let seen = row[COUNTER_NAMES.len()].load(Relaxed);
                row[COUNTER_NAMES.len()].store(0, Relaxed);
                for (index, (name, cell)) in COUNTER_NAMES.iter().zip(row).enumerate() {
                    let value = cell.load(Relaxed) as u64;
                    cell.store(0, Relaxed);
                    // Preserve zero-valued counters as well as late-series backfill.
                    if seen & (1 << index) != 0 {
                        *s.pending_counts.entry((owner, name)).or_default() += value;
                    }
                }
            }
            let pending_counts = std::mem::take(&mut s.pending_counts);
            let previous = s.frames.min(10000);
            for (name, value) in &pending_counts {
                let v = s.counters.entry(name.clone()).or_insert_with(|| CounterSamples { frames: vec![0; previous], total: 0 });
                v.total += value;
            }
            for (name, v) in &mut s.counters {
                if v.frames.len() < 10000 { v.frames.push(*pending_counts.get(name).unwrap_or(&0)); }
            }
            let pending_models = std::mem::take(&mut s.pending_models);
            for (model, value) in &pending_models {
                let v = s.models.entry(*model).or_insert_with(|| CounterSamples { frames: vec![0; previous], total: 0 }); v.total += value;
            }
            for (model,v) in &mut s.models { if v.frames.len() < 10000 { v.frames.push(*pending_models.get(model).unwrap_or(&0)); } }
            let counts = std::mem::take(&mut s.pending_world);
            s.world.add(&counts); s.window_world.add(&counts);
            s.window_cpu_ms += totals.get("frame_cpu").copied().unwrap_or(0.);
            s.frames += 1;
            if s.frames % 10 == 0 {
                let window = serde_json::json!({"mode":s.mode.name(), "map":s.map, "gameplay_frames_end":s.frames,
                    "frame_cpu_mean_ms":s.window_cpu_ms/10., "heap_uordblks_bytes":heap_used(), "counts":s.window_world});
                println!("[lg/profile] window {}", window);
                s.windows.push(window); s.window_world=WorldCounts::default(); s.window_cpu_ms=0.;
            }
            if s.mode.warmup_frames() > 0 { s.frames == 120 } else { s.frames % 60 == 0 }
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
        let (name, mut value) = STATE.with(|s| {
            let s=s.borrow();
            let spans=s.labels.iter().map(|(label,v)| {
                (*label, serde_json::json!({"per_frame":stats(&v.frames),"exclusive_per_frame":stats(&v.exclusive),"calls_per_frame":stats(&v.frame_calls),"calls":v.calls,"per_invocation":stats(&v.invocations),"all_invocations_total_ms":v.invocation_total_ms,"frame_values_ms":v.frames,"exclusive_frame_values_ms":v.exclusive,"frame_calls":v.frame_calls}))
            }).collect::<BTreeMap<_,_>>();
            let counters = s.counters.iter().map(|(name, v)| {
                let mut sorted = v.frames.clone(); sorted.sort_unstable(); let n = sorted.len();
                (if name.0.is_empty() { name.1.to_owned() } else { format!("{}/{}",name.0,name.1) }, serde_json::json!({"total":v.total,"samples":n,"median":sorted.get(n/2),"p95":sorted.get(n*95/100),"mean":if n>0 {Some(v.frames.iter().sum::<u64>() as f64/n as f64)} else {None},"frame_values":v.frames}))
            }).collect::<BTreeMap<_,_>>();
            let model_rebuilds = s.models.iter().map(|(model,v)| {
                let mut sorted = v.frames.clone(); sorted.sort_unstable(); let n = sorted.len();
                (*model, serde_json::json!({"total":v.total,"median":sorted.get(n/2),"p95":sorted.get(n*95/100),"frame_values":v.frames}))
            }).collect::<BTreeMap<_,_>>();
            let storage_bytes: usize = s.labels.values().map(|v| 8 * (v.frames.capacity() + v.exclusive.capacity() + v.frame_calls.capacity() + v.invocations.capacity())).sum::<usize>() + s.counters.values().map(|v| 8*v.frames.capacity()).sum::<usize>() + s.models.values().map(|v|8*v.frames.capacity()).sum::<usize>() + s.pending.capacity()*std::mem::size_of::<(&str,f64,f64)>() + 8*s.frame_wall_ms.capacity();
            (format!("{map}-{}-{}.json",s.mode.name(),s.frames), serde_json::json!({
                "map":map,"mode":s.mode.name(),"gameplay_frames":s.frames,"clocks_raised":s.clocks_raised,
                "warmup_frames":s.mode.warmup_frames(),
                "frame_wall_ms":{"frame_values_ms":s.frame_wall_ms,"per_frame":stats(&s.frame_wall_ms)},
                "heap":{"source":"newlib mallinfo.uordblks", "before_measured_bytes":s.heap_before,"end_measured_bytes":s.heap_uordblks.last(),
                  "delta_bytes":s.heap_before.zip(s.heap_uordblks.last().copied()).map(|(a,b)|b as i64-a as i64),
                  "frame_end_bytes":s.heap_uordblks,"includes_profile_allocations":true},
                "quantiles":"sorted[n/2] and sorted[floor(n*0.95)] (zero-based)",
                "population":"cumulative measured accepted gameplay frames since map/mode entry, after warmup_frames; zero for labels absent in a frame; max 10000 frame samples per series; first 1024 invocation samples per label; all invocation totals/calls retained",
                "spans":spans,"counters":counters,"model_rebuilds":model_rebuilds,"hierarchy_errors":s.hierarchy_errors,"profile_vector_capacity_bytes":storage_bytes,"world_counts":s.world,"windows":s.windows,
            }))
        });
        #[cfg(feature = "vita_renderer")]
        { value["vita_renderer"] = crate::render::vita_renderer::report(); }
        std::fs::create_dir_all(PROFILE_DIR)?;
        let path = std::path::Path::new(PROFILE_DIR).join(name);
        std::fs::write(&path, serde_json::to_vec_pretty(&value)?)?;
        println!("[lg/profile] report {}", path.display());
        Ok(())
    }
}

#[cfg(test)]
mod bench_policy_tests {
    use super::*;
    #[test]
    fn benchmark_modes_parse_and_keep_world_visible() {
        for (name, mode) in [("renderbench", Mode::RenderBench), ("fullbench", Mode::FullBench)] {
            assert_eq!(Mode::parse(&format!(" {name}\n")), mode);
            assert_eq!(mode.name(), name);
            assert!(mode.is_bench());
            assert!(!mode.no_world());
            assert!(!mode.skip_static_prep());
            assert_eq!(mode.warmup_frames(), 60);
        }
    }
    #[test]
    fn historical_mode_policies_remain_unchanged() {
        for (name, mode) in [("full",Mode::Full),("nopreview",Mode::NoPreview),("noprep",Mode::NoPrep),
            ("noworld",Mode::NoWorld),("clk444",Mode::Clk444),("bench",Mode::Bench)] {
            assert_eq!(Mode::parse(name),mode);
            assert_eq!(mode.name(),name);
            assert_eq!(mode.no_world(),matches!(mode,Mode::NoWorld|Mode::Bench));
            assert_eq!(mode.skip_static_prep(),mode==Mode::NoPrep);
            assert_eq!(mode.warmup_frames(),0);
            assert_eq!(mode.throttle_preview(),mode!=Mode::Full);
        }
        assert_eq!(Mode::parse("unknown"),Mode::Full);
        assert_eq!(Mode::parse(""),Mode::Full);
    }
    #[test]
    fn benchmark_sessions_never_capture_or_write_saves() {
        for mode in [Mode::Bench,Mode::RenderBench,Mode::FullBench] {
            let mut throttle=PreviewThrottle::default();
            for _ in 0..361 {
                throttle.advance_frame();
                assert!(!throttle.capture(mode));
                assert!(!saves_writable(true,mode.is_bench()));
            }
        }
        assert!(saves_writable(true,false));
        assert!(!saves_writable(false,false));
        assert!(!saves_writable(false,true));
    }
    #[test]
    fn wall_intervals_belong_to_previous_measured_frame() {
        use std::time::Duration;
        let start=Instant::now();
        let measured_start=start+Duration::from_millis(600);
        let after_measured=measured_start+Duration::from_millis(25);
        assert_eq!(wall_sample(None,false,start),None);
        assert_eq!(wall_sample(Some(start),false,measured_start),None); // warmup/rejection
        assert_eq!(wall_sample(Some(measured_start),true,after_measured),Some(25.));
        let last_start=after_measured+Duration::from_millis(10);
        assert_eq!(wall_sample(Some(after_measured),false,last_start),None);
        assert_eq!(wall_sample(Some(last_start),true,last_start+Duration::from_millis(40)),Some(40.));
    }
    #[test]
    fn historical_preview_throttles_remain_unchanged() {
        for mode in [Mode::Full,Mode::NoPreview,Mode::NoPrep,Mode::NoWorld,Mode::Clk444] {
            let mut throttle=PreviewThrottle::default();
            let captures=(0..361).filter(|_| {throttle.advance_frame();throttle.capture(mode)}).collect::<Vec<_>>();
            if mode==Mode::Full {assert_eq!(captures.len(),361);}
            else {assert_eq!(captures,vec![0,120,240,360]);}
        }
    }
}
