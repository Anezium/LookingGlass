//! Explicit, save-free console survey. Absent/disabled config has no effect.
use std::time::{Duration, Instant};

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Config { enabled: bool, map: String, entry: Option<String> }

pub(crate) struct Sweep {
    started: Instant,
    first: Option<Instant>,
    previous: Option<Instant>,
    rows: Vec<(u64, u64)>,
    dead: bool,
    window_started: bool,
    pub(crate) done: bool,
}

fn validate(config: &Config) -> anyhow::Result<()> {
    anyhow::ensure!(config.map.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
        && !config.map.is_empty(), "invalid sweep map");
    anyhow::ensure!(match config.entry.as_deref() {
        None => true,
        Some("fortress1_start2") => config.map == "fortress1",
        Some("skool1_start2") => config.map == "skool1",
        Some("wforest_start2") => config.map == "wforest",
        _ => false,
    }, "invalid sweep return entry");
    Ok(())
}

fn interval(first: Instant, previous: Instant, now: Instant) -> Option<(u64, u64)> {
    let begin = previous.duration_since(first);
    let end = now.duration_since(first);
    (begin >= Duration::from_secs(10) && end <= Duration::from_secs(40))
        .then(|| (end.as_micros() as u64, now.duration_since(previous).as_micros() as u64))
}

impl Sweep {
    #[cfg(target_os = "vita")]
    pub(crate) fn open(options: &mut crate::Options) -> anyhow::Result<Option<Self>> {
        let path = "ux0:data/alice/private/r12-sweep.json";
        let raw = match std::fs::read(path) {
            Ok(raw) => raw,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e.into()),
        };
        let config: Config = serde_json::from_slice(&raw)?;
        if !config.enabled { return Ok(None); }
        validate(&config)?;
        anyhow::ensure!(crate::frame_profile::Mode::read() == crate::frame_profile::Mode::Full,
            "sweep requires full profile mode");
        options.map = config.map;
        options.entry = config.entry;
        options.direct_map = true;
        options.new_game = false;
        options.load = None;
        println!("[lg/r12-sweep] entry map={} entry={} settle_seconds=10 window_seconds=30 input=disabled saves=disabled",
            options.map, options.entry.as_deref().unwrap_or("first"));
        Ok(Some(Self { started: Instant::now(), first: None, previous: None,
            rows: Vec::with_capacity(8192), dead: false, window_started: false, done: false }))
    }

    // Called after next_frame returns: intervals include actual presentation.
    #[cfg(target_os = "vita")]
    pub(crate) fn presented(&mut self, map: &str, alive: bool, gameplay: bool) -> anyhow::Result<()> {
        if self.done { return Ok(()); }
        let now = Instant::now();
        let first = *self.first.get_or_insert_with(|| {
            println!("[lg/r12-sweep] first_frame map={map} load_us={} alive={alive} gameplay={gameplay}",
                now.duration_since(self.started).as_micros());
            now
        });
        if !self.window_started && now.duration_since(first) >= Duration::from_secs(10) {
            println!("[lg/r12-sweep] window_begin map={map} since_first_us={}", now.duration_since(first).as_micros());
            self.window_started = true;
        }
        if !alive && !self.dead {
            self.dead = true;
            println!("[lg/r12-sweep] death map={map} since_first_us={}", now.duration_since(first).as_micros());
        }
        if let Some(previous) = self.previous {
            if let Some(row) = interval(first, previous, now) {
                anyhow::ensure!(self.rows.len() < self.rows.capacity(), "sweep sample capacity exceeded");
                self.rows.push(row);
            }
        }
        self.previous = Some(now);
        if now.duration_since(first) < Duration::from_secs(40) { return Ok(()); }
        // Same newlib statistic and pre-export placement as frame_total_profile.
        #[repr(C)]
        struct MallInfo { arena: usize, ordblks: usize, smblks: usize, hblks: usize,
            hblkhd: usize, usmblks: usize, fsmblks: usize, uordblks: usize,
            fordblks: usize, keepcost: usize }
        extern "C" { fn mallinfo() -> MallInfo; }
        println!("[lg/frame-total-heap] heap_uordblks_bytes={}", unsafe { mallinfo() }.uordblks);
        use std::io::Write;
        let dir = std::path::Path::new("ux0:data/alice/private/frame_profile");
        anyhow::ensure!(dir.is_dir(), "existing frame_profile directory required");
        let mut out = std::io::BufWriter::new(std::fs::File::create(dir.join(format!("{map}-r12-wall.csv")))?);
        writeln!(out, "frame,since_first_us,wall_us")?;
        for (index, (end, wall)) in self.rows.iter().enumerate() {
            writeln!(out, "{},{end},{wall}", index + 1)?;
        }
        out.flush()?;
        println!("[lg/r12-sweep] done map={map} samples={} window_start_us=10000000 window_end_us=40000000 died={}", self.rows.len(), self.dead);
        self.done = true;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_complete_intervals_inside_identical_window_are_accepted() {
        let t = Instant::now();
        let at = |ms| t + Duration::from_millis(ms);
        assert_eq!(interval(t, at(9990), at(10010)), None);
        assert_eq!(interval(t, at(10000), at(10020)), Some((10020000, 20000)));
        assert_eq!(interval(t, at(39980), at(40000)), Some((40000000, 20000)));
        assert_eq!(interval(t, at(39990), at(40010)), None);
    }
    #[test]
    fn return_entries_cannot_silently_select_another_visit() {
        for (map, entry, valid) in [("fortress1", Some("fortress1_start2"), true),
            ("skool1", Some("skool1_start2"), true), ("wforest", Some("wforest_start2"), true),
            ("fortress2", Some("fortress1_start2"), false), ("../map", None, false)] {
            assert_eq!(validate(&Config { enabled: true, map: map.into(), entry: entry.map(str::to_owned) }).is_ok(), valid);
        }
    }
}
