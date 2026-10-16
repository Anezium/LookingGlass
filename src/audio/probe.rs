//! Read the existing CPAL callback thread; never create or reschedule audio work.
use std::time::{Duration, Instant};
#[derive(Default)]
pub(super) struct Probe {
    started: Option<Instant>, previous: Option<Instant>,
    report: Option<(Instant, u64)>, late: u64, over_budget: u64, blocks: u64,
}
impl Probe {
    pub(super) fn begin(&mut self) {
        if !crate::light_profile::probe_active() { return; }
        let now = Instant::now();
        let block = Duration::from_secs_f64(512. / 44100.);
        self.late += u64::from(self.previous.is_some_and(|p| now.duration_since(p) > block + Duration::from_millis(1)));
        self.previous = Some(now);
        self.started = Some(now);
    }
    pub(super) fn end(&mut self) {
        let Some(started) = self.started.take() else { return; };
        self.over_budget += u64::from(started.elapsed() > Duration::from_secs_f64(512. / 44100.));
        self.blocks += 1;
        if self.report.is_some_and(|(time, _)| time.elapsed() < Duration::from_secs(1)) { return; }
        match crate::skin_worker::current_runtime() {
            Ok((id, clocks, affinity, priority)) => {
                let now = Instant::now();
                if let Some((time, old)) = self.report {
                    eprintln!("[lg/audio-cpu] thread_id={id} affinity={affinity:#x} priority={priority} window_us={} audio_thread_cpu_us={} monitor_blocks={} monitor_late_blocks={} monitor_over_budget_blocks={} underruns=see_cpal_underrun_estimates",
                        now.duration_since(time).as_micros(), clocks.saturating_sub(old), self.blocks, self.late, self.over_budget);
                } else {
                    eprintln!("[lg/audio-cpu] baseline thread_id={id} affinity={affinity:#x} priority={priority} units=us sampling=1024_stereo_samples");
                }
                self.report = Some((now, clocks));
                self.blocks = 0; self.late = 0; self.over_budget = 0;
            }
            Err(rc) => { eprintln!("[lg/audio-cpu] thread_info_failed={rc:#x}"); }
        }
    }
}
