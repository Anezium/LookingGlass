//! Opt-in live survey. With no manifest this module does not change game policy.
use crate::{assets::Assets, Options};
use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    cell::RefCell,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
    time::{Instant, SystemTime, UNIX_EPOCH},
};
static ACTIVE: AtomicBool = AtomicBool::new(false);
pub fn active() -> bool {
    ACTIVE.load(Ordering::Relaxed)
}
pub fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn identifier(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 64
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}
fn relative(s: &str) -> bool {
    !s.is_empty()
        && Path::new(s)
            .components()
            .all(|p| matches!(p, std::path::Component::Normal(_)))
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub version: u32,
    pub run_id: String,
    pub repeats: u32,
    pub overall_timeout_s: u64,
    pub seed_policy: String,
    #[serde(default)]
    pub debug_fixtures: bool,
    pub scenes: Vec<Scene>,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Checkpoint {
    pub path: String,
    pub sha256: String,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct View {
    pub feet: [f32; 3],
    pub yaw: f32,
    pub pitch: f32,
    pub third_person: bool,
}
#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Expected {
    #[serde(default)]
    pub guards: Option<u64>,
    #[serde(default)]
    pub min_attacks: u64,
    #[serde(default)]
    pub min_shots: u64,
    #[serde(default)]
    pub min_impacts: u64,
    #[serde(default)]
    pub min_deaths: u64,
    #[serde(default)]
    pub min_dismemberment: u64,
    #[serde(default)]
    pub min_clusters: usize,
    #[serde(default)]
    pub min_level_active: u64,
    #[serde(default)]
    pub min_level_species: u64,
    #[serde(default)]
    pub min_motion: f32,
    #[serde(default)]
    pub destination: Option<String>,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Scene {
    pub id: String,
    pub family: String,
    pub map: String,
    pub entry: Option<String>,
    pub checkpoint: Option<Checkpoint>,
    pub view: Option<View>,
    pub warmup_s: f64,
    pub timed_s: f64,
    #[serde(default = "one_epoch")]
    pub epochs: u32,
    pub timeout_s: u64,
    #[serde(default)]
    pub school_clubs: Vec<[f32; 4]>,
    #[serde(default)]
    pub queen_fixture: Option<String>,
    #[serde(default)]
    pub tape: Vec<Sample>,
    #[serde(default)]
    pub expected: Expected,
    #[serde(default)]
    pub uncovered: Option<String>,
}
#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Sample {
    pub tick: u64,
    pub movement: [f32; 2],
    pub look: [f32; 2],
    pub jump: bool,
    pub run: bool,
    pub primary: bool,
    pub alternate: bool,
    pub select: Option<usize>,
    pub use_action: bool,
    pub skip: bool,
}
fn one_epoch() -> u32 {
    1
}
impl Sample {
    pub fn held(&self, name: &str) -> bool {
        match name {
            "Space" => self.jump,
            "Shift" => self.run,
            "Mouse 1" => self.primary,
            "Mouse 2" => self.alternate,
            "E" => self.use_action,
            "Enter" => self.skip,
            _ => self
                .select
                .is_some_and(|i| name == ["1", "2", "3", "4", "5", "6", "7", "8", "9", "0"][i]),
        }
    }
}
#[derive(Clone)]
pub struct Cell {
    pub scene: Scene,
    pub output: PathBuf,
    pub prefix: String,
}
thread_local! {
    static CELL: RefCell<Option<Cell>> = const { RefCell::new(None) };
    static METRICS: RefCell<Metrics> = RefCell::new(Metrics::default());
}
#[derive(Default)]
struct Metrics {
    phase: Option<(usize, Instant)>,
    spans: [u64; 5],
    counters: [u64; 32],
}
pub fn school_clubs() -> Option<Vec<[f32; 4]>> {
    if !active() {
        return None;
    }
    CELL.with(|c| {
        c.borrow()
            .as_ref()
            .filter(|c| !c.scene.school_clubs.is_empty())
            .map(|c| c.scene.school_clubs.clone())
    })
}
pub fn phase(next: Option<usize>) {
    if !active() {
        return;
    }
    let now = Instant::now();
    METRICS.with(|m| {
        let mut m = m.borrow_mut();
        if let Some((p, start)) = m.phase.take() {
            m.spans[p] += now.duration_since(start).as_micros() as u64;
        }
        m.phase = next.map(|p| (p, now));
    });
}
pub fn set(index: usize, n: u64) {
    if active() {
        METRICS.with(|m| m.borrow_mut().counters[index] = n);
    }
}
pub fn add(index: usize, n: u64) {
    if active() {
        METRICS.with(|m| m.borrow_mut().counters[index] += n);
    }
}
pub fn milestone(scope: &str, name: &str) {
    if !active() {
        return;
    }
    CELL.with(|c| { if let Some(c)=c.borrow().as_ref() {
        let row=serde_json::json!({"scope":scope,"milestone":name,"memory":memory(),"unix_ms":SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis()});
        if let Err(e)=append(&c.output.join(format!("{}-milestones.jsonl",c.prefix)),&row) { eprintln!("[lg/coverage] milestone error {e:#}"); }
    }});
}
fn append(path: &Path, row: &serde_json::Value) -> Result<()> {
    use std::io::Write;
    let mut out = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    serde_json::to_writer(&mut out, row)?;
    writeln!(out)?;
    Ok(())
}
#[cfg(target_os = "vita")]
pub fn memory() -> serde_json::Value {
    #[repr(C)]
    struct MallInfo {
        arena: usize,
        ordblks: usize,
        smblks: usize,
        hblks: usize,
        hblkhd: usize,
        usmblks: usize,
        fsmblks: usize,
        uordblks: usize,
        fordblks: usize,
        keepcost: usize,
    }
    extern "C" {
        fn mallinfo() -> MallInfo;
        fn vglMemFree(pool: i32) -> usize;
    }
    let m = unsafe { mallinfo() };
    serde_json::json!({"source":"newlib/vitaGL","used":m.uordblks,"committed":m.arena,"free_arena":m.fordblks,"arena_high_water":m.usmblks,
        "vgl_ram_free":unsafe{vglMemFree(1)},"vgl_vram_free":unsafe{vglMemFree(0)},"largest_contiguous":null,"allocation_failures":null})
}
#[cfg(not(target_os = "vita"))]
pub fn memory() -> serde_json::Value {
    serde_json::json!({"source":"host proc status, not Vita pools","proc_status":std::fs::read_to_string("/proc/self/status").ok().and_then(|s|s.lines().find(|l|l.starts_with("VmRSS:")).map(str::to_owned))})
}
impl Manifest {
    pub fn open(path: Option<&Path>) -> Result<Option<(Self, PathBuf)>> {
        #[cfg(target_os = "vita")]
        let default = Path::new("ux0:data/alice/private/coverage_survey.json");
        #[cfg(not(target_os = "vita"))]
        let default = Path::new("private/coverage_survey.json");
        // Hosts opt in explicitly; the Vita bubble cannot rely on argv.
        #[cfg(not(target_os = "vita"))]
        if path.is_none() {
            return Ok(None);
        }
        let path = path.unwrap_or(default);
        if !path.exists() && path == default {
            return Ok(None);
        }
        let bytes =
            std::fs::read(path).with_context(|| format!("coverage manifest {}", path.display()))?;
        ensure!(
            bytes.len() <= 4 * 1024 * 1024,
            "coverage manifest too large"
        );
        let m: Self = serde_json::from_slice(&bytes)?;
        m.validate()?;
        Ok(Some((m, path.to_path_buf())))
    }
    fn validate(&self) -> Result<()> {
        ensure!(
            self.version == 1 && identifier(&self.run_id),
            "invalid coverage version/run ID"
        );
        ensure!(
            (1..=3).contains(&self.repeats) && (1..=7200).contains(&self.overall_timeout_s),
            "invalid repeats/overall bound"
        );
        ensure!(
            self.seed_policy == "restore-checkpoint-or-authored-load",
            "unsupported seed policy"
        );
        ensure!(
            !self.scenes.is_empty() && self.scenes.len() <= 128,
            "invalid scene count"
        );
        let mut names = std::collections::BTreeSet::new();
        for s in &self.scenes {
            ensure!(
                matches!(
                    s.family.as_str(),
                    "V0" | "C1" | "C4" | "F4" | "W" | "E" | "B" | "T"
                ),
                "unknown scene family"
            );
            ensure!(
                s.uncovered
                    .as_ref()
                    .is_none_or(|r| !r.trim().is_empty() && r.len() <= 2048),
                "uncovered scene needs a bounded reason"
            );
            ensure!(
                (1..=30).contains(&s.epochs)
                    && (s.epochs == 1 || (!s.school_clubs.is_empty() && s.checkpoint.is_none())),
                "epochs require bounded school debug fixture"
            );
            ensure!(
                identifier(&s.id) && names.insert(&s.id) && identifier(&s.map),
                "invalid/duplicate scene ID/map"
            );
            ensure!(
                s.entry.as_ref().is_none_or(|e| identifier(e)),
                "invalid entry"
            );
            ensure!(
                s.warmup_s.is_finite()
                    && s.timed_s.is_finite()
                    && s.warmup_s >= 0.
                    && s.timed_s > 0.
                    && s.warmup_s + s.timed_s <= 600.,
                "invalid duration"
            );
            ensure!(
                (1..=300).contains(&s.timeout_s),
                "cell watchdog must be 1..300 seconds"
            );
            if let Some(v) = &s.view {
                ensure!(
                    v.feet
                        .iter()
                        .chain([&v.yaw, &v.pitch])
                        .all(|f| f.is_finite())
                        && v.pitch.abs() < 1.5,
                    "invalid view"
                );
            }
            if let Some(c) = &s.checkpoint {
                ensure!(
                    relative(&c.path)
                        && c.sha256.len() == 64
                        && c.sha256.bytes().all(|b| b.is_ascii_hexdigit()),
                    "invalid checkpoint path/hash"
                );
            }
            ensure!(
                s.school_clubs.is_empty()
                    || (self.debug_fixtures
                        && s.map == "skool1"
                        && s.entry.is_none()
                        && matches!(s.school_clubs.len(), 1 | 4)),
                "school-clubs is a manifest-only debug fixture, N=1/4"
            );
            ensure!(
                s.school_clubs.iter().flatten().all(|f| f.is_finite()),
                "invalid guard positions"
            );
            ensure!(
                s.queen_fixture.is_none()
                    || (self.debug_fixtures
                        && s.map == "qlair"
                        && matches!(
                            s.queen_fixture.as_deref(),
                            Some(
                                "queen2-hatter"
                                    | "queen2-scream"
                                    | "intro"
                                    | "birth"
                                    | "death-explosions"
                                    | "death-burst"
                            )
                        )),
                "invalid Queen fixture"
            );
            ensure!(s.tape.len() <= 100_000, "oversized tape");
            let mut last = None;
            for t in &s.tape {
                ensure!(
                    last.is_none_or(|n| t.tick > n)
                        && t.tick <= 72000
                        && t.select.is_none_or(|n| n < 10)
                        && t.movement
                            .iter()
                            .chain(&t.look)
                            .all(|n| n.is_finite() && n.abs() <= 1.),
                    "invalid tick tape"
                );
                last = Some(t.tick);
            }
            ensure!(
                s.tape.first().is_none_or(|t| t.tick == 0),
                "tape must begin at tick zero"
            );
        }
        Ok(())
    }
}
#[cfg(feature = "vita_renderer")]
pub async fn run(
    assets: Assets,
    options: Options,
    manifest: Manifest,
    path: PathBuf,
) -> Result<()> {
    let cfg = crate::render::vita_renderer::config();
    ensure!(
        !cfg.worker_probe && !cfg.frame_phase_probe && !cfg.render_draw_timing,
        "coverage requires probes/per-draw timing off"
    );
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let root = path
        .parent()
        .unwrap_or(Path::new("."))
        .join("coverage-results")
        .join(format!("{}-{stamp}", manifest.run_id));
    std::fs::create_dir_all(root.parent().context("survey output parent")?)?;
    std::fs::create_dir(&root)?;
    std::fs::write(
        root.join("effective-manifest.json"),
        serde_json::to_vec_pretty(&manifest)?,
    )?;
    std::fs::write(
        root.join("identity.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "manifest_sha256":hash(&std::fs::read(&path)?),"renderer":cfg,"simulation":crate::vita_sim::effective(),
            "host_timings_are_acceptance":false,"clock_policy":"see startup effective clock log","resolution":[macroquad::prelude::screen_width(),macroquad::prelude::screen_height()],
            "seed_policy":manifest.seed_policy,"checkpoints_are_read_only":true
        }))?,
    )?;
    let start = Instant::now();
    let mut results = Vec::new();
    ACTIVE.store(true, Ordering::Relaxed);
    let mut first = Some(assets);
    let stopped = std::sync::Arc::new(AtomicBool::new(false));
    let deadline = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(300));
    let watchdog_stopped = stopped.clone();
    let watchdog_deadline = deadline.clone();
    let watchdog_root = root.clone();
    let overall = manifest.overall_timeout_s;
    let watchdog = std::thread::spawn(move || {
        while !watchdog_stopped.load(Ordering::Relaxed) {
            if start.elapsed().as_secs() >= overall
                || start.elapsed().as_secs() >= watchdog_deadline.load(Ordering::Relaxed)
            {
                let row = serde_json::json!({"status":"failed","reason":"coverage load/cell/overall watchdog","elapsed_s":start.elapsed().as_secs_f64()});
                let _ =
                    std::fs::write(watchdog_root.join("watchdog-failure.json"), row.to_string());
                eprintln!("[lg/coverage] watchdog failure {row}");
                std::process::exit(1);
            }
            std::thread::sleep(std::time::Duration::from_secs(1));
        }
    });
    for s in &manifest.scenes {
        for repeat in 1..=manifest.repeats {
            let prefix = format!("{}-r{repeat}", s.id);
            deadline.store(start.elapsed().as_secs() + s.timeout_s, Ordering::Relaxed);
            let result = if let Some(reason) = &s.uncovered {
                serde_json::json!({"scene":s.id,"repeat":repeat,"status":"uncovered","reason":reason})
            } else if start.elapsed().as_secs() >= manifest.overall_timeout_s {
                serde_json::json!({"scene":s.id,"repeat":repeat,"status":"failed","reason":"overall watchdog"})
            } else {
                let mut o = options.clone();
                o.map = s.map.clone();
                o.entry = s.entry.clone();
                o.direct_map = true;
                o.new_game = false;
                o.frames = None;
                o.load = None;
                o.fly = false;
                o.capture = None;
                o.start_at = s
                    .view
                    .as_ref()
                    .map(|v| macroquad::prelude::Vec3::from_array(v.feet));
                o.combat_preview = !s.school_clubs.is_empty();
                o.weapon_preview = o.combat_preview;
                o.save_dir = root.join(format!("{prefix}-saves"));
                let cell = Cell {
                    scene: s.clone(),
                    output: root.clone(),
                    prefix: prefix.clone(),
                };
                o.coverage_cell = Some(cell.clone());
                CELL.with(|c| *c.borrow_mut() = Some(cell));
                METRICS.with(|m| *m.borrow_mut() = Metrics::default());
                let prepared = (|| -> Result<()> {
                    std::fs::create_dir_all(&o.save_dir)?;
                    if let Some(c) = &s.checkpoint {
                        let bytes =
                            std::fs::read(path.parent().unwrap_or(Path::new(".")).join(&c.path))?;
                        ensure!(hash(&bytes) == c.sha256, "checkpoint SHA-256 mismatch");
                        std::fs::write(o.save_dir.join("slot1.json"), bytes)?;
                        o.load = Some(crate::save::Slot::parse("slot1")?);
                    }
                    Ok(())
                })();
                let cell_start = Instant::now();
                milestone("cell", "before-load");
                let result = match prepared {
                    Err(e) => Err(e),
                    Ok(()) => {
                        let a = match first.take() {
                            Some(a) => Ok(a),
                            None => Assets::open(&o.data),
                        };
                        match a {
                            Ok(a) => crate::viewer::run(a, o).await,
                            Err(e) => Err(e),
                        }
                    }
                };
                let _ = crate::light_profile::survey_flush();
                match result {
                    Ok(()) => {
                        serde_json::json!({"scene":s.id,"repeat":repeat,"status":"complete","elapsed_s":cell_start.elapsed().as_secs_f64()})
                    }
                    Err(e) => {
                        serde_json::json!({"scene":s.id,"repeat":repeat,"status":"failed","reason":format!("{e:#}"),"elapsed_s":cell_start.elapsed().as_secs_f64()})
                    }
                }
            };
            println!("[lg/coverage] cell {}", result);
            std::fs::write(
                root.join(format!("{prefix}-result.json")),
                serde_json::to_vec_pretty(&result)?,
            )?;
            results.push(result);
            std::fs::write(
                root.join("results.json"),
                serde_json::to_vec_pretty(&results)?,
            )?;
            crate::light_profile::reset();
        }
    }
    stopped.store(true, Ordering::Relaxed);
    let _ = watchdog.join();
    CELL.with(|c| *c.borrow_mut() = None);
    ACTIVE.store(false, Ordering::Relaxed);
    println!(
        "[lg/coverage] done output={} elapsed_s={:.3}",
        root.display(),
        start.elapsed().as_secs_f64()
    );
    Ok(())
}
pub struct Session {
    cell: Cell,
    start: Instant,
    ready: Option<Instant>,
    last_memory: Instant,
    cursor: usize,
    previous: Sample,
    current: Sample,
    frames: u64,
    clusters: std::collections::BTreeSet<usize>,
    origin: [f32; 3],
    max_motion: f32,
    attacks: u64,
    shots: u64,
    impacts: u64,
    deaths: u64,
    dismemberment: u64,
    initial_guards: Option<u64>,
    pending: bool,
    finished: bool,
    epoch: u32,
    reset_due: bool,
    destination_seen: bool,
    destination_start: Option<Instant>,
    destination_switch_due: bool,
    level_active: u64,
    level_species: u64,
}
impl Session {
    pub fn new(cell: Cell) -> Result<Self> {
        let prefix = if cell.scene.expected.destination.is_some() {
            format!("{}-source", cell.prefix)
        } else {
            cell.prefix.clone()
        };
        crate::light_profile::survey_reset(cell.output.clone(), prefix);
        let now = Instant::now();
        Ok(Self {
            cell,
            start: now,
            ready: None,
            last_memory: now,
            cursor: 0,
            previous: Sample::default(),
            current: Sample::default(),
            frames: 0,
            clusters: Default::default(),
            origin: [0.; 3],
            max_motion: 0.,
            attacks: 0,
            shots: 0,
            impacts: 0,
            deaths: 0,
            dismemberment: 0,
            initial_guards: None,
            pending: false,
            finished: false,
            epoch: 1,
            reset_due: false,
            destination_seen: false,
            destination_start: None,
            destination_switch_due: false,
            level_active: 0,
            level_species: 0,
        })
    }
    pub fn ready(
        &mut self,
        map: &str,
        entry: Option<&str>,
        feet: [f32; 3],
        yaw: f32,
        pitch: f32,
        flying: bool,
        restored: bool,
        audio: &str,
        identity: serde_json::Value,
    ) -> Result<()> {
        ensure!(
            map == self.cell.scene.map && entry == self.cell.scene.entry.as_deref(),
            "map/entry mismatch"
        );
        ensure!(
            !flying,
            "fixture entered flight instead of supported gameplay"
        );
        ensure!(
            self.cell.scene.checkpoint.is_none() || restored,
            "checkpoint failed to restore"
        );
        if let Some(v) = &self.cell.scene.view {
            ensure!(
                macroquad::prelude::Vec3::from_array(feet)
                    .distance(macroquad::prelude::Vec3::from_array(v.feet))
                    < 2.
                    && (yaw - v.yaw).abs() < 0.001
                    && (pitch - v.pitch).abs() < 0.001,
                "fixture view mismatch"
            );
        }
        self.origin = feet;
        self.ready = Some(Instant::now());
        milestone("viewer", "scene-ready");
        append(
            &self
                .cell
                .output
                .join(format!("{}-validation.jsonl", self.cell.prefix)),
            &serde_json::json!({"map":map,"entry":entry,"feet":feet,"yaw":yaw,"pitch":pitch,"restored":restored,"audio":audio,"gameplay_identity":identity,"load_s":self.start.elapsed().as_secs_f64()}),
        )?;
        Ok(())
    }
    pub fn begin(&mut self) -> Result<bool> {
        // Complete the previous frame before testing duration: final swap/tail is retained.
        crate::light_profile::survey_begin()?;
        if self.destination_switch_due {
            crate::light_profile::survey_flush()?;
            crate::light_profile::survey_reset(
                self.cell.output.clone(),
                format!("{}-destination", self.cell.prefix),
            );
            milestone("transition", "destination-first-presented");
            crate::light_profile::survey_begin()?;
            self.destination_start = Some(Instant::now());
            self.destination_switch_due = false;
        }
        if self.pending && self.frames == 1 {
            milestone("viewer", "first-presented");
        }
        ensure!(
            self.start.elapsed().as_secs() < self.cell.scene.timeout_s,
            "cell progress watchdog"
        );
        let age = self
            .ready
            .context("coverage scene not ready")?
            .elapsed()
            .as_secs_f64();
        let due = if self.cell.scene.expected.destination.is_some() {
            self.destination_start
                .is_some_and(|s| s.elapsed().as_secs_f64() >= self.cell.scene.timed_s)
        } else {
            age >= self.cell.scene.warmup_s
                + self.cell.scene.timed_s / self.cell.scene.epochs as f64
        };
        if due {
            crate::light_profile::survey_flush()?;
            let e = &self.cell.scene.expected;
            ensure!(
                self.frames > 0 && (!self.pending || self.frames > 1),
                "cell has no presented live samples"
            );
            ensure!(
                e.guards.is_none_or(|n| self.initial_guards == Some(n)),
                "expected guard count failed"
            );
            ensure!(
                self.attacks >= e.min_attacks
                    && self.shots >= e.min_shots
                    && self.impacts >= e.min_impacts
                    && self.deaths >= e.min_deaths,
                "activity predicate failed attacks={} shots={} impacts={} deaths={}",
                self.attacks,
                self.shots,
                self.impacts,
                self.deaths
            );
            ensure!(
                self.clusters.len() >= e.min_clusters && self.max_motion >= e.min_motion,
                "motion/cluster predicate failed"
            );
            ensure!(
                e.destination.is_none() || self.destination_seen,
                "destination never presented"
            );
            ensure!(
                self.level_active >= e.min_level_active
                    && self.level_species >= e.min_level_species,
                "natural enemy predicate failed active={} species={}",
                self.level_active,
                self.level_species
            );
            ensure!(
                self.dismemberment >= e.min_dismemberment,
                "dismemberment not observed"
            );
            if self.epoch < self.cell.scene.epochs {
                append(
                    &self
                        .cell
                        .output
                        .join(format!("{}-epochs.jsonl", self.cell.prefix)),
                    &serde_json::json!({"epoch":self.epoch,"frames":self.frames,"attacks":self.attacks,"shots":self.shots,"impacts":self.impacts,"deaths":self.deaths}),
                )?;
                self.epoch += 1;
                self.reset_due = true;
                self.cursor = 0;
                self.previous = Sample::default();
                self.current = Sample::default();
                self.attacks = 0;
                self.shots = 0;
                self.impacts = 0;
                self.deaths = 0;
                self.dismemberment = 0;
                self.initial_guards = None;
                crate::light_profile::survey_reset(
                    self.cell.output.clone(),
                    format!("{}-e{:02}", self.cell.prefix, self.epoch),
                );
                return Ok(false);
            }
            self.finished = true;
            append(
                &self
                    .cell
                    .output
                    .join(format!("{}-validation.jsonl", self.cell.prefix)),
                &serde_json::json!({"finished":true,"frames":self.frames,"attacks":self.attacks,"shots":self.shots,"impacts":self.impacts,"deaths":self.deaths,"clusters":self.clusters,"max_motion":self.max_motion}),
            )?;
            return Ok(true);
        }
        self.previous = self.current.clone();
        let tick = (age * 120.).floor() as u64;
        while self.cursor + 1 < self.cell.scene.tape.len()
            && self.cell.scene.tape[self.cursor + 1].tick <= tick
        {
            self.cursor += 1;
        }
        self.current = self
            .cell
            .scene
            .tape
            .get(self.cursor)
            .cloned()
            .unwrap_or_default();
        METRICS.with(|m| *m.borrow_mut() = Metrics::default());
        if self.last_memory.elapsed().as_secs() >= 1 {
            milestone("live", "one-second");
            self.last_memory = Instant::now();
        }
        Ok(false)
    }
    pub fn finished(&self) -> bool {
        self.finished
    }
    pub fn take_reset(&mut self) -> bool {
        std::mem::take(&mut self.reset_due)
    }
    pub fn reset_complete(&mut self) -> Result<()> {
        self.previous = Sample::default();
        self.current = self.cell.scene.tape.first().cloned().unwrap_or_default();
        METRICS.with(|m| *m.borrow_mut() = Metrics::default());
        #[cfg(feature = "vita_renderer")]
        crate::render_tail::end();
        crate::coverage::milestone("epoch", "reset-complete");
        crate::light_profile::survey_begin()?;
        self.ready = Some(Instant::now());
        Ok(())
    }
    pub fn input(&self) -> (Sample, Sample) {
        (self.current.clone(), self.previous.clone())
    }
    pub fn look(&self, yaw: &mut f32, pitch: &mut f32, dt: f32) {
        *yaw += self.current.look[0] * 2.5 * dt;
        *pitch = (*pitch + self.current.look[1] * 2.5 * dt).clamp(-1.4, 1.4);
    }
    #[allow(clippy::too_many_arguments)]
    pub fn observe(
        &mut self,
        map: &str,
        entry: Option<&str>,
        feet: [f32; 3],
        alive: bool,
        flying: bool,
        paused: bool,
        scripted: bool,
        guards: [u64; 8],
        shots: u64,
        impacts: u64,
        cluster: Option<usize>,
        camera: [f32; 3],
        target: [f32; 3],
        clock: f32,
    ) -> Result<()> {
        let expected_map = self.cell.scene.expected.destination.as_deref();
        ensure!(
            map == self.cell.scene.map
                || expected_map.is_some_and(|d| d == crate::save::visit_key(map, entry)),
            "unexpected map replacement"
        );
        if expected_map.is_some_and(|d| d == crate::save::visit_key(map, entry))
            && !self.destination_seen
        {
            self.destination_seen = true;
            self.destination_switch_due = true;
        }
        ensure!(
            alive && !flying && !paused,
            "cell left live supported state"
        );
        ensure!(
            !scripted
                || self.cell.scene.queen_fixture.is_some()
                || expected_map.is_some()
                || self.cell.scene.family == "W",
            "unexpected cinematic"
        );
        if self.initial_guards.is_none() {
            self.initial_guards = Some(guards[1]);
            if let Some(n) = self.cell.scene.expected.guards {
                ensure!(
                    guards[1] == n && guards[3] == n,
                    "initial live/view guard predicate failed"
                );
            }
        }
        self.attacks = self.attacks.max(guards[4]);
        self.deaths = self.deaths.max(guards[5]);
        self.dismemberment = self.dismemberment.max(guards[7]);
        self.shots = self.shots.max(shots);
        self.impacts = self.impacts.max(impacts);
        if let Some(c) = cluster {
            self.clusters.insert(c);
        }
        self.max_motion = self.max_motion.max(
            macroquad::prelude::Vec3::from_array(feet)
                .distance(macroquad::prelude::Vec3::from_array(self.origin)),
        );
        phase(None);
        let (spans, mut counts) = METRICS.with(|m| {
            let m = m.borrow();
            (m.spans, m.counters)
        });
        self.level_active = self.level_active.max(counts[28]);
        self.level_species = self.level_species.max(counts[29]);
        counts[..8].copy_from_slice(&guards);
        counts[8] = shots;
        counts[9] = impacts;
        crate::light_profile::survey_metrics(spans, counts);
        let age = self.ready.unwrap().elapsed().as_secs_f64();
        // Keep warmup frames for the historical V0 61..180 comparison.
        let (label_age, timed) = if self.cell.scene.expected.destination.is_some() {
            if let Some(start) = self.destination_start {
                (start.elapsed().as_secs_f64(), true)
            } else {
                (
                    age,
                    !self.destination_seen && age <= self.cell.scene.warmup_s,
                )
            }
        } else {
            (age, age >= self.cell.scene.warmup_s)
        };
        crate::light_profile::survey_label(self.frames + 1, label_age, timed);
        crate::light_profile::end(true);
        self.pending = true;
        self.frames += 1;
        if self.frames == 1 || self.frames % 120 == 0 {
            append(
                &self
                    .cell
                    .output
                    .join(format!("{}-validation.jsonl", self.cell.prefix)),
                &serde_json::json!({"frame":self.frames,"age_s":age,"map":map,"entry":entry,"feet":feet,"camera":camera,"target":target,"environment_clock":clock,"npc":guards,"shots":shots,"impacts":impacts,"input_tick":self.current.tick,"scripted":scripted}),
            )?;
            println!(
                "[lg/coverage] progress scene={} frame={} age_s={:.3} map={map}",
                self.cell.scene.id, self.frames, age
            );
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn manifest_rejects_unknown_fields_and_unsafe_paths() {
        assert!(!relative("../saves/auto.json"));
        assert!(!relative("/tmp/x"));
        assert!(relative("fixtures/v0.json"));
        assert!(serde_json::from_str::<Sample>(r#"{"tick":0,"typo":true}"#).is_err());
        let t:Sample=serde_json::from_str(r#"{"tick":12,"primary":true,"alternate":false,"select":7,"use_action":true,"skip":true}"#).unwrap();
        assert!(t.held("Mouse 1") && t.held("8") && t.held("E") && t.held("Enter"));
        assert!(!t.held("Mouse 2"));
    }
}

#[cfg(all(feature = "vita_renderer", not(target_os = "vita")))]
pub async fn prepare(assets: &mut Assets, output: &Path) -> Result<()> {
    std::fs::create_dir_all(output)?;
    let mut records = Vec::new();
    for family in ["T", "W", "E"] {
        let result = (|| -> Result<crate::route::Route> {
            match family {
                "T" => {
                    let mut r = crate::route::Route::new(assets, "gvillage", None)?;
                    crate::village_route::coverage_pre_exit(&mut r)?;
                    Ok(r)
                }
                "W" => {
                    let mut r =
                        crate::route::Route::new(assets, "garden3", Some("garden3_start1"))?;
                    r.wait_for_cinematic()?;
                    Ok(r)
                }
                _ => {
                    let mut r =
                        crate::route::Route::new(assets, "wchess1", Some("wchess1_start1"))?;
                    crate::levels::wchess1::coverage_pre_fight(&mut r)?;
                    Ok(r)
                }
            }
        })();
        match result {
            Err(e) => records.push(
                serde_json::json!({"family":family,"status":"uncovered","reason":format!("{e:#}")}),
            ),
            Ok(r) => {
                let yaw = if family == "T" {
                    (2584_f32 - r.player.feet.y).atan2(-3112. - r.player.feet.x)
                } else {
                    r.player.script_facing
                };
                let mut character = crate::character::Character::load(assets)?;
                character.reset(&r.player, yaw);
                let mut checkpoint = r.checkpoint();
                let npcs = crate::npc::Npcs::load(
                    assets,
                    &r.map,
                    &checkpoint.level.map,
                    checkpoint.level.entry.as_deref(),
                    false,
                    false,
                )?;
                checkpoint.level.npcs = npcs.snapshot();
                let game = crate::campaign_chain::save_chain::build_game(
                    &checkpoint,
                    &checkpoint.ledger,
                    yaw,
                    false,
                    character.snapshot(),
                );
                let dir = output.join(family);
                let store = crate::save::Store::new(dir.clone(), assets.fingerprint()?);
                store.write(crate::save::Slot::parse("slot1")?, &game)?;
                let loaded = store.read(crate::save::Slot::parse("slot1")?)?;
                let restored = crate::save::Restored::build(assets, loaded.game)?;
                ensure!(
                    restored.game.current == game.current
                        && restored.scene.world.body_clear(restored.game.player.feet),
                    "fixture restore mismatch"
                );
                records.push(serde_json::json!({"family":family,"status":"prepared-needs-live-validation","path":format!("{family}/slot1.json"),"sha256":hash(&std::fs::read(dir.join("slot1.json"))?),"map":checkpoint.level.map,"entry":checkpoint.level.entry,"feet":r.player.feet,"yaw":yaw,"route_ticks":r.ticks,"player_sanity":r.stats.sanity()}));
            }
        }
        std::fs::write(
            output.join("prepared.json"),
            serde_json::to_vec_pretty(&records)?,
        )?;
        println!("[lg/coverage] prepared {}", records.last().unwrap());
        macroquad::prelude::next_frame().await;
    }
    Ok(())
}
