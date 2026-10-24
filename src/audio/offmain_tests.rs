//! Controlled sample clock: both arms consume identical production mixer blocks
//! after publishing each frame's controls. Hardware scheduling is not modeled.
use super::*;
use sha2::{Digest, Sha256};
fn test_box(center: Vec3, half: Vec3) -> crate::collision::Collider {
    crate::collision::Collider::box_bounds(center - half, center + half)
}
fn setup(off: bool) -> (Audio, Monitor<output::Mixer>) {
    let mut audio = Audio::new(true, None);
    audio.settings = Settings::default();
    let (mut mixer, mut source) = output::mixer();
    mixer.acoustics = None;
    source.acoustics = None;
    let jobs = if off {
        let (client, server) = offmain::channel();
        mixer.acoustics = Some(client);
        Some(server)
    } else {
        None
    };
    // Actual production producer loop services jobs with its full PCM cushion.
    // The audible mixer is stepped explicitly below to fix control/sample order.
    let (worker, _callback) = output::buffered_audio(
        rodio::source::Zero::<f32>::new(2, 44100),
        mixer.metrics.clone(),
        jobs,
    )
    .unwrap();
    audio.playback = Some(Playback {
        _stream: None,
        mixer,
        _worker: Some(worker),
    });
    let monitor = Monitor {
        source,
        data: audio.monitor.clone(),
        block: Vec::with_capacity(1024),
        record: false,
    };
    (audio, monitor)
}
fn compare(
    mut a: Audio,
    mut ma: Monitor<output::Mixer>,
    mut b: Audio,
    mut mb: Monitor<output::Mixer>,
    mut assets: Option<&mut Assets>,
    world: &mut World,
    label: &str,
    eye: Vec3,
    frames: usize,
) {
    let mut hash = Sha256::new();
    let mut samples = 0;
    let mut hot_allocs = 0;
    for frame in 0..frames {
        let ear = if frame >= 80 {
            eye
        } else {
            eye + vec3(
                (frame % 9) as f32 * 2.,
                (frame % 7) as f32,
                -((frame % 3) as f32),
            )
        };
        let paused = (40..50).contains(&frame);
        let submerged = (60..80).contains(&frame);
        world.set_dynamic(vec![test_box(ear + Vec3::X * 50., Vec3::splat(10.))]);
        for (arm, audio) in [&mut a, &mut b].into_iter().enumerate() {
            crate::phase_census::test_begin(arm==1);
            crate::phase_census::scene(world.acoustic_scene_key());
            audio.settings.muted = (90..95).contains(&frame);
            if let Some(assets) = assets.as_deref_mut() {
                if frame % 20 == 0 {
                    audio.world_effect_at(EFFECTS[0], ear + Vec3::X * 80.);
                }
                audio.prepare(assets, world, ear, submerged, 1. / 30.);
            } else {
                let scope = crate::kernel_alloc::Scope::begin();
                let mut client = audio.playback.as_mut().unwrap().mixer.acoustics.take();
                if let Some(c) = &mut client {
                    c.begin(world);
                    if audio.acoustic_clock - 1. / 30. <= 0. {
                        acoustics::room_with(ear, |a, b| c.query(a, b));
                        for e in &audio.emitters {
                            if e.spec.origin.distance(ear) < e.spec.radius * 4. {
                                acoustics::obstruction_with(ear, e.spec.origin, |a, b| {
                                    c.query(a, b)
                                });
                            }
                        }
                    }
                    c.execute();
                }
                if let Some(c) = client.as_mut() {
                    audio.prepare_acoustics(ear, submerged, 1. / 30., |a,b| c.complete(world,a,b));
                } else {
                    audio.prepare_acoustics(ear, submerged, 1. / 30., |a,b| world.sweep(a,b,Vec3::ZERO));
                }
                audio.playback.as_mut().unwrap().mixer.acoustics = client;
                if frame > 0 {
                    hot_allocs += scope.finish();
                } else {
                    scope.finish();
                }
            }
            audio.update_clocks(
                1. / 30.,
                if frame % 11 == 0 { 0. } else { 1. / 30. },
                ear,
                frame as f32 * 0.03,
                paused,
                frame % 17 == 0,
            );
        }
        // 1536 stereo frames at a deterministic sample boundary each game frame.
        for sample in 0..3072 {
            let x = ma.next().unwrap().to_bits();
            let y = mb.next().unwrap().to_bits();
            assert_eq!(x, y, "{label} frame={frame} sample={sample}");
            hash.update(x.to_le_bytes());
            samples += 1;
        }
    }
    crate::phase_census::test_begin(false);
    if let Some(c) = b
        .playback
        .as_ref()
        .unwrap()
        .mixer
        .acoustics
        .as_ref()
    {
    assert!(c.submitted > 0);
    assert_eq!(c.fallback, 0);
    assert_eq!(hot_allocs, 0);
    println!("AUDIO_OFFMAIN_EQUAL fixture={label} frames={frames} scalar_samples={samples} sha256={:x} new_hot_allocations={hot_allocs} submitted={} reused={} fallback={} host_only=true",hash.finalize(),c.submitted,c.reused,c.fallback);
    } else {
        assert_eq!(hot_allocs, 0);
        println!("AUDIO_CENSUS_EQUAL fixture={label} frames={frames} scalar_samples={samples} sha256={:x} new_hot_allocations={hot_allocs}",hash.finalize());
    }
}
#[test]
fn synthetic_mixed_output_is_sample_exact_without_hot_allocations() {
    let mut world = World::fixture(&[(vec3(-5., -100., -100.), vec3(5., 100., 100.))]);
    let (mut a, ma) = setup(false);
    let (mut b, mb) = setup(true);
    for audio in [&mut a, &mut b] {
        audio.scene_presented = true;
        for i in 0..8 {
            let speaker = Speaker {
                path: "fixture".into(),
                origin: vec3(40. + i as f32 * 12., 50., 0.),
                volume: 0.4,
                radius: 384.,
                random: false,
                delay: (1., 2.),
                chance: 1.,
            };
            let clip = Clip {
                samples: (0..4000)
                    .map(|j| ((j * 17 + i * 13) as f32 * 0.03).sin() * 0.08)
                    .collect::<Vec<_>>()
                    .into(),
                rate: 22050,
            };
            let mut cache = BTreeMap::new();
            cache.insert("fixture".into(), Some(clip));
            audio.add_speakers(vec![speaker], &cache);
        }
    }
    compare(
        a,
        ma,
        b,
        mb,
        None,
        &mut world,
        "synthetic-moving-listener-mover-pause-mute-water",
        vec3(-80., 20., 0.),
        120,
    );
}
#[test]
#[ignore = "requires LOOKING_GLASS_TEST_DATA; deterministic decode (audio_music_thread=false)"]
fn map_mixed_output_is_sample_exact() -> Result<()> {
    assert!(
        !block::settings().music_thread,
        "use the deterministic host config"
    );
    let mut assets = Assets::open(&PathBuf::from(std::env::var("LOOKING_GLASS_TEST_DATA")?))?;
    for (name, eye) in [
        ("hedge3", vec3(6168., -7300., 280.15625)),
        ("gvillage", vec3(-4144., 6120., 280.)),
        ("skool2", Vec3::ZERO),
        ("hatter1", Vec3::ZERO),
    ] {
        let map = Bsp::parse(&assets.read(&format!("maps/{name}.bsp"))?)?;
        let mut world = World::from_bsp(&map)?;
        let (mut a, ma) = setup(false);
        let (mut b, mb) = setup(true);
        a.load(&mut assets, name, &map);
        b.load(&mut assets, name, &map);
        a.scene_presented();
        b.scene_presented();
        compare(a, ma, b, mb, Some(&mut assets), &mut world, name, eye, 120);
    }
    Ok(())
}

#[cfg(feature = "native_timing")]
#[test]
#[ignore = "requires LOOKING_GLASS_TEST_DATA and producer/audio ON config; host scheduler only"]
fn paced_pcm_producer_preserves_audio_health() -> Result<()> {
    use crate::native_timing::{audio_counters::Tag, clock::Native, Metadata, Recorder};
    use std::time::Instant;
    let _serial = music::TEST_COUNTER_LOCK
        .lock()
        .unwrap_or_else(|p| p.into_inner());
    assert!(block::settings().offmain && block::settings().music_thread);
    let mut assets = Assets::open(&PathBuf::from(std::env::var("LOOKING_GLASS_TEST_DATA")?))?;
    for (name, eye) in [
        ("hedge3", vec3(6168., -7300., 280.15625)),
        ("gvillage", vec3(-4144., 6120., 280.)),
    ] {
        eprintln!("AUDIO_OFFMAIN_PACED_BEGIN map={name}");
        let map = Bsp::parse(&assets.read(&format!("maps/{name}.bsp"))?)?;
        let world = World::from_bsp(&map)?;
        let mut audio = Audio::new(true, None);
        audio.settings = Settings::default();
        let (mixer, mut source) = output::mixer();
        let jobs = source.acoustics.take();
        let metrics = mixer.metrics.clone();
        let monitor = Monitor {
            source,
            data: audio.monitor.clone(),
            block: Vec::with_capacity(1024),
            record: false,
        };
        let mut recorder = Recorder::new(Native, Some([91; 16]), Metadata::default());
        recorder.raw_requested = true;
        recorder.bind_raw_counters(Some(&metrics.raw_starvation), Some(&music::RAW_UNDERRUNS));
        let (worker, mut callback) = output::buffered_audio(monitor, metrics.clone(), jobs)?;
        audio.playback = Some(Playback {
            _stream: None,
            mixer,
            _worker: Some(worker),
        });
        audio.load(&mut assets, name, &map);
        let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let done = stop.clone();
        let reader = std::thread::spawn(move || {
            let mut deadline = Instant::now();
            let mut blocks = 0;
            while !done.load(Ordering::Acquire) {
                for _ in 0..1024 {
                    std::hint::black_box(callback.next());
                }
                blocks += 1;
                deadline += Duration::from_secs_f64(512. / 44100.);
                if let Some(wait) = deadline.checked_duration_since(Instant::now()) {
                    std::thread::sleep(wait);
                }
            }
            blocks
        });
        audio.scene_presented();
        let mut deadline = Instant::now();
        for frame in 0..150 {
            let ear = eye + Vec3::X * (frame % 12) as f32;
            if frame % 30 == 0 {
                audio.world_effect_at(EFFECTS[0], ear + Vec3::Y * 60.);
            }
            audio.prepare(&mut assets, &world, ear, false, 1. / 30.);
            audio.update_clocks(
                1. / 30.,
                1. / 30.,
                ear,
                frame as f32 * 0.01,
                (60..75).contains(&frame),
                false,
            );
            deadline += Duration::from_secs_f64(1. / 30.);
            if let Some(wait) = deadline.checked_duration_since(Instant::now()) {
                std::thread::sleep(wait);
            }
        }
        stop.store(true, Ordering::Release);
        let blocks = reader.join().unwrap();
        recorder.sample_raw_counters(Tag::Requested);
        let rows = &recorder.raw_counters.as_ref().unwrap().snapshots;
        assert_eq!(rows.len(), 4);
        assert_eq!(rows[2].value, rows[0].value, "{name} PCM starvation");
        assert_eq!(rows[3].value, rows[1].value, "{name} music underruns");
        let c = audio
            .playback
            .as_ref()
            .unwrap()
            .mixer
            .acoustics
            .as_ref()
            .unwrap();
        assert_eq!(c.fallback, 0);
        eprintln!("AUDIO_OFFMAIN_PACED_END map={name} callback_blocks={blocks} pcm_starvation_delta={} music_underrun_delta={} fallback={} host_only=true",rows[2].value-rows[0].value,rows[3].value-rows[1].value,c.fallback);
        audio.finish()?;
    }
    Ok(())
}

#[test]
#[ignore = "requires local LOOKING_GLASS_TEST_DATA; deterministic sample clock"]
fn census_map_output_is_sample_exact() -> Result<()> {
    let mut assets = Assets::open(&PathBuf::from(std::env::var("LOOKING_GLASS_TEST_DATA")?))?;
    for name in ["hedge3", "pandemonium"] {
        let map = Bsp::parse(&assets.read(&format!("maps/{name}.bsp"))?)?;
        let mut world = World::from_bsp(&map)?;
        let (mut a,ma)=setup(false);let (mut b,mb)=setup(false);
        a.load(&mut assets,name,&map);b.load(&mut assets,name,&map);
        a.scene_presented();b.scene_presented();
        compare(a,ma,b,mb,Some(&mut assets),&mut world,name,Vec3::ZERO,120);
    }
    Ok(())
}
