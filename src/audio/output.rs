//! The game publishes source handles without locks. Optional PCM buffering keeps
//! every source/DSP/decoder call off the high-priority device callback.
use super::{
    block, queue,
    sink::{Output, Rendered, Sink},
};
use rodio::Source;
use std::{
    sync::{
        atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering},
        Arc,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};
pub const SAMPLES: usize = 1024;
/// Ready-block ring slots: the largest configurable cushion.
const READY_SLOTS: usize = 8;
/// A voice outside the ambience cap must be this much louder than the quietest
/// held voice (about 2 dB) before they swap.
const HYSTERESIS: f32 = 1.25;
#[derive(Default)]
pub struct Metrics {
    pub rejected: AtomicUsize,
    pub starved: AtomicUsize,
    /// Subset of `starved`: the first silent block immediately after an
    /// intentional prefetch flush. Total starvation remains visible unchanged.
    flush_silence: AtomicUsize,
    pub queued_samples: AtomicUsize,
    pub active_peak: AtomicUsize,
    pub audible_ambient: AtomicUsize,
    pub virtual_ambient: AtomicUsize,
    epoch: AtomicU32,
    flushes: AtomicUsize,
    movie_serial: AtomicU32,
    movie: Arc<AtomicU32>,
}
pub struct Control {
    pending: queue::Sender<Output>,
    pub metrics: Arc<Metrics>,
}
pub struct Mixer {
    pending: queue::Receiver<Output>,
    sources: Vec<Output>,
    metrics: Arc<Metrics>,
    sample: usize,
    block: [f32; SAMPLES],
    without_movie: [f32; SAMPLES],
    movie: u32,
    has_movies: bool,
    at: usize,
    ranks: Vec<(usize, f32)>,
    config: block::Settings,
}
pub fn mixer() -> (Control, Mixer) {
    let (tx, rx) = queue::channel(128);
    let metrics = Arc::new(Metrics::default());
    (
        Control {
            pending: tx,
            metrics: metrics.clone(),
        },
        Mixer {
            pending: rx,
            sources: Vec::with_capacity(128),
            metrics,
            sample: 0,
            block: [0.; SAMPLES],
            without_movie: [0.; SAMPLES],
            movie: 0,
            has_movies: false,
            at: SAMPLES,
            ranks: Vec::with_capacity(128),
            config: block::settings(),
        },
    )
}
impl Control {
    pub fn flush(&self) {
        self.metrics.epoch.fetch_add(1, Ordering::Release);
        self.metrics.flushes.fetch_add(1, Ordering::Relaxed);
    }
    pub fn sink(&self) -> Sink {
        let _api = super::sink::ApiTimer::new();
        let (sink, output) = Sink::new_idle();
        self.publish(sink, output)
    }
    pub fn movie_sink(&self) -> Sink {
        let _api = super::sink::ApiTimer::new();
        // Never recycle an identity, including on wrap. New publication revokes
        // every older movie; at most one movie can contribute to an interval.
        let id = self.metrics.movie_serial.fetch_update(Ordering::Relaxed, Ordering::Relaxed,
            |n| n.checked_add(1)).expect("movie instance IDs exhausted") + 1;
        let (sink, output) = Sink::new_movie(id, self.metrics.movie.clone());
        self.metrics.movie.store(id, Ordering::Release);
        self.publish(sink, output)
    }
    fn publish(&self, sink: Sink, output: Output) -> Sink {
        if self.pending.push(output).is_err() {
            self.metrics.rejected.fetch_add(1, Ordering::Relaxed);
            eprintln!("[lg/audio-control] mixer_queue_full dropped=1 wait_us=0");
        }
        sink
    }
}
impl Mixer {
    pub(super) fn premix(&mut self, epoch: u32) -> Block {
        if self.config.mix {
            self.render();
            let mut block = Block::silence(epoch);
            block.samples = self.block;
            block.movie = self.movie;
            if self.movie != 0 { block.without_movie = Some(self.without_movie); }
            block
        } else {
            // Legacy sample mixer: still render each source exactly once per
            // value, retaining its ordinary sum order and control cadence.
            while let Some(s) = self.pending.pop() { self.has_movies |= s.movie_id() != 0; self.sources.push(s); }
            let active = self.metrics.movie.load(Ordering::Acquire);
            let mut block = Block::silence(epoch);
            block.movie = active;
            if active != 0 { block.without_movie = Some([0.; SAMPLES]); }
            for i in 0..SAMPLES {
                let (normal, other) = self.legacy_sample(Some(active));
                block.samples[i] = normal;
                if let Some(samples) = &mut block.without_movie { samples[i] = other; }
            }
            block
        }
    }
    fn render(&mut self) {
        block::reset();
        let start = Instant::now();
        while let Some(s) = self.pending.pop() {
            self.has_movies |= s.movie_id() != 0;
            self.sources.push(s);
        }
        self.metrics
            .active_peak
            .fetch_max(self.sources.len(), Ordering::Relaxed);
        let config = self.config;
        self.ranks.clear();
        for (i, s) in self.sources.iter().enumerate() {
            if let Some(gain) = s.rank() {
                if gain > config.gain {
                    self.ranks.push((i, if s.selected { gain * HYSTERESIS } else { gain }));
                }
            }
        }
        self.ranks
            .sort_unstable_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
        if config.voices > 0 {
            self.ranks.truncate(config.voices);
        }
        for s in &mut self.sources {
            s.selected = false;
        }
        for &(i, _) in &self.ranks {
            self.sources[i].selected = true;
        }
        self.metrics
            .audible_ambient
            .store(self.ranks.len(), Ordering::Relaxed);
        self.metrics.virtual_ambient.store(
            self.sources.iter().filter(|s| s.rank().is_some()).count() - self.ranks.len(),
            Ordering::Relaxed,
        );
        block::charge(6, start);
        self.block.fill(0.);
        // Snapshot before rendering, never retag PCM with a later control ID.
        let active = self.metrics.movie.load(Ordering::Acquire);
        self.movie = if active != 0 && self.sources.iter().any(|s| s.movie_id() == active) { active } else { 0 };
        let movie = self.movie;
        let has_movies = self.has_movies;
        if movie != 0 { self.without_movie.fill(0.); }
        let without_movie = &mut self.without_movie;
        let mut scratch = [0.; SAMPLES];
        let mixed = &mut self.block;
        self.sources.retain_mut(|s| {
            let audible = s.selected || s.rank().is_none();
            match s.render(&mut scratch, audible) {
                Rendered::Dead => return false,
                Rendered::Silent => return true,
                Rendered::Audio => {}
            }
            let start = Instant::now();
            let id = if has_movies { s.movie_id() } else { 0 };
            if id != 0 && id != movie { return true; }
            for (dst, src) in mixed.iter_mut().zip(scratch.iter()) {
                *dst += *src;
            }
            if movie != 0 && id == 0 {
                for (dst, src) in without_movie.iter_mut().zip(scratch.iter()) { *dst += *src; }
            }
            block::charge(4, start);
            true
        });
        if has_movies { self.has_movies = self.sources.iter().any(|s| s.movie_id() != 0); }
        self.at = 0;
    }
    fn legacy_sample(&mut self, movie: Option<u32>) -> (f32, f32) {
        // Moving pending owned sources cannot run a decoder. Each Output
        // normalizes its source only when the consumer first asks for samples.
        let mut added = false;
        while let Some(source) = self.pending.pop() {
            self.has_movies |= source.movie_id() != 0;
            self.sources.push(source);
            added = true;
        }
        self.sample = self.sample.wrapping_add(1);
        if added || self.sample & 1023 == 0 {
            self.metrics
                .active_peak
                .fetch_max(self.sources.len(), Ordering::Relaxed);
        }
        let mut sum = 0.;
        let mut other = 0.;
        let active = &self.metrics.movie;
        let has_movies = self.has_movies;
        self.sources.retain_mut(|s| {
            let id = if has_movies { s.movie_id() } else { 0 };
            // A replacement arriving during legacy sample mixing starts in
            // the next interval, with its own first stereo pair intact. Other
            // sinks keep their existing per-sample publication semantics.
            if movie.is_some_and(|interval| id != 0 && id != interval)
                && active.load(Ordering::Acquire) == id { return true; }
            match s.next() {
                Some(v) => {
                    if movie.is_none() || id == 0 || movie == Some(id) { sum += v; }
                    if movie.is_some_and(|id| id != 0) && id == 0 { other += v; }
                    true
                }
                None => false,
            }
        });
        (sum, other)
    }
}
impl Iterator for Mixer {
    type Item = f32;
    fn next(&mut self) -> Option<f32> {
        if self.config.mix {
            if self.at == SAMPLES { self.render(); }
            let v = self.block[self.at];
            self.at += 1;
            Some(v)
        } else {
            Some(self.legacy_sample(None).0)
        }
    }
}
impl Source for Mixer {
    fn current_frame_len(&self) -> Option<usize> {
        None
    }
    fn channels(&self) -> u16 {
        2
    }
    fn sample_rate(&self) -> u32 {
        44100
    }
    fn total_duration(&self) -> Option<Duration> {
        None
    }
}
pub(super) struct Block {
    epoch: u32,
    pub(super) samples: [f32; SAMPLES],
    pub(super) without_movie: Option<[f32; SAMPLES]>,
    movie: u32,
}
impl Block {
    fn silence(epoch: u32) -> Self {
        Self { epoch, samples: [0.; SAMPLES], without_movie: None, movie: 0 }
    }
}
/// Runs exclusively on the producer. Both premixes must already be limited.
pub(super) trait PcmSource: Send + 'static {
    fn pcm_block(&mut self, epoch: u32) -> Block;
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ProducerPoint { Full, Epoch, Rendered, PrePush, Pushed }
/// The real worker's single step. The empty hook optimizes away in production;
/// deterministic tests stop this exact path without replacing the PCM mixer.
fn producer_step<S: PcmSource>(source: &mut S, tx: &queue::Sender<Block>,
    metrics: &Metrics, cushion: usize, mut point: impl FnMut(ProducerPoint)) -> Option<f64>
{
    if tx.len() >= cushion {
        point(ProducerPoint::Full);
        return None;
    }
    let epoch = metrics.epoch.load(Ordering::Acquire);
    point(ProducerPoint::Epoch);
    let start = Instant::now();
    let block = source.pcm_block(epoch);
    let total = start.elapsed().as_secs_f64() * 1000.;
    point(ProducerPoint::Rendered);
    metrics.queued_samples.fetch_add(SAMPLES, Ordering::Relaxed);
    point(ProducerPoint::PrePush);
    if tx.push(block).is_err() { metrics.queued_samples.fetch_sub(SAMPLES, Ordering::Relaxed); }
    point(ProducerPoint::Pushed);
    Some(total)
}
pub struct Callback {
    ready: queue::Receiver<Block>,
    block: [f32; SAMPLES],
    without_movie: [f32; SAMPLES],
    movie: u32,
    use_movie: bool,
    at: usize,
    epoch: u32,
    metrics: Arc<Metrics>,
}
impl Iterator for Callback {
    type Item = f32;
    fn next(&mut self) -> Option<f32> {
        self.next_with(|| {})
    }
}
impl Callback {
    fn next_with(&mut self, mut dequeued: impl FnMut()) -> Option<f32> {
        let epoch = self.metrics.epoch.load(Ordering::Acquire);
        let flushed = self.at % 2 == 0 && self.epoch != epoch;
        if flushed {
            self.at = SAMPLES;
        }
        if self.at == SAMPLES {
            self.epoch = epoch;
            self.block = [0.; SAMPLES];
            self.movie = 0;
            let mut found = false;
            // The queue has at most eight slots; stale prefetch is discarded in
            // bounded work without changing either endpoint's ownership.
            for _ in 0..READY_SLOTS {
                let Some(block) = self.ready.pop() else {
                    break;
                };
                dequeued();
                self.metrics
                    .queued_samples
                    .fetch_sub(SAMPLES, Ordering::Relaxed);
                if block.epoch == epoch {
                    self.block = block.samples;
                    if let Some(samples) = block.without_movie { self.without_movie = samples; }
                    self.movie = block.movie;
                    found = true;
                    break;
                }
            }
            if !found {
                self.metrics.starved.fetch_add(1, Ordering::Relaxed);
                if flushed {
                    self.metrics.flush_silence.fetch_add(1, Ordering::Relaxed);
                }
            }
            self.at = 0;
        }
        if self.at % 2 == 0 {
            self.use_movie = self.movie == 0 || self.metrics.movie.load(Ordering::Acquire) == self.movie;
        }
        let v = if self.use_movie { self.block[self.at] } else { self.without_movie[self.at] };
        self.at += 1;
        Some(v)
    }
}
impl Source for Callback {
    fn current_frame_len(&self) -> Option<usize> {
        None
    }
    fn channels(&self) -> u16 {
        2
    }
    fn sample_rate(&self) -> u32 {
        44100
    }
    fn total_duration(&self) -> Option<Duration> {
        None
    }
}
pub struct Worker {
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
    reporter: Option<JoinHandle<()>>,
}
impl Drop for Worker {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        for handle in [self.handle.take(), self.reporter.take()].into_iter().flatten() {
            let _ = handle.join();
        }
    }
}
/// Thread placement readback, recorded by the thread it describes.
#[derive(Clone, Copy, Default)]
pub struct ThreadPolicy {
    host: bool,
    id: i32,
    priority_requested: i32,
    priority_rc: i32,
    affinity_requested: i32,
    affinity_rc: i32,
    info_rc: i32,
    priority: i32,
    affinity: i32,
    stack: i32,
}
impl ThreadPolicy {
    fn applied(&self) -> bool {
        self.host
            || (self.priority_rc >= 0
                && self.affinity_rc >= 0
                && self.info_rc >= 0
                && self.priority == self.priority_requested
                && self.affinity == self.affinity_requested)
    }
    pub fn describe(&self) -> String {
        if self.host {
            format!("host policy=normal stack_bytes={}", self.stack)
        } else {
            format!("id={} priority_requested={} priority_rc={} affinity_requested={:#x} affinity_rc={} info_rc={} effective_priority={} effective_affinity={:#x} stack_bytes={}",
                self.id, self.priority_requested, self.priority_rc, self.affinity_requested, self.affinity_rc, self.info_rc, self.priority, self.affinity, self.stack)
        }
    }
}
/// Applies priority and, when given, CPU affinity to the calling thread.
pub fn thread_policy(priority: i32, affinity: i32) -> ThreadPolicy {
    #[cfg(target_os = "vita")]
    unsafe {
        #[repr(C)]
        struct Info {
            size: u32,
            process: i32,
            name: [u8; 32],
            attr: u32,
            status: u32,
            entry: u32,
            stack: u32,
            stack_size: i32,
            init_priority: i32,
            priority: i32,
            init_affinity: i32,
            affinity: i32,
            cpu: i32,
            last_cpu: i32,
            wait_type: u32,
            wait_id: i32,
            exit_status: i32,
            run_clocks: u64,
            intr_preempt: u32,
            preempt: u32,
            release: u32,
            changes: i32,
            notify: i32,
            reserved: i32,
        }
        const _: [(); 0x80] = [(); std::mem::size_of::<Info>()];
        extern "C" {
            fn sceKernelGetThreadId() -> i32;
            fn sceKernelChangeThreadPriority(id: i32, priority: i32) -> i32;
            fn sceKernelChangeThreadCpuAffinityMask(id: i32, mask: i32) -> i32;
            fn sceKernelGetThreadInfo(id: i32, info: *mut Info) -> i32;
        }
        let id = sceKernelGetThreadId();
        let priority_rc = sceKernelChangeThreadPriority(id, priority);
        let affinity_rc = if affinity != 0 { sceKernelChangeThreadCpuAffinityMask(id, affinity) } else { 0 };
        let mut info: Info = std::mem::zeroed();
        info.size = std::mem::size_of::<Info>() as u32;
        let info_rc = sceKernelGetThreadInfo(id, &mut info);
        ThreadPolicy {
            host: false,
            id,
            priority_requested: priority,
            priority_rc,
            affinity_requested: if affinity != 0 { affinity } else { info.affinity },
            affinity_rc,
            info_rc,
            priority: info.priority,
            affinity: info.affinity,
            stack: info.stack_size,
        }
    }
    #[cfg(not(target_os = "vita"))]
    {
        let _ = (priority, affinity);
        ThreadPolicy { host: true, stack: 1024 * 1024, ..Default::default() }
    }
}
/// One second of producer statistics, built without allocating so the
/// producer can hand it to the reporter thread instead of printing.
#[derive(Clone, Copy, Default)]
struct Window {
    window_ms: u128,
    blocks: usize,
    mix: [f64; 3],
    starved: usize,
    flush_silence: usize,
    rejected: usize,
    flushes: usize,
    queued: usize,
    api_max_us: u32,
    live_peak: usize,
    game_stage: (u32, u32),
    stages: Option<([[f64; 3]; 7], usize)>,
    voices: (usize, usize),
    /// Longest interval between producer loop wakeups, its 1 ms sleep
    /// overshoot, and wakeups more than one block apart.
    gap_max_ms: f64,
    sleep_over_ms: f64,
    gaps_over_block: u32,
    /// Time the producer spent printing the previous window (direct mode).
    log_ms: f64,
    dropped_reports: u32,
    music: Option<[f64; 6]>,
}
#[derive(Clone, Copy)]
enum Report {
    Policy(ThreadPolicy, block::Settings),
    Window(Window),
}
fn percentiles(values: &mut [f64]) -> [f64; 3] {
    values.sort_by(f64::total_cmp);
    let n = values.len();
    if n == 0 {
        return [0.; 3];
    }
    [values[n / 2], values[(n - 1) * 95 / 100], values[n - 1]]
}
fn print(report: &Report, config: block::Settings) {
    match report {
        Report::Policy(p, c) => eprintln!("[lg/audio-producer] {} ready_blocks={} cushion_ms={:.3} log_path={}",
            p.describe(), c.cushion, c.cushion as f64 * 512. / 44.1, if c.log_thread { "reporter" } else { "producer" }),
        Report::Window(w) => {
            let [p50, p95, max] = w.mix;
            eprintln!("[lg/audio-realtime] window_ms={} producer_blocks={} producer_mix_p50_ms={p50:.3} producer_mix_p95_ms={p95:.3} producer_mix_max_ms={max:.3} pcm_starved_blocks={} rejected_sinks={} intentional_pcm_flushes={} pcm_flush_silence_blocks={} queued_samples={} game_audio_lock_attempts=0 game_audio_lock_wait_max_us=0 producer_game_control_lock_attempts=0 producer_lock_wait_max_us=0 game_audio_api_max_us={} live_sink_peak={} game_stage={} game_stage_age_ms={} callback_source=bounded-pcm producer_gap_max_ms={:.3} producer_sleep_overshoot_max_ms={:.3} producer_gaps_over_block={} producer_log_ms={:.3} log_path={} dropped_reports={}",
                w.window_ms, w.blocks, w.starved, w.rejected, w.flushes, w.flush_silence, w.queued, w.api_max_us, w.live_peak, w.game_stage.0, w.game_stage.1,
                w.gap_max_ms, w.sleep_over_ms, w.gaps_over_block, w.log_ms, if config.log_thread { "reporter" } else { "producer" }, w.dropped_reports);
            if let Some((stages, n)) = w.stages {
                for (name, [p50, p95, max]) in block::STAGES.iter().zip(stages) {
                    eprintln!("[lg/audio-stage] stage={name} blocks={n} p50_ms={p50:.3} p95_ms={p95:.3} max_ms={max:.3}");
                }
                eprintln!("[lg/audio-voices] audible_ambient={} virtual_ambient={} cap={} cull_gain={}", w.voices.0, w.voices.1, config.voices, config.gain);
            }
            if let Some([decode_ms, chunk_max_ms, chunks, underruns, ring_min_ms, streams]) = w.music {
                eprintln!("[lg/audio-music] decode_ms={decode_ms:.3} chunk_max_ms={chunk_max_ms:.3} chunks={chunks} underrun_frames={underruns} ring_min_ms={ring_min_ms:.1} streams={streams}");
            }
        }
    }
}
pub fn buffered<S: PcmSource>(
    source: S,
    metrics: Arc<Metrics>,
) -> std::io::Result<(Worker, Callback)> {
    buffered_with(source, metrics, block::settings())
}
fn buffered_with<S: PcmSource>(
    mut source: S,
    metrics: Arc<Metrics>,
    config: block::Settings,
) -> std::io::Result<(Worker, Callback)> {
    let (tx, rx) = queue::channel(READY_SLOTS);
    let ready_blocks = config.cushion;
    for _ in 0..ready_blocks {
        assert!(tx
            .push(Block::silence(0))
            .is_ok());
        metrics.queued_samples.fetch_add(SAMPLES, Ordering::Relaxed);
    }
    let stop = Arc::new(AtomicBool::new(false));
    let done = stop.clone();
    let stats = metrics.clone();
    let (reports, inbox) = queue::channel::<Report>(8);
    let reporter = if config.log_thread {
        let done = stop.clone();
        Some(thread::Builder::new().name("audio-report".into()).stack_size(256 * 1024).spawn(move || {
            // Lowest application priority on any core: it may run late, never early.
            let _ = thread_policy(191, 0);
            loop {
                let stopping = done.load(Ordering::Acquire);
                while let Some(report) = inbox.pop() {
                    print(&report, config);
                }
                if stopping {
                    break;
                }
                thread::sleep(Duration::from_millis(100));
            }
        })?)
    } else {
        None
    };
    let handle=thread::Builder::new().name("audio-producer".into()).stack_size(1024*1024).spawn(move||{
        let policy = thread_policy(config.priority, config.affinity);
        let dropped = std::cell::Cell::new(0);
        let emit = |report: Report| -> f64 {
            if config.log_thread {
                if reports.push(report).is_err() { dropped.set(dropped.get() + 1); }
                0.
            } else {
                let start = Instant::now();
                print(&report, config);
                start.elapsed().as_secs_f64() * 1000.
            }
        };
        emit(Report::Policy(policy, config));
        if !policy.applied() { return; }
        let block_ms = SAMPLES as f64 / 2. / 44.1;
        let mut report=Instant::now();let mut timings=Vec::with_capacity(128);
        let mut stages:[Vec<f64>;7]=std::array::from_fn(|_|Vec::with_capacity(128));
        let mut wake = Instant::now();
        let (mut gap_max, mut sleep_over, mut gaps, mut log_ms) = (0f64, 0f64, 0u32, 0f64);
        while !done.load(Ordering::Acquire) {
            let now = Instant::now();
            let gap = (now - wake).as_secs_f64() * 1000.;
            wake = now;
            gap_max = gap_max.max(gap);
            if gap > block_ms { gaps += 1; }
            if let Some(total) = producer_step(&mut source, &tx, &stats, ready_blocks, |_| {}) {
                timings.push(total);
                if config.mix {let mut costs=block::costs();costs[5]=(total-costs.iter().sum::<f64>()).max(0.);for (dst,v) in stages.iter_mut().zip(costs){dst.push(v);}}
            } else {
                thread::sleep(Duration::from_millis(1));
                sleep_over = sleep_over.max(wake.elapsed().as_secs_f64() * 1000. - 1.);
            }
            if report.elapsed()>=Duration::from_secs(1){
                let music = config.music_thread.then(|| {
                    let m = &super::music::STATS;
                    let ring = m.ring_min.swap(usize::MAX, Ordering::Relaxed);
                    [m.decode_us.swap(0, Ordering::Relaxed) as f64 / 1000., m.chunk_max_us.swap(0, Ordering::Relaxed) as f64 / 1000.,
                     m.chunks.swap(0, Ordering::Relaxed) as f64, m.underrun_frames.swap(0, Ordering::Relaxed) as f64,
                     if ring == usize::MAX { -1. } else { ring as f64 / 2. / 44.1 }, m.streams.load(Ordering::Relaxed) as f64]
                });
                let n = timings.len();
                let window = Window {
                    window_ms: report.elapsed().as_millis(), blocks: n, mix: percentiles(&mut timings),
                    starved: stats.starved.swap(0,Ordering::Relaxed), flush_silence: stats.flush_silence.swap(0,Ordering::Relaxed), rejected: stats.rejected.swap(0,Ordering::Relaxed),
                    flushes: stats.flushes.swap(0,Ordering::Relaxed), queued: stats.queued_samples.load(Ordering::Relaxed),
                    api_max_us: super::sink::API_MAX_US.swap(0,Ordering::Relaxed), live_peak: stats.active_peak.swap(0,Ordering::Relaxed),
                    game_stage: super::game_stage_snapshot(),
                    stages: config.mix.then(|| (std::array::from_fn(|i| percentiles(&mut stages[i])), stages[0].len())),
                    voices: (stats.audible_ambient.load(Ordering::Relaxed), stats.virtual_ambient.load(Ordering::Relaxed)),
                    gap_max_ms: gap_max, sleep_over_ms: sleep_over.max(0.), gaps_over_block: gaps, log_ms,
                    dropped_reports: dropped.get(), music,
                };
                log_ms = emit(Report::Window(window));
                for values in &mut stages { values.clear(); }
                (gap_max, sleep_over, gaps) = (0., 0., 0);
                timings.clear();report=Instant::now();
            }
        }
    })?;
    Ok((
        Worker {
            stop,
            handle: Some(handle),
            reporter,
        },
        Callback {
            ready: rx,
            block: [0.; SAMPLES],
            without_movie: [0.; SAMPLES],
            movie: 0,
            use_movie: true,
            at: SAMPLES,
            epoch: 0,
            metrics,
        },
    ))
}
#[cfg(test)]
mod tests {
    use super::*;
    struct Plain<S>(S);
    impl<S: Source<Item=f32> + Send + 'static> PcmSource for Plain<S> {
        fn pcm_block(&mut self, epoch: u32) -> Block {
            let mut block = Block::silence(epoch);
            for v in &mut block.samples { *v = self.0.next().unwrap_or(0.); }
            block
        }
    }
    /// Synthetic 86-sink scene matching the hedge3 voice inventory (8 audible,
    /// 69 virtual ambience, 9 uncapped sources). No original game data.
    fn hedge_inventory_pcm(timed: bool) -> (String, Vec<f64>) {
        use sha2::{Digest, Sha256};
        let (control, mut mixer) = mixer();
        mixer.config.mix = true;
        mixer.config.voices = 8;
        mixer.config.gain = 0.001;
        let clip = super::super::Clip {
            samples: (0..4001).map(|i| ((i % 113) as f32 - 56.) / 1120.).collect::<Vec<_>>().into(),
            rate: 11025,
        };
        let mut sinks = Vec::new();
        for i in 0..86 {
            let sink = control.sink();
            if i < 77 { sink.set_ambient(); }
            sink.set_volume(0.05 + (i % 17) as f32 * 0.01);
            let pan = super::super::Pan::new();
            pan.set(0.4, 0.7);
            let acoustic = super::super::acoustics::Controls::new();
            acoustic.set(0.3, false, 0.6);
            sink.append_effect(super::super::Pcm {
                clip: clip.clone(), index: i * 14, looping: true, pan,
            }, acoustic);
            sinks.push(sink);
        }
        let mut times = Vec::new();
        let mut digest = Sha256::new();
        for block in 0..600 {
            if block % 97 == 0 { sinks[block % 77].set_volume(0.9); }
            if block == 200 { sinks[80].pause(); }
            if block == 220 { sinks[80].play(); }
            let start = timed.then(Instant::now);
            mixer.render();
            if let Some(start) = start { times.push(start.elapsed().as_secs_f64() * 1000.); }
            for sample in mixer.block { digest.update(sample.to_bits().to_le_bytes()); }
        }
        assert_eq!(control.metrics.virtual_ambient.load(Ordering::Relaxed), 69);
        (format!("{:x}", digest.finalize()), times)
    }
    #[test]
    #[ignore = "requires pinned r3j audio configuration; PCM equivalence only, no timing"]
    fn hedge_voice_inventory_pcm_without_timing() {
        let config = block::settings();
        assert!(config.mix && config.preresample && config.parameters
            && config.voices == 8 && config.gain == 0.001,
            "set LOOKING_GLASS_TEST_AUDIO_CONFIG to the pinned r3j reference config");
        let (hash, times) = hedge_inventory_pcm(false);
        assert!(times.is_empty());
        println!("hedge_voice_inventory pcm_sha256={hash} timing=false");
        assert_eq!(hash, "a33978c46f2bf621e70015352f86604c360af0d92ae662e9a72271743253c1dd");
    }
    #[test]
    #[ignore = "host timing/equivalence diagnostic; run explicitly in release mode"]
    fn hedge_voice_inventory_profile() {
        let (hash, mut times) = hedge_inventory_pcm(true);
        println!("hedge_voice_inventory p50/p95/max_ms={:?} pcm_sha256={hash}", percentiles(&mut times));
    }
    #[test]
    fn ambience_cap_reselects_gain_and_preserves_player_and_voice_time() {
        let (control,mut mixer)=mixer();
        mixer.config.mix=true;mixer.config.voices=1;mixer.config.gain=0.01;
        let a=control.sink();a.set_ambient();a.set_volume(0.5);
        let b=control.sink();b.set_ambient();b.set_volume(0.25);
        let idle=control.sink();idle.set_ambient();idle.set_volume(1.);
        let player=control.sink();
        for sink in [&a,&b,&player]{sink.append(rodio::buffer::SamplesBuffer::new(2,44100,vec![0.1;4096]));}
        for _ in 0..SAMPLES{assert!((mixer.next().unwrap()-0.15).abs()<1e-6);}
        assert_eq!(a.get_pos(),b.get_pos());assert!(a.get_pos()>Duration::ZERO);
        a.set_volume(0.05);b.set_volume(0.7);
        // One block crossfades a out and b in; the next is steady.
        let mut last=0.15;
        for _ in 0..SAMPLES{let v=mixer.next().unwrap();assert!(v>=last-1e-6&&v<=0.17+1e-6);last=v;}
        assert!((last-0.17).abs()<1e-6);
        for _ in 0..SAMPLES{assert!((mixer.next().unwrap()-0.17).abs()<1e-6);}
        assert_eq!(control.metrics.audible_ambient.load(Ordering::Relaxed),1);
        assert_eq!(control.metrics.virtual_ambient.load(Ordering::Relaxed),2);
    }
    /// Largest sample-to-sample step and lowest sample of the mixed output over
    /// `blocks` blocks, calling `before` ahead of each block.
    fn max_step(mixer:&mut Mixer,blocks:usize,mut before:impl FnMut(usize))->(f32,f32){
        let mut prev=None;let mut worst=0f32;let mut low=f32::MAX;
        for block in 0..blocks{
            before(block);
            for _ in 0..SAMPLES/2{
                let frame=[mixer.next().unwrap(),mixer.next().unwrap()];
                if let Some(p)=prev{let p:[f32;2]=p;worst=worst.max((frame[0]-p[0]).abs()).max((frame[1]-p[1]).abs());}
                low=low.min(frame[0]);prev=Some(frame);
            }
        }
        (worst,low)
    }
    #[test]
    fn ambience_near_the_cap_boundary_fades_without_discontinuities() {
        let (control,mut mixer)=mixer();
        mixer.config.mix=true;mixer.config.voices=8;mixer.config.gain=0.001;
        let dc=|v:f32|rodio::buffer::SamplesBuffer::new(2,44100,vec![v;2*44100]);
        let held:Vec<_>=(0..7).map(|_|{let s=control.sink();s.set_ambient();s.set_volume(0.5);s.append(dc(0.1));s}).collect();
        // Opposite DC makes any hard swap between the two contenders a large step.
        let a=control.sink();a.set_ambient();a.set_volume(0.30);a.append(dc(0.5));
        let b=control.sink();b.set_ambient();b.set_volume(0.29);b.append(dc(-0.5));
        for _ in 0..SAMPLES{mixer.next();}
        // Near-equal gains trading 8th place every block.
        let (near,low)=max_step(&mut mixer,40,|block|{
            let (x,y)=if block%2==0{(0.29,0.30)}else{(0.30,0.29)};
            a.set_volume(x);b.set_volume(y);
        });
        assert!(near<0.001,"near-equal swap step {near}");
        // The incumbent (positive DC) holds 8th place throughout.
        assert!(low>7.*0.05+0.5*0.29-1e-5,"contender swapped in: {low}");
        // A real overtake crossfades over one block, then holds.
        let (overtake,_)=max_step(&mut mixer,4,|_|{a.set_volume(0.30);b.set_volume(0.6);});
        assert!(overtake<0.001,"overtake step {overtake}");
        let steady=mixer.next().unwrap();
        assert!((steady-(7.*0.05-0.5*0.6)).abs()<1e-5,"{steady}");
        // Without the fade the same overtake would step by about 0.45.
        drop(held);
    }
    #[test]
    fn eight_block_cushion_with_reporter_thread_delivers_in_order() {
        let mut config=block::settings();config.cushion=8;config.log_thread=true;
        let metrics=Arc::new(Metrics::default());
        let (worker,mut callback)=buffered_with(Plain(rodio::buffer::SamplesBuffer::new(2,44100,vec![0.5;2*44100])),metrics.clone(),config).unwrap();
        assert_eq!(metrics.queued_samples.load(Ordering::Relaxed),8*SAMPLES);
        // Eight silent prefill blocks (92.88 ms), then producer output.
        assert!(callback.by_ref().take(8*SAMPLES).all(|v|v==0.));
        std::thread::sleep(Duration::from_millis(50));
        assert!(callback.by_ref().take(4*SAMPLES).all(|v|v==0.5));
        assert_eq!(metrics.starved.load(Ordering::Relaxed),0);
        drop(worker);
    }
    #[test]
    fn film_stop_flush_rejects_already_prefetched_samples() {
        let (tx, rx) = queue::channel(4);
        let metrics = Arc::new(Metrics::default());
        assert!(tx
            .push(Block {
                epoch: 0,
                samples: [0.8; SAMPLES], ..Block::silence(0)
            })
            .is_ok());
        metrics.queued_samples.store(SAMPLES, Ordering::Relaxed);
        metrics.epoch.store(1, Ordering::Release);
        let mut callback = Callback {
            ready: rx,
            block: [0.; SAMPLES],
            at: SAMPLES,
            epoch: 0,
            without_movie: [0.; SAMPLES], movie: 0, use_movie: true,
            metrics: metrics.clone(),
        };
        assert!(callback.by_ref().take(SAMPLES).all(|v| v == 0.));
        assert_eq!(metrics.queued_samples.load(Ordering::Relaxed), 0);
        assert_eq!(metrics.starved.load(Ordering::Relaxed), 1);
        assert_eq!(metrics.flush_silence.load(Ordering::Relaxed), 1);
        // A producer still missing a whole block later is a genuine further
        // underrun: the flush must not mask it or keep extending a grace period.
        assert!(callback.by_ref().take(SAMPLES).all(|v| v == 0.));
        assert_eq!(metrics.starved.load(Ordering::Relaxed), 2);
        assert_eq!(metrics.flush_silence.load(Ordering::Relaxed), 1);
        assert!(tx
            .push(Block {
                epoch: 1,
                samples: [0.3; SAMPLES], ..Block::silence(1)
            })
            .is_ok());
        metrics.queued_samples.store(SAMPLES, Ordering::Relaxed);
        assert_eq!(callback.next(), Some(0.3));
        // Fresh PCM already available at another flush needs no silent block.
        metrics.epoch.store(2, Ordering::Release);
        assert!(tx.push(Block { epoch: 2, samples: [0.6; SAMPLES], ..Block::silence(2) }).is_ok());
        metrics.queued_samples.store(SAMPLES, Ordering::Relaxed);
        assert_eq!(callback.next(), Some(0.3)); // finish the current stereo pair
        assert_eq!(callback.next(), Some(0.6));
        assert_eq!(metrics.starved.load(Ordering::Relaxed), 2);
        assert_eq!(metrics.flush_silence.load(Ordering::Relaxed), 1);
    }
    #[test]
    fn callback_never_waits_for_blocked_mixer_and_reports_starvation() {
        use std::sync::mpsc;
        struct Blocked {
            entered: Option<mpsc::Sender<()>>,
            release: mpsc::Receiver<()>,
        }
        impl Iterator for Blocked {
            type Item = f32;
            fn next(&mut self) -> Option<f32> {
                if let Some(tx) = self.entered.take() {
                    tx.send(()).unwrap();
                    self.release.recv().unwrap();
                }
                Some(0.2)
            }
        }
        impl Source for Blocked {
            fn current_frame_len(&self) -> Option<usize> {
                None
            }
            fn channels(&self) -> u16 {
                2
            }
            fn sample_rate(&self) -> u32 {
                44100
            }
            fn total_duration(&self) -> Option<Duration> {
                None
            }
        }
        let (entered, seen) = mpsc::channel();
        let (release, blocked) = mpsc::channel();
        let metrics = Arc::new(Metrics::default());
        let (worker, mut callback) = buffered(
            Plain(Blocked {
                entered: Some(entered),
                release: blocked,
            }),
            metrics.clone(),
        )
        .unwrap();
        callback
            .by_ref()
            .take(block::settings().cushion * SAMPLES)
            .for_each(drop);
        seen.recv_timeout(Duration::from_secs(1)).unwrap();
        let start = Instant::now();
        let samples: Vec<_> = callback.by_ref().take(SAMPLES).collect();
        assert!(start.elapsed() < Duration::from_millis(5));
        assert!(samples.iter().all(|v| *v == 0.));
        assert_eq!(metrics.starved.load(Ordering::Relaxed), 1);
        assert_eq!(metrics.flush_silence.load(Ordering::Relaxed), 0);
        release.send(()).unwrap();
        drop(worker);
    }
}

#[cfg(test)]
#[path = "output/film_tests.rs"]
mod film_tests;
