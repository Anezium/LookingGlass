// Adversarial interleavings and multi-writer stress on the production Raw counter.
use super::*;
use crate::native_timing::clock::{Checked, Native};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

fn fresh(start: u32) -> Arc<Raw> {
    let mut c = Checked::new(Native);
    let r = Raw::activate(&OnceLock::new(), &mut c, Some([0x5a; 16]));
    r.value.store(start, SeqCst);
    r
}

/// Several writers (legacy closure inside), several readers, a wrap in the middle.
/// Invariant: a NON-busy snapshot whose numeric value is below the start value
/// (i.e. wrapped) MUST carry the sticky wrap fault. Values seen by one reader
/// never go backwards unless wrapped=true. Totals and legacy counts are exact.
#[test]
fn multi_writer_multi_reader_wrap_never_masquerades() {
    const WRITERS: usize = 4;
    const EVENTS: usize = 3000;
    const ROUNDS: usize = 60;
    let mut stats = [0usize; 4]; // non-busy, busy, wrapped-flagged, wrapped-region observed
    for _ in 0..ROUNDS {
        let start = u32::MAX - (WRITERS * EVENTS / 2) as u32;
        let r = fresh(start);
        let legacy = Arc::new(AtomicUsize::new(0));
        let done = Arc::new(AtomicBool::new(false));
        let mut readers = Vec::new();
        for _ in 0..2 {
            let r = r.clone();
            let done = done.clone();
            readers.push(std::thread::spawn(move || {
                let mut c = Checked::new(Native);
                let mut s = [0usize; 4];
                let mut last_unwrapped = start;
                let mut n = 0u64;
                while !done.load(Ordering::Acquire) || n < 10 {
                    n += 1;
                    let snap = r.read(&mut c, n, Field::PcmStarvedBlocks, Tag::Requested);
                    assert!(
                        snap.pre_us.is_some()
                            && snap.post_us.is_some()
                            && snap.pre_us <= snap.post_us
                    );
                    if snap.busy {
                        s[1] += 1;
                        continue;
                    }
                    s[0] += 1;
                    if snap.wrapped {
                        s[2] += 1;
                    }
                    if snap.value < start {
                        s[3] += 1;
                        assert!(
                            snap.wrapped,
                            "wrapped value {} without sticky fault",
                            snap.value
                        );
                    } else if !snap.wrapped {
                        assert!(
                            snap.value >= last_unwrapped,
                            "non-wrapped value went backwards"
                        );
                        last_unwrapped = snap.value;
                    }
                }
                s
            }));
        }
        let mut writers = Vec::new();
        for _ in 0..WRITERS {
            let r = r.clone();
            let legacy = legacy.clone();
            writers.push(std::thread::spawn(move || {
                for _ in 0..EVENTS {
                    r.event_with(|| {
                        legacy.fetch_add(1, Ordering::Relaxed);
                    });
                }
            }));
        }
        for w in writers {
            w.join().unwrap();
        }
        done.store(true, Ordering::Release);
        for h in readers {
            let s = h.join().unwrap();
            for i in 0..4 {
                stats[i] += s[i];
            }
        }
        assert_eq!(legacy.load(Ordering::Relaxed), WRITERS * EVENTS);
        assert_eq!(
            r.value.load(SeqCst),
            start.wrapping_add((WRITERS * EVENTS) as u32)
        );
        assert_eq!(r.writers.load(SeqCst), 0);
        assert!(r.wrapped.load(SeqCst));
        let mut c = Checked::new(Native);
        let last = r.read(&mut c, 1, Field::PcmStarvedBlocks, Tag::Requested);
        assert!(!last.busy && last.wrapped);
    }
    println!(
        "REVIEW stress nonbusy={} busy={} wrapped_flag={} wrapped_region={}",
        stats[0], stats[1], stats[2], stats[3]
    );
    // Stress observations describe scheduling; the channel test proves busy deterministically.
}

/// Deterministic interleavings through the production read hooks: a COMPLETE
/// wrapping event at each read point must leave the snapshot either busy or
/// carrying the fault; never a silent small value.
#[test]
fn complete_wrap_at_each_read_point_is_never_silent() {
    for point in 0..4u8 {
        let r = fresh(u32::MAX);
        let mut c = Checked::new(Native);
        let snap = r.read_with(
            &mut c,
            1,
            Field::MusicUnderrunFrames,
            Tag::Requested,
            |at| {
                let here = match at {
                    ReadPoint::BeforePre => 0,
                    ReadPoint::AfterPre => 1,
                    ReadPoint::AfterValue => 2,
                    ReadPoint::AfterPost => 3,
                };
                if here == point {
                    r.event();
                }
            },
        );
        if point <= 1 {
            assert_eq!(snap.value, 0);
            assert!(snap.wrapped && !snap.busy, "point {point}");
        } else if point == 2 {
            // Value read before the wrap; fault published before the wrapped read.
            assert_eq!(snap.value, u32::MAX);
            assert!(snap.wrapped, "fault must be visible once published");
        } else {
            assert_eq!(snap.value, u32::MAX);
            assert!(!snap.wrapped && !snap.busy);
        }
    }
}

/// Deterministic writer held inside the actual legacy closure by channels.
#[test]
fn writer_inside_legacy_marks_busy_until_exit() {
    let r = fresh(7);
    let worker = r.clone();
    let (entered, seen) = std::sync::mpsc::channel();
    let (release, gate) = std::sync::mpsc::channel();
    let thread = std::thread::spawn(move || {
        worker.event_with(|| {
            entered.send(()).unwrap();
            gate.recv_timeout(std::time::Duration::from_secs(2))
                .unwrap();
        })
    });
    seen.recv_timeout(std::time::Duration::from_secs(2))
        .unwrap();
    let mut c = Checked::new(Native);
    let snap = r.read(&mut c, 1, Field::PcmStarvedBlocks, Tag::Requested);
    assert!(snap.busy);
    assert_eq!(snap.value, 7);
    release.send(()).unwrap();
    thread.join().unwrap();
    assert_eq!(r.value.load(SeqCst), 8);
}

/// Activation returns the SAME lifetime for an already-initialized slot: the
/// lifetime is a counter lifetime, never re-randomized/reset by a second bind.
#[test]
fn reactivation_keeps_lifetime_value_and_activation_pre() {
    let slot = OnceLock::new();
    let mut c = Checked::new(Native);
    let a = Raw::activate(&slot, &mut c, Some([1; 16]));
    a.event();
    let b = Raw::activate(&slot, &mut c, Some([2; 16]));
    assert!(Arc::ptr_eq(&a, &b));
    assert_eq!(b.binding.lifetime_id, Some([1; 16]));
    assert_eq!(b.value.load(SeqCst), 1);
}
