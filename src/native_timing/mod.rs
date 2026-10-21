//! Phase 1 passive observations only. Feature AND startup config gated.
//! No audio fences, image tokens, review acknowledgement, native-age scheduling,
//! pair equivalence or campaign qualification. See docs/NATIVE-TIMING-PHASE1.md.
pub mod audio_counters;
pub mod audio_producer;
#[path = "../audio/output_types.rs"]
pub mod audio_types;
pub mod clock;
mod export;
#[cfg(test)]
mod tests;
#[path = "../audio/queue.rs"]
pub mod transfer;
use clock::{Checked, Native, Source};
use serde::{Deserialize, Serialize};

pub const FRAME_CAP: usize = 6144;
pub const EVENT_CAP: usize = 512;
pub const WORLD_CAP: usize = 128;
pub const BUDGET: usize = 2 * 1024 * 1024;
pub const INCOMPLETE: u32 = 1;
pub const INACTIVE: u32 = 2;
pub const CINEMA: u32 = 4;
pub const PAUSED: u32 = 8;
pub const MODAL: u32 = 16;
pub const INSTALL: u32 = 32;
pub const OPERATION: u32 = 64;
pub const FROZEN_SIM: u32 = 128;
pub const TRANSITION: u32 = 256;
pub const SKIP: u32 = 512;
pub const UNFOCUSED: u32 = 1024;
pub const OVERLAY: u32 = 2048;
pub const DEAD: u32 = 4096;
pub const FLYING: u32 = 8192;

#[derive(Clone, Copy, Default, Debug, Serialize, PartialEq, Eq)]
pub struct State {
    pub environment_bits: u32,
    pub pickup_bits: u32,
    pub physics_ticks: u64,
    /// Eye xyz, yaw, pitch; exact f32 bit patterns, no interpolation/digest claim.
    pub pose_bits: [u32; 5],
}
impl State {
    pub fn observe(environment: f32, pickup: f32, ticks: u64, pose: [f32; 5]) -> Self {
        Self {
            environment_bits: environment.to_bits(),
            pickup_bits: pickup.to_bits(),
            physics_ticks: ticks,
            pose_bits: pose.map(f32::to_bits),
        }
    }
}
#[derive(Clone, Copy, Default, Debug, Serialize)]
pub struct Simulation {
    pub elapsed_bits: u32,
    pub dt_bits: u32,
    pub world_dt_bits: u32,
}
#[derive(Clone, Copy, Debug, Serialize)]
pub struct Frame {
    pub viewer_seq: u64,
    /// Exact legacy measured ID if supplied by its real end_frame path; unavailable
    /// on host. Never inferred from our viewer sequence or eligibility.
    pub measured_id: Option<u64>,
    pub begin_us: Option<u64>,
    pub end_us: Option<u64>,
    pub begin_generation: u64,
    pub end_generation: u64,
    pub begin_operation: u64,
    pub end_operation: u64,
    pub reasons: u32,
    pub cpu_begin_us: Option<u64>,
    pub cpu_end_us: Option<u64>,
    /// Explicitly unavailable: recorder is viewer-owned, backend swap is outside it.
    pub swap_begin_us: Option<u64>,
    pub swap_end_us: Option<u64>,
    pub simulation: Option<Simulation>,
    pub before: State,
    pub after: Option<State>,
}
#[derive(Clone, Copy, Debug, Serialize)]
pub enum Reason {
    Fresh,
    StartSave,
    Load,
    Retry,
    ChapterOrConsole,
    Natural,
}
#[derive(Clone, Copy)]
pub struct Text<const N: usize> {
    bytes: [u8; N],
    len: usize,
}
impl<const N: usize> Text<N> {
    fn new(value: &str) -> Option<Self> {
        if value.len() > N {
            return None;
        }
        let mut text = Self {
            bytes: [0; N],
            len: value.len(),
        };
        text.bytes[..value.len()].copy_from_slice(value.as_bytes());
        Some(text)
    }
}
impl<const N: usize> Serialize for Text<N> {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(std::str::from_utf8(&self.bytes[..self.len]).expect("copied UTF8"))
    }
}
#[derive(Clone, Copy, Serialize)]
pub struct World {
    pub generation: u64,
    pub ready_us: Option<u64>,
    pub map: Text<64>,
    pub entry: Option<Text<128>>,
    pub reason: Reason,
    pub baseline: State,
    /// Configuration-provided startup save association is not reused on reloads.
    pub save_sha256: Option<[u8; 32]>,
}
#[derive(Clone, Copy, Debug, Serialize)]
pub enum Milestone {
    InstallBegin,
    Ready,
    FirstPlayable,
    Status,
    ModalEnter,
    ModalExit,
    OpeningEnter,
    OpeningExit,
    EndingEnter,
    EndingExit,
    SkipComplete,
    TransitionBegin,
    LoadAttempt,
    LoadFailed,
    StopRequested,
    Recovery,
}
#[derive(Clone, Copy, Serialize)]
pub struct Event {
    pub ordinal: u64,
    pub native_us: Option<u64>,
    pub viewer_seq: u64,
    pub generation: u64,
    pub operation: u64,
    pub kind: Milestone,
    pub reasons: u32,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Config {
    enabled: bool,
    #[serde(default)]
    audio_raw_counters: bool,
    #[serde(default)]
    audio_producer_capture: bool,
    source_sha256: Option<String>,
    build_sha256: Option<String>,
    game_config_sha256: Option<String>,
    startup_save_sha256: Option<String>,
}
#[derive(Clone, Copy, Default, Serialize)]
pub struct Metadata {
    pub source_sha256: Option<[u8; 32]>,
    pub build_sha256: Option<[u8; 32]>,
    pub game_config_sha256: Option<[u8; 32]>,
    pub startup_save_sha256: Option<[u8; 32]>,
    pub recorder_config_sha256: [u8; 32],
}

pub struct Recorder<C: Source> {
    pub producer_requested: bool,
    producer_start_sent: bool,
    pub audio_producer: Option<Box<audio_producer::Owner>>,
    pub raw_requested: bool,
    pub raw_counters: Option<audio_counters::Capture>,
    pub clock: Checked<C>,
    pub run_id: Option<[u8; 16]>,
    pub metadata: Metadata,
    pub frames: Vec<Frame>,
    pub worlds: Vec<World>,
    pub events: Vec<Event>,
    pub pending: Option<Frame>,
    pub viewer_seq: u64,
    pub generation: u64,
    pub operation: u64,
    pub event_count: u64,
    pub completed: u64,
    pub lost_frames: u64,
    pub lost_worlds: u64,
    pub lost_events: u64,
    pub invalid_identity: bool,
    pub arithmetic_overflow: bool,
    pub stopped: bool,
    pub capture_stop_us: Option<u64>,
    pub capture_stop_viewer_seq: Option<u64>,
    stop_requested: bool,
    pub export_attempts: u64,
    pub first_playable: bool,
    status: Option<u32>,
}
pub const STORAGE_BYTES: usize = std::mem::size_of::<Recorder<Native>>()
    + FRAME_CAP * std::mem::size_of::<Frame>()
    + WORLD_CAP * std::mem::size_of::<World>()
    + EVENT_CAP * std::mem::size_of::<Event>();
// Reserve 64KiB for config, export streaming/hash/file buffers and startup stack
// scratch. Export never clones datasets. Capacities are measured again at runtime.
pub const AUX_RESERVE: usize = 64 * 1024;
pub const MAX_RAW_STORAGE: usize = audio_counters::SNAPSHOT_CAP
    * std::mem::size_of::<audio_counters::Snapshot>()
    + 2 * (std::mem::size_of::<audio_counters::Raw>() + 2 * std::mem::size_of::<usize>());
// The two OnceLock slots are in existing Metrics and music static state; reserve
// them in auxiliary scratch even when the feature's config is disabled.
pub const RAW_SLOT_BYTES: usize =
    2 * std::mem::size_of::<std::sync::OnceLock<std::sync::Arc<audio_counters::Raw>>>();
const _: () =
    assert!(STORAGE_BYTES + MAX_RAW_STORAGE + audio_producer::MAX_STORAGE + AUX_RESERVE <= BUDGET);

impl<C: Source> Recorder<C> {
    pub fn new(clock: C, run_id: Option<[u8; 16]>, metadata: Metadata) -> Self {
        Self {
            producer_requested: false,
            producer_start_sent: false,
            audio_producer: None,
            raw_requested: false,
            raw_counters: None,
            clock: Checked::new(clock),
            run_id,
            metadata,
            frames: Vec::with_capacity(FRAME_CAP),
            worlds: Vec::with_capacity(WORLD_CAP),
            events: Vec::with_capacity(EVENT_CAP),
            pending: None,
            viewer_seq: 0,
            generation: 0,
            operation: 0,
            event_count: 0,
            completed: 0,
            lost_frames: 0,
            lost_worlds: 0,
            lost_events: 0,
            invalid_identity: run_id.is_none(),
            arithmetic_overflow: false,
            stopped: false,
            capture_stop_us: None,
            capture_stop_viewer_seq: None,
            stop_requested: false,
            export_attempts: 0,
            first_playable: false,
            status: None,
        }
    }
    fn inc(value: &mut u64, fault: &mut bool) {
        match value.checked_add(1) {
            Some(n) => *value = n,
            None => *fault = true,
        }
    }
    pub fn event(&mut self, kind: Milestone, reasons: u32) {
        Self::inc(&mut self.event_count, &mut self.arithmetic_overflow);
        let event = Event {
            ordinal: self.event_count,
            native_us: self.clock.read(),
            viewer_seq: self.viewer_seq,
            generation: self.generation,
            operation: self.operation,
            kind,
            reasons,
        };
        if self.events.len() < EVENT_CAP {
            self.events.push(event);
        } else {
            Self::inc(&mut self.lost_events, &mut self.arithmetic_overflow);
        }
    }
    pub fn discontinuity(&mut self, kind: Milestone, reason: u32) {
        Self::inc(&mut self.operation, &mut self.arithmetic_overflow);
        if let Some(row) = &mut self.pending {
            row.reasons |= reason | OPERATION;
        }
        self.event(kind, reason);
    }
    /// Call before the first mutation of an ACTUAL installation. The pending
    /// interval remains present and explicitly invalid until its next real begin.
    pub fn before_install(&mut self) {
        self.discontinuity(Milestone::InstallBegin, INSTALL);
    }
    /// Call only after the entire successful live state/audio/resource installation.
    pub fn ready(&mut self, map: &str, entry: Option<&str>, reason: Reason, baseline: State) {
        Self::inc(&mut self.generation, &mut self.arithmetic_overflow);
        self.first_playable = false;
        self.status = None;
        if let Some(row) = &mut self.pending {
            row.reasons |= INSTALL;
        }
        let name = Text::new(map);
        let spawn = entry.map(Text::new);
        if name.is_none() || spawn.is_some_and(|v| v.is_none()) {
            self.invalid_identity = true;
        }
        let world = World {
            generation: self.generation,
            ready_us: self.clock.read(),
            map: name.unwrap_or_else(|| Text::new("UNKNOWN").unwrap()),
            entry: spawn.flatten(),
            reason,
            baseline,
            save_sha256: if matches!(reason, Reason::StartSave) {
                self.metadata.startup_save_sha256
            } else {
                None
            },
        };
        if self.worlds.len() < WORLD_CAP {
            self.worlds.push(world);
        } else {
            Self::inc(&mut self.lost_worlds, &mut self.arithmetic_overflow);
        }
        self.event(Milestone::Ready, 0);
    }
    pub fn begin(&mut self, before: State) {
        Self::inc(&mut self.viewer_seq, &mut self.arithmetic_overflow);
        if self.stopped {
            return;
        }
        let now = self.clock.read();
        if !self.producer_start_sent {
            if let Some(owner) = &self.audio_producer {
                owner.request_start(now);
                self.producer_start_sent = true;
            }
        }
        if let Some(mut row) = self.pending.take() {
            row.end_us = now;
            row.end_generation = self.generation;
            row.end_operation = self.operation;
            if row.begin_generation != row.end_generation {
                row.reasons |= INSTALL;
            }
            if row.begin_operation != row.end_operation {
                row.reasons |= OPERATION;
            }
            Self::inc(&mut self.completed, &mut self.arithmetic_overflow);
            if self.frames.len() < FRAME_CAP {
                self.frames.push(row);
            } else {
                Self::inc(&mut self.lost_frames, &mut self.arithmetic_overflow);
            }
        }
        if self.stop_requested {
            self.stopped = true;
            self.capture_stop_us = now;
            self.capture_stop_viewer_seq = Some(self.viewer_seq);
            return;
        }
        self.pending = Some(Frame {
            viewer_seq: self.viewer_seq,
            measured_id: None,
            begin_us: now,
            end_us: None,
            begin_generation: self.generation,
            end_generation: self.generation,
            begin_operation: self.operation,
            end_operation: self.operation,
            reasons: INCOMPLETE,
            cpu_begin_us: None,
            cpu_end_us: None,
            swap_begin_us: None,
            swap_end_us: None,
            simulation: None,
            before,
            after: None,
        });
    }
    pub fn cpu_begin(&mut self) {
        let now = self.clock.read();
        if let Some(r) = &mut self.pending {
            r.cpu_begin_us = now;
        }
    }
    pub fn simulation(&mut self, elapsed: f32, dt: f32, world_dt: f32) {
        if let Some(r) = &mut self.pending {
            r.simulation = Some(Simulation {
                elapsed_bits: elapsed.to_bits(),
                dt_bits: dt.to_bits(),
                world_dt_bits: world_dt.to_bits(),
            });
        }
    }
    pub fn end(&mut self, after: State, reasons: u32) {
        let now = self.clock.read();
        if self.status != Some(reasons) {
            self.discontinuity(Milestone::Status, reasons);
            self.status = Some(reasons);
        }
        if reasons == 0
            && !self.first_playable
            && self.pending.as_ref().is_some_and(|r| {
                r.begin_generation == self.generation
                    && r.reasons & (INSTALL | TRANSITION | SKIP | MODAL) == 0
            })
        {
            self.event(Milestone::FirstPlayable, 0);
            self.first_playable = true;
        }
        if let Some(row) = &mut self.pending {
            row.after = Some(after);
            row.cpu_end_us = now;
            row.reasons = (row.reasons & !INCOMPLETE) | reasons;
        }
    }
    pub fn measured(&mut self, id: Option<u64>) {
        if let Some(row) = &mut self.pending {
            row.measured_id = id;
        }
    }
    /// Stop acquisition at the NEXT actual viewer begin, never at export time.
    pub fn request_stop(&mut self) {
        if !self.stop_requested && !self.stopped {
            self.event(Milestone::StopRequested, 0);
            self.stop_requested = true;
        }
    }
    /// Setup-only, after Session::open; a worker/callback can already be active.
    /// Missing routes stay absent. OnceLock is read-only on the event hot path.
    pub fn bind_raw_counters(
        &mut self,
        pcm: Option<&std::sync::OnceLock<std::sync::Arc<audio_counters::Raw>>>,
        music: Option<&std::sync::OnceLock<std::sync::Arc<audio_counters::Raw>>>,
    ) {
        use audio_counters::{Capture, Raw, Tag};
        if !self.raw_requested || self.raw_counters.is_some() {
            return;
        }
        let pcm = pcm.map(|slot| Raw::activate(slot, &mut self.clock, export::random_id()));
        let music = music.map(|slot| Raw::activate(slot, &mut self.clock, export::random_id()));
        let mut capture = Capture::new(pcm, music);
        capture.sample(&mut self.clock, Tag::Binding);
        self.raw_counters = Some(capture);
    }
    /// Functional outside-window API for a future scheduler. No schedule is
    /// implemented here; caller must retain the request chronology separately.
    pub fn sample_raw_counters(&mut self, tag: audio_counters::Tag) {
        if let Some(raw) = &mut self.raw_counters {
            raw.sample(&mut self.clock, tag);
        }
    }
    pub fn actual_storage_bytes(&self) -> usize {
        std::mem::size_of::<Self>()
            + self
                .audio_producer
                .as_ref()
                .map_or(audio_producer::BOOTSTRAP_RESERVE, |o| o.reserved)
            + self.frames.capacity() * std::mem::size_of::<Frame>()
            + self.worlds.capacity() * std::mem::size_of::<World>()
            + self.events.capacity() * std::mem::size_of::<Event>()
            + self
                .raw_counters
                .as_ref()
                .map_or(0, audio_counters::Capture::storage_bytes)
    }
}

pub struct Session {
    pub recorder: Option<Box<Recorder<Native>>>,
}
static SESSION_CLAIM: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
impl Session {
    /// Config is <=4096 bytes, read once before world install. Missing/disabled
    /// config performs no recorder allocation. Invalid config fails explicitly.
    pub fn open() -> anyhow::Result<Self> {
        use std::io::Read;
        let file = match std::fs::File::open(crate::preferences::path("native-timing-config.json")) {
            Ok(f) => f,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self { recorder: None })
            }
            Err(e) => return Err(e.into()),
        };
        let mut bytes = Vec::with_capacity(4097);
        file.take(4097).read_to_end(&mut bytes)?;
        anyhow::ensure!(
            bytes.len() <= 4096,
            "timing config exceeds bounded 4096 bytes"
        );
        let cfg: Config = serde_json::from_slice(&bytes)?;
        if !cfg.enabled {
            return Ok(Self { recorder: None });
        }
        anyhow::ensure!(
            !SESSION_CLAIM.swap(true, std::sync::atomic::Ordering::SeqCst),
            "native timing Session already claimed"
        );
        let metadata = Metadata {
            source_sha256: export::parse_hash(cfg.source_sha256)?,
            build_sha256: export::parse_hash(cfg.build_sha256)?,
            game_config_sha256: export::parse_hash(cfg.game_config_sha256)?,
            startup_save_sha256: export::parse_hash(cfg.startup_save_sha256)?,
            recorder_config_sha256: export::hash(&bytes),
        };
        let mut recorder = Box::new(Recorder::new(Native, export::random_id(), metadata));
        recorder.raw_requested = cfg.audio_raw_counters;
        recorder.producer_requested = cfg.audio_producer_capture;
        anyhow::ensure!(
            recorder.actual_storage_bytes() + AUX_RESERVE <= BUDGET,
            "timing allocation budget"
        );
        Ok(Self {
            recorder: Some(recorder),
        })
    }
    #[inline]
    pub fn with(&mut self, f: impl FnOnce(&mut Recorder<Native>)) {
        if let Some(r) = self.recorder.as_mut() {
            f(r);
        }
    }
    /// Invoked only after the viewer loop has ended. No pending finalization.
    pub fn export_after_viewer(&mut self) -> anyhow::Result<()> {
        self.export_after_viewer_to(&crate::preferences::path("native-timing"))
    }
    pub(crate) fn export_after_viewer_to(&mut self, root: &std::path::Path) -> anyhow::Result<()> {
        if let Some(r) = &mut self.recorder {
            if let Some(owner) = &mut r.audio_producer {
                owner.request_seal(&mut r.clock);
                owner.collect(std::time::Duration::from_secs(2));
                owner.freeze();
            }
            if let Some(raw) = &mut r.raw_counters {
                raw.sample(&mut r.clock, audio_counters::Tag::AfterViewer);
            }
            r.export(root)?;
        }
        Ok(())
    }
}
