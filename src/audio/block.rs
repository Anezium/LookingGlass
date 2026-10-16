//! Block-rate audio only. This module never participates in game simulation.
use super::{acoustics, Pcm};
use rodio::Source;
use std::{cell::Cell, time::Instant};
#[derive(Clone, Copy)]
pub struct Settings {
    pub mix: bool,
    pub preresample: bool,
    pub parameters: bool,
    pub voices: usize,
    pub gain: f32,
    pub priority: i32,
    pub affinity: i32,
    pub cushion: usize,
}
pub fn settings() -> Settings {
    #[cfg(feature = "vita_renderer")]
    {
        #[cfg(test)]
        let c = {
            static TEST: std::sync::OnceLock<crate::render::vita_renderer::Config> =
                std::sync::OnceLock::new();
            TEST.get_or_init(|| {
                std::env::var("LOOKING_GLASS_TEST_AUDIO_CONFIG")
                    .ok()
                    .map(|p| serde_json::from_slice(&std::fs::read(p).unwrap()).unwrap())
                    .unwrap_or_default()
            })
        };
        #[cfg(not(test))]
        let c = crate::render::vita_renderer::config();
        Settings {
            mix: c.audio_block_mix,
            preresample: c.audio_preresample,
            parameters: c.audio_block_parameters,
            voices: c.audio_ambient_voices.min(128),
            gain: c.audio_cull_gain.clamp(0., 0.1),
            priority: c.audio_producer_priority.clamp(160, 191),
            affinity: match c.audio_producer_affinity {
                0x10000 | 0x20000 | 0x40000 => c.audio_producer_affinity,
                _ => 0x40000,
            },
            cushion: c.audio_cushion_blocks.clamp(1, 4),
        }
    }
    #[cfg(not(feature = "vita_renderer"))]
    {
        Settings {
            mix: false,
            preresample: false,
            parameters: false,
            voices: 0,
            gain: 0.,
            priority: 161,
            affinity: 0x40000,
            cushion: 2,
        }
    }
}
pub const STAGES: [&str; 7] = [
    "music_decode",
    "effect_decode",
    "resampling",
    "voice_dsp",
    "summing",
    "conversion",
    "control",
];
thread_local! {static COST:Cell<[f64;7]>=const {Cell::new([0.;7])};}
pub fn reset() {
    COST.with(|c| c.set([0.; 7]));
}
pub fn costs() -> [f64; 7] {
    COST.with(Cell::get)
}
pub fn charge(stage: usize, start: Instant) {
    COST.with(|c| {
        let mut a = c.get();
        a[stage] += start.elapsed().as_secs_f64() * 1000.;
        c.set(a)
    });
}

/// Decodes a bounded input span first, then resamples in a separate tight loop.
/// Rational phase persists across blocks; EOF never manufactures an extra block.
pub struct Stream {
    source: Box<dyn Source<Item = f32> + Send>,
    frames: Vec<[f32; 2]>,
    phase: u32,
    rate: u32,
    channels: u16,
    eof: bool,
}
impl Stream {
    pub fn new(source: Box<dyn Source<Item = f32> + Send>) -> Option<Self> {
        let rate = source.sample_rate();
        let channels = source.channels();
        if !(8000..=96000).contains(&rate) || !(1..=2).contains(&channels) {
            return None;
        }
        Some(Self {
            source,
            frames: Vec::with_capacity(1200),
            phase: 0,
            rate,
            channels,
            eof: false,
        })
    }
    pub fn render(&mut self, out: &mut [f32], music: bool) -> usize {
        let start = Instant::now();
        let need =
            ((self.phase + (out.len() / 2 - 1) as u32 * self.rate as u32) / 44100) as usize + 2;
        while self.frames.len() < need && !self.eof {
            if let Some(l) = self.source.next() {
                let r = if self.channels == 2 {
                    self.source.next().unwrap_or(l)
                } else {
                    l
                };
                self.frames.push([l, r]);
            } else {
                self.eof = true;
            }
        }
        charge(if music { 0 } else { 1 }, start);
        let start = Instant::now();
        let mut n = 0;
        for frame in out.chunks_exact_mut(2) {
            let i = (self.phase / 44100) as usize;
            if i >= self.frames.len() {
                break;
            }
            let a = self.frames[i];
            let b = self.frames[(i + 1).min(self.frames.len() - 1)];
            let f = (self.phase % 44100) as f32 / 44100.;
            frame[0] = a[0] + (b[0] - a[0]) * f;
            frame[1] = a[1] + (b[1] - a[1]) * f;
            self.phase += self.rate as u32;
            n += 2;
        }
        let used = ((self.phase / 44100) as usize).min(self.frames.len());
        self.frames.drain(..used);
        self.phase -= used as u32 * 44100;
        charge(2, start);
        n
    }
}

/// Output frames per clip frame as a power-of-two shift, when 44.1 kHz is 1x,
/// 2x or 4x the clip rate. Other rates are resampled once at load.
pub fn step(rate: u32) -> Option<u32> {
    match rate {
        44100 => Some(0),
        22050 => Some(1),
        11025 => Some(2),
        _ => None,
    }
}

pub struct Effect {
    pcm: Pcm,
    filter: acoustics::FilterState,
    tail: usize,
    shift: u32,
    /// Interpolation weights for the in-between frames, identical to load-time
    /// resampling: `(k * rate) / 44100` for k in 0..(1 << shift).
    weights: [f32; 4],
    /// Pan applied at the end of the last audible block; None after a
    /// virtual block, where the sink's gain fade covers the re-entry.
    pan: Option<[f32; 2]>,
    virtualized: bool,
    parameters: bool,
}
impl Effect {
    pub fn new(mut pcm: Pcm, controls: std::sync::Arc<acoustics::Controls>) -> Self {
        let filter = acoustics::FilterState::new(44100, controls);
        let shift = step(pcm.clip.rate).expect("block effects need a 44.1 kHz multiple");
        let mut weights = [0.; 4];
        for (k, w) in weights.iter_mut().enumerate().take(1 << shift) {
            *w = (k as u32 * pcm.clip.rate) as f32 / 44100.;
        }
        // `index` counts interleaved samples; move it from clip frames to output frames.
        pcm.index <<= shift;
        Self {
            pcm,
            filter,
            tail: 0,
            shift,
            weights,
            pan: None,
            virtualized: false,
            parameters: settings().parameters,
        }
    }
    /// Clip length in 44.1 kHz output frames.
    fn frames(&self) -> usize {
        self.pcm.clip.samples.len() << self.shift
    }
    fn fetch(&self, frame: usize) -> f32 {
        let samples = &self.pcm.clip.samples;
        let j = frame >> self.shift;
        let Some(&a) = samples.get(j) else {
            return 0.;
        };
        let k = frame & ((1 << self.shift) - 1);
        if k == 0 {
            return a;
        }
        let b = samples[(j + 1).min(samples.len() - 1)];
        a + (b - a) * self.weights[k]
    }
    pub fn render(&mut self, out: &mut [f32], audible: bool) -> usize {
        let start = Instant::now();
        let len = self.frames();
        if !audible {
            if !self.virtualized {
                self.filter.clear();
                self.pan = None;
                self.virtualized = true;
            }
            let frames = out.len() / 2;
            let index = self.pcm.index / 2;
            let n = if self.pcm.looping {
                self.pcm.index = ((index + frames) % len) * 2;
                out.len()
            } else {
                let left = len.saturating_sub(index);
                let tail_left = 44100usize.saturating_sub(self.tail) / 2;
                let count = frames.min(left + tail_left);
                self.pcm.index += count * 2;
                self.tail += count.saturating_sub(left) * 2;
                count * 2
            };
            out.fill(0.);
            charge(3, start);
            return n;
        }
        self.virtualized = false;
        let mut n = 0;
        let target = [
            f32::from_bits(self.pcm.pan.left.load(std::sync::atomic::Ordering::Relaxed)),
            f32::from_bits(self.pcm.pan.right.load(std::sync::atomic::Ordering::Relaxed)),
        ];
        // Pan moves linearly across the block instead of stepping at its start.
        let [l0, r0] = self.pan.unwrap_or(target);
        let frames = (out.len() / 2) as f32;
        let (dl, dr) = ((target[0] - l0) / frames, (target[1] - r0) / frames);
        self.pan = Some(target);
        for (i, frame) in out.chunks_exact_mut(2).enumerate() {
            if self.pcm.index / 2 >= len {
                if self.pcm.looping {
                    self.pcm.index = 0;
                } else {
                    self.tail += 2;
                    if self.tail > 44100 {
                        break;
                    }
                }
            }
            let dry = self.fetch(self.pcm.index / 2);
            self.pcm.index += 2;
            let t = (i + 1) as f32;
            frame[0] = dry * (l0 + dl * t);
            frame[1] = dry * (r0 + dr * t);
            n += 2;
        }
        // Clips are decoded at load; this is PCM fetch, 2x/4x interpolation and spatial.
        charge(3, start);
        let start = Instant::now();
        self.filter.process(&mut out[..n], self.parameters);
        charge(3, start);
        n
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_effect_matches_filter_at_output_rate_and_virtual_phase() {
        let clip = super::super::Clip {
            samples: (0..2000)
                .map(|i| (i as f32 * 0.13).sin() * 0.1)
                .collect::<Vec<_>>()
                .into(),
            rate: 44100,
        };
        let pan = super::super::Pan::new();
        pan.set(0.3, 0.7);
        let controls = acoustics::Controls::new();
        controls.set(0.4, false, 0.6);
        let pcm = || Pcm {
            clip: clip.clone(),
            index: 0,
            looping: true,
            pan: pan.clone(),
        };
        let mut legacy = acoustics::Filter::new(pcm(), controls.clone());
        let mut native = Effect::new(pcm(), controls);
        let mut block = [0.; 1024];
        for _ in 0..6 {
            assert_eq!(native.render(&mut block, true), 1024);
            for v in block {
                assert_eq!(Some(v), legacy.next());
            }
        }
        let before = native.pcm.index;
        native.render(&mut block, false);
        assert_eq!(native.pcm.index, (before + 1024) % (clip.samples.len() * 2));
        assert!(block.iter().all(|v| *v == 0.));
    }
    #[test]
    fn native_rate_clip_matches_load_time_resampling() {
        let mono: Vec<f32> = (0..3001).map(|i| (i as f32 * 0.071).sin() * 0.2).collect();
        let up = super::super::resample_linear(&mono, 22050);
        assert_eq!(up.len(), mono.len() * 2);
        let pan = super::super::Pan::new();
        pan.set(0.4, 0.9);
        let controls = acoustics::Controls::new();
        controls.set(0.3, false, 0.5);
        for looping in [true, false] {
            let effect = |samples: &[f32], rate, index| {
                let clip = super::super::Clip { samples: samples.into(), rate };
                Effect::new(Pcm { clip, index, looping, pan: pan.clone() }, controls.clone())
            };
            let mut native = effect(&mono, 22050, 700 * 2);
            let mut resampled = effect(&up, 44100, 1400 * 2);
            let (mut a, mut b) = ([0.; 1024], [0.; 1024]);
            for block in 0..60 {
                let audible = block % 17 != 3;
                assert_eq!(native.render(&mut a, audible), resampled.render(&mut b, audible));
                assert_eq!(a, b, "looping={looping} block={block}");
                assert_eq!(native.pcm.index, resampled.pcm.index);
            }
        }
    }
    #[test]
    fn pan_and_reflection_changes_ramp_across_the_block() {
        let clip = super::super::Clip { samples: vec![0.5; 4410].into(), rate: 22050 };
        let pan = super::super::Pan::new();
        pan.set(1., 0.);
        let controls = acoustics::Controls::new();
        let mut effect = Effect::new(Pcm { clip, index: 0, looping: true, pan: pan.clone() }, controls.clone());
        effect.parameters = true;
        let mut block = [0.; 1024];
        let mut prev: Option<[f32; 2]> = None;
        let mut worst = 0f32;
        for n in 0..24 {
            // Hard-left to hard-right, then an open room switching on.
            if n == 12 {
                pan.set(0., 1.);
            }
            if n == 18 {
                controls.set(0., false, 1.);
            }
            effect.render(&mut block, true);
            for frame in block.chunks_exact(2) {
                if let Some([l, r]) = prev {
                    if n >= 8 {
                        worst = worst.max((frame[0] - l).abs()).max((frame[1] - r).abs());
                    }
                }
                prev = Some([frame[0], frame[1]]);
            }
        }
        // A stepped pan would jump by 0.5, the reflections by about 0.1.
        assert!(worst < 0.003, "largest step {worst}");
        assert!(block[1023] > 0.5 && block[1022] < 0.05, "{:?}", &block[1022..]);
    }
    #[test]
    fn resampling_is_continuous_across_blocks_and_preserves_eof() {
        for rate in [8000, 22050, 44100, 48000, 96000] {
            let input: Vec<f32> = (0..5000).map(|i| i as f32 / 5000.).collect();
            let source = || {
                Box::new(rodio::buffer::SamplesBuffer::new(1, rate, input.clone()))
                    as Box<dyn Source<Item = f32> + Send>
            };
            let mut a = Stream::new(source()).unwrap();
            let mut b = Stream::new(source()).unwrap();
            let mut whole = vec![0.; 8192];
            let n = a.render(&mut whole, false);
            let mut split = Vec::new();
            for _ in 0..8 {
                let mut block = [0.; 1024];
                let n = b.render(&mut block, false);
                split.extend_from_slice(&block[..n]);
            }
            assert_eq!(&whole[..n], split.as_slice(), "rate {rate}");
        }
    }
}
