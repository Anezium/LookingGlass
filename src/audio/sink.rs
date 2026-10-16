//! Game controls are atomics; source ownership crosses a bounded SPSC queue.
//! No callback/renderer mutex, sleep-until-end, or synchronous seek is exposed.
use super::{block, queue};
use rodio::{source::UniformSourceIterator, Source};
use std::{
    sync::{
        atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};
pub static API_MAX_US: AtomicU32 = AtomicU32::new(0);
pub struct ApiTimer(std::time::Instant);
impl ApiTimer {
    pub fn new() -> Self {
        Self(std::time::Instant::now())
    }
}
impl Drop for ApiTimer {
    fn drop(&mut self) {
        API_MAX_US.fetch_max(
            self.0.elapsed().as_micros().min(u32::MAX as u128) as u32,
            Ordering::Relaxed,
        );
    }
}
type SourceBox = Box<dyn Source<Item = f32> + Send>;
struct Packet {
    source: Queued,
    generation: u32,
}
enum Queued {
    Source(SourceBox),
    Effect(block::Effect),
}
enum Current {
    Legacy(UniformSourceIterator<SourceBox, f32>),
    Stream(block::Stream),
    Effect(block::Effect),
}
struct Controls {
    pause: AtomicBool,
    alive: AtomicBool,
    generation: AtomicU32,
    volume: AtomicU32,
    count: AtomicUsize,
    position: AtomicU32,
    rejected: AtomicU32,
    ambient: AtomicBool,
    music: AtomicBool,
}
pub struct Sink {
    controls: Arc<Controls>,
    pending: queue::Sender<Packet>,
}
pub struct Output {
    controls: Arc<Controls>,
    pending: queue::Receiver<Packet>,
    current: Option<Current>,
    generation: u32,
    position: u32,
    channel: bool,
    paused: bool,
    gain: f32,
    /// Gain applied at the end of the last block; None before the first one.
    level: Option<f32>,
    /// Held by the mixer's ambience cap for the next ranking (hysteresis).
    pub(super) selected: bool,
}
impl Sink {
    pub fn new_idle() -> (Self, Output) {
        let controls = Arc::new(Controls {
            pause: AtomicBool::new(false),
            alive: AtomicBool::new(true),
            generation: AtomicU32::new(0),
            volume: AtomicU32::new(1f32.to_bits()),
            count: AtomicUsize::new(0),
            position: AtomicU32::new(0),
            rejected: AtomicU32::new(0),
            ambient: AtomicBool::new(false),
            music: AtomicBool::new(false),
        });
        let (tx, rx) = queue::channel(8);
        (
            Self {
                controls: controls.clone(),
                pending: tx,
            },
            Output {
                controls,
                pending: rx,
                current: None,
                generation: 0,
                position: 0,
                channel: false,
                paused: false,
                gain: 1.,
                level: None,
                selected: false,
            },
        )
    }
    pub fn append<S>(&self, source: S)
    where
        S: Source + Send + 'static,
        S::Item: rodio::Sample,
        f32: rodio::cpal::FromSample<S::Item>,
    {
        let _api = ApiTimer::new();
        if !self.controls.alive.load(Ordering::Acquire) {
            return;
        }
        let packet = Packet {
            source: Queued::Source(Box::new(source.convert_samples::<f32>())),
            generation: self.controls.generation.load(Ordering::Relaxed),
        };
        self.controls.count.fetch_add(1, Ordering::Relaxed);
        if self.pending.push(packet).is_err() {
            self.controls.count.fetch_sub(1, Ordering::Relaxed);
            self.controls.rejected.fetch_add(1, Ordering::Relaxed);
            eprintln!("[lg/audio-control] source_queue_full dropped=1 wait_us=0");
        }
    }
    pub fn set_ambient(&self) {
        self.controls.ambient.store(true, Ordering::Relaxed);
    }
    pub fn set_music(&self) {
        self.controls.music.store(true, Ordering::Relaxed);
    }
    pub(super) fn append_effect(&self, pcm: super::Pcm, controls: Arc<super::acoustics::Controls>) {
        let config = block::settings();
        if !config.mix || !config.preresample || block::step(pcm.clip.rate).is_none() {
            self.append(super::acoustics::Filter::new(pcm, controls));
            return;
        }
        let _api = ApiTimer::new();
        let packet = Packet {
            source: Queued::Effect(block::Effect::new(pcm, controls)),
            generation: self.controls.generation.load(Ordering::Relaxed),
        };
        self.controls.count.fetch_add(1, Ordering::Relaxed);
        if self.pending.push(packet).is_err() {
            self.controls.count.fetch_sub(1, Ordering::Relaxed);
            self.controls.rejected.fetch_add(1, Ordering::Relaxed);
        }
    }
    pub fn pause(&self) {
        let _api = ApiTimer::new();
        self.controls.pause.store(true, Ordering::Relaxed);
    }
    pub fn play(&self) {
        let _api = ApiTimer::new();
        self.controls.pause.store(false, Ordering::Relaxed);
    }
    pub fn is_paused(&self) -> bool {
        let _api = ApiTimer::new();
        self.controls.pause.load(Ordering::Relaxed)
    }
    pub fn set_volume(&self, volume: f32) {
        let _api = ApiTimer::new();
        self.controls
            .volume
            .store(volume.to_bits(), Ordering::Relaxed);
    }
    pub fn volume(&self) -> f32 {
        let _api = ApiTimer::new();
        f32::from_bits(self.controls.volume.load(Ordering::Relaxed))
    }
    pub fn stop(&self) {
        let _api = ApiTimer::new();
        self.controls.generation.fetch_add(1, Ordering::Relaxed);
    }
    pub fn len(&self) -> usize {
        let _api = ApiTimer::new();
        self.controls.count.load(Ordering::Relaxed)
    }
    pub fn empty(&self) -> bool {
        self.len() == 0
    }
    pub fn get_pos(&self) -> Duration {
        let _api = ApiTimer::new();
        Duration::from_secs_f64(self.controls.position.load(Ordering::Relaxed) as f64 / 88200.)
    }
}
impl Drop for Sink {
    fn drop(&mut self) {
        self.stop();
        self.controls.alive.store(false, Ordering::Release);
    }
}
impl Output {
    pub fn rank(&self) -> Option<f32> {
        if self.controls.ambient.load(Ordering::Relaxed) {
            Some(if self.controls.pause.load(Ordering::Relaxed)
                || !self.controls.alive.load(Ordering::Acquire)
                || self.controls.count.load(Ordering::Relaxed) == 0
            {
                0.
            } else {
                f32::from_bits(self.controls.volume.load(Ordering::Relaxed)).abs()
            })
        } else {
            None
        }
    }
    /// Poll controls once per block; no game-owned lock is taken. The applied
    /// gain moves linearly from the previous block's value to this one's, so a
    /// voice dropped by the ambience cap fades out over one block and a
    /// returning voice fades in from silence.
    pub fn render(&mut self, out: &mut [f32], audible: bool) -> bool {
        out.fill(0.);
        if !self.controls.alive.load(Ordering::Acquire) {
            self.discard_current();
            while self.pending.pop().is_some() {
                self.controls.count.fetch_sub(1, Ordering::Relaxed);
            }
            return false;
        }
        let generation = self.controls.generation.load(Ordering::Acquire);
        if generation != self.generation {
            self.discard_current();
            self.generation = generation;
        }
        if self.controls.pause.load(Ordering::Relaxed) {
            return true;
        }
        let gain = f32::from_bits(self.controls.volume.load(Ordering::Relaxed));
        let target = if audible { gain } else { 0. };
        let from = self.level.unwrap_or(target);
        // A culled voice keeps rendering for the block that fades it out.
        let live = audible || from != 0.;
        let mut at = 0;
        while at < out.len() {
            if self.current.is_none() {
                while let Some(packet) = self.pending.pop() {
                    if packet.generation != generation {
                        self.controls.count.fetch_sub(1, Ordering::Relaxed);
                        continue;
                    }
                    self.current = Some(match packet.source {
                        Queued::Effect(e) => Current::Effect(e),
                        Queued::Source(source) => {
                            if (8000..=96000).contains(&source.sample_rate())
                                && (1..=2).contains(&source.channels())
                            {
                                Current::Stream(block::Stream::new(source).unwrap())
                            } else {
                                Current::Legacy(UniformSourceIterator::new(source, 2, 44100))
                            }
                        }
                    });
                    self.position = 0;
                    break;
                }
            }
            let Some(source) = self.current.as_mut() else {
                break;
            };
            let n = match source {
                Current::Effect(e) => e.render(&mut out[at..], live),
                Current::Stream(s) => {
                    s.render(&mut out[at..], self.controls.music.load(Ordering::Relaxed))
                }
                Current::Legacy(s) => {
                    let start = std::time::Instant::now();
                    let mut n = 0;
                    for v in &mut out[at..] {
                        if let Some(x) = s.next() {
                            *v = x;
                            n += 1;
                        } else {
                            break;
                        }
                    }
                    block::charge(1, start);
                    n
                }
            };
            self.position = self.position.wrapping_add(n as u32);
            at += n;
            if at < out.len() {
                self.discard_current();
            }
        }
        self.controls
            .position
            .store(self.position, Ordering::Relaxed);
        let start = std::time::Instant::now();
        if from == target {
            for v in out.iter_mut() {
                *v *= target;
            }
        } else {
            let step = (target - from) / (out.len() / 2) as f32;
            for (i, frame) in out.chunks_exact_mut(2).enumerate() {
                let g = from + step * (i + 1) as f32;
                frame[0] *= g;
                frame[1] *= g;
            }
        }
        self.level = Some(target);
        block::charge(3, start);
        true
    }
    fn discard_current(&mut self) {
        if self.current.take().is_some() {
            self.controls.count.fetch_sub(1, Ordering::Relaxed);
        }
    }
}
impl Iterator for Output {
    type Item = f32;
    fn next(&mut self) -> Option<f32> {
        if !self.controls.alive.load(Ordering::Acquire) {
            self.discard_current();
            while self.pending.pop().is_some() {
                self.controls.count.fetch_sub(1, Ordering::Relaxed);
            }
            return None;
        }
        let generation = self.controls.generation.load(Ordering::Relaxed);
        if generation != self.generation {
            self.discard_current();
            self.generation = generation;
        }
        loop {
            if self.current.is_none() {
                while let Some(packet) = self.pending.pop() {
                    let latest = self.controls.generation.load(Ordering::Acquire);
                    if packet.generation != latest {
                        self.controls.count.fetch_sub(1, Ordering::Relaxed);
                        continue;
                    }
                    self.generation = latest;
                    self.current = Some(match packet.source {
                        Queued::Source(source) => {
                            Current::Legacy(UniformSourceIterator::new(source, 2, 44100))
                        }
                        Queued::Effect(effect) => Current::Effect(effect),
                    });
                    self.position = 0;
                    self.controls.position.store(0, Ordering::Relaxed);
                    break;
                }
            }
            if !self.channel {
                let paused = self.controls.pause.load(Ordering::Relaxed);
                if paused && !self.paused {
                    self.controls
                        .position
                        .store(self.position, Ordering::Relaxed);
                }
                self.paused = paused;
                self.gain = f32::from_bits(self.controls.volume.load(Ordering::Relaxed));
            }
            if self.paused {
                self.channel = !self.channel;
                return Some(0.);
            }
            if let Some(source) = &mut self.current {
                if let Some(value) = match source {
                    Current::Legacy(s) => s.next(),
                    _ => None,
                } {
                    self.position = self.position.wrapping_add(1);
                    if self.position & 127 == 0 {
                        self.controls
                            .position
                            .store(self.position, Ordering::Relaxed);
                    }
                    self.channel = !self.channel;
                    return Some(value * self.gain);
                }
                self.discard_current();
                continue;
            }
            return self.controls.alive.load(Ordering::Acquire).then_some(0.);
        }
    }
}
impl Source for Output {
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
impl Drop for Output {
    fn drop(&mut self) {
        self.controls.alive.store(false, Ordering::Release);
        self.discard_current();
        while self.pending.pop().is_some() {
            self.controls.count.fetch_sub(1, Ordering::Relaxed);
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn block_pause_stop_replacement_and_end_accounting() {
        let (sink, mut out) = Sink::new_idle();
        sink.append(rodio::buffer::SamplesBuffer::new(
            2,
            44100,
            vec![0.25; 2048],
        ));
        let mut block = [0.; 1024];
        sink.pause();
        out.render(&mut block, true);
        assert!(block.iter().all(|v| *v == 0.));
        assert_eq!(sink.get_pos(), Duration::ZERO);
        sink.play();
        out.render(&mut block, true);
        assert!(block.iter().all(|v| *v == 0.25));
        sink.stop();
        sink.append(rodio::buffer::SamplesBuffer::new(1, 22050, vec![0.5; 256]));
        out.render(&mut block, true);
        assert!(block.iter().all(|v| *v == 0.5));
        out.render(&mut block, true);
        assert!(block.iter().all(|v| *v == 0.));
        assert_eq!(sink.len(), 0);
        drop(sink);
        assert!(!out.render(&mut block, true));
    }
    #[test]
    fn controls_do_not_wait_for_a_source_blocked_on_another_thread() {
        use std::sync::mpsc;
        struct Slow {
            entered: Option<mpsc::Sender<()>>,
            release: mpsc::Receiver<()>,
        }
        impl Iterator for Slow {
            type Item = f32;
            fn next(&mut self) -> Option<f32> {
                if let Some(tx) = self.entered.take() {
                    tx.send(()).unwrap();
                    self.release.recv().unwrap();
                }
                Some(0.25)
            }
        }
        impl Source for Slow {
            fn channels(&self) -> u16 {
                2
            }
            fn sample_rate(&self) -> u32 {
                44100
            }
            fn current_frame_len(&self) -> Option<usize> {
                None
            }
            fn total_duration(&self) -> Option<Duration> {
                None
            }
        }
        let (sink, mut output) = Sink::new_idle();
        let (entered, seen) = mpsc::channel();
        let (release, blocked) = mpsc::channel();
        sink.append(Slow {
            entered: Some(entered),
            release: blocked,
        });
        let worker = std::thread::spawn(move || {
            output.next();
        });
        seen.recv_timeout(Duration::from_secs(1)).unwrap();
        let start = std::time::Instant::now();
        for n in 0..1000 {
            sink.set_volume(n as f32 / 1000.);
            sink.pause();
            sink.play();
            std::hint::black_box(sink.get_pos());
        }
        assert!(start.elapsed() < Duration::from_millis(20));
        release.send(()).unwrap();
        worker.join().unwrap();
    }
    #[test]
    fn stop_then_append_does_not_wait_and_rejects_old_generation() {
        let (s, mut out) = Sink::new_idle();
        s.append(rodio::buffer::SamplesBuffer::new(2, 44100, vec![0.25; 100]));
        assert_eq!(out.next(), Some(0.25));
        s.stop();
        s.append(rodio::buffer::SamplesBuffer::new(2, 44100, vec![0.5; 100]));
        assert_eq!(out.next(), Some(0.5));
        assert_eq!(s.len(), 1);
    }
}
