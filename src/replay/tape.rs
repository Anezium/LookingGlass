//! The tape file: UTF-8 text, one record per line.
//!
//! ```text
//! LGTAPE 1
//! H {json header}
//! S {json game}                  only for a save start: the exact game the scene is built from
//! C <step> <digest> <state|-> <map> <x>,<y>,<z> <sanity> <hostiles>   check before step <step>
//! E <step> <kind>                event seen before step <step>
//! F <dt> [field ...]             input of one step, in step order
//! M <step> <name>                marker to log when the replay reaches <step>; added on the host
//! END {json}                     clean end of a recording
//! ```
//!
//! F fields are written only when they differ from the default: `kd`, `md`, `mx` and `pad` keep
//! their value until changed (sticky); `kp`, `kr`, `mp`, `mr`, `wh`, `ch` and `quit` hold for one
//! step. Floats are f32 bit patterns in hex, so a replay sees the exact recorded values.
use super::input::Frame;
use anyhow::{bail, ensure, Context, Result};
use serde::{Deserialize, Serialize};
use std::io::Write;

pub const MAGIC: &str = "LGTAPE 1";
pub const VERSION: u32 = 1;
pub const MAX_BYTES: u64 = 32 * 1024 * 1024;
/// Two hours at 60 steps per second.
pub const MAX_STEPS: u64 = 432_000;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Start {
    /// "save" or "chapter".
    pub kind: String,
    pub slot: Option<String>,
    pub map: String,
    pub entry: Option<String>,
    pub difficulty: String,
    pub game_sha256: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Header {
    pub version: u32,
    pub platform: String,
    pub created_unix_ms: u64,
    pub package: String,
    pub keys: Vec<u16>,
    pub check_every: u32,
    pub state_every: u32,
    pub start: Start,
    pub preferences: serde_json::Value,
    pub renderer: serde_json::Value,
    pub sim: serde_json::Value,
}
pub fn platform() -> &'static str {
    if cfg!(target_os = "vita") { "vita" } else { "host" }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Check {
    pub step: u64,
    pub digest: String,
    pub state: Option<String>,
    pub map: String,
    pub feet: [f32; 3],
    pub sanity: f32,
    pub hostiles: usize,
}
impl Check {
    pub fn line(&self) -> String {
        format!("C {} {} {} {} {:.2},{:.2},{:.2} {:.1} {}", self.step, self.digest,
            self.state.as_deref().unwrap_or("-"), self.map, self.feet[0], self.feet[1], self.feet[2],
            self.sanity, self.hostiles)
    }
    fn parse(rest: &str) -> Result<Self> {
        let t: Vec<&str> = rest.split(' ').collect();
        ensure!(t.len() == 7, "check line needs 7 fields");
        let feet: Vec<f32> = t[4].split(',').map(str::parse).collect::<Result<_, _>>()?;
        ensure!(feet.len() == 3, "check feet needs 3 values");
        ensure!(t[1].len() == 16 && t[1].bytes().all(|b| b.is_ascii_hexdigit()), "bad check digest");
        Ok(Self {
            step: t[0].parse()?,
            digest: t[1].into(),
            state: (t[2] != "-").then(|| t[2].to_owned()),
            map: t[3].into(),
            feet: [feet[0], feet[1], feet[2]],
            sanity: t[5].parse()?,
            hostiles: t[6].parse()?,
        })
    }
}

fn hex_u128(v: u128) -> String { format!("{v:x}") }
fn hex_pair(p: (f32, f32)) -> String { format!("{:08x},{:08x}", p.0.to_bits(), p.1.to_bits()) }
fn parse_pair(s: &str) -> Result<(f32, f32)> {
    let (a, b) = s.split_once(',').context("pair needs a comma")?;
    Ok((f32::from_bits(u32::from_str_radix(a, 16)?), f32::from_bits(u32::from_str_radix(b, 16)?)))
}

/// One F line. `previous` supplies the sticky fields.
pub fn encode_frame(previous: &Frame, f: &Frame) -> String {
    let mut s = format!("F {:08x}", f.dt.to_bits());
    let mut field = |name: &str, value: String| { s.push(' '); s.push_str(name); s.push('='); s.push_str(&value); };
    if f.keys_down != previous.keys_down { field("kd", hex_u128(f.keys_down)); }
    if f.keys_pressed != 0 { field("kp", hex_u128(f.keys_pressed)); }
    if f.keys_released != 0 { field("kr", hex_u128(f.keys_released)); }
    if f.mouse_down != previous.mouse_down { field("md", format!("{:x}", f.mouse_down)); }
    if f.mouse_pressed != 0 { field("mp", format!("{:x}", f.mouse_pressed)); }
    if f.mouse_released != 0 { field("mr", format!("{:x}", f.mouse_released)); }
    if f.mouse.0.to_bits() != previous.mouse.0.to_bits() || f.mouse.1.to_bits() != previous.mouse.1.to_bits() {
        field("mx", hex_pair(f.mouse));
    }
    if f.wheel.0.to_bits() != 0 || f.wheel.1.to_bits() != 0 { field("wh", hex_pair(f.wheel)); }
    if f.pad != previous.pad {
        field("pad", f.pad.map_or_else(|| "-".into(), |p| format!("{:08x}", u32::from_be_bytes(p))));
    }
    if !f.chars.is_empty() {
        let text: String = f.chars.iter().collect();
        field("ch", text.bytes().map(|b| format!("{b:02x}")).collect());
    }
    if f.quit { s.push_str(" quit"); }
    s
}
pub fn decode_frame(previous: &Frame, rest: &str) -> Result<Frame> {
    let mut tokens = rest.split(' ');
    let dt = f32::from_bits(u32::from_str_radix(tokens.next().context("frame needs a time")?, 16)?);
    ensure!(dt.is_finite() && (0. ..=60.).contains(&dt), "frame time out of range: {dt}");
    let mut f = Frame {
        dt,
        keys_down: previous.keys_down,
        mouse_down: previous.mouse_down,
        mouse: previous.mouse,
        pad: previous.pad,
        ..Default::default()
    };
    for token in tokens {
        if token == "quit" { f.quit = true; continue; }
        let (name, value) = token.split_once('=').with_context(|| format!("bad frame field {token}"))?;
        match name {
            "kd" => f.keys_down = u128::from_str_radix(value, 16)?,
            "kp" => f.keys_pressed = u128::from_str_radix(value, 16)?,
            "kr" => f.keys_released = u128::from_str_radix(value, 16)?,
            "md" => f.mouse_down = u8::from_str_radix(value, 16)?,
            "mp" => f.mouse_pressed = u8::from_str_radix(value, 16)?,
            "mr" => f.mouse_released = u8::from_str_radix(value, 16)?,
            "mx" => f.mouse = parse_pair(value)?,
            "wh" => f.wheel = parse_pair(value)?,
            "pad" => f.pad = if value == "-" { None } else { Some(u32::from_str_radix(value, 16)?.to_be_bytes()) },
            "ch" => {
                ensure!(value.len() % 2 == 0, "odd char bytes");
                let bytes = (0..value.len()).step_by(2)
                    .map(|i| u8::from_str_radix(&value[i..i + 2], 16)).collect::<Result<Vec<_>, _>>()?;
                f.chars = String::from_utf8(bytes)?.chars().collect();
            }
            _ => bail!("unknown frame field {name}"),
        }
    }
    let keys = super::input::KEYS.len() as u32;
    ensure!([f.keys_down, f.keys_pressed, f.keys_released].iter().all(|m| m >> keys == 0), "key bit outside the table");
    ensure!([f.mouse_down, f.mouse_pressed, f.mouse_released].iter().all(|m| m >> 4 == 0), "mouse bit outside the table");
    Ok(f)
}

/// A recording in progress. Lines go through a 64 KiB buffer; `end` flushes and syncs.
pub struct Writer {
    file: std::io::BufWriter<std::fs::File>,
    previous: Frame,
    bytes: u64,
    pub frames: u64,
}
impl Writer {
    /// Fails if the file exists: a recording never replaces another one.
    pub fn create(path: &std::path::Path, header: &Header, game: Option<&[u8]>) -> Result<Self> {
        ensure!(!path.exists(), "{} exists; a recording never replaces a tape", path.display());
        let file = match std::fs::OpenOptions::new().write(true).create_new(true).open(path) {
            Ok(file) => file,
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => return Err(e).with_context(|| format!("create {}", path.display())),
            // A file system without exclusive create: the path was absent a moment ago.
            Err(_) => std::fs::OpenOptions::new().write(true).create(true).truncate(true).open(path)
                .with_context(|| format!("create {}", path.display()))?,
        };
        let mut w = Self { file: std::io::BufWriter::with_capacity(64 * 1024, file), previous: Frame::default(), bytes: 0, frames: 0 };
        w.line(MAGIC)?;
        w.line(&format!("H {}", serde_json::to_string(header)?))?;
        if let Some(game) = game {
            ensure!(!game.contains(&b'\n'), "game JSON must be one line");
            w.line(&format!("S {}", std::str::from_utf8(game)?))?;
        }
        Ok(w)
    }
    fn line(&mut self, text: &str) -> Result<()> {
        self.bytes += text.len() as u64 + 1;
        ensure!(self.bytes <= MAX_BYTES, "tape exceeds {MAX_BYTES} bytes");
        self.file.write_all(text.as_bytes())?;
        self.file.write_all(b"\n")?;
        Ok(())
    }
    pub fn frame(&mut self, f: &Frame) -> Result<()> {
        ensure!(self.frames < MAX_STEPS, "tape exceeds {MAX_STEPS} steps");
        let line = encode_frame(&self.previous, f);
        self.line(&line)?;
        self.previous = f.clone();
        self.frames += 1;
        Ok(())
    }
    pub fn previous_frame(&self) -> &Frame { &self.previous }
    pub fn check(&mut self, c: &Check) -> Result<()> { self.line(&c.line()) }
    pub fn event(&mut self, step: u64, kind: &str) -> Result<()> { self.line(&format!("E {step} {kind}")) }
    pub fn end(mut self, summary: &serde_json::Value) -> Result<()> {
        self.line(&format!("END {summary}"))?;
        self.file.flush()?;
        // The bytes are written; a file system without fsync only loses the durability promise.
        if let Err(e) = self.file.get_ref().sync_all() {
            println!("[lg/replay] tape fsync failed ({e}); the tape is written");
        }
        Ok(())
    }
}

/// A tape read whole and checked before the game starts.
pub struct Tape {
    pub header: Header,
    pub game: Option<Vec<u8>>,
    pub marks: std::collections::BTreeMap<u64, Vec<String>>,
    pub frames: u64,
    pub ended: Option<serde_json::Value>,
    /// The file ended inside a line (a recording cut short); that line was dropped.
    pub truncated: bool,
}
/// Keeps only the text after the header and start game, and walks it one step at a time, so a
/// replay holds about the file's frame bytes in memory and nothing per step.
pub struct Reader {
    text: Vec<u8>,
    pos: usize,
    pub tape: Tape,
    previous: Frame,
}
/// The next non-empty line at `pos` and the position after it.
fn line_at(text: &[u8], pos: usize) -> Option<(&str, usize)> {
    let mut pos = pos;
    while pos < text.len() {
        let end = text[pos..].iter().position(|b| *b == b'\n').map_or(text.len(), |i| pos + i);
        let raw = &text[pos..end];
        let next = (end + 1).min(text.len());
        if !raw.is_empty() { return Some((std::str::from_utf8(raw).unwrap_or(""), next)); }
        pos = next;
    }
    None
}
impl Reader {
    pub fn open(path: &std::path::Path) -> Result<Self> {
        let size = std::fs::metadata(path).with_context(|| format!("read {}", path.display()))?.len();
        ensure!(size <= MAX_BYTES, "tape exceeds {MAX_BYTES} bytes");
        Self::parse(std::fs::read(path)?)
    }
    pub fn parse(mut text: Vec<u8>) -> Result<Self> {
        let truncated = !text.is_empty() && !text.ends_with(b"\n");
        if truncated {
            let keep = text.iter().rposition(|b| *b == b'\n').map_or(0, |i| i + 1);
            text.truncate(keep);
        }
        let mut header = None;
        let mut game = None;
        let mut body = 0;
        let mut marks: std::collections::BTreeMap<u64, Vec<String>> = Default::default();
        let mut frames = 0u64;
        let mut ended = None;
        let mut previous = Frame::default();
        let mut last_check = None;
        let mut offset = 0;
        for (n, raw) in text.split(|b| *b == b'\n').enumerate() {
            let next = offset + raw.len() + 1;
            let here = offset;
            offset = next;
            if raw.is_empty() { continue; }
            let line = std::str::from_utf8(raw).with_context(|| format!("line {}: not UTF-8", n + 1))?;
            let at = || format!("line {}", n + 1);
            if n == 0 { ensure!(line == MAGIC, "not a tape (first line {line:?})"); continue; }
            let (tag, rest) = line.split_once(' ').unwrap_or((line, ""));
            // Markers may be appended after END by the host; nothing else may.
            ensure!(ended.is_none() || tag == "M", "{}: data after END", at());
            match tag {
                "H" => {
                    ensure!(header.is_none() && body == 0, "{}: misplaced header", at());
                    header = Some(serde_json::from_str::<Header>(rest).with_context(at)?);
                    body = next;
                }
                "S" => {
                    ensure!(header.is_some() && game.is_none() && body == here, "{}: misplaced start game", at());
                    game = Some(rest.as_bytes().to_vec());
                    body = next;
                }
                "C" => {
                    let c = Check::parse(rest).with_context(at)?;
                    ensure!(c.step == frames && last_check.is_none_or(|s| s < c.step), "{}: check out of order", at());
                    last_check = Some(c.step);
                }
                "E" => {
                    let (step, _) = rest.split_once(' ').with_context(at)?;
                    ensure!(step.parse::<u64>().with_context(at)? == frames, "{}: event out of order", at());
                }
                "F" => {
                    previous = decode_frame(&previous, rest).with_context(at)?;
                    frames += 1;
                    ensure!(frames <= MAX_STEPS, "tape exceeds {MAX_STEPS} steps");
                }
                "M" => {
                    let (step, name) = rest.split_once(' ').with_context(at)?;
                    marks.entry(step.parse().with_context(at)?).or_default().push(name.into());
                }
                "END" => ended = Some(serde_json::from_str(rest).with_context(at)?),
                _ => bail!("{}: unknown record {tag:?}", at()),
            }
        }
        let header = header.context("tape has no header")?;
        ensure!(header.version == VERSION, "tape version {} (this build reads {VERSION})", header.version);
        ensure!(header.keys == super::input::key_codes(), "tape key table differs from this build");
        ensure!(header.check_every > 0, "tape check cadence is zero");
        match header.start.kind.as_str() {
            "save" => {
                let g = game.as_ref().context("save start without its game")?;
                use sha2::{Digest, Sha256};
                ensure!(header.start.game_sha256.as_deref() == Some(&format!("{:x}", Sha256::digest(g))[..]),
                    "start game does not match its hash");
            }
            "chapter" => ensure!(game.is_none(), "chapter start with a game"),
            other => bail!("unknown start kind {other}"),
        }
        ensure!(frames > 0, "tape has no frames");
        ensure!(marks.keys().all(|s| *s < frames), "marker after the last step");
        // Keep the step records only; the header and start game are parsed already.
        let text = text[body.min(text.len())..].to_vec();
        Ok(Self { text, pos: 0, tape: Tape { header, game, marks, frames, ended, truncated }, previous: Frame::default() })
    }
    /// Checks and events recorded before the next step, then that step's frame. After the last
    /// frame it returns the trailing checks and events with no frame, then None.
    pub fn step(&mut self) -> Result<Option<(Vec<Check>, Vec<String>, Option<Frame>)>> {
        let mut checks = Vec::new();
        let mut events = Vec::new();
        let mut any = false;
        while let Some((line, next)) = line_at(&self.text, self.pos) {
            self.pos = next;
            let (tag, rest) = line.split_once(' ').unwrap_or((line, ""));
            match tag {
                "C" => { any = true; checks.push(Check::parse(rest)?); }
                "E" => { any = true; events.push(rest.split_once(' ').map_or("", |(_, k)| k).to_owned()); }
                "F" => {
                    let f = decode_frame(&self.previous, rest)?;
                    self.previous = f.clone();
                    return Ok(Some((checks, events, Some(f))));
                }
                _ => {}
            }
        }
        Ok(any.then_some((checks, events, None)))
    }
    /// The check recorded before the next frame, if any, without consuming it.
    pub fn next_check(&self) -> Option<Check> {
        let mut pos = self.pos;
        while let Some((line, next)) = line_at(&self.text, pos) {
            pos = next;
            match line.split_once(' ').unwrap_or((line, "")) {
                ("C", rest) => return Check::parse(rest).ok(),
                ("F", _) => return None,
                _ => {}
            }
        }
        None
    }
    /// Drop the start game once the scene is built from it.
    pub fn take_game(&mut self) -> Option<Vec<u8>> { self.tape.game.take() }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn header(start: Start) -> Header {
        Header { version: VERSION, platform: platform().into(), created_unix_ms: 1, package: "t".into(),
            keys: super::super::input::key_codes(), check_every: 2, state_every: 0, start,
            preferences: serde_json::Value::Null, renderer: serde_json::Value::Null, sim: serde_json::Value::Null }
    }
    fn chapter() -> Start {
        Start { kind: "chapter".into(), slot: None, map: "potears2".into(), entry: None, difficulty: "normal".into(), game_sha256: None }
    }
    fn frames() -> Vec<Frame> {
        let mut a = Frame { dt: 1. / 30., keys_down: 1 << 51 | 1 << 121, keys_pressed: 1 << 51, mouse: (3.5, -0.0), pad: Some([128, 127, 0, 255]), ..Default::default() };
        let b = Frame { dt: 0.016_666_668, keys_down: a.keys_down, mouse_down: 1, mouse_pressed: 1, mouse: a.mouse, wheel: (0., -1.), pad: a.pad, chars: vec!['é', 'a'], ..Default::default() };
        let c = Frame { dt: 0.05, keys_released: a.keys_down, mouse_released: 1, pad: None, quit: true, mouse: (f32::MIN_POSITIVE, 1e9), ..Default::default() };
        a.keys_released = 0;
        vec![a, b, c]
    }
    fn write(dir: &std::path::Path, name: &str, start: Start, game: Option<&[u8]>) -> std::path::PathBuf {
        let path = dir.join(name);
        let _ = std::fs::remove_file(&path);
        let mut w = Writer::create(&path, &header(start), game).unwrap();
        for (i, f) in frames().iter().enumerate() {
            if i % 2 == 0 {
                w.check(&Check { step: i as u64, digest: "0123456789abcdef".into(), state: None, map: "potears2".into(), feet: [1., 2., 3.], sanity: 99.5, hostiles: i }).unwrap();
            }
            if i == 1 { w.event(1, "death").unwrap(); }
            w.frame(f).unwrap();
        }
        w.check(&Check { step: 3, digest: "fedcba9876543210".into(), state: Some("00112233aabbccdd".into()), map: "potears2".into(), feet: [0.; 3], sanity: 0., hostiles: 0 }).unwrap();
        w.end(&serde_json::json!({"steps": 3})).unwrap();
        path
    }
    fn scratch() -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("lg-tape-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }
    #[test]
    fn frames_checks_and_events_round_trip_bit_exact() {
        let dir = scratch();
        let path = write(&dir, "a.lgtape", chapter(), None);
        let mut r = Reader::open(&path).unwrap();
        assert_eq!(r.tape.frames, 3);
        assert!(r.tape.ended.is_some());
        for (i, expected) in frames().iter().enumerate() {
            assert_eq!(r.next_check().map(|c| c.step), (i % 2 == 0).then_some(i as u64));
            let (checks, events, f) = r.step().unwrap().unwrap();
            let f = f.unwrap();
            assert_eq!(checks.len(), usize::from(i % 2 == 0));
            assert_eq!(events, if i == 1 { vec!["death".to_string()] } else { vec![] });
            assert_eq!(f.dt.to_bits(), expected.dt.to_bits());
            assert_eq!(f.mouse.1.to_bits(), expected.mouse.1.to_bits());
            assert_eq!(&f, expected);
        }
        let (checks, _, f) = r.step().unwrap().unwrap();
        assert_eq!((checks.len(), f), (1, None));
        assert!(r.step().unwrap().is_none());
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn refuses_overwrite_bad_hash_and_unknown_fields() {
        let dir = scratch();
        let game = br#"{"current":"x"}"#;
        use sha2::{Digest, Sha256};
        let mut start = Start { kind: "save".into(), slot: Some("slot2".into()), map: "gvillage".into(), entry: None,
            difficulty: "normal".into(), game_sha256: Some(format!("{:x}", Sha256::digest(game))) };
        let path = write(&dir, "b.lgtape", start.clone(), Some(game));
        assert!(Writer::create(&path, &header(start.clone()), Some(game)).is_err());
        let r = Reader::open(&path).unwrap();
        assert_eq!(r.tape.game.as_deref(), Some(&game[..]));
        start.game_sha256 = Some("0".repeat(64));
        let bad = write(&dir, "c.lgtape", start, Some(game));
        assert!(Reader::open(&bad).is_err());
        let text = std::fs::read_to_string(&path).unwrap().replace("F 3d088889", "F 3d088889 zz=1");
        assert!(Reader::parse(text.into_bytes()).is_err());
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn markers_added_after_recording_are_read_and_a_partial_tape_still_replays() {
        let dir = scratch();
        let path = write(&dir, "d.lgtape", chapter(), None);
        let text = std::fs::read_to_string(&path).unwrap();
        let (body, _) = text.rsplit_once("END ").unwrap();
        let marked = format!("{body}M 1 explosion near the river\nM 2 second\n");
        let after_end = format!("{text}M 2 appended after the end\n");
        assert_eq!(Reader::parse(after_end.into_bytes()).unwrap().tape.marks[&2].len(), 1);
        assert!(Reader::parse(format!("{text}F 3d088889\n").into_bytes()).is_err());
        let r = Reader::parse(marked.clone().into_bytes()).unwrap();
        assert!(r.tape.ended.is_none());
        assert_eq!(r.tape.marks[&1], vec!["explosion near the river".to_string()]);
        let cut = &text[..text.rfind("\nF ").unwrap() + 5];
        let r = Reader::parse(cut.as_bytes().to_vec()).unwrap();
        assert!(r.tape.truncated && r.tape.ended.is_none() && r.tape.frames == 2);
        let late = format!("{body}M 3 too late\n");
        assert!(Reader::parse(late.into_bytes()).is_err());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
