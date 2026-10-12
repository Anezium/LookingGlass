//! Coarse load markers and aggregate texture/shader timings. No per-image logs.
use std::{cell::RefCell, time::Instant};

#[derive(Clone, Copy, Default)]
struct Counters { calls: [u64; 4], ms: [f64; 4] }
thread_local! { static STATE: RefCell<(usize, Counters)> = RefCell::new((0, Counters::default())); }
pub(crate) enum Work { TextureRead, TextureDecode, TextureUpload, ShaderCreation }
pub(crate) struct WorkTimer { start: Option<Instant>, index: usize }
impl WorkTimer {
    pub fn new(work: Work) -> Self {
        Self { start: STATE.with(|s| (s.borrow().0 > 0).then(Instant::now)), index: work as usize }
    }
}
impl Drop for WorkTimer {
    fn drop(&mut self) {
        if let Some(start) = self.start {
            STATE.with(|s| {
                let mut state = s.borrow_mut();
                state.1.calls[self.index] += 1;
                state.1.ms[self.index] += start.elapsed().as_secs_f64() * 1000.;
            });
        }
    }
}
pub(crate) struct Phase {
    scope: &'static str,
    start: Instant,
    last: Instant,
    previous: Counters,
}
impl Phase {
    pub fn new(scope: &'static str) -> Self {
        let previous = STATE.with(|s| { let mut s = s.borrow_mut(); s.0 += 1; s.1 });
        let now = Instant::now();
        eprintln!("[lg/startup] {scope} begin elapsed_ms=0");
        Self { scope, start: now, last: now, previous }
    }
    pub fn mark(&mut self, label: &str) {
        let now = Instant::now();
        let counters = STATE.with(|s| s.borrow().1);
        eprintln!("[lg/startup] {} {label} elapsed_ms={:.3} total_ms={:.3}", self.scope,
            now.duration_since(self.last).as_secs_f64()*1000., now.duration_since(self.start).as_secs_f64()*1000.);
        for (i, name) in ["texture_read", "texture_decode", "texture_upload_dxt_on_vita", "shader_creation"].iter().enumerate() {
            let calls = counters.calls[i] - self.previous.calls[i];
            if calls > 0 {
                eprintln!("[lg/startup] {} {label} work={name} calls={calls} elapsed_ms={:.3}", self.scope, counters.ms[i] - self.previous.ms[i]);
            }
        }
        self.last = now;
        self.previous = counters;
    }
}
impl Drop for Phase {
    fn drop(&mut self) {
        self.mark("end");
        STATE.with(|s| s.borrow_mut().0 -= 1);
    }
}
