//! Tape-driven capture. Replay frame 1 presents tape step 0, matching spike_log.
use super::*;
use std::{io::BufWriter, path::Path};

pub(crate) struct ReplayWindow {
    tape: &'static str,
    first: u64,
    last: u64,
    initial_map: Option<String>,
}

impl ReplayWindow {
    pub(crate) fn configured() -> Option<Self> {
        let c = crate::render::vita_renderer::config();
        c.phase_census_window.then(|| Self::new(
            &c.replay_play, c.spike_log_first_frame.into(), c.spike_log_last_frame.into(),
        ))
    }

    fn new(tape: &'static str, first: u64, last: u64) -> Self {
        // Reserve the existing fixed storage before the first observed frame.
        STATE.with(|s| {
            let mut s = s.borrow_mut();
            s.count = 0;
            s.accepted = 0;
            s.sequence = 0;
            s.start = None;
            s.pending = false;
            s.overflow = false;
            s.tls_window = false;
            s.table.fill(Entry::EMPTY);
            s.exact.fill(ExactEntry::EMPTY);
        });
        Self { tape, first, last, initial_map: None }
    }

    pub(crate) fn start(&mut self, frame: u64, map: &str) -> Option<ReplayFrame> {
        if frame < self.first || frame > self.last || frame - self.first >= CAPACITY as u64 {
            return None;
        }
        self.initial_map.get_or_insert_with(|| map.to_owned());
        STATE.with(|s| {
            let mut s = s.borrow_mut();
            s.replay_window = true;
            s.sequence = frame - 1;
        });
        ARMED.fetch_add(REPLAY_FRAME, Relaxed);
        begin_row(Instant::now());
        Some(ReplayFrame { presented: false })
    }

    pub(crate) fn export_if_due(window: &mut Option<Self>, previous_frame: u64) {
        if window.as_ref().is_some_and(|w| previous_frame >= w.last) {
            window.take().unwrap().export(previous_frame);
        }
    }

    fn write(&self, s: &State, map: &str, out: &mut impl Write) -> anyhow::Result<()> {
        let rows = &s.rows[..s.count];
        anyhow::ensure!(!s.overflow && rows.first().is_some_and(|r| r.sequence == self.first)
            && rows.iter().enumerate().all(|(i, r)| r.measured && r.sequence == self.first + i as u64)
            && rows.last().is_some_and(|r| r.sequence <= self.last),
            "replay census frame alignment");
        write_document(s, map, Some(serde_json::json!({
            "tape": self.tape,
            "requested_first_frame": self.first,
            "requested_last_frame": self.last,
            "captured_first_frame": rows[0].sequence,
            "captured_last_frame": rows[rows.len() - 1].sequence,
            "policy": "first 180 presented frames",
            "frame_index": "one-based replay frame (row sequence); tape steps are zero-based",
            "render_other": "exclusive time in render; includes bracket overhead and gaps",
            "world_gxm": "successful direct draws are included in world_state_bindings",
        })), out)
    }

    pub(crate) fn export(self, previous_frame: u64) {
        if let Err(error) = self.write_files(previous_frame) {
            eprintln!("[lg/replay-census] export failed: {error:#}");
        }
    }

    fn write_files(&self, previous_frame: u64) -> anyhow::Result<()> {
        #[cfg(target_os = "vita")]
        let dir = Path::new("ux0:data/alice/private/frame_profile");
        #[cfg(not(target_os = "vita"))]
        let dir = Path::new("private/frame_profile");
        STATE.with(|s| {
            let s = s.borrow();
            let rows = &s.rows[..s.count];
            if rows.is_empty() {
                println!("[lg/replay-census] no presented rows tape={} requested={}..{} replay_frame={previous_frame}",
                    self.tape, self.first, self.last);
                return Ok(());
            }

            std::fs::create_dir_all(dir)?;
            let stem = self.tape.strip_suffix(".lgtape").unwrap();
            let path = dir.join(format!("{stem}-census-{}-{}.json", self.first, self.last));
            let mut json = BufWriter::new(std::fs::File::create(&path)?);
            self.write(&s, self.initial_map.as_deref().unwrap(), &mut json)?;
            json.flush()?;
            let mut csv = BufWriter::new(std::fs::File::create(path.with_extension("csv"))?);
            write_csv_rows(&s, REGIONS, &mut csv)?;
            csv.flush()?;

            println!("[lg/replay-census] file={} rows={} captured={}..{} requested={}..{} replay_frame={previous_frame}",
                path.display(), rows.len(), rows[0].sequence, rows[rows.len() - 1].sequence, self.first, self.last);
            let mean_ms = |ns: u64| ns as f64 / (rows.len() as f64 * 1_000_000.);
            for (i, name) in NAMES.iter().enumerate() {
                let inclusive = rows.iter().map(|r| r.inclusive[i]).sum();
                let exclusive = rows.iter().map(|r| r.exclusive[i]).sum();
                let calls: u64 = rows.iter().map(|r| u64::from(r.calls[i])).sum();
                println!("[lg/replay-census] region={name} inclusive_ms={:.4} exclusive_ms={:.4} calls={calls}",
                    mean_ms(inclusive), mean_ms(exclusive));
            }
            let render: u64 = rows.iter().map(|r| r.inclusive[Region::Render as usize]).sum();
            let other: u64 = rows.iter().map(|r| r.exclusive[Region::Render as usize]).sum();
            let share = if render == 0 { 0. } else { 100. * other as f64 / render as f64 };
            println!("[lg/replay-census] region=render_other ms={:.4} share_pct={share:.2}", mean_ms(other));
            Ok(())
        })
    }
}

pub(crate) struct ReplayFrame {
    presented: bool,
}

impl ReplayFrame {
    pub(crate) fn presented(frame: Option<Self>) {
        if let Some(mut frame) = frame {
            frame.presented = true;
        }
    }
}

impl Drop for ReplayFrame {
    fn drop(&mut self) {
        let now = Instant::now();
        ACTIVE.with(|a| a.set(false));
        ARMED.fetch_sub(REPLAY_FRAME, Relaxed);
        STATE.with(|s| {
            let mut s = s.borrow_mut();
            s.pending = self.presented;
            s.current.measured = self.presented;
            s.close(now);
            s.replay_window = false;
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn captures_inclusive_first_180_with_no_hot_path_allocations() {
        let mut window = ReplayWindow::new("fixture.lgtape", 59, 300);
        assert!(window.start(58, "fixture").is_none());
        let mut first = window.start(59, "fixture");
        let scope = crate::kernel_alloc::Scope::begin();
        for frame in 59..=301 {
            let arm = if frame == 59 { first.take() } else { window.start(frame, "fixture") };
            assert_eq!(arm.is_some(), frame <= 238);
            assert_eq!(active(), frame <= 238);
            {
                let _render = span(Region::Render);
                let _collect = span(Region::ActorCollect);
                drop(span(Region::ActorWorkerDrain));
                drop(span(Region::ActorLayerMaterialize));
            }
            ReplayFrame::presented(arm);
            assert!(!active());
        }
        assert_eq!(scope.finish(), 0);
        STATE.with(|s| {
            let s = s.borrow();
            assert_eq!(s.count, CAPACITY);
            let mut json = Vec::new();
            window.write(&s, "fixture", &mut json).unwrap();
            let doc: serde_json::Value = serde_json::from_slice(&json).unwrap();
            assert_eq!(doc["schema"], 8);
            assert_eq!(doc["replay_window"]["captured_first_frame"], 59);
            assert_eq!(doc["replay_window"]["captured_last_frame"], 238);
            assert_eq!(doc["rows"][179]["measured_frame"], 180);
            let row = &doc["rows"][0];
            for region in [Region::ActorWorkerDrain, Region::ActorLayerMaterialize] {
                assert_eq!(row["parent_masks"][region as usize], 1u64 << Region::ActorCollect as usize);
            }
        });
    }

    #[test]
    fn releases_the_gate_and_rejects_an_unpresented_frame() {
        let mut window = ReplayWindow::new("fixture.lgtape", 7, 9);
        let frame = window.start(7, "fixture");
        drop(span(Region::Render));
        ReplayFrame::presented(frame);
        let frame = window.start(8, "fixture");
        accept(true, true); // The generic profiler nominated it, but it never presents.
        drop(frame);
        assert!(!active());
        STATE.with(|s| {
            let s = s.borrow();
            assert_eq!(s.count, 1);
            assert_eq!(s.rows[0].sequence, 7);
            window.write(&s, "fixture", &mut Vec::new()).unwrap();
        });
    }
}
