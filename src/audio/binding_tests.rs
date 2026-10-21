// The production Audio::bind_native_counters routes (worker, non-worker, none, config off).
use super::*;
use crate::native_timing::{clock::Native, Metadata, Recorder, Session};

fn session(raw: bool) -> Session {
    let mut r = Box::new(Recorder::new(Native, Some([3; 16]), Metadata::default()));
    r.raw_requested = raw;
    Session { recorder: Some(r) }
}

#[test]
fn production_binding_targets_worker_callback_metrics_only() -> Result<()> {
    #[cfg(feature = "native_timing")]
    let _music_serial = output::native_music_test_guard();
    // PCM worker present: binding must be the SAME OnceLock the callback reads.
    let mut audio = Audio::new(true, None);
    let (mixer, source) = output::mixer();
    let metrics = mixer.metrics.clone();
    let monitor = Monitor {
        source,
        data: audio.monitor.clone(),
        block: Vec::with_capacity(1024),
        record: false,
    };
    let (producer, _callback) = output::buffered(monitor, metrics.clone())?;
    audio.playback = Some(Playback {
        _stream: None,
        mixer,
        _worker: Some(producer),
    });
    let mut timing = session(true);
    audio.bind_native_counters(&mut timing);
    let r = timing.recorder.as_ref().unwrap();
    let capture = r.raw_counters.as_ref().unwrap();
    assert!(capture.pcm.is_some(), "worker route must bind PCM");
    assert_eq!(capture.music.is_some(), block::settings().music_thread);
    assert!(
        metrics.raw_starvation.get().is_some(),
        "binding must activate the callback's own slot"
    );
    assert_eq!(
        capture.pcm.unwrap().lifetime_id,
        metrics.raw_starvation.get().unwrap().binding.lifetime_id
    );
    assert_eq!(
        capture.snapshots[0].lifetime_id,
        capture.pcm.unwrap().lifetime_id
    );
    drop(audio);
    // Non-worker route: PCM must remain absent (no fake zero route).
    let mut audio = Audio::new(true, None);
    let (mixer, _source) = output::mixer();
    let metrics = mixer.metrics.clone();
    audio.playback = Some(Playback {
        _stream: None,
        mixer,
        _worker: None,
    });
    let mut timing = session(true);
    audio.bind_native_counters(&mut timing);
    let capture = timing
        .recorder
        .as_ref()
        .unwrap()
        .raw_counters
        .as_ref()
        .unwrap();
    assert!(
        capture.pcm.is_none(),
        "non-worker playback must not bind a PCM route"
    );
    assert!(metrics.raw_starvation.get().is_none());
    // No playback at all.
    let audio = Audio::new(true, None);
    let mut timing = session(true);
    audio.bind_native_counters(&mut timing);
    assert!(timing
        .recorder
        .as_ref()
        .unwrap()
        .raw_counters
        .as_ref()
        .unwrap()
        .pcm
        .is_none());
    // Config false: nothing activated even with a worker.
    let mut audio = Audio::new(true, None);
    let (mixer, source) = output::mixer();
    let metrics = mixer.metrics.clone();
    let monitor = Monitor {
        source,
        data: audio.monitor.clone(),
        block: Vec::with_capacity(1024),
        record: false,
    };
    let (producer, _callback) = output::buffered(monitor, metrics.clone())?;
    audio.playback = Some(Playback {
        _stream: None,
        mixer,
        _worker: Some(producer),
    });
    let mut timing = session(false);
    audio.bind_native_counters(&mut timing);
    assert!(timing.recorder.as_ref().unwrap().raw_counters.is_none());
    assert!(metrics.raw_starvation.get().is_none());
    Ok(())
}

#[test]
fn actual_audio_session_begin_and_export_request_real_worker_capture() -> Result<()> {
    #[cfg(feature = "native_timing")]
    let _music_serial = output::native_music_test_guard();
    use std::{
        sync::mpsc,
        time::{Duration, Instant},
    };
    struct Held {
        entered: Option<mpsc::Sender<()>>,
        gate: mpsc::Receiver<()>,
    }
    impl Iterator for Held {
        type Item = f32;
        fn next(&mut self) -> Option<f32> {
            if let Some(tx) = self.entered.take() {
                tx.send(()).unwrap();
                self.gate.recv_timeout(Duration::from_secs(2)).unwrap();
            }
            Some(0.2)
        }
    }
    impl Source for Held {
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
    let mut audio = Audio::new(true, None);
    let universe = Box::leak(Box::new(
        crate::native_timing::audio_producer::Universe::new(),
    ));
    let (mixer, source) = output::mixer_with(universe);
    let metrics = mixer.metrics.clone();
    let sink = mixer.sink();
    let (entered, seen) = mpsc::channel();
    let (release, gate) = mpsc::channel();
    sink.append(Held {
        entered: Some(entered),
        gate,
    });
    let monitor = Monitor {
        source,
        data: audio.monitor.clone(),
        block: Vec::with_capacity(1024),
        record: false,
    };
    let (worker, mut callback) = output::buffered(monitor, metrics)?;
    let t = Instant::now();
    while worker.bootstrap.entry.get().is_none() {
        assert!(t.elapsed() < Duration::from_secs(2));
        std::thread::yield_now();
    }
    audio.playback = Some(Playback {
        _stream: None,
        mixer,
        _worker: Some(worker),
    });
    let mut timing = session(true);
    timing.recorder.as_mut().unwrap().producer_requested = true;
    audio.bind_native_counters(&mut timing);
    assert_eq!(
        timing
            .recorder
            .as_ref()
            .unwrap()
            .audio_producer
            .as_ref()
            .unwrap()
            .control
            .as_ref()
            .unwrap()
            .request
            .load(std::sync::atomic::Ordering::Acquire)
            & crate::native_timing::audio_producer::START,
        0,
        "binding alone must not request producer start"
    );
    timing.with(|r| r.begin(crate::native_timing::State::default()));
    {
        let r = timing.recorder.as_ref().unwrap();
        let c = r.audio_producer.as_ref().unwrap().control.as_ref().unwrap();
        assert_eq!(
            Some(c.start_us.load(std::sync::atomic::Ordering::Relaxed)),
            r.pending.as_ref().unwrap().begin_us,
            "start must reuse the first begin timestamp"
        );
    }
    assert!(
        timing
            .recorder
            .as_ref()
            .unwrap()
            .audio_producer
            .as_ref()
            .unwrap()
            .control
            .as_ref()
            .unwrap()
            .request
            .load(std::sync::atomic::Ordering::Acquire)
            & crate::native_timing::audio_producer::START
            != 0,
        "first real begin must request producer start"
    );
    callback
        .by_ref()
        .take(block::settings().cushion * output::SAMPLES)
        .for_each(drop);
    seen.recv_timeout(Duration::from_secs(2)).unwrap();
    release.send(()).unwrap();
    let root = std::path::PathBuf::from(std::env::var("ALICE_TIMING_TEST_OUTPUT")?)
        .join("actual-audio-session");
    timing.export_after_viewer_to(&root)?;
    let owner = timing
        .recorder
        .as_ref()
        .unwrap()
        .audio_producer
        .as_ref()
        .unwrap();
    assert_eq!(owner.status, "Delivered");
    assert!(owner.completed.as_ref().unwrap().receipt.blocks_produced > 0);
    let raw = timing
        .recorder
        .as_ref()
        .unwrap()
        .raw_counters
        .as_ref()
        .unwrap();
    assert!(raw
        .snapshots
        .iter()
        .any(|row| row.tag == crate::native_timing::audio_counters::Tag::AfterViewer));
    let seal_post = owner.seal.unwrap().post_us.unwrap();
    assert!(
        raw.snapshots
            .iter()
            .filter(|row| row.tag == crate::native_timing::audio_counters::Tag::AfterViewer)
            .all(|row| row.pre_us.unwrap() >= seal_post
                && row.pre_us.unwrap()
                    >= owner.completed.as_ref().unwrap().receipt.final_us.unwrap()),
        "AfterViewer reads must follow seal handling"
    );
    let bundle = std::fs::read_dir(root)?.next().unwrap()?.path();
    let value: serde_json::Value =
        serde_json::from_slice(&std::fs::read(bundle.join("records.json"))?)?;
    assert_eq!(value["audio_producer"]["status"], "Delivered");
    assert_eq!(
        value["audio_producer"]["lifecycle"]["producer_loop_closed"],
        false
    );
    drop(audio);
    Ok(())
}
