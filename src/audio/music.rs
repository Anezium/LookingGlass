//! Music decoding off the mix producer. One lower-priority thread decodes every
//! live music source into its own single-producer/single-consumer PCM ring; the
//! producer copies decoded samples and never waits. An empty ring yields
//! silence for whole frames and is counted, so a decoder burst costs at most a
//! music dropout, never a late mix block.
use rodio::Source;
use std::{
    cell::UnsafeCell,
    sync::{
        atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering},
        Arc, Mutex, OnceLock,
    },
    time::{Duration, Instant},
};

/// 32,768 stereo frames: 0.74 s of 44.1 kHz music, 256 KiB per live stream.
const RING_SAMPLES: usize = 1 << 16;
/// Frames decoded per pass over a stream.
const CHUNK_FRAMES: usize = 1024;
/// Samples the reader takes per index exchange (about 23 ms of stereo).
const BATCH: usize = 2048;

/// Decoder-thread counters, swapped by the producer once per report window.
pub struct Stats {
    pub decode_us: AtomicU32,
    pub chunk_max_us: AtomicU32,
    pub chunks: AtomicU32,
    pub underrun_frames: AtomicU32,
    /// Lowest ring fill seen by the producer, in samples (usize::MAX when unseen).
    pub ring_min: AtomicUsize,
    pub streams: AtomicUsize,
}
#[cfg(feature = "native_timing")]
pub(super) static RAW_UNDERRUNS: OnceLock<Arc<crate::native_timing::audio_counters::Raw>> = OnceLock::new();
pub static STATS: Stats = Stats {
    decode_us: AtomicU32::new(0),
    chunk_max_us: AtomicU32::new(0),
    chunks: AtomicU32::new(0),
    underrun_frames: AtomicU32::new(0),
    ring_min: AtomicUsize::new(usize::MAX),
    streams: AtomicUsize::new(0),
};

struct Ring {
    slots: Box<[UnsafeCell<f32>]>,
    read: AtomicUsize,
    write: AtomicUsize,
    eof: AtomicBool,
    closed: AtomicBool,
}
// Samples in [read, write) belong to the consumer; the rest to the decoder.
// Each side publishes its index with Release after touching its own slots.
unsafe impl Sync for Ring {}
unsafe impl Send for Ring {}
impl Ring {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            slots: (0..RING_SAMPLES).map(|_| UnsafeCell::new(0.)).collect(),
            read: AtomicUsize::new(0),
            write: AtomicUsize::new(0),
            eof: AtomicBool::new(false),
            closed: AtomicBool::new(false),
        })
    }
    fn free(&self) -> usize {
        RING_SAMPLES
            - self
                .write
                .load(Ordering::Relaxed)
                .wrapping_sub(self.read.load(Ordering::Acquire))
    }
    /// Decoder side; the caller checked `free`.
    fn push(&self, samples: &[f32]) {
        let write = self.write.load(Ordering::Relaxed);
        for (i, v) in samples.iter().enumerate() {
            unsafe { *self.slots[write.wrapping_add(i) % RING_SAMPLES].get() = *v };
        }
        self.write
            .store(write.wrapping_add(samples.len()), Ordering::Release);
    }
}

struct Job {
    source: Box<dyn Source<Item = f32> + Send>,
    ring: Arc<Ring>,
    channels: usize,
}

/// Producer-side reader. Reports the original rate and layout, so the block
/// stream resamples exactly as it would the decoder itself.
pub struct RingSource {
    ring: Arc<Ring>,
    read: usize,
    ready: usize,
    rate: u32,
    channels: u16,
    /// Remaining samples of a silent underrun frame.
    silence: usize,
    phase: usize,
}
impl Iterator for RingSource {
    type Item = f32;
    fn next(&mut self) -> Option<f32> {
        if self.silence > 0 {
            self.silence -= 1;
            return Some(0.);
        }
        if self.ready == 0 {
            // Publish what was consumed, then look for more in one exchange.
            // Take at most BATCH so the decoder sees freed space promptly.
            self.ring.read.store(self.read, Ordering::Release);
            let eof = self.ring.eof.load(Ordering::Acquire);
            let available = self
                .ring
                .write
                .load(Ordering::Acquire)
                .wrapping_sub(self.read);
            STATS.ring_min.fetch_min(available, Ordering::Relaxed);
            self.ready = available.min(BATCH);
            if self.ready == 0 {
                if eof {
                    return None;
                }
                // The decoder publishes whole frames, so this is a frame start.
                debug_assert_eq!(self.phase, 0);
                #[cfg(feature = "native_timing")]
                if let Some(raw) = RAW_UNDERRUNS.get() {
                    raw.event_with(|| { STATS.underrun_frames.fetch_add(1, Ordering::Relaxed); });
                } else { STATS.underrun_frames.fetch_add(1, Ordering::Relaxed); }
                #[cfg(not(feature = "native_timing"))]
                STATS.underrun_frames.fetch_add(1, Ordering::Relaxed);
                self.silence = self.channels as usize - 1;
                return Some(0.);
            }
        }
        let v = unsafe { *self.ring.slots[self.read % RING_SAMPLES].get() };
        self.read = self.read.wrapping_add(1);
        self.ready -= 1;
        self.phase = (self.phase + 1) % self.channels as usize;
        Some(v)
    }
}
impl Source for RingSource {
    fn current_frame_len(&self) -> Option<usize> {
        None
    }
    fn channels(&self) -> u16 {
        self.channels
    }
    fn sample_rate(&self) -> u32 {
        self.rate
    }
    fn total_duration(&self) -> Option<Duration> {
        None
    }
}
impl Drop for RingSource {
    fn drop(&mut self) {
        self.ring.closed.store(true, Ordering::Release);
    }
}

/// Hands `source` to the music thread, starting it on first use. Called by the
/// game thread when it appends to a music sink; the producer never takes this lock.
pub fn decode_ahead(
    source: Box<dyn Source<Item = f32> + Send>,
    priority: i32,
    affinity: i32,
) -> Box<dyn Source<Item = f32> + Send> {
    static JOBS: OnceLock<Mutex<super::queue::Sender<Job>>> = OnceLock::new();
    let rate = source.sample_rate();
    let channels = source.channels();
    if !(8000..=96000).contains(&rate) || !(1..=2).contains(&channels) {
        return source;
    }
    let ring = Ring::new();
    let jobs = JOBS.get_or_init(|| {
        let (tx, rx) = super::queue::channel(16);
        std::thread::Builder::new()
            .name("audio-music".into())
            .stack_size(1024 * 1024)
            .spawn(move || run(rx, priority, affinity))
            .expect("audio-music thread");
        Mutex::new(tx)
    });
    let job = Job {
        source,
        ring: ring.clone(),
        channels: channels as usize,
    };
    if let Err(job) = jobs.lock().unwrap().push(job) {
        // Queue full: decode on the producer as before.
        return job.source;
    }
    Box::new(RingSource {
        ring,
        read: 0,
        ready: 0,
        rate,
        channels,
        silence: 0,
        phase: 0,
    })
}

fn run(jobs: super::queue::Receiver<Job>, priority: i32, affinity: i32) {
    // One startup line from this thread; it never logs afterwards.
    let p = super::output::thread_policy(priority, affinity);
    eprintln!("[lg/audio-music] {} ring_ms={:.1} chunk_frames={CHUNK_FRAMES}", p.describe(), RING_SAMPLES as f64 / 2. / 44.1);
    let mut live: Vec<Job> = Vec::with_capacity(8);
    let mut scratch = vec![0f32; CHUNK_FRAMES * 2];
    loop {
        while let Some(job) = jobs.pop() {
            live.push(job);
        }
        STATS.streams.store(live.len(), Ordering::Relaxed);
        let mut worked = false;
        live.retain_mut(|job| {
            if job.ring.closed.load(Ordering::Acquire) {
                return false;
            }
            let want = CHUNK_FRAMES * job.channels;
            if job.ring.free() < want {
                return true;
            }
            let start = Instant::now();
            let mut n = 0;
            while n < want {
                match job.source.next() {
                    Some(v) => {
                        scratch[n] = v;
                        n += 1;
                    }
                    None => break,
                }
            }
            // Only whole frames are published.
            let whole = n - n % job.channels;
            job.ring.push(&scratch[..whole]);
            let us = start.elapsed().as_micros().min(u32::MAX as u128) as u32;
            STATS.decode_us.fetch_add(us, Ordering::Relaxed);
            STATS.chunk_max_us.fetch_max(us, Ordering::Relaxed);
            STATS.chunks.fetch_add(1, Ordering::Relaxed);
            worked = true;
            if n < want {
                job.ring.eof.store(true, Ordering::Release);
                return false;
            }
            true
        });
        if !worked {
            std::thread::sleep(Duration::from_millis(2));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(feature = "native_timing")]
    #[test]
    fn native_raw_music_counts_actual_unavailable_source_frames_and_not_eof() {
        use crate::native_timing::{audio_counters::{Raw,Capture,Tag},clock::{Checked,Native}};
        let mut clock=Checked::new(Native);
        let raw=Raw::activate(&RAW_UNDERRUNS,&mut clock,Some([8;16]));
        let mut capture=Capture::new(None,Some(raw));capture.sample(&mut clock,Tag::Binding);
        let before=capture.snapshots[0].value;
        let ring=Ring::new();
        let mut source=RingSource{ring:ring.clone(),read:0,ready:0,rate:44100,channels:2,silence:0,phase:0};
        assert_eq!(source.next(),Some(0.));assert_eq!(source.next(),Some(0.));
        capture.sample(&mut clock,Tag::Requested);
        // Other host tests can also pull global music sources concurrently.
        // The isolated Raw unit test pins exact counts; this pins the actual
        // production unavailable-frame branch and channel pairing.
        assert!(capture.snapshots[1].value>=before+1);
        ring.eof.store(true,Ordering::Release);
        assert_eq!(source.next(),None);
        ring.push(&[0.25,0.75]);assert_eq!(source.next(),Some(0.25));assert_eq!(source.next(),Some(0.75));
    }
    #[test]
    fn ring_source_preserves_samples_reports_underruns_as_silent_frames_and_ends() {
        let input: Vec<f32> = (0..5000).map(|i| i as f32).collect();
        let source = Box::new(rodio::buffer::SamplesBuffer::new(2, 22050, input.clone()));
        let mut ring = decode_ahead(source, 170, 0x40000);
        assert_eq!((ring.channels(), ring.sample_rate()), (2, 22050));
        let mut out = Vec::new();
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            match ring.next() {
                Some(v) => out.push(v),
                None => break,
            }
            assert!(Instant::now() < deadline);
        }
        // Underruns insert silent whole frames; real samples keep their order and pairing.
        assert_eq!(out.len() % 2, 0);
        let real: Vec<f32> = out
            .chunks_exact(2)
            .filter(|f| !(f[0] == 0. && f[1] == 0.))
            .flatten()
            .copied()
            .collect();
        assert_eq!(real, input);
    }
}
