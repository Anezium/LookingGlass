// Production Callback under a concurrent legacy reporter reset and intentional flushes.
use super::*;
use crate::native_timing::{audio_counters::Tag, clock::Native, Metadata, Recorder};
use std::sync::atomic::AtomicBool;

#[test]
fn concurrent_legacy_swaps_and_flushes_never_reset_or_subtract_raw() {
    let (tx, rx) = queue::channel(4);
    let metrics = Arc::new(Metrics::default());
    let mut callback = Callback {
        ready: rx,
        block: [0.; SAMPLES],
        without_movie: [0.; SAMPLES],
        movie: 0,
        use_movie: true,
        at: SAMPLES,
        epoch: 0,
        metrics: metrics.clone(),
    };
    let mut recorder = Recorder::new(Native, Some([7; 16]), Metadata::default());
    recorder.raw_requested = true;
    recorder.bind_raw_counters(Some(&metrics.raw_starvation), None);
    let base = recorder.raw_counters.as_ref().unwrap().snapshots[0].value;
    assert_eq!(base, 0);
    // Legacy reporter role: swap the per-report counter to zero concurrently.
    let stop = Arc::new(AtomicBool::new(false));
    let reporter = {
        let (m, stop) = (metrics.clone(), stop.clone());
        std::thread::spawn(move || {
            let mut legacy_sum = 0usize;
            while !stop.load(Ordering::Acquire) {
                legacy_sum += m.starved.swap(0, Ordering::Relaxed);
                std::thread::yield_now();
            }
            legacy_sum + m.starved.swap(0, Ordering::Relaxed)
        })
    };
    // 4000 starvation blocks, interleaved with healthy blocks and intentional flushes.
    let mut expected = 0u32;
    for i in 0..4000u32 {
        if i % 5 == 0 {
            tx.push(Block {
                epoch: metrics.epoch.load(Ordering::Acquire),
                samples: [0.25; SAMPLES],
                ..Block::silence(0)
            })
            .ok()
            .unwrap();
            if i % 10 == 0 {
                // Intentional flush: the queued healthy block becomes stale -> RAW starvation.
                metrics.epoch.fetch_add(1, Ordering::Release);
                expected += 1;
            }
        } else {
            expected += 1;
        }
        assert_eq!(callback.by_ref().take(SAMPLES).count(), SAMPLES);
        if i % 997 == 0 {
            recorder.sample_raw_counters(Tag::Requested);
        }
    }
    stop.store(true, Ordering::Release);
    let legacy = reporter.join().unwrap();
    recorder.sample_raw_counters(Tag::AfterViewer);
    let snaps = &recorder.raw_counters.as_ref().unwrap().snapshots;
    let last = snaps.last().unwrap();
    assert!(!last.busy && !last.wrapped);
    assert_eq!(
        last.value, expected,
        "raw total must count every starvation incl. flush"
    );
    assert_eq!(
        legacy as u32, expected,
        "legacy counter still counts each event exactly once"
    );
    for w in snaps.windows(2) {
        assert!(w[0].value <= w[1].value && w[0].post_us <= w[1].pre_us);
    }
    println!(
        "REVIEW callback starvations={expected} legacy_swapped_total={legacy} snapshots={}",
        snaps.len()
    );
}

#[test]
fn config_false_binds_nothing_and_counts_only_legacy() {
    let (_tx, rx) = queue::channel(4);
    let metrics = Arc::new(Metrics::default());
    let mut callback = Callback {
        ready: rx,
        block: [0.; SAMPLES],
        without_movie: [0.; SAMPLES],
        movie: 0,
        use_movie: true,
        at: SAMPLES,
        epoch: 0,
        metrics: metrics.clone(),
    };
    let mut recorder = Recorder::new(Native, Some([7; 16]), Metadata::default());
    recorder.bind_raw_counters(Some(&metrics.raw_starvation), None);
    assert!(metrics.raw_starvation.get().is_none() && recorder.raw_counters.is_none());
    for _ in 0..3 {
        callback.by_ref().take(SAMPLES).count();
    }
    assert_eq!(metrics.starved.load(Ordering::Relaxed), 3);
    assert!(metrics.raw_starvation.get().is_none());
}
