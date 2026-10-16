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
const READY_BLOCKS:usize=2;
#[derive(Default)]
pub struct Metrics {
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
                    self.ranks.push((i, gain));
                }
            }
        }
        self.ranks
            .sort_unstable_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
        if config.voices > 0 {
            self.ranks.truncate(config.voices);
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
            let audible = s.rank().is_none() || self.ranks.iter().any(|(j, _)| *j == i);
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
            // The queue has only four slots; stale prefetch is discarded in
            // bounded work without changing either endpoint's ownership.
            for _ in 0..4 {
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
}
impl Drop for Worker {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}
pub fn buffered<S: Source<Item = f32> + Send + 'static>(
    mut source: S,
    metrics: Arc<Metrics>,
) -> std::io::Result<(Worker, Callback)> {
    let (tx, rx) = queue::channel(4);
    let config = block::settings();
    let ready_blocks = 2;
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
    let handle=thread::Builder::new().name("audio-producer".into()).stack_size(1024*1024).spawn(move||{
        if !policy() { return; }
        let mut report=Instant::now();let mut timings=Vec::with_capacity(128);
        let mut stages:[Vec<f64>;7]=std::array::from_fn(|_|Vec::with_capacity(128));
        while !done.load(Ordering::Acquire) {
            if tx.len()>=ready_blocks {thread::sleep(Duration::from_millis(1));}
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
                timings.sort_by(f64::total_cmp);let n=timings.len();
                let (p50,p95,max)=if n>0 {(timings[n/2],timings[(n-1)*95/100],timings[n-1])}else{(0.,0.,0.)};
                eprintln!("[lg/audio-realtime] window_ms={} producer_blocks={n} producer_mix_p50_ms={p50:.3} producer_mix_p95_ms={p95:.3} producer_mix_max_ms={max:.3} pcm_starved_blocks={} rejected_sinks={} intentional_pcm_flushes={} queued_samples={} game_audio_lock_attempts=0 game_audio_lock_wait_max_us=0 producer_game_control_lock_attempts=0 producer_lock_wait_max_us=0 game_audio_api_max_us={} live_sink_peak={} game_stage={} game_stage_age_ms={} callback_source=bounded-pcm",
                    report.elapsed().as_millis(),stats.starved.swap(0,Ordering::Relaxed),stats.rejected.swap(0,Ordering::Relaxed),stats.flushes.swap(0,Ordering::Relaxed),stats.queued_samples.load(Ordering::Relaxed),super::sink::API_MAX_US.swap(0,Ordering::Relaxed),stats.active_peak.swap(0,Ordering::Relaxed),super::game_stage_snapshot().0,super::game_stage_snapshot().1);
                if config.mix {
                    for (name,values) in block::STAGES.iter().zip(stages.iter_mut()){
                        values.sort_by(f64::total_cmp);let n=values.len();
                        if n>0{eprintln!("[lg/audio-stage] stage={name} blocks={n} p50_ms={:.3} p95_ms={:.3} max_ms={:.3}",values[n/2],values[(n-1)*95/100],values[n-1]);}values.clear();
                    }
                    eprintln!("[lg/audio-voices] audible_ambient={} virtual_ambient={} cap={} cull_gain={}",stats.audible_ambient.load(Ordering::Relaxed),stats.virtual_ambient.load(Ordering::Relaxed),config.voices,config.gain);
                }
                timings.clear();report=Instant::now();
            }
        }
    })?;
    Ok((
        Worker {
            stop,
            handle: Some(handle),
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
fn policy() -> bool {
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
        let priority = sceKernelChangeThreadPriority(id, 161);
        let affinity = sceKernelChangeThreadCpuAffinityMask(id, 0x40000);
        let mut info: Info = std::mem::zeroed();
        info.size = std::mem::size_of::<Info>() as u32;
        let readback = sceKernelGetThreadInfo(id, &mut info);
        eprintln!("[lg/audio-producer] id={id} priority_requested=161 priority_rc={priority} affinity_requested=0x40000 affinity_rc={affinity} info_rc={readback} effective_priority={} effective_affinity={:#x} stack_bytes={} ready_blocks={READY_BLOCKS}", info.priority,info.affinity,info.stack_size);
        return priority >= 0
            && affinity >= 0
            && readback >= 0
            && info.priority == 161
            && info.affinity == 0x40000;
    }
    #[cfg(not(target_os = "vita"))]
    {
        eprintln!("[lg/audio-producer] host policy=normal stack_bytes=1048576 ready_blocks={READY_BLOCKS}");
        true
    }
}
#[cfg(test)]
mod tests {
    use super::*;
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
