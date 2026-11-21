//! Ordered simulation audio requests, applied by the audio owner.
use super::{events, world, AcousticWorld, Audio, LoopCue};
use crate::{assets::Assets, combat::Feedback, interaction::Interactions};
use macroquad::prelude::Vec3;
use std::borrow::Cow;

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum VoiceRequest {
    Play { path: String, offset_seconds: f32 },
    Stop,
}

pub(crate) enum AudioOp {
    Movement {
        distance: f32,
        grounded: bool,
        jumped: bool,
        landed: bool,
    },
    Water {
        dt: f32,
        before: u8,
        now: u8,
        speed: f32,
    },
    Effect {
        path: Cow<'static, str>,
        origin: Option<Vec3>,
    },
    Animation(events::Cue),
    Voice(VoiceRequest),
    World {
        clocks: Vec<world::Clock>,
        mood: Option<&'static str>,
        loops: Vec<LoopCue>,
    },
    Player {
        sanity: f32,
        swimming: bool,
        underwater: bool,
        drowning: bool,
    },
    Listen {
        listener: Vec3,
        submerged: bool,
        dt: f32,
        world_dt: f32,
        yaw: f32,
        paused: bool,
        voice_paused: bool,
    },
}

#[derive(Default)]
pub(crate) struct AudioStep {
    pub frame: u64,
    pub ops: Vec<AudioOp>,
}

const _: fn() = || {
    fn check<T: Send>() {}
    check::<AudioStep>();
};

impl AudioStep {
    pub fn effect(&mut self, path: impl Into<Cow<'static, str>>, origin: Option<Vec3>) {
        self.ops.push(AudioOp::Effect {
            path: path.into(),
            origin,
        });
    }

    pub fn feedback(&mut self, feedback: Feedback) {
        for (path, origin) in feedback.cue_sounds {
            self.effect(path, Some(origin));
        }
        for path in feedback.sounds {
            self.effect(path, None);
        }
        for (path, origin) in feedback.spatial_sounds {
            self.effect(path, Some(origin));
        }
    }

    pub fn world(&mut self, interactions: &Interactions, mut loops: Vec<LoopCue>) {
        let clocks = world::collect(interactions, &mut loops);
        let mood = interactions.levels.iter().find_map(|s| s.ctl.music_mood());
        self.ops.push(AudioOp::World {
            clocks,
            mood,
            loops,
        });
    }

    pub fn apply_at_barrier(
        &mut self,
        _barrier: &'static str,
        audio: &mut Audio,
        assets: &mut Assets,
        world: &impl AcousticWorld,
    ) {
        if self.ops.is_empty() {
            return;
        }
        #[cfg(all(feature = "sim_maps_probe", not(target_os = "vita")))]
        if super::proof::enabled() {
            println!(
                "AUDIO_STEP_BARRIER frame={} barrier={_barrier} ops={}",
                self.frame,
                self.ops.len()
            );
        }
        self.apply(audio, assets, world);
    }

    pub fn apply(&mut self, audio: &mut Audio, assets: &mut Assets, world: &impl AcousticWorld) {
        #[cfg(all(feature = "sim_maps_probe", not(target_os = "vita")))]
        let _lazy_profile = crate::collision::lazy_profile::enter(crate::collision::lazy_profile::Lane::Audio);
        for op in self.ops.drain(..) {
            match op {
                AudioOp::Movement {
                    distance,
                    grounded,
                    jumped,
                    landed,
                } => {
                    audio.movement(distance, grounded, jumped, landed);
                }
                AudioOp::Water {
                    dt,
                    before,
                    now,
                    speed,
                } => audio.water(dt, before, now, speed),
                AudioOp::Effect {
                    path,
                    origin: Some(origin),
                } => audio.world_effect_at(&path, origin),
                AudioOp::Effect { path, origin: None } => audio.world_effect(&path),
                AudioOp::Animation(cue) => audio.animation(cue),
                AudioOp::Voice(VoiceRequest::Play {
                    path,
                    offset_seconds,
                }) => {
                    audio.speak_from(assets, &path, offset_seconds);
                }
                AudioOp::Voice(VoiceRequest::Stop) => audio.stop_voice(),
                AudioOp::World {
                    clocks,
                    mood,
                    loops,
                } => audio.world(assets, clocks, mood, loops),
                AudioOp::Player {
                    sanity,
                    swimming,
                    underwater,
                    drowning,
                } => {
                    audio.player_state(assets, sanity, swimming, underwater, drowning);
                }
                AudioOp::Listen {
                    listener,
                    submerged,
                    dt,
                    world_dt,
                    yaw,
                    paused,
                    voice_paused,
                } => {
                    audio.prepare(assets, world, listener, submerged, dt);
                    audio.update_clocks(dt, world_dt, listener, yaw, paused, voice_paused);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::{output, Playback};
    use crate::collision::World;
    use std::io::{Cursor, Write};

    #[test]
    fn ordered_step_matches_direct_calls_at_admission_and_mix() -> anyhow::Result<()> {
        const CUE: &str = "sound/test/step.wav";
        const DT: f32 = 1. / 30.;

        let mut wav = Cursor::new(Vec::new());
        let mut writer = hound::WavWriter::new(
            &mut wav,
            hound::WavSpec {
                channels: 1,
                sample_rate: 44_100,
                bits_per_sample: 16,
                sample_format: hound::SampleFormat::Int,
            },
        )?;
        for sample in 0..4096 {
            writer.write_sample(((sample % 64) * 400 - 12_800) as i16)?;
        }
        writer.finalize()?;

        let root = std::env::temp_dir().join(format!("lg-audio-step-{}", std::process::id()));
        std::fs::create_dir_all(&root)?;
        let mut archive = zip::ZipWriter::new(std::fs::File::create(root.join("test.pk3"))?);
        archive.start_file(CUE, zip::write::FileOptions::default())?;
        archive.write_all(wav.get_ref())?;
        archive.finish()?;
        let mut assets = Assets::open(&root)?;

        let create = || {
            let mut audio = Audio::new(true, None);
            let (controls, mixer) = output::mixer();
            audio.playback = Some(Playback {
                _stream: None,
                mixer: controls,
                _worker: None,
            });
            audio.settings = Default::default();
            audio.scene_presented();
            (audio, mixer)
        };
        let (mut direct, mut direct_mix) = create();
        let (mut replay, mut replay_mix) = create();
        let world = World::fixture(&[]);
        let mut step = AudioStep::default();

        direct.speak_from(&mut assets, CUE, 0.);
        step.ops.push(AudioOp::Voice(VoiceRequest::Play {
            path: CUE.into(),
            offset_seconds: 0.,
        }));
        for i in 0..130 {
            // Repeated paths and origins exercise FIFO admission across a barrier.
            let origin = Vec3::Y * (i % 8) as f32;
            direct.world_effect_at(CUE, origin);
            step.effect(CUE, Some(origin));
            if i == 64 {
                step.apply_at_barrier("test-prefix", &mut replay, &mut assets, &world);
                assert!(step.ops.is_empty());
                assert_eq!(replay.pending.len(), 65);
            }
        }

        let loops = || {
            (0..33)
                .map(|id| LoopCue {
                    clock: Some(id as f32 / 31.),
                    id,
                    path: CUE,
                    origin: Vec3::X * id as f32,
                })
                .collect()
        };
        direct.world(&mut assets, Vec::new(), None, loops());
        step.ops.push(AudioOp::World {
            clocks: Vec::new(),
            mood: None,
            loops: loops(),
        });
        direct.stop_voice();
        step.ops.push(AudioOp::Voice(VoiceRequest::Stop));
        step.apply(&mut replay, &mut assets, &world);
        assert!(step.ops.is_empty());
        assert_eq!(replay.pending.len(), 128);
        assert_eq!(direct.pending.len(), replay.pending.len());
        for (a, b) in direct.pending.iter().zip(&replay.pending) {
            assert_eq!(
                (&a.path, a.origin, a.volume.to_bits()),
                (&b.path, b.origin, b.volume.to_bits())
            );
        }
        assert_eq!(replay.loops.len(), 32);
        assert!(direct.loops.keys().eq(replay.loops.keys()));
        assert!(replay.voice.is_none());

        for _ in 0..3 {
            direct.prepare(&mut assets, &world, Vec3::ZERO, false, DT);
            direct.update_clocks(DT, DT, Vec3::ZERO, 0., false, false);
            step.ops.push(AudioOp::Listen {
                listener: Vec3::ZERO,
                submerged: false,
                dt: DT,
                world_dt: DT,
                yaw: 0.,
                paused: false,
                voice_paused: false,
            });
            step.apply(&mut replay, &mut assets, &world);
            assert_eq!(direct.resolved, replay.resolved);
            assert_eq!(direct.effects.len(), replay.effects.len());
            let mut audible = false;
            for _ in 0..2940 {
                let sample = direct_mix.next().unwrap();
                audible |= sample != 0.;
                assert_eq!(sample.to_bits(), replay_mix.next().unwrap().to_bits());
            }
            assert!(audible);
        }
        assert_eq!(replay.resolved, 128);
        assert!(step.ops.is_empty());

        drop(assets);
        std::fs::remove_dir_all(root)?;
        Ok(())
    }
}
