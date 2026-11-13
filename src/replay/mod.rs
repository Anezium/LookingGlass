//! Record a human play and replay it as a benchmark.
//!
//! The world advances once per viewer frame by that frame's time (clamped to 50 ms); Alice's body
//! advances in fixed 1/120 s ticks from the same unclamped time. Random numbers come from per-actor
//! generators with fixed seeds. So a step is exact when it sees the same frame time and the same
//! input reads. A recording stores both for every step (`input::Frame`); a replay installs them
//! instead of the pad and the clock, so the simulation follows the recording at any render speed.
//!
//! Both runs start from the same place: a save slot, whose game is copied into the tape, or a
//! chapter start. Every `check_every` steps both runs log a check value (player position and
//! velocity, health, will, weapon, physics ticks, environment clock, each live hostile's place);
//! a recording stores it, a replay compares with it and reports the first step that differs.
pub mod input;
pub mod tape;

use crate::save::{Game, Loaded, Slot, Store};
use anyhow::{bail, ensure, Context, Result};
use macroquad::prelude::Vec3;
use sha2::{Digest, Sha256};

/// Runtime keys from vita_renderer.json.
#[derive(Clone, Debug, Default)]
pub struct Settings {
    pub record: String,
    pub play: String,
    pub start: String,
    pub check_every: u32,
    pub state_every: u32,
    pub stop_after: u32,
}
fn settings() -> Settings {
    #[cfg(feature = "vita_renderer")]
    {
        let c = crate::render::vita_renderer::config();
        Settings { record: c.replay_record.clone(), play: c.replay_play.clone(), start: c.replay_start.clone(),
            check_every: c.replay_check_every, state_every: c.replay_state_every, stop_after: c.replay_stop_after }
    }
    #[cfg(not(feature = "vita_renderer"))]
    { Settings::default() }
}
fn root() -> std::path::PathBuf {
    #[cfg(target_os = "vita")]
    { std::path::PathBuf::from("ux0:data/alice/private") }
    #[cfg(not(target_os = "vita"))]
    { std::path::PathBuf::from("private") }
}

/// A tape file name: letters, digits, '.', '_', '-', ending in ".lgtape". A recording name may hold
/// one '#', replaced by the first free number 01..99.
pub fn valid_name(name: &str, record: bool) -> bool {
    name.len() <= 64 && name.ends_with(".lgtape") && name.len() > ".lgtape".len()
        && name.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-') || (record && b == b'#'))
        && name.bytes().filter(|b| *b == b'#').count() <= 1
}
/// `save:<slot>` or `chapter:<map>[$<entrance>][@<difficulty>]`.
pub fn parse_start(spec: &str) -> Result<tape::Start> {
    if let Some(slot) = spec.strip_prefix("save:") {
        Slot::parse(slot)?;
        return Ok(tape::Start { kind: "save".into(), slot: Some(slot.into()), map: String::new(), entry: None,
            difficulty: String::new(), game_sha256: None });
    }
    let Some(rest) = spec.strip_prefix("chapter:") else { bail!("replay_start must be save:<slot> or chapter:<map>") };
    let (rest, difficulty) = rest.split_once('@').map_or((rest, None), |(a, b)| (a, Some(b)));
    let (map, entry) = rest.split_once('$').map_or((rest, None), |(a, b)| (a, Some(b.to_owned())));
    ensure!(!map.is_empty() && map.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_'), "bad chapter map {map:?}");
    ensure!(entry.as_deref().is_none_or(|e| !e.is_empty() && !e.contains(char::is_whitespace)), "bad chapter entrance");
    let difficulty = difficulty.map(crate::powerups::Difficulty::parse).transpose()?.unwrap_or_default();
    Ok(tape::Start { kind: "chapter".into(), slot: None, map: map.into(), entry, difficulty: difficulty.name().into(), game_sha256: None })
}
pub fn validate_settings(s: &Settings) -> Result<()> {
    ensure!(s.record.is_empty() || s.play.is_empty(), "replay_record and replay_play are exclusive");
    ensure!(s.record.is_empty() || valid_name(&s.record, true), "replay_record must be a file name ending in .lgtape");
    ensure!(s.play.is_empty() || valid_name(&s.play, false), "replay_play must be a file name ending in .lgtape");
    ensure!(s.record.is_empty() || !s.start.is_empty(), "replay_record needs replay_start");
    if !s.start.is_empty() { parse_start(&s.start)?; }
    ensure!((1..=3600).contains(&s.check_every), "replay_check_every must be 1..3600");
    ensure!(s.state_every <= 1_000_000, "replay_state_every must be 0..1000000");
    Ok(())
}

/// What a check covers. The viewer fills it only on steps that need a check.
pub struct Probe<'a> {
    pub map: &'a str,
    pub feet: Vec3,
    pub velocity: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub sanity: f32,
    pub will: f32,
    pub alive: bool,
    pub weapon: usize,
    pub ticks: u64,
    pub environment_clock: f32,
    pub targets: &'a [crate::combat::Target],
    /// Serialized full simulation state, when the step asks for it.
    pub state: Option<Vec<u8>>,
    /// Time spent gathering this probe, logged so the cost of checks on the console is known.
    pub cost_us: u64,
}
pub fn digest(p: &Probe) -> String {
    let mut h = Sha256::new();
    h.update((p.map.len() as u32).to_le_bytes());
    h.update(p.map.as_bytes());
    for v in p.feet.to_array().iter().chain(&p.velocity.to_array()).chain(&[p.yaw, p.pitch, p.sanity, p.will, p.environment_clock]) {
        h.update(v.to_bits().to_le_bytes());
    }
    h.update([u8::from(p.alive)]);
    h.update((p.weapon as u32).to_le_bytes());
    h.update(p.ticks.to_le_bytes());
    h.update((p.targets.len() as u32).to_le_bytes());
    for t in p.targets {
        h.update((t.id as u32).to_le_bytes());
        for v in t.center.to_array().iter().chain(&t.half.to_array()) { h.update(v.to_bits().to_le_bytes()); }
    }
    format!("{:x}", h.finalize())[..16].to_owned()
}
fn check_row(step: u64, p: &Probe) -> (tape::Check, u64) {
    let started = std::time::Instant::now();
    let c = check_value(step, p);
    (c, p.cost_us + started.elapsed().as_micros() as u64)
}
fn check_value(step: u64, p: &Probe) -> tape::Check {
    tape::Check {
        step,
        digest: digest(p),
        state: p.state.as_ref().map(|s| format!("{:x}", Sha256::digest(s))[..16].to_owned()),
        map: p.map.into(),
        feet: p.feet.to_array(),
        sanity: p.sanity,
        hostiles: p.targets.len(),
    }
}

pub enum Step {
    /// Run this step. `first` is the first step: reset per-run input state.
    Run { first: bool },
    /// The tape is exhausted or a recording reached its step limit: leave the loop.
    Finish,
    /// Recording ended earlier in this run; play on without a tape.
    Off,
}

enum Mode {
    Record { writer: Option<tape::Writer>, pending: Option<input::Frame>, synthetic: bool, aim: Option<(f32, f32)> },
    Replay { reader: Box<tape::Reader>, total: u64 },
}
pub struct Session {
    mode: Mode,
    name: String,
    settings: Settings,
    step: u64,
    resume: Option<Loaded>,
    last_map: Option<String>,
    last_alive: Option<bool>,
    events: u64,
    checks: u64,
    matched: u64,
    differed: u64,
    first_difference: Option<u64>,
    event_mismatches: u64,
    started: std::time::Instant,
    ended: bool,
}
fn configure(options: &mut crate::Options, start: &tape::Start) -> Result<()> {
    options.map = start.map.clone();
    options.entry = start.entry.clone();
    options.difficulty = crate::powerups::Difficulty::parse(&start.difficulty)?;
    options.direct_map = true;
    options.load = None;
    // A chapter start takes the menu's Chapter path; a save start resumes the game.
    options.new_game = start.kind == "chapter";
    Ok(())
}
fn free_name(pattern: &str) -> Result<String> {
    if !pattern.contains('#') { return Ok(pattern.into()); }
    (1..100).map(|n| pattern.replace('#', &format!("{n:02}")))
        .find(|name| !root().join(name).exists())
        .context("no free recording number 01..99")
}

impl Session {
    pub fn open(options: &mut crate::Options, store: &Store, preferences: &mut crate::preferences::Preferences) -> Result<Option<Self>> {
        let settings = settings();
        if settings.record.is_empty() && settings.play.is_empty() { return Ok(None); }
        validate_settings(&settings)?;
        let mut session = Self {
            mode: Mode::Record { writer: None, pending: None, synthetic: false, aim: None },
            name: String::new(), settings: settings.clone(), step: 0, resume: None, last_map: None, last_alive: None,
            events: 0, checks: 0, matched: 0, differed: 0, first_difference: None, event_mismatches: 0,
            started: std::time::Instant::now(), ended: false,
        };
        if !settings.play.is_empty() {
            let path = root().join(&settings.play);
            let mut reader = tape::Reader::open(&path).with_context(|| format!("replay {}", path.display()))?;
            let h = reader.tape.header.clone();
            ensure!(h.platform == tape::platform(), "tape recorded on {} cannot replay on {}", h.platform, tape::platform());
            // The recorded control settings decide how sticks turn into motion. In memory only.
            *preferences = serde_json::from_value(h.preferences.clone()).context("tape preferences")?;
            if let Some(bytes) = reader.take_game() {
                let game: Game = serde_json::from_slice(&bytes).context("tape start game")?;
                session.resume = Some(Loaded { game, saved_at: 0, backup: false, preview: None });
            }
            let mut start = h.start.clone();
            if let Some(l) = &session.resume {
                let level = l.game.level()?;
                start.map = level.map.clone();
                start.entry = level.entry.clone();
                start.difficulty = l.game.stats.difficulty.name().into();
            }
            configure(options, &start)?;
            println!("[lg/replay] play {} steps={} start={}:{} recorded_on={} package={} checks_every={} ended={} truncated={}",
                settings.play, reader.tape.frames, h.start.kind, h.start.slot.as_deref().unwrap_or(&start.map),
                h.platform, h.package, h.check_every, reader.tape.ended.is_some(), reader.tape.truncated);
            for (step, names) in &reader.tape.marks { for n in names { println!("[lg/replay] marker planned step={step} {n}"); } }
            for line in config_differences("renderer", &h.renderer, &current_renderer())
                .chain(config_differences("sim", &h.sim, &serde_json::to_value(crate::vita_sim::effective())?)) {
                println!("{line}");
            }
            session.name = settings.play.clone();
            session.mode = Mode::Replay { total: reader.tape.frames, reader: Box::new(reader) };
            return Ok(Some(session));
        }
        let mut start = parse_start(&settings.start)?;
        let mut game_bytes = None;
        if let Some(slot) = &start.slot {
            let mut loaded = store.read(Slot::parse(slot)?).with_context(|| format!("replay_start {}", settings.start))?;
            // Build the scene from the exact bytes the tape stores, as the replay will.
            let bytes = serde_json::to_vec(&loaded.game)?;
            loaded.game = serde_json::from_slice(&bytes)?;
            let level = loaded.game.level()?;
            start.map = level.map.clone();
            start.entry = level.entry.clone();
            start.difficulty = loaded.game.stats.difficulty.name().into();
            start.game_sha256 = Some(format!("{:x}", Sha256::digest(&bytes)));
            game_bytes = Some(bytes);
            loaded.backup = false;
            session.resume = Some(loaded);
        }
        configure(options, &start)?;
        let name = free_name(&settings.record)?;
        let path = root().join(&name);
        let header = tape::Header {
            version: tape::VERSION,
            platform: tape::platform().into(),
            created_unix_ms: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_millis() as u64,
            package: env!("CARGO_PKG_VERSION").into(),
            keys: input::key_codes(),
            check_every: settings.check_every,
            state_every: settings.state_every,
            start: start.clone(),
            preferences: serde_json::to_value(&*preferences)?,
            renderer: current_renderer(),
            sim: serde_json::to_value(crate::vita_sim::effective())?,
        };
        let writer = tape::Writer::create(&path, &header, game_bytes.as_deref())?;
        #[cfg(not(target_os = "vita"))]
        let synthetic = std::env::var_os("LOOKING_GLASS_REPLAY_SYNTH").is_some();
        #[cfg(target_os = "vita")]
        let synthetic = false;
        println!("[lg/replay] record {name} start={} map={} entry={:?} difficulty={} synthetic={synthetic}",
            settings.start, start.map, start.entry, start.difficulty);
        session.name = name;
        session.mode = Mode::Record { writer: Some(writer), pending: None, synthetic, aim: None };
        Ok(Some(session))
    }
    pub fn take_resume(&mut self) -> Option<Loaded> { self.resume.take() }
    /// A synthetic host recording steers toward the nearest hostile; see input::synthetic.
    pub fn wants_aim(&self) -> bool { !self.ended && matches!(self.mode, Mode::Record { synthetic: true, .. }) }
    pub fn aim(&mut self, turn_and_distance: Option<(f32, f32)>) {
        if let Mode::Record { aim, .. } = &mut self.mode { *aim = turn_and_distance; }
    }
    pub fn replaying(&self) -> bool { matches!(self.mode, Mode::Replay { .. }) }
    pub fn running(&self) -> bool { !self.ended }
    /// None: no check before this step. Some(true): a check with the full state hash.
    pub fn due(&self) -> Option<bool> {
        if self.ended { return None; }
        match &self.mode {
            Mode::Replay { reader, .. } => reader.next_check().filter(|c| c.step == self.step).map(|c| c.state.is_some()),
            Mode::Record { .. } => (self.step % u64::from(self.settings.check_every) == 0)
                .then(|| self.settings.state_every > 0 && self.step % u64::from(self.settings.state_every) == 0),
        }
    }
    /// Call at the top of every viewer frame, before any input is read.
    pub fn begin_step(&mut self, map: &str, alive: bool, probe: Option<Probe>) -> Result<Step> {
        if self.ended { return Ok(Step::Off); }
        let step = self.step;
        let mut events = Vec::new();
        if self.last_map.as_deref() != Some(map) {
            events.push(format!("map:{map}"));
            self.last_map = Some(map.into());
        }
        if self.last_alive.is_some_and(|a| a != alive) { events.push(if alive { "alive" } else { "death" }.to_owned()); }
        self.last_alive = Some(alive);
        let (check, cost_us) = probe.as_ref().map(|p| check_row(step, p)).unzip();
        let cost = cost_us.map_or_else(String::new, |us| format!(" cost_us={us}"));
        match &mut self.mode {
            Mode::Record { writer, pending, synthetic, aim } => {
                let w = writer.as_mut().context("recording writer closed")?;
                if let Some(mut f) = pending.take() {
                    f.pad = input::last_pad();
                    w.frame(&f)?;
                }
                if let Some(c) = &check {
                    w.check(c)?;
                    self.checks += 1;
                    println!("[lg/replay] check {}{cost}", check_text(c));
                }
                for e in &events {
                    w.event(step, e)?;
                    self.events += 1;
                    println!("[lg/replay] event step={step} {e}");
                }
                if self.settings.stop_after > 0 && step >= u64::from(self.settings.stop_after) {
                    return Ok(Step::Finish);
                }
                #[cfg(not(target_os = "vita"))]
                let f = if *synthetic {
                    let previous = w.previous_frame().clone();
                    input::synthetic(step, &previous, *aim)
                } else { input::capture_live() };
                #[cfg(target_os = "vita")]
                let f = { let _ = (synthetic, aim); input::capture_live() };
                if step == 0 { input::activate(false); }
                input::install(&f);
                *pending = Some(f);
            }
            Mode::Replay { reader, .. } => {
                let Some((recorded, recorded_events, frame)) = reader.step()? else { return Ok(Step::Finish); };
                match (recorded.first(), &check) {
                    (Some(r), Some(c)) => {
                        self.checks += 1;
                        let same = r.digest == c.digest && (r.state.is_none() || r.state == c.state);
                        if same { self.matched += 1; } else {
                            self.differed += 1;
                            if self.first_difference.is_none() {
                                self.first_difference = Some(step);
                                println!("[lg/replay] DIVERGED step={step} recorded: {} replay: {}", check_text(r), check_text(c));
                            }
                        }
                        println!("[lg/replay] check {} rec={}{cost}", check_text(c), if same { "match" } else { "DIFF" });
                    }
                    (Some(_), None) => bail!("replay step {step}: the tape has a check the viewer did not compute"),
                    (None, Some(c)) => println!("[lg/replay] check {} rec=-{cost}", check_text(c)),
                    (None, None) => {}
                }
                for e in &events { println!("[lg/replay] event step={step} {e}"); }
                self.events += events.len() as u64;
                if events != recorded_events {
                    self.event_mismatches += 1;
                    println!("[lg/replay] event mismatch step={step} recorded={recorded_events:?} replay={events:?}");
                }
                // The recording ended here: its last check covers the state after the last step.
                let Some(frame) = frame else { return Ok(Step::Finish); };
                if let Some(names) = reader.tape.marks.get(&step) {
                    for n in names { println!("[lg/replay] mark step={step} {n}"); }
                }
                if step == 0 { input::activate(true); }
                input::install(&frame);
            }
        }
        self.step += 1;
        Ok(Step::Run { first: step == 0 })
    }
    /// End now, inside a step: the step in progress is not part of the tape.
    pub fn stop(&mut self, reason: &str) -> Result<()> {
        if self.ended { return Ok(()); }
        self.ended = true;
        input::deactivate();
        let seconds = self.started.elapsed().as_secs_f64();
        // Steps begun minus the one in progress.
        let steps = self.step.saturating_sub(1);
        match &mut self.mode {
            Mode::Record { writer, pending, .. } => {
                pending.take();
                let w = writer.take().context("recording writer closed")?;
                let frames = w.frames;
                w.end(&serde_json::json!({"steps": frames, "reason": reason, "checks": self.checks,
                    "events": self.events, "seconds": seconds}))?;
                println!("[lg/replay] recorded {} steps={frames} reason={reason} checks={} events={} seconds={seconds:.1}",
                    self.name, self.checks, self.events);
            }
            Mode::Replay { total, .. } => {
                let complete = steps >= *total;
                let result = if self.differed > 0 || self.event_mismatches > 0 { "DIVERGED" }
                    else if !complete { "SHORT" } else { "PASS" };
                println!("[lg/replay] done {} steps={steps}/{total} reason={reason} checks={} match={} diff={} first_diff={} event_mismatches={} seconds={seconds:.1} result={result}",
                    self.name, self.checks, self.matched, self.differed,
                    self.first_difference.map_or_else(|| "-".into(), |s| s.to_string()), self.event_mismatches);
            }
        }
        Ok(())
    }
    /// End after the viewer loop: the tape ran out, the step limit was hit, or the game quit.
    pub fn finish(&mut self, reason: &str) -> Result<()> {
        // A clean end from begin_step left no step in progress.
        if reason == "tape-end" || reason == "step-limit" { self.step += 1; }
        self.stop(reason)
    }
}
fn check_text(c: &tape::Check) -> String {
    format!("step={} map={} feet={:.2},{:.2},{:.2} sanity={:.1} hostiles={} digest={}{}", c.step, c.map,
        c.feet[0], c.feet[1], c.feet[2], c.sanity, c.hostiles, c.digest,
        c.state.as_ref().map_or_else(String::new, |s| format!(" state={s}")))
}
fn current_renderer() -> serde_json::Value {
    #[cfg(feature = "vita_renderer")]
    { serde_json::to_value(crate::render::vita_renderer::config()).unwrap_or_default() }
    #[cfg(not(feature = "vita_renderer"))]
    { serde_json::Value::Null }
}
/// Keys whose value differs between the recording and this run. The replay keys themselves
/// always differ and are left out.
fn config_differences<'a>(what: &'a str, recorded: &'a serde_json::Value, now: &'a serde_json::Value) -> impl Iterator<Item = String> + 'a {
    let empty = serde_json::Map::new();
    let a = recorded.as_object().unwrap_or(&empty).clone();
    let b = now.as_object().unwrap_or(&empty).clone();
    let keys: std::collections::BTreeSet<String> = a.keys().chain(b.keys()).cloned().collect();
    keys.into_iter().filter(|k| !k.starts_with("replay_")).filter_map(move |k| {
        let (x, y) = (a.get(&k), b.get(&k));
        (x != y).then(|| format!("[lg/replay] config {what}.{k} recorded={} now={}",
            x.map_or_else(|| "absent".into(), ToString::to_string), y.map_or_else(|| "absent".into(), ToString::to_string)))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn names_and_starts() {
        assert!(valid_name("a2-fight.lgtape", false));
        assert!(valid_name("a2-#.lgtape", true));
        assert!(!valid_name("a2-#.lgtape", false));
        assert!(!valid_name("a2-##.lgtape", true));
        assert!(!valid_name("../x.lgtape", false));
        assert!(!valid_name("vita_renderer.json", false));
        assert!(!valid_name(".lgtape", false));
        let s = parse_start("chapter:potears2$riverbank@hard").unwrap();
        assert_eq!((s.map.as_str(), s.entry.as_deref(), s.difficulty.as_str()), ("potears2", Some("riverbank"), "Hard"));
        assert_eq!(parse_start("chapter:gvillage").unwrap().difficulty, "Normal");
        assert_eq!(parse_start("save:slot2").unwrap().slot.as_deref(), Some("slot2"));
        assert!(parse_start("save:slot9").is_err());
        assert!(parse_start("chapter:Bad Map").is_err());
        assert!(parse_start("map:potears2").is_err());
        let mut s = Settings { record: "a.lgtape".into(), check_every: 30, ..Default::default() };
        assert!(validate_settings(&s).is_err());
        s.start = "chapter:potears2".into();
        assert!(validate_settings(&s).is_ok());
        s.play = "b.lgtape".into();
        assert!(validate_settings(&s).is_err());
    }
    #[test]
    fn digest_sees_every_covered_value() {
        let targets = [crate::combat::Target { id: 3, center: Vec3::new(1., 2., 3.), half: Vec3::ONE }];
        let base = || Probe { map: "potears2", feet: Vec3::ZERO, velocity: Vec3::ZERO, yaw: 0., pitch: 0., sanity: 100.,
            will: 50., alive: true, weapon: 1, ticks: 9, environment_clock: 2., targets: &targets, state: None, cost_us: 0 };
        let d = digest(&base());
        assert_eq!(d.len(), 16);
        let changes: [Box<dyn Fn(&mut Probe)>; 8] = [
            Box::new(|p| p.feet.x = f32::EPSILON), Box::new(|p| p.sanity = 99.99), Box::new(|p| p.ticks += 1),
            Box::new(|p| p.map = "potears1"), Box::new(|p| p.alive = false), Box::new(|p| p.targets = &[]),
            Box::new(|p| p.velocity.z = -1.), Box::new(|p| p.yaw = -0.),
        ];
        for change in changes {
            let mut p = base();
            change(&mut p);
            assert_ne!(digest(&p), d);
        }
    }
}
