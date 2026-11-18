//! Versioned local saves. Only gameplay state is stored; assets and collision are rebuilt.
use crate::{
    assets::Assets, bsp::Bsp, character::Character, cheshire::Hints, collision::World,
    interaction::Interactions, inventory::Stats, movement::Player, npc::Npcs, recovery::Recovery,
    render::Scene, story::Story,
};
use anyhow::{ensure, Context, Result};
use macroquad::prelude::*;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::mpsc::{self, Receiver, SyncSender, TryRecvError},
    thread::{self, JoinHandle},
    time::{SystemTime, UNIX_EPOCH},
};

/// Format 12 is the only bump the campaign makes (DG-6, F2): it adds the registered
/// controllers' state (`interactions.levels`). Older executables then refuse such a file with a
/// version message instead of failing later on an event-signature mismatch.
const VERSION: u32 = 12;
/// The first format whose files may carry a non-empty `interactions.levels` map.
const LEVELS_SINCE: u32 = 12;
pub(crate) const LIMIT: u64 = 8 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Slot {
    Quick,
    Auto,
    One,
    Two,
    Three,
    Four,
}
impl Slot {
    pub const ALL: [Self; 6] = [
        Self::One,
        Self::Two,
        Self::Three,
        Self::Four,
        Self::Quick,
        Self::Auto,
    ];
    pub fn parse(s: &str) -> Result<Self> {
        match s {
            "quick" => Ok(Self::Quick),
            "auto" => Ok(Self::Auto),
            "slot1" => Ok(Self::One),
            "slot2" => Ok(Self::Two),
            "slot3" => Ok(Self::Three),
            "slot4" => Ok(Self::Four),
            _ => anyhow::bail!("Use quick, auto or slot1-slot4"),
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::Quick => "quick",
            Self::Auto => "auto",
            Self::One => "slot1",
            Self::Two => "slot2",
            Self::Three => "slot3",
            Self::Four => "slot4",
        }
    }
    pub fn title(self) -> &'static str {
        match self {
            Self::One => "Slot 1",
            Self::Two => "Slot 2",
            Self::Three => "Slot 3",
            Self::Four => "Slot 4",
            Self::Quick => "Quick save",
            Self::Auto => "Automatic save",
        }
    }
}

#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Campaign {
    pub levels: BTreeMap<String, Level>,
    pub completed: BTreeSet<String>,
}
/// The supported return visits have distinct setup and must never overwrite first visits.
/// Named first entrances and the default entrance share the same logical visit.
pub fn visit_key(map: &str, entry: Option<&str>) -> String {
    format!(
        "{map}${}",
        if entry.is_some() && entry == crate::campaign::return_entry(map) {
            "return"
        } else {
            "first"
        }
    )
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Level {
    pub map: String,
    pub entry: Option<String>,
    pub interactions: crate::interaction::Snapshot,
    pub npcs: crate::npc::Snapshot,
    pub story: crate::story::Snapshot,
    pub hints: crate::cheshire::Snapshot,
    pub environment_clock: f32,
    pub pickup_clock: f32,
}
impl Level {
    pub fn key(&self) -> String {
        visit_key(&self.map, self.entry.as_deref())
    }
    pub fn restore_logic(
        &self,
        assets: &mut Assets,
        map: &Bsp,
    ) -> Result<(Interactions, Story, Hints)> {
        validate_entry(map, self.entry.as_deref())?;
        let mut interactions = Interactions::load(map)?;
        interactions.set_entry(assets, map, &self.map, self.entry.as_deref())?;
        interactions.restore(&self.interactions, map)?;
        let mut hints = Hints::load(assets, map, &self.map)?;
        hints.restore(&self.hints)?;
        hints.sync(&interactions);
        let mut story = Story::load(assets, &self.map);
        story.restore(&self.story, &hints)?;
        Ok((interactions, story, hints))
    }
}
pub fn validate_entry(map: &Bsp, entry: Option<&str>) -> Result<()> {
    if let Some(entry) = entry {
        ensure!(
            map.entities.iter().any(|e| e
                .get("classname")
                .is_some_and(|s| s == "info_player_start")
                && e.get("targetname").is_some_and(|s| s == entry)),
            "Saved entrance is missing from the level"
        );
    }
    Ok(())
}

#[derive(Clone, Serialize, Deserialize)]
pub struct View {
    pub position: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub third_person: bool,
    pub flying: bool,
    pub fullbright: bool,
    pub spawn_landing: bool,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Game {
    pub current: String,
    pub campaign: Campaign,
    pub stats: Stats,
    pub player: Player,
    pub view: View,
    pub recovery: Recovery,
    pub character: crate::character::Snapshot,
}
impl Game {
    pub fn level(&self) -> Result<&Level> {
        self.campaign
            .levels
            .get(&self.current)
            .context("Saved current level is missing")
    }
    pub fn validate(&self) -> Result<()> {
        self.stats.validate_save()?;
        ensure!(
            self.campaign.levels.len() <= 72 && self.campaign.completed.len() <= 72,
            "Saved campaign is too large"
        );
        for (key, level) in &self.campaign.levels {
            ensure!(
                *key == level.key()
                    && crate::interaction::destination(&level.map).is_some()
                    && level.environment_clock >= 0.
                    && level.pickup_clock >= 0.,
                "Invalid saved campaign visit"
            );
        }
        ensure!(
            self.campaign
                .completed
                .iter()
                .all(|k| self.campaign.levels.contains_key(k)),
            "Saved campaign completion lacks level state"
        );
        self.level()?;
        ensure!(
            self.player.feet.is_finite()
                && self.player.velocity.is_finite()
                && self.view.position.is_finite()
                && self.view.yaw.is_finite()
                && self.view.pitch.is_finite()
                && self.view.pitch.abs() <= 1.56,
            "Invalid saved player position/view"
        );
        self.player.validate_save()?;
        Ok(())
    }
}

#[derive(Serialize, Deserialize)]
struct Envelope {
    format: String,
    version: u32,
    game_data: String,
    saved_at: u64,
    checksum: String,
    payload: serde_json::Value,
    // Cosmetic metadata shares the atomic file/backup but never invalidates
    // gameplay state when absent or malformed. Older readers ignore this field.
    #[serde(default, skip_serializing_if = "serde_json::Value::is_null")]
    preview: serde_json::Value,
}
pub struct Loaded {
    pub game: Game,
    pub saved_at: u64,
    pub backup: bool,
    pub preview: Option<crate::save_preview::Preview>,
}
pub struct Store {
    pub directory: PathBuf,
    fingerprint: String,
    /// A replay builds every save as usual but never writes it: the player's saves stay as they were.
    discard_writes: bool,
}
impl Store {
    pub fn new(directory: PathBuf, fingerprint: String) -> Self {
        Self {
            directory,
            fingerprint,
            discard_writes: false,
        }
    }
    pub fn discarding_writes(mut self) -> Self {
        self.discard_writes = true;
        self
    }
    pub fn for_replay(self, options: &crate::Options) -> Self {
        #[cfg(not(target_os = "vita"))]
        if let Some(directory) = isolated_replay_directory(
            std::env::var_os("LOOKING_GLASS_REPLAY_WRITES").as_deref(),
            options.save_dir_explicit.then_some(self.directory.as_path()),
            &crate::default_save_dir(),
        ) {
            return Self { directory, ..self };
        }
        #[cfg(target_os = "vita")]
        let _ = options;
        self.discarding_writes()
    }
    fn path(&self, slot: Slot, backup: bool) -> PathBuf {
        self.directory.join(format!(
            "{}{}.json",
            slot.name(),
            if backup { ".previous" } else { "" }
        ))
    }
    pub fn exists(&self, slot: Slot) -> bool {
        self.path(slot, false).is_file() || self.path(slot, true).is_file()
    }
    pub fn write(&self, slot: Slot, game: &Game) -> Result<()> {
        self.write_with_preview(slot, game, None)
    }
    pub fn write_with_preview(
        &self,
        slot: Slot,
        game: &Game,
        preview: Option<&crate::save_preview::Preview>,
    ) -> Result<()> {
        self.write_with_preview_saved_at(slot, game, preview, SavedAt::Now)
    }
    fn write_with_preview_saved_at(
        &self,
        slot: Slot,
        game: &Game,
        preview: Option<&crate::save_preview::Preview>,
        saved_at: SavedAt,
    ) -> Result<()> {
        #[cfg(feature="native_timing")]
        crate::native_timing::operations::attempt();
        game.validate()?;
        ensure!(
            game.campaign
                .levels
                .values()
                .all(|l| l.map != "potears3" || l.interactions.has_duchess()),
            "Saved Duchess state is missing"
        );
        ensure!(
            game.campaign
                .levels
                .values()
                .all(|l| l.map != "pandemonium" || l.interactions.has_pandemonium_cinema()),
            "Saved cinematic state is missing"
        );
        ensure!(
            game.campaign
                .levels
                .values()
                .all(|l| l.key() != "skool1$return" || l.interactions.has_school_return()),
            "Saved observatory state is missing"
        );
        ensure!(
            game.campaign
                .levels
                .values()
                .all(|l| l.interactions.has_shared_state()),
            "Load legacy level state before saving in the current format"
        );
        let payload = serde_json::to_value(game)?;
        validate_json(&payload)?;
        // The writer guard: registered state never leaves in a format that predates it.
        require_format_for_levels(VERSION, &payload)?;
        let data = serde_json::to_vec(&payload)?;
        let envelope = Envelope {
            format: "looking-glass-save".into(),
            version: VERSION,
            game_data: self.fingerprint.clone(),
            saved_at: saved_at.read()?,
            checksum: format!("{:x}", Sha256::digest(&data)),
            payload,
            preview: preview
                .map(serde_json::to_value)
                .transpose()?
                .unwrap_or_default(),
        };
        let bytes = serde_json::to_vec(&envelope)?;
        ensure!(bytes.len() as u64 <= LIMIT, "Save exceeds size limit");
        if self.discard_writes {
            println!("[lg/replay] save not written slot={} map={} bytes={}", slot.name(), game.current, bytes.len());
            return Ok(());
        }
        fs::create_dir_all(&self.directory).context("Cannot create save folder")?;
        let path = self.path(slot, false);
        // Never replace a usable backup with a damaged primary file.
        if let Ok(previous) = read_limited(&path) {
            if self.decode(&previous).is_ok() {
                atomic_write(&self.path(slot, true), &previous)?;
            }
        }
        atomic_write(&path, &bytes)
            .context("Could not write save; the previous save remains available")?;
        println!(
            "Saved {}: {} ({})",
            slot.name(),
            game.current,
            path.display()
        );
        Ok(())
    }
    fn decode(&self, bytes: &[u8]) -> Result<Loaded> {
        let e: Envelope =
            serde_json::from_slice(bytes).context("Save file is damaged or incomplete")?;
        ensure!(
            e.format == "looking-glass-save" && (1..=VERSION).contains(&e.version),
            "This save uses an unsupported format version"
        );
        ensure!(
            e.game_data == self.fingerprint,
            "This save belongs to different game data"
        );
        let digest = format!("{:x}", Sha256::digest(serde_json::to_vec(&e.payload)?));
        ensure!(digest == e.checksum, "Save integrity check failed");
        validate_json(&e.payload)?;
        require_format_for_levels(e.version, &e.payload)?;
        ensure!(
            e.version < 3
                || (e.payload["character"]["projectiles"]["dice"].is_object()
                    && e.payload["character"]["projectiles"]["dice_count"].is_u64()),
            "Saved Demon Dice state is missing"
        );
        ensure!(
            e.version < 9
                || (e.payload["player"]["breath"].is_object()
                    && e.payload["stats"]["turtle_air"].is_boolean()),
            "Saved breath state is missing"
        );
        ensure!(
            e.version < 10
                || (e.payload["stats"]["powers"].is_object()
                    && e.payload["stats"]["loot"].is_object()
                    && e.payload["stats"]["difficulty"].is_string()),
            "Saved power-up state is missing"
        );
        ensure!(
            e.version < 11 || e.payload["stats"]["notarget"].is_boolean(),
            "Saved console targeting flag is missing"
        );
        let mut game: Game =
            serde_json::from_value(e.payload).context("Save contains invalid gameplay state")?;
        if e.version < 9 {
            let level = game.level()?;
            game.stats.turtle_air =
                crate::campaign::after_temple(&level.map, level.entry.as_deref())
                    || game.campaign.levels.values().any(|l| l.map == "utemple");
            game.player.breath.shell = game.stats.turtle_air;
        }
        game.validate()?;
        ensure!(
            e.version < 8
                || game
                    .campaign
                    .levels
                    .values()
                    .all(|l| l.map != "potears3" || l.interactions.has_duchess()),
            "Saved Duchess state is missing"
        );
        ensure!(
            e.version < 7
                || game
                    .campaign
                    .levels
                    .values()
                    .all(|l| l.map != "pandemonium" || l.interactions.has_pandemonium_cinema()),
            "Saved cinematic state is missing"
        );
        ensure!(
            e.version < 6
                || game
                    .campaign
                    .levels
                    .values()
                    .all(|l| l.key() != "skool1$return" || l.interactions.has_school_return()),
            "Saved observatory state is missing"
        );
        ensure!(
            e.version < 4
                || game
                    .campaign
                    .levels
                    .values()
                    .all(|l| l.map != "potears1" || l.interactions.has_encounters()),
            "Saved Ladybug encounter state is missing"
        );
        ensure!(
            e.version < 5
                || game.campaign.levels.values().all(|l| l.map != "pandemonium"
                    || (l.interactions.has_pandemonium() && l.interactions.has_encounters())),
            "Saved Pandemonium state is missing"
        );
        ensure!(
            e.version == 1
                || game
                    .campaign
                    .levels
                    .values()
                    .all(|l| l.interactions.has_shared_state()),
            "Saved shared entity state is missing"
        );
        Ok(Loaded {
            game,
            saved_at: e.saved_at,
            backup: false,
            preview: crate::save_preview::Preview::from_value(e.preview),
        })
    }
    // The simulation bench must use the exact reference save, never its backup.
    #[cfg(any(target_os = "vita", test))]
    pub fn read_bench_auto(&self) -> Result<Loaded> {
        read_limited(&self.path(Slot::Auto, false)).and_then(|b| self.decode(&b))
    }
    pub fn read(&self, slot: Slot) -> Result<Loaded> {
        match read_limited(&self.path(slot, false)).and_then(|b| self.decode(&b)) {
            Ok(s) => Ok(s),
            Err(primary) => {
                match read_limited(&self.path(slot, true)).and_then(|b| self.decode(&b)) {
                    Ok(mut s) => {
                        s.backup = true;
                        Ok(s)
                    }
                    Err(_) => Err(primary).context(format!("Cannot load {} save", slot.name())),
                }
            }
        }
    }
    pub fn latest(&self) -> Option<(Slot, Loaded)> {
        let latest = Slot::ALL
            .into_iter()
            .filter_map(|slot| self.read(slot).ok().map(|s| (slot, s)))
            .max_by_key(|(_, s)| s.saved_at);
        // Completion leaves the six player slots intact. Continue can replay
        // the final encounter from its separate, pre-ending checkpoint.
        if let Ok(bytes) = read_limited(&self.directory.join("campaign-complete.json")) {
            if let Ok(at) = serde_json::from_slice::<u64>(&bytes) {
                if latest.as_ref().is_none_or(|(_, s)| s.saved_at <= at) {
                    if let Ok(mut saved) = self.finale_store().read(Slot::Auto) {
                        saved.game.campaign.completed.insert("qlair$first".into());
                        return Some((Slot::Auto, saved));
                    }
                }
            }
        }
        latest
    }
    fn finale_store(&self) -> Self {
        Self {
            directory: self.directory.join("finale-checkpoint"),
            fingerprint: self.fingerprint.clone(),
            discard_writes: self.discard_writes,
        }
    }
    pub fn write_finale_checkpoint(&self, game: &Game) -> Result<()> {
        self.write_finale_checkpoint_with_preview(game, None)
    }
    pub fn write_finale_checkpoint_with_preview(
        &self,
        game: &Game,
        preview: Option<&crate::save_preview::Preview>,
    ) -> Result<()> {
        self.write_checkpoint_pair(|store, _| store.write_with_preview(Slot::Auto, game, preview))
    }
    fn write_checkpoint_pair(&self, mut write: impl FnMut(&Self, usize) -> Result<()>) -> Result<()> {
        write(&self.finale_store(), 0)?;
        write(self, 1)
    }
    pub fn mark_complete(&self) -> Result<()> {
        if self.discard_writes {
            println!("[lg/replay] completion marker not written");
            return Ok(());
        }
        std::fs::create_dir_all(&self.directory)?;
        let at = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis() as u64;
        atomic_write(
            &self.directory.join("campaign-complete.json"),
            &serde_json::to_vec(&at)?,
        )
    }
}

/// Reserve a fresh directory exclusively for this diagnostic run.
#[cfg(not(target_os = "vita"))]
fn isolated_replay_directory(
    enabled: Option<&std::ffi::OsStr>,
    explicit: Option<&Path>,
    player: &Path,
) -> Option<PathBuf> {
    fn canonical_destination(path: &Path) -> Option<PathBuf> {
        let parent = path.parent()?;
        let parent = if parent.as_os_str().is_empty() { Path::new(".") } else { parent };
        let parent = parent.canonicalize().ok()?;
        Some(parent.join(path.file_name()?))
    }
    if enabled != Some(std::ffi::OsStr::new("1")) {
        return None;
    }
    let directory = canonical_destination(explicit?)?;
    if canonical_destination(player).is_some_and(|player| {
        player.parent() == directory.parent()
            && player.file_name().unwrap().eq_ignore_ascii_case(directory.file_name().unwrap())
    }) {
        return None;
    }
    fs::create_dir(&directory).ok()?;
    Some(directory)
}

/// Where the envelope's `saved_at` comes from: the wall clock read where the write runs
/// (RC19, and every store without the background writer), or a value captured at the
/// frame boundary that requested the save, so the bytes cannot depend on when the
/// writer gets to them.
#[derive(Clone, Copy)]
enum SavedAt {
    Now,
    At(u64),
}
impl SavedAt {
    fn read(self) -> Result<u64> {
        match self {
            Self::Now => saved_at_now(),
            Self::At(at) => Ok(at),
        }
    }
}
/// The envelope timestamp. A test may pin this clock so a byte-for-byte comparison
/// of the foreground and background paths does not depend on the wall.
fn saved_at_now() -> Result<u64> {
    #[cfg(test)]
    if let Some(at) = TEST_SAVED_AT.with(std::cell::Cell::get) {
        return Ok(at);
    }
    Ok(SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis() as u64)
}
#[cfg(test)]
thread_local! {
    static TEST_SAVED_AT: std::cell::Cell<Option<u64>> = const { std::cell::Cell::new(None) };
}

/// The frame captures both envelope timestamps before handing the checkpoint to the writer.
struct CheckpointJob {
    game: Box<Game>,
    preview: Option<crate::save_preview::Preview>,
    saved_at: [u64; 2],
    completed: SyncSender<Result<()>>,
}

pub struct PendingCheckpoint {
    completed: Receiver<Result<()>>,
    retry_snapshot: Box<Game>,
}
impl PendingCheckpoint {
    /// Apply the same controller and retry updates the foreground path makes on success.
    /// A pending or failed job leaves both the live request and the old retry point intact.
    pub fn finish(
        pending: &mut Option<Self>,
        wait: bool,
        levels: &mut [crate::level::Slot],
        retry: &mut crate::recovery::RetryPoint,
    ) -> Option<Result<()>> {
        let job = pending.as_ref()?;
        let result = if wait {
            job.completed.recv().context("The save writer stopped")
        } else {
            match job.completed.try_recv() {
                Err(TryRecvError::Empty) => return None,
                result => result.context("The save writer stopped"),
            }
        }
        .and_then(|result| result);
        let job = pending.take().unwrap();
        if result.is_ok() {
            for slot in levels {
                slot.ctl.checkpoint_written();
            }
            retry.remember(*job.retry_snapshot);
        }
        Some(result)
    }
}

/// One writer and one permit. A second submission waits for the first pair to finish,
/// and each submitter keeps its own result even if it waits to collect it.
pub struct CheckpointWriter {
    submit: Option<SyncSender<CheckpointJob>>,
    idle: Receiver<()>,
    thread: Option<JoinHandle<()>>,
}
impl CheckpointWriter {
    pub fn start(store: &Store) -> std::io::Result<Self> {
        let store = Store {
            directory: store.directory.clone(),
            fingerprint: store.fingerprint.clone(),
            discard_writes: store.discard_writes,
        };
        Self::spawn(move |job| {
            store.write_checkpoint_pair(|destination, index| {
                destination.write_with_preview_saved_at(
                    Slot::Auto, &job.game, job.preview.as_ref(), SavedAt::At(job.saved_at[index]),
                )
            })
        })
    }

    fn spawn(write: impl Fn(&CheckpointJob) -> Result<()> + Send + 'static) -> std::io::Result<Self> {
        let (submit, jobs) = mpsc::sync_channel::<CheckpointJob>(1);
        let (ready, idle) = mpsc::sync_channel(1);
        ready.send(()).unwrap();
        let thread = thread::Builder::new()
            .name("save-writer".into())
            .stack_size(1024 * 1024)
            .spawn(move || {
                #[cfg(target_os = "vita")]
                {
                    let policy = crate::audio::thread_policy(191, 0);
                    eprintln!("[lg/worker-probe] role=save-writer {}", policy.describe());
                }
                for job in jobs {
                    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| write(&job)))
                        .unwrap_or_else(|panic| Err(anyhow::anyhow!("The save writer failed unexpectedly: {panic:?}")));
                    let _ = job.completed.send(result);
                    if ready.send(()).is_err() {
                        break;
                    }
                }
            })?;
        Ok(Self { submit: Some(submit), idle, thread: Some(thread) })
    }

    pub fn submit(
        &self,
        game: Game,
        preview: Option<crate::save_preview::Preview>,
        retry_snapshot: Game,
    ) -> Result<PendingCheckpoint> {
        let saved_at = [saved_at_now()?, saved_at_now()?];
        let (completed, result) = mpsc::sync_channel(1);
        self.idle.recv().context("The save writer stopped")?;
        self.submit.as_ref().unwrap().send(CheckpointJob {
            game: Box::new(game),
            preview,
            saved_at,
            completed,
        })
        .map_err(|_| anyhow::anyhow!("The save writer stopped"))?;
        Ok(PendingCheckpoint { completed: result, retry_snapshot: Box::new(retry_snapshot) })
    }
}
impl Drop for CheckpointWriter {
    fn drop(&mut self) {
        // Close submissions, then drain and join even on an early return from the viewer.
        drop(self.submit.take());
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
/// What the viewer does with the map after a bench validation.
/// `Restored::build` already called `Scene::load`. Loading again builds the village twice
/// and the second scene is thrown away when that `Restored` is installed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BenchScene {
    ReuseValidated,
    LoadFresh,
}
pub fn bench_scene(validated: bool) -> BenchScene {
    if validated {
        BenchScene::ReuseValidated
    } else {
        BenchScene::LoadFresh
    }
}
fn read_limited(path: &Path) -> Result<Vec<u8>> {
    let file =
        File::open(path).with_context(|| format!("No readable save at {}", path.display()))?;
    ensure!(file.metadata()?.len() <= LIMIT, "Save file is too large");
    let mut bytes = Vec::new();
    file.take(LIMIT + 1).read_to_end(&mut bytes)?;
    ensure!(bytes.len() as u64 <= LIMIT, "Save file is too large");
    Ok(bytes)
}
fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let temp = path.with_extension(format!("json.{}.tmp", std::process::id()));
    // A per-process temporary file cannot replace the committed save before flush succeeds.
    let result = (|| -> Result<()> {
        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&temp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temp, path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}
/// Whether any cached visit holds registered-controller state. It fails closed: a `levels` value
/// that is anything but absent, null or an empty object counts as state.
fn holds_registry_state(payload: &serde_json::Value) -> bool {
    payload["campaign"]["levels"]
        .as_object()
        .is_some_and(|visits| {
            visits
                .values()
                .any(|visit| match &visit["interactions"]["levels"] {
                    serde_json::Value::Null => false,
                    serde_json::Value::Object(map) => !map.is_empty(),
                    _ => true,
                })
        })
}
/// Registered-controller state exists only in format 12 and later. The reader rejects it in an
/// older file; the writer never writes it under an older version.
fn require_format_for_levels(version: u32, payload: &serde_json::Value) -> Result<()> {
    ensure!(
        version >= LEVELS_SINCE || !holds_registry_state(payload),
        "Saved level-controller state needs save format {LEVELS_SINCE}"
    );
    Ok(())
}
pub(crate) fn validate_json(value: &serde_json::Value) -> Result<()> {
    match value {
        serde_json::Value::Number(n) => ensure!(
            n.as_f64().is_some_and(|v| v.is_finite() && v.abs() <= 1e12),
            "Invalid save number"
        ),
        serde_json::Value::String(s) => ensure!(s.len() <= 1024, "Invalid save string"),
        serde_json::Value::Array(a) => {
            ensure!(a.len() <= 10000, "Invalid save array");
            for v in a {
                validate_json(v)?;
            }
        }
        serde_json::Value::Object(o) => {
            for v in o.values() {
                validate_json(v)?;
            }
        }
        _ => (),
    }
    Ok(())
}

/// Construct a replacement scene first. A failed load cannot partially mutate live play.
pub struct Restored {
    pub scene: Scene,
    pub interactions: Interactions,
    pub story: Story,
    pub hints: Hints,
    pub npcs: Npcs,
    pub alice: Character,
    pub game: Game,
    pub steam: crate::particles::Steam,
}
impl Level {
    /// A cached visit whose saved logic predates a shared-state, traversal or controller upgrade.
    fn needs_upgrade(&self) -> bool {
        let i = &self.interactions;
        !i.has_traversal()
            || !i.has_shared_state()
            || (self.map == "potears3" && !i.has_duchess())
            || (self.map == "fortress1" && !i.has_fortress())
            || (self.map == "fortress2" && !i.has_beyond())
            || (self.map == "potears1" && !i.has_encounters())
            || (self.map == "skool2" && !i.has_encounters())
            || (self.map == "potears1" && !i.has_pool())
            || (self.map == "potears1" && !i.has_pool_pilot())
            || (self.map == "potears1" && !i.has_pool_arrival())
            || (self.map == "potears1" && !i.has_pool_boulders())
            || (self.map == "pandemonium" && !i.has_pandemonium_cinema())
            || (self.key() == "skool1$return" && !i.has_school_return())
            || self.registry_upgrade()
    }
    /// A registered controller now serves this visit that the save has no state for (F2).
    fn registry_upgrade(&self) -> bool {
        self.interactions
            .registry_upgrade(&self.map, self.entry.as_deref())
    }
    /// Rebuild a cached visit's logic in the current format. The fortress cast is regenerated
    /// from its models, which needs a window: window-free callers pass `regenerate_cast: false`.
    fn upgrade(
        &mut self,
        assets: &mut Assets,
        difficulty: crate::powerups::Difficulty,
        regenerate_cast: bool,
    ) -> Result<()> {
        let mut map = Bsp::parse(&assets.read(&format!("maps/{}.bsp", self.map))?)?;
        map.difficulty = difficulty;
        let registry = self.registry_upgrade();
        let school2 = self.map == "skool2" && !self.interactions.has_encounters();
        let (mut i, _, _) = self.restore_logic(assets, &map)?;
        if school2 {
            // Keep legacy decorative slots so saved Club hit/loot identities do
            // not shift. The rebuilt NPC owner suppresses Diamonds everywhere.
            self.npcs
                .matches_placements(&map, "skool2", self.entry.as_deref())?;
            if regenerate_cast {
                let fresh =
                    Npcs::load(assets, &map, &self.map, self.entry.as_deref(), false, false)?
                        .snapshot();
                self.npcs = self.npcs.adopt(fresh, &[]);
            }
        }
        if self.map == "pandemonium" {
            if let Some(c) = &mut i.encounters {
                self.npcs.import_guards(c);
            }
        }
        if regenerate_cast && matches!(self.map.as_str(), "fortress1" | "fortress2") {
            self.npcs = Npcs::load(assets, &map, &self.map, self.entry.as_deref(), false, false)?
                .snapshot();
        }
        // F2 step 7: a controller that took actors over changes who the generic cast simulates.
        // The saved cast is regenerated from the models and keeps the state of every actor the
        // controller does not own.
        if regenerate_cast && registry {
            let serving =
                crate::levels::serving(&self.map, self.entry.as_deref()).collect::<Vec<_>>();
            let entry = self.entry.clone();
            if let Some(cast) = adopt_cast(&self.npcs, &serving, || {
                Ok(Npcs::load(assets, &map, &self.map, entry.as_deref(), false, false)?.snapshot())
            })? {
                self.npcs = cast;
            }
        }
        self.interactions = i.snapshot();
        Ok(())
    }
}

/// F2 step 7: the saved cast after registered controllers took actors over, or `None` when they
/// own none of it and the saved cast stands. `fresh` loads the cast a new visit spawns (it needs
/// the NPC models, so it is only called when something changed).
fn adopt_cast(
    saved: &crate::npc::Snapshot,
    serving: &[&'static crate::levels::Registration],
    fresh: impl FnOnce() -> Result<crate::npc::Snapshot>,
) -> Result<Option<crate::npc::Snapshot>> {
    if !saved.owned_by(serving) {
        return Ok(None);
    }
    Ok(Some(saved.adopt(fresh()?, serving)))
}

/// Visits whose older saves start at the level entrance, because their movement was rebuilt.
#[derive(Clone, Copy)]
struct Migrations {
    pool: bool,
    beyond: bool,
    fortress: bool,
    duchess: bool,
    /// A registered controller joined the visit after the save was made (F2).
    levels: bool,
}
/// What a restore needs from the map's logic and collision alone: no window, model or texture.
pub struct Logic {
    pub interactions: Interactions,
    pub story: Story,
    pub hints: Hints,
}
/// Validation and cached-visit upgrades shared by `Restored::build` and `rebuild_headless`.
/// Returns the current visit and the player-moving migrations that apply to it.
fn prepare(
    assets: &mut Assets,
    game: &mut Game,
    regenerate_cast: bool,
) -> Result<(Level, Migrations)> {
    game.validate()?;
    let migrate = Migrations {
        pool: game.level()?.map == "potears1" && !game.level()?.interactions.has_pool(),
        beyond: game.level()?.map == "fortress2" && !game.level()?.interactions.has_beyond(),
        fortress: game.level()?.map == "fortress1" && !game.level()?.interactions.has_fortress(),
        duchess: game.level()?.map == "potears3" && !game.level()?.interactions.has_duchess(),
        levels: game.level()?.registry_upgrade(),
    };
    // Upgrade every cached visit, including newly supported encounter ownership.
    let difficulty = game.stats.difficulty;
    for level in game
        .campaign
        .levels
        .values_mut()
        .filter(|l| l.needs_upgrade())
    {
        level.upgrade(assets, difficulty, regenerate_cast)?;
    }
    let level = game.level()?.clone();
    let maps = assets.maps();
    ensure!(
        game.campaign.levels.values().all(|l| maps.contains(&l.map)),
        "A saved campaign level is unavailable"
    );
    Ok((level, migrate))
}
/// Restore the current visit's logic against its map and collision world, then validate and
/// (for migrated visits) reset the player. Shared by `Restored::build` and `rebuild_headless`.
fn resume(
    assets: &mut Assets,
    game: &mut Game,
    level: &Level,
    map: &Bsp,
    world: &mut World,
    migrate: Migrations,
) -> Result<Logic> {
    let (mut interactions, story, hints) = level.restore_logic(assets, map)?;
    for slot in &mut interactions.levels { slot.ctl.restore_position(&game.player, map)?; }
    settle(
        game,
        level.entry.as_deref(),
        &interactions,
        map,
        world,
        migrate,
    )?;
    Ok(Logic {
        interactions,
        story,
        hints,
    })
}
/// Put Alice back in a restored visit: sync the world, validate her, move her to the entrance
/// where a migration or an upgrade says so, and refuse a position that cannot stand.
fn settle(
    game: &mut Game,
    entry: Option<&str>,
    interactions: &Interactions,
    map: &Bsp,
    world: &mut World,
    migrate: Migrations,
) -> Result<()> {
    interactions.sync(world);
    game.stats.prepare_player(&mut game.player);
    game.player.validate_world(world)?;
    // F2 step 6: a visit a controller joined restarts at the entrance instead of failing when the
    // controller rejects the saved position or her body is not clear there.
    let joined =
        migrate.levels && interactions.levels_respawn(&game.player, world, game.view.flying);
    if migrate.beyond
        || migrate.pool
        || (migrate.duchess && !world.body_clear(game.player.feet))
        || (migrate.fortress && (game.player.feet.x > 2000. || !world.body_clear(game.player.feet)))
        || joined
    {
        let (eye, yaw) = crate::interaction::spawn(map, entry);
        game.player = Player::spawn(world, eye).context("Migrated level entrance obstructed")?;
        game.stats.prepare_player(&mut game.player);
        game.view.position = game.player.eye();
        game.view.yaw = yaw;
        game.recovery.clear();
    }
    if let Some(p) = &interactions.pandemonium {
        p.validate_player(&game.player)?;
    }
    interactions.validate_levels(&game.player)?;
    ensure!(
        game.view.flying
            || interactions
                .village
                .as_ref()
                .is_some_and(|v| v.cinema.active())
            || interactions.duchess.as_ref().is_some_and(|d| d.cinematic())
            || interactions
                .pandemonium
                .as_ref()
                .is_some_and(|p| p.cinematic())
            || interactions.school.as_ref().is_some_and(|s| s.cinematic())
            || interactions.levels_scripted()
            || world.body_clear(game.player.feet),
        "The saved player position is obstructed"
    );
    Ok(())
}
/// The window-free part of `Restored::build`, in the same order: validation, cached-visit
/// upgrades, then the current visit's logic against its collision world. Only the fortress
/// cast regeneration, the scene, NPC models, Alice and steam need a window and are left out.
pub struct Headless {
    pub game: Game,
    pub level: Level,
    pub map: Bsp,
    pub world: World,
    pub logic: Logic,
}
pub fn rebuild_headless(assets: &mut Assets, mut game: Game) -> Result<Headless> {
    let (level, migrate) = prepare(assets, &mut game, false)?;
    let mut map = Bsp::parse(&assets.read(&format!("maps/{}.bsp", level.map))?)?;
    // `Scene::load` builds its world before the saved difficulty is applied to the map.
    let mut world = World::from_bsp(&map)?;
    map.difficulty = game.stats.difficulty;
    let logic = resume(assets, &mut game, &level, &map, &mut world, migrate)?;
    Ok(Headless {
        game,
        level,
        map,
        world,
        logic,
    })
}
impl Restored {
    pub fn build(assets: &mut Assets, game: Game) -> Result<Self> {
        crate::loading_screen::run_ready(Self::build_async(assets, game))
    }
    /// `build` with its stage boundaries open to a loading frame.
    pub async fn build_async(assets: &mut Assets, mut game: Game) -> Result<Self> {
        let mut timing = crate::startup::Phase::new("save_restore");
        let (level, migrate) = prepare(assets, &mut game, true)?;
        timing.mark("prepare_migrate");
        crate::transition_memory::fault("prepare")?;
        let mut scene = Scene::load(assets, &level.map)?;
        timing.mark("scene_ready");
        crate::loading_screen::between("scene_ready").await;
        crate::transition_memory::fault("scene")?;
        scene.map.difficulty = game.stats.difficulty;
        let Logic {
            interactions,
            story,
            hints,
        } = resume(
            assets,
            &mut game,
            &level,
            &scene.map,
            &mut scene.world,
            migrate,
        )?;
        timing.mark("logic_story_hints");
        crate::loading_screen::between("logic_story_hints").await;
        crate::transition_memory::fault("logic")?;
        let mut npcs = Npcs::load(
            assets,
            &scene.map,
            &level.map,
            level.entry.as_deref(),
            false,
            false,
        )?;
        npcs.restore(&level.npcs)?;
        timing.mark("npc_models_restore");
        crate::loading_screen::between("npc_models_restore").await;
        crate::transition_memory::fault("npcs")?;
        let mut alice = Character::load(assets)?;
        alice.restore(&game.character)?;
        scene.world.set_weapon_obstacles(alice.ice_targets());
        alice.power_appearance(&game.stats);
        timing.mark("alice_models_restore");
        crate::loading_screen::between("alice_models_restore").await;
        crate::transition_memory::fault("alice")?;
        let steam = crate::particles::Steam::load(assets, &scene.map)?;
        timing.mark("steam_textures");
        crate::transition_memory::fault("steam")?;
        Ok(Self {
            scene,
            interactions,
            story,
            hints,
            npcs,
            alice,
            game,
            steam,
        })
    }
}

#[cfg(test)]
mod format12;
#[cfg(test)]
mod tests {
    use super::*;
    /// Pin the envelope clock on this thread so byte-for-byte comparisons of the frame
    /// path and the background path do not depend on the wall. The suite runs one thread.
    struct Pinned(u64);
    impl Pinned {
        fn new(at: u64) -> Self {
            TEST_SAVED_AT.with(|c| c.set(Some(at)));
            Self(at)
        }
    }
    impl Drop for Pinned {
        fn drop(&mut self) {
            TEST_SAVED_AT.with(|c| c.set(None));
        }
    }
    fn jlair2_game() -> Game {
        format12::game(vec![format12::level("jlair2", None, serde_json::Value::Null)])
    }
    fn finale_file(root: &std::path::Path) -> PathBuf {
        root.join("finale-checkpoint").join("auto.json")
    }
    #[test]
    fn integrity_checked_decode_rejects_a_missing_current_level() {
        let scratch = format12::Scratch::new("missing-current-load");
        let store = scratch.store();
        let game = format12::game(vec![format12::level("gvillage", None, serde_json::Value::Null)]);
        store.write(Slot::Quick, &game).unwrap();
        let path = scratch.file(Slot::Quick);
        let mut envelope: Envelope = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        envelope.payload["current"] = "missing-current-fixture".into();
        envelope.checksum = format!("{:x}", Sha256::digest(serde_json::to_vec(&envelope.payload).unwrap()));
        fs::write(&path, serde_json::to_vec(&envelope).unwrap()).unwrap();
        let error = store.read(Slot::Quick).err().expect("invalid save must be refused");
        assert!(format!("{error:#}").contains("Saved current level is missing"));
    }
    #[test]
    fn throttled_capture_for_save_persists_the_latest_decodable_menu_thumbnail() {
        let scratch = format12::Scratch::new("throttled-preview");
        let store = scratch.store();
        let game = format12::game(vec![format12::level("gvillage", None, serde_json::Value::Null)]);
        let mut throttle = crate::frame_profile::PreviewThrottle::default();
        let mut preview = None;
        for frame in 0..38 {
            throttle.advance_frame();
            let required = frame == 37; // save/menu between periodic captures
            if throttle.capture_requested(crate::frame_profile::Mode::Full, true, required) {
                preview = Some(crate::save_preview::Preview::fixture(if required { macroquad::prelude::Color::new(1., 0., 0., 1.) } else { macroquad::prelude::Color::new(0., 0., 1., 1.) }));
            }
            if required {
                store.write_with_preview(Slot::Quick, &game, preview.as_ref()).unwrap();
                store.write_with_preview(Slot::One, &game, preview.as_ref()).unwrap();
                store.write_finale_checkpoint_with_preview(&game, preview.as_ref()).unwrap();
            }
        }
        assert!(store.finale_store().read(Slot::Auto).unwrap().preview.is_some());
        for slot in [Slot::Quick, Slot::One, Slot::Auto] {
            let loaded = store.read(slot).unwrap();
            let menu_image = loaded.preview.unwrap().image().unwrap();
            assert_eq!(menu_image.dimensions(), (320, 240));
            assert!(menu_image.get_pixel(160, 120)[0] > 240);
            assert!(menu_image.get_pixel(160, 120)[2] < 10);
        }
    }
    #[test]
    fn bench_auto_reads_only_the_reference_without_modifying_it_or_falling_back() {
        let scratch = format12::Scratch::new("bench-auto");
        let store = scratch.store();
        assert!(store.read_bench_auto().is_err());
        let game = format12::game(vec![format12::level("gvillage", None, serde_json::Value::Null)]);
        store.write(Slot::Auto, &game).unwrap();
        let path = scratch.file(Slot::Auto);
        let reference = fs::read(&path).unwrap();
        let loaded = store.read_bench_auto().unwrap();
        assert_eq!(loaded.game.current, "gvillage$first");
        assert!(!loaded.backup);
        assert_eq!(fs::read(&path).unwrap(), reference);
        fs::write(store.path(Slot::Auto, true), &reference).unwrap();
        fs::remove_file(&path).unwrap();
        assert!(store.read_bench_auto().is_err());
        assert!(store.read(Slot::Auto).unwrap().backup);
        fs::write(&path, b"invalid json").unwrap();
        assert!(store.read_bench_auto().is_err());
        assert!(store.read(Slot::Auto).unwrap().backup);
        assert_eq!(fs::read(&path).unwrap(), b"invalid json");
    }
    #[test]
    fn bench_startup_reuses_a_validated_scene_and_loads_when_the_auto_save_is_absent() {
        assert_eq!(bench_scene(false), BenchScene::LoadFresh);
        let scratch = format12::Scratch::new("bench-scene");
        let store = scratch.store();
        assert!(store.read_bench_auto().is_err());
        assert_eq!(
            bench_scene(store.read_bench_auto().is_ok()),
            BenchScene::LoadFresh
        );
        let game = format12::game(vec![format12::level("gvillage", None, serde_json::Value::Null)]);
        store.write(Slot::Auto, &game).unwrap();
        assert!(store.read_bench_auto().is_ok());
        assert_eq!(
            bench_scene(store.read_bench_auto().is_ok()),
            BenchScene::ReuseValidated
        );
    }
    #[test]
    fn manual_slots_have_separate_files_and_cannot_escape_save_directory() {
        let store = Store::new(PathBuf::from("test-saves"), "fixture".into());
        let paths = Slot::ALL.map(|slot| store.path(slot, false));
        assert_eq!(
            paths.iter().collect::<std::collections::HashSet<_>>().len(),
            6
        );
        for slot in Slot::ALL {
            assert_eq!(Slot::parse(slot.name()).unwrap(), slot);
            assert_eq!(
                store.path(slot, false).parent(),
                Some(Path::new("test-saves"))
            );
            assert_ne!(store.path(slot, false), store.path(slot, true));
        }
        for invalid in ["slot0", "slot5", "../quick", "C:\\save"] {
            assert!(Slot::parse(invalid).is_err());
        }
    }
    #[test]
    fn campaign_cache_unifies_first_entrances_without_overwriting_return_visits() {
        for (map, first, returning) in [
            ("skool1", "skool1_start1", "skool1_start2"),
            ("fortress1", "fortress1_start1", "fortress1_start2"),
            ("wforest", "wforest_start1", "wforest_start2"),
        ] {
            assert_eq!(visit_key(map, None), visit_key(map, Some(first)));
            assert_ne!(visit_key(map, None), visit_key(map, Some(returning)));
        }
        assert_ne!(visit_key("skool1", None), visit_key("skool2", None));
    }
    fn finish(pending: &mut Option<PendingCheckpoint>) -> Result<()> {
        PendingCheckpoint::finish(pending, true, &mut [], &mut Default::default()).unwrap()
    }

    // Pause an active pair immediately before auto, after the finale write committed.
    fn gated_writer(store: Store) -> (CheckpointWriter, Receiver<()>, SyncSender<()>) {
        use std::time::Duration;
        let (entered, active) = mpsc::sync_channel(1);
        let (release, proceed) = mpsc::sync_channel(1);
        let writer = CheckpointWriter::spawn(move |job| {
            store.write_checkpoint_pair(|destination, index| {
                if index == 1 {
                    entered.send(())?;
                    proceed.recv_timeout(Duration::from_secs(5))?;
                }
                destination.write_with_preview_saved_at(
                    Slot::Auto, &job.game, job.preview.as_ref(), SavedAt::At(job.saved_at[index]),
                )
            })
        }).unwrap();
        (writer, active, release)
    }

    struct Request(bool);
    impl crate::level::LevelController for Request {
        fn id(&self) -> &'static str { "checkpoint-fixture" }
        fn checkpoint_requested(&self) -> bool { self.0 }
        fn checkpoint_written(&mut self) { self.0 = false; }
        fn snapshot(&self) -> serde_json::Value { serde_json::json!({ "checkpoint": self.0 }) }
        fn checkpoint_retry_snapshot(&self) -> serde_json::Value {
            Request(false).snapshot()
        }
        fn as_any(&self) -> &dyn std::any::Any { self }
        fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
    }

    fn requesting_interactions() -> Interactions {
        let mut interactions = Interactions::empty();
        interactions.levels.push(crate::level::Slot {
            reg: crate::levels::LEVELS[0],
            ctl: Box::new(Request(true)),
        });
        interactions
    }

    #[test]
    fn finale_checkpoint_background_writes_identical_bytes_to_the_frame_path() {
        let _pin = Pinned::new(1_795_031_414_000);
        let plain = format12::Scratch::new("checkpoint-frame");
        let background = format12::Scratch::new("checkpoint-background");
        let game = jlair2_game();
        let preview = crate::save_preview::Preview::fixture(RED);
        plain.store().write_finale_checkpoint_with_preview(&game, Some(&preview)).unwrap();
        let writer = CheckpointWriter::start(&background.store()).unwrap();
        let mut pending = Some(writer.submit(game.clone(), Some(preview), game).unwrap());
        finish(&mut pending).unwrap();
        assert_eq!(fs::read(finale_file(&plain.0)).unwrap(), fs::read(finale_file(&background.0)).unwrap());
        assert_eq!(fs::read(plain.file(Slot::Auto)).unwrap(), fs::read(background.file(Slot::Auto)).unwrap());
        assert!(background.store().read(Slot::Auto).unwrap().preview.is_some());
    }

    #[test]
    fn an_ordinary_auto_save_is_unchanged_with_the_background_writer_installed() {
        let _pin = Pinned::new(1_795_031_415_000);
        let plain = format12::Scratch::new("checkpoint-ordinary-plain");
        let background = format12::Scratch::new("checkpoint-ordinary-background");
        let game = jlair2_game();
        let store = background.store();
        let _writer = CheckpointWriter::start(&store).unwrap();
        plain.store().write(Slot::Auto, &game).unwrap();
        store.write(Slot::Auto, &game).unwrap();
        assert_eq!(fs::read(plain.file(Slot::Auto)).unwrap(), fs::read(background.file(Slot::Auto)).unwrap());
    }

    #[test]
    fn pending_checkpoint_acknowledges_both_commits_and_restores_the_request_frame_on_retry() {
        use std::time::Duration;
        let scratch = format12::Scratch::new("checkpoint-ack");
        let mut interactions = requesting_interactions();
        let game = jlair2_game();
        let mut request_frame = game.clone();
        request_frame.view.yaw = 1.25;
        request_frame.campaign.levels.get_mut(&game.current).unwrap().interactions =
            interactions.snapshot_for_checkpoint(true);
        assert!(interactions.levels[0].ctl.checkpoint_requested());
        let mut foreground = requesting_interactions();
        foreground.levels[0].ctl.checkpoint_written();
        assert_eq!(
            serde_json::to_value(&request_frame.level().unwrap().interactions).unwrap(),
            serde_json::to_value(foreground.snapshot()).unwrap(),
        );

        let (writer, active, release) = gated_writer(scratch.store());
        let mut retry = crate::recovery::RetryPoint::default();
        retry.remember(game.clone());
        let mut pending = Some(writer.submit(game.clone(), None, request_frame).unwrap());
        active.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(finale_file(&scratch.0).is_file());
        assert!(!scratch.file(Slot::Auto).exists());
        assert!(PendingCheckpoint::finish(&mut pending, false, &mut interactions.levels, &mut retry).is_none());
        assert!(interactions.levels[0].ctl.checkpoint_requested());
        assert_eq!(retry.game_for(&game.current).unwrap().view.yaw, 0.);

        thread::scope(|scope| {
            let (returned, early_return) = mpsc::sync_channel(1);
            scope.spawn(move || {
                assert!(matches!(
                    early_return.recv_timeout(Duration::from_millis(30)),
                    Err(mpsc::RecvTimeoutError::Timeout)
                ));
                release.send(()).unwrap();
            });
            PendingCheckpoint::finish(&mut pending, true, &mut interactions.levels, &mut retry)
                .unwrap().unwrap();
            returned.send(()).ok();
        });
        let restored = retry.game_for(&game.current).unwrap();
        assert_eq!(restored.view.yaw, 1.25);
        assert_eq!(
            serde_json::to_value(&restored.level().unwrap().interactions).unwrap(),
            serde_json::to_value(foreground.snapshot()).unwrap(),
        );
        assert!(scratch.file(Slot::Auto).is_file());
        assert!(!interactions.levels[0].ctl.checkpoint_requested());
        assert!(pending.is_none());
    }

    #[test]
    fn finale_failure_keeps_auto_and_its_backup_untouched_and_the_request_set() {
        let scratch = format12::Scratch::new("checkpoint-failure");
        let store = scratch.store();
        let game = jlair2_game();
        store.write(Slot::Auto, &game).unwrap();
        let primary = fs::read(scratch.file(Slot::Auto)).unwrap();
        let backup = scratch.0.join("auto.previous.json");
        fs::write(&backup, b"previous").unwrap();
        fs::write(store.finale_store().directory, b"blocks creation of the directory").unwrap();
        let writer = CheckpointWriter::start(&store).unwrap();
        let mut retry = crate::recovery::RetryPoint::default();
        retry.remember(game.clone());
        let mut next = game.clone();
        next.view.yaw = 1.;
        let mut pending = Some(writer.submit(next.clone(), None, next).unwrap());
        let mut interactions = requesting_interactions();
        let error = PendingCheckpoint::finish(&mut pending, true, &mut interactions.levels, &mut retry)
            .unwrap().unwrap_err();
        assert!(format!("{error:#}").contains("Cannot create save folder"));
        assert!(interactions.levels[0].ctl.checkpoint_requested());
        assert_eq!(retry.game_for(&game.current).unwrap().view.yaw, 0.);
        assert_eq!(fs::read(scratch.file(Slot::Auto)).unwrap(), primary);
        assert_eq!(fs::read(backup).unwrap(), b"previous");
    }

    #[test]
    fn second_request_waits_and_preserves_both_stores_backup_order() {
        use std::time::Duration;
        let scratch = format12::Scratch::new("checkpoint-order");
        let store = scratch.store();
        let (writer, active, release) = gated_writer(scratch.store());
        let one = jlair2_game();
        let mut two = one.clone();
        two.stats.damage(5.);
        let mut first = Some(writer.submit(one.clone(), None, one.clone()).unwrap());
        active.recv_timeout(Duration::from_secs(5)).unwrap();
        thread::scope(|scope| {
            let (starting, started) = mpsc::sync_channel(1);
            let (submitted, pending) = mpsc::sync_channel(1);
            let expected = two.stats.sanity();
            scope.spawn(move || {
                starting.send(()).unwrap();
                let second = writer.submit(two.clone(), None, two).unwrap();
                submitted.send(second).ok();
            });
            started.recv_timeout(Duration::from_secs(5)).unwrap();
            assert!(pending.recv_timeout(Duration::from_millis(30)).is_err());
            release.send(()).unwrap();
            let mut second = Some(pending.recv_timeout(Duration::from_secs(5)).unwrap());
            active.recv_timeout(Duration::from_secs(5)).unwrap();
            finish(&mut first).unwrap();
            assert_eq!(store.read(Slot::Auto).unwrap().game.stats.sanity(), one.stats.sanity());
            release.send(()).unwrap();
            finish(&mut second).unwrap();
            for destination in [&store, &store.finale_store()] {
                assert_eq!(destination.read(Slot::Auto).unwrap().game.stats.sanity(), expected);
                let backup = fs::read(destination.path(Slot::Auto, true)).unwrap();
                assert_eq!(destination.decode(&backup).unwrap().game.stats.sanity(), one.stats.sanity());
            }
        });
    }

    #[test]
    fn load_and_manual_save_follow_the_checkpoint_flush() {
        use std::time::Duration;
        let scratch = format12::Scratch::new("checkpoint-load-and-save");
        let store = scratch.store();
        let initial = jlair2_game();
        store.write(Slot::Auto, &initial).unwrap();
        let mut checkpoint = initial.clone();
        checkpoint.stats.damage(5.);
        let mut saved = checkpoint.clone();
        saved.stats.damage(5.);
        let (writer, active, release) = gated_writer(scratch.store());
        let mut pending = Some(writer.submit(checkpoint.clone(), None, checkpoint.clone()).unwrap());
        active.recv_timeout(Duration::from_secs(5)).unwrap();
        thread::scope(|scope| {
            let (starting, started) = mpsc::sync_channel(1);
            let (result, completed) = mpsc::sync_channel(1);
            let (pending, store, saved) = (&mut pending, &store, &saved);
            scope.spawn(move || {
                starting.send(()).unwrap();
                finish(pending).unwrap();
                let loaded = store.read(Slot::Auto).unwrap();
                store.write(Slot::Auto, saved).unwrap();
                result.send(loaded.game.stats.sanity()).unwrap();
            });
            started.recv_timeout(Duration::from_secs(5)).unwrap();
            assert!(completed.recv_timeout(Duration::from_millis(30)).is_err());
            release.send(()).unwrap();
            assert_eq!(completed.recv_timeout(Duration::from_secs(5)).unwrap(), checkpoint.stats.sanity());
        });
        assert_eq!(store.read(Slot::Auto).unwrap().game.stats.sanity(), saved.stats.sanity());
        let backup = fs::read(store.path(Slot::Auto, true)).unwrap();
        assert_eq!(store.decode(&backup).unwrap().game.stats.sanity(), checkpoint.stats.sanity());
    }

    #[test]
    fn exit_flushes_an_active_checkpoint_before_quitting() {
        use std::time::Duration;
        let scratch = format12::Scratch::new("checkpoint-flush");
        let (writer, active, release) = gated_writer(scratch.store());
        let game = jlair2_game();
        let mut pending = Some(writer.submit(game.clone(), None, game).unwrap());
        active.recv_timeout(Duration::from_secs(5)).unwrap();
        let (starting, started) = mpsc::sync_channel(1);
        let (result, exited) = mpsc::sync_channel(1);
        let exit = thread::spawn(move || {
            starting.send(()).unwrap();
            let completion = finish(&mut pending);
            drop(writer);
            result.send(completion).unwrap();
        });
        started.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(exited.recv_timeout(Duration::from_millis(30)).is_err());
        release.send(()).unwrap();
        exited.recv_timeout(Duration::from_secs(5)).unwrap().unwrap();
        exit.join().unwrap();
        assert!(finale_file(&scratch.0).is_file());
        assert!(scratch.file(Slot::Auto).is_file());
    }

    #[test]
    fn the_discarding_store_still_writes_nothing_in_both_arms() {
        let scratch = format12::Scratch::new("checkpoint-discard");
        let game = jlair2_game();
        let store = scratch.store().discarding_writes();
        store.write_finale_checkpoint_with_preview(&game, None).unwrap();
        let writer = CheckpointWriter::start(&store).unwrap();
        let mut pending = Some(writer.submit(game.clone(), None, game).unwrap());
        finish(&mut pending).unwrap();
        assert!(!scratch.file(Slot::Auto).exists());
        assert!(!finale_file(&scratch.0).exists());
    }

    #[cfg(not(target_os = "vita"))]
    #[test]
    fn replay_write_opt_in_requires_one_and_a_fresh_explicit_directory() {
        use std::ffi::OsStr;
        let scratch = format12::Scratch::new("checkpoint-replay-writes");
        let player = scratch.0.join("default-saves");
        let isolated = scratch.0.join("isolated");
        for value in [None, Some(OsStr::new("0")), Some(OsStr::new("true")), Some(OsStr::new(""))] {
            assert!(isolated_replay_directory(value, Some(&isolated), &player).is_none());
        }
        let enabled = Some(OsStr::new("1"));
        assert!(isolated_replay_directory(enabled, None, &player).is_none());
        assert!(isolated_replay_directory(enabled, Some(&player), &player).is_none());
        let case_alias = scratch.0.join("DEFAULT-SAVES");
        assert!(isolated_replay_directory(enabled, Some(&case_alias), &player).is_none());
        assert!(!player.exists());
        #[cfg(unix)]
        {
            let alias = scratch.0.join("alias");
            std::os::unix::fs::symlink(&scratch.0, &alias).unwrap();
            assert!(isolated_replay_directory(enabled, Some(&alias.join("default-saves")), &player).is_none());
            let link = scratch.0.join("link");
            std::os::unix::fs::symlink(&player, &link).unwrap();
            assert!(isolated_replay_directory(enabled, Some(&link), &player).is_none());
        }
        assert_eq!(
            isolated_replay_directory(enabled, Some(&isolated), &player),
            Some(scratch.0.canonicalize().unwrap().join("isolated")),
        );
        assert!(isolated_replay_directory(enabled, Some(&isolated), &player).is_none());
    }
}

/// Serialize the real format-12 save types/checksum with a fixed test timestamp.
/// This keeps simulation equivalence tests independent of wall or frame time.
#[cfg(test)]
pub(crate) fn simulation_save_bytes(player: &Player, village: crate::village::Snapshot) -> Vec<u8> {
    let mut level = format12::level("gvillage", None, serde_json::Value::Null);
    let mut interactions = serde_json::to_value(&level.interactions).unwrap();
    interactions["village"] = serde_json::to_value(village).unwrap();
    level.interactions = serde_json::from_value(interactions).unwrap();
    let mut game = format12::game(vec![level]);
    game.player = player.clone();
    game.view.position = player.eye();
    game.validate().unwrap();
    let payload = serde_json::to_value(&game).unwrap();
    validate_json(&payload).unwrap();
    require_format_for_levels(VERSION, &payload).unwrap();
    let checksum = format!("{:x}", Sha256::digest(serde_json::to_vec(&payload).unwrap()));
    serde_json::to_vec(&Envelope {
        format: "looking-glass-save".into(), version: VERSION,
        game_data: "synthetic-game-data".into(), saved_at: 0,
        checksum, payload, preview: serde_json::Value::Null,
    }).unwrap()


}

#[cfg(all(test,feature="native_timing"))]
mod native_isolation_tests {
    use super::*;
    #[cfg(feature="native_timing")]
    #[test]
    fn actual_failed_save_attempt_is_visible_to_native_isolation_guard() {
        crate::native_timing::operations::enable();
        let mut game=format12::game(vec![format12::level("gvillage",None,serde_json::Value::Null)]);
        game.current="missing-visit".into();
        let before=crate::native_timing::operations::snapshot().0;
        let store=Store::new(PathBuf::from("unused-invalid-save-must-not-be-created"),"fixture".into());
        assert!(store.write(Slot::Auto,&game).is_err());
        assert_eq!(crate::native_timing::operations::snapshot().0,before+1);
        assert!(!store.directory.exists());
    }
}
