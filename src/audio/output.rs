//! The game publishes source handles without locks. Optional PCM buffering keeps
//! every source/DSP/decoder call off the high-priority device callback.
use super::{
    block, queue,
    sink::{Output, Sink},
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
    #[cfg(feature = "native_timing")]
    pub raw_starvation: std::sync::OnceLock<Arc<crate::native_timing::audio_counters::Raw>>,
    pub rejected: AtomicUsize,
    pub starved: AtomicUsize,
    pub queued_samples: AtomicUsize,
    pub active_peak: AtomicUsize,
    pub audible_ambient: AtomicUsize,
    pub virtual_ambient: AtomicUsize,
    epoch: AtomicU32,
    flushes: AtomicUsize,
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
        if self.pending.push(output).is_err() {
            self.metrics.rejected.fetch_add(1, Ordering::Relaxed);
            eprintln!("[lg/audio-control] mixer_queue_full dropped=1 wait_us=0");
        }
        sink
    }
}
impl Mixer {
    fn render(&mut self) {
        block::reset();
        let start = Instant::now();
        while let Some(s) = self.pending.pop() {
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
        let mut scratch = [0.; SAMPLES];
        let mut dead = Vec::new();
        for (i, s) in self.sources.iter_mut().enumerate() {
            let audible = s.selected || s.rank().is_none();
            if !s.render(&mut scratch, audible) {
                dead.push(i);
                continue;
            }
            let start = Instant::now();
            for (dst, src) in self.block.iter_mut().zip(scratch.iter()) {
                *dst += *src;
            }
            block::charge(4, start);
        }
        for i in dead.into_iter().rev() {
            self.sources.remove(i);
        }
        self.at = 0;
    }
}
impl Iterator for Mixer {
    type Item = f32;
    fn next(&mut self) -> Option<f32> {
        if self.config.mix {
            if self.at == SAMPLES {
                self.render();
            }
            let v = self.block[self.at];
            self.at += 1;
            return Some(v);
        }
        // Moving pending owned sources cannot run a decoder. Each Output
        // normalizes its source only when the consumer first asks for samples.
        let mut added = false;
        while let Some(source) = self.pending.pop() {
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
        self.sources.retain_mut(|s| match s.next() {
            Some(v) => {
                sum += v;
                true
            }
            None => false,
        });
        Some(sum)
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
struct Block {
    epoch: u32,
    samples: [f32; SAMPLES],
}
pub struct Callback {
    ready: queue::Receiver<Block>,
    block: [f32; SAMPLES],
    at: usize,
    epoch: u32,
    metrics: Arc<Metrics>,
}
impl Iterator for Callback {
    type Item = f32;
    fn next(&mut self) -> Option<f32> {
        let epoch = self.metrics.epoch.load(Ordering::Acquire);
        if self.at % 2 == 0 && self.epoch != epoch {
            self.at = SAMPLES;
        }
        if self.at == SAMPLES {
            self.epoch = epoch;
            self.block = [0.; SAMPLES];
            let mut found = false;
            // The queue has at most eight slots; stale prefetch is discarded in
            // bounded work without changing either endpoint's ownership.
            for _ in 0..READY_SLOTS {
                let Some(block) = self.ready.pop() else {
                    break;
                };
                self.metrics
                    .queued_samples
                    .fetch_sub(SAMPLES, Ordering::Relaxed);
                if block.epoch == epoch {
                    self.block = block.samples;
                    found = true;
                    break;
                }
            }
            if !found {
                #[cfg(feature = "native_timing")]
                if let Some(raw) = self.metrics.raw_starvation.get() {
                    raw.event_with(|| { self.metrics.starved.fetch_add(1, Ordering::Relaxed); });
                } else { self.metrics.starved.fetch_add(1, Ordering::Relaxed); }
                #[cfg(not(feature = "native_timing"))]
                self.metrics.starved.fetch_add(1, Ordering::Relaxed);
            }
            self.at = 0;
        }
        let v = self.block[self.at];
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
            eprintln!("[lg/audio-realtime] window_ms={} producer_blocks={} producer_mix_p50_ms={p50:.3} producer_mix_p95_ms={p95:.3} producer_mix_max_ms={max:.3} pcm_starved_blocks={} rejected_sinks={} intentional_pcm_flushes={} queued_samples={} game_audio_lock_attempts=0 game_audio_lock_wait_max_us=0 producer_game_control_lock_attempts=0 producer_lock_wait_max_us=0 game_audio_api_max_us={} live_sink_peak={} game_stage={} game_stage_age_ms={} callback_source=bounded-pcm producer_gap_max_ms={:.3} producer_sleep_overshoot_max_ms={:.3} producer_gaps_over_block={} producer_log_ms={:.3} log_path={} dropped_reports={}",
                w.window_ms, w.blocks, w.starved, w.rejected, w.flushes, w.queued, w.api_max_us, w.live_peak, w.game_stage.0, w.game_stage.1,
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
pub fn buffered<S: Source<Item = f32> + Send + 'static>(
    source: S,
    metrics: Arc<Metrics>,
) -> std::io::Result<(Worker, Callback)> {
    buffered_with(source, metrics, block::settings())
}
fn buffered_with<S: Source<Item = f32> + Send + 'static>(
    mut source: S,
    metrics: Arc<Metrics>,
    config: block::Settings,
) -> std::io::Result<(Worker, Callback)> {
    let (tx, rx) = queue::channel(READY_SLOTS);
    let ready_blocks = config.cushion;
    for _ in 0..ready_blocks {
        assert!(tx
            .push(Block {
                epoch: 0,
                samples: [0.; SAMPLES]
            })
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
            if tx.len()>=ready_blocks {
                thread::sleep(Duration::from_millis(1));
                sleep_over = sleep_over.max(wake.elapsed().as_secs_f64() * 1000. - 1.);
            }
            else {
                let epoch=stats.epoch.load(Ordering::Acquire);
                let start=Instant::now();let mut block=[0.;SAMPLES];
                for v in &mut block {*v=source.next().unwrap_or(0.);}
                let total=start.elapsed().as_secs_f64()*1000.;timings.push(total);
                if config.mix {let mut costs=block::costs();costs[5]=(total-costs.iter().sum::<f64>()).max(0.);for (dst,v) in stages.iter_mut().zip(costs){dst.push(v);}}
                stats.queued_samples.fetch_add(SAMPLES,Ordering::Relaxed);
                if tx.push(Block{epoch,samples:block}).is_err(){stats.queued_samples.fetch_sub(SAMPLES,Ordering::Relaxed);}
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
                    starved: stats.starved.swap(0,Ordering::Relaxed), rejected: stats.rejected.swap(0,Ordering::Relaxed),
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
            at: SAMPLES,
            epoch: 0,
            metrics,
        },
    ))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(feature = "native_timing")]
    #[test]
    fn native_raw_starvation_observes_actual_callback_late_binding_and_legacy_resets() {
        use crate::native_timing::{audio_counters::Tag, clock::Native, Metadata, Recorder};
        let (tx,rx)=queue::channel(4);let metrics=Arc::new(Metrics::default());
        let mut callback=Callback{ready:rx,block:[0.;SAMPLES],at:SAMPLES,epoch:0,metrics:metrics.clone()};
        // The callback has already run before the viewer Session can bind.
        assert!(callback.by_ref().take(SAMPLES).all(|v|v==0.));
        assert_eq!(metrics.starved.load(Ordering::Relaxed),1);
        assert!(metrics.raw_starvation.get().is_none());
        let mut recorder=Recorder::new(Native,Some([7;16]),Metadata::default());
        recorder.raw_requested=true;recorder.bind_raw_counters(Some(&metrics.raw_starvation),None);
        assert_eq!(recorder.raw_counters.as_ref().unwrap().snapshots[0].value,0);
        let scope=crate::kernel_alloc::Scope::begin();
        for _ in 0..2 { assert!(callback.by_ref().take(SAMPLES).all(|v|v==0.)); }
        assert_eq!(scope.finish(),0);
        assert_eq!(metrics.starved.swap(0,Ordering::Relaxed),3);
        recorder.sample_raw_counters(Tag::Requested);
        assert_eq!(recorder.raw_counters.as_ref().unwrap().snapshots[1].value,2);
        tx.push(Block{epoch:0,samples:[0.3;SAMPLES]}).ok().unwrap();
        metrics.queued_samples.store(SAMPLES,Ordering::Relaxed);
        assert!(callback.by_ref().take(SAMPLES).all(|v|v==0.3));
        recorder.sample_raw_counters(Tag::Requested);
        assert_eq!(recorder.raw_counters.as_ref().unwrap().snapshots[2].value,2);
        // Intentional flush invalidates stale PCM. It remains RAW starvation;
        // this baseline has no film-branch flush subset to subtract.
        tx.push(Block{epoch:0,samples:[0.8;SAMPLES]}).ok().unwrap();
        metrics.queued_samples.store(SAMPLES,Ordering::Relaxed);
        metrics.epoch.store(1,Ordering::Release);
        assert!(callback.by_ref().take(SAMPLES).all(|v|v==0.));
        recorder.sample_raw_counters(Tag::Requested);
        assert_eq!(recorder.raw_counters.as_ref().unwrap().snapshots[3].value,3);
        assert_eq!(metrics.starved.load(Ordering::Relaxed),1);
    }
    #[cfg(feature = "native_timing")]
    #[test]
    fn host_only_counter_callback_cost_and_event_counts() {
        use crate::native_timing::{audio_counters::Tag, clock::Native, Metadata, Recorder};
        let mut costs=Vec::new();
        for enabled in [false,true] {
            let (_tx,rx)=queue::channel(4);let metrics=Arc::new(Metrics::default());
            let mut callback=Callback{ready:rx,block:[0.;SAMPLES],at:SAMPLES,epoch:0,metrics:metrics.clone()};
            let mut recorder=Recorder::new(Native,Some([7;16]),Metadata::default());
            recorder.raw_requested=enabled;recorder.bind_raw_counters(Some(&metrics.raw_starvation),None);
            let scope=crate::kernel_alloc::Scope::begin();let start=Instant::now();
            for _ in 0..1024*SAMPLES { std::hint::black_box(callback.next()); }
            let ns=start.elapsed().as_nanos();assert_eq!(scope.finish(),0);
            assert_eq!(metrics.starved.load(Ordering::Relaxed),1024);
            if enabled {
                recorder.sample_raw_counters(Tag::Requested);
                assert_eq!(recorder.raw_counters.as_ref().unwrap().snapshots[1].value,1024);
            } else {assert!(metrics.raw_starvation.get().is_none());}
            costs.push((enabled,ns));
        }
        println!("HOST ONLY actual callback 1048576 scalar samples/1024 starvation events; raw config/enabled+ns={costs:?}; zero hot allocations; not Vita estimate");
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
        let (worker,mut callback)=buffered_with(rodio::buffer::SamplesBuffer::new(2,44100,vec![0.5;2*44100]),metrics.clone(),config).unwrap();
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
                samples: [0.8; SAMPLES]
            })
            .is_ok());
        metrics.queued_samples.store(SAMPLES, Ordering::Relaxed);
        metrics.epoch.store(1, Ordering::Release);
        let mut callback = Callback {
            ready: rx,
            block: [0.; SAMPLES],
            at: SAMPLES,
            epoch: 0,
            metrics: metrics.clone(),
        };
        assert!(callback.by_ref().take(SAMPLES).all(|v| v == 0.));
        assert_eq!(metrics.queued_samples.load(Ordering::Relaxed), 0);
        assert!(tx
            .push(Block {
                epoch: 1,
                samples: [0.3; SAMPLES]
            })
            .is_ok());
        metrics.queued_samples.store(SAMPLES, Ordering::Relaxed);
        assert_eq!(callback.next(), Some(0.3));
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
            Blocked {
                entered: Some(entered),
                release: blocked,
            },
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
        release.send(()).unwrap();
        drop(worker);
    }
}
