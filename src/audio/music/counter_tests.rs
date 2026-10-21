// Production RingSource with the process-wide raw slot; legacy count stays exact.
use super::*;
use crate::native_timing::{
    audio_counters::{Capture, Raw, Tag},
    clock::{Checked, Native},
};

#[test]
fn music_raw_branch_keeps_legacy_count_exact_and_eof_not_underrun() {
    let _serial = TEST_COUNTER_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let mut clock = Checked::new(Native);
    let raw = Raw::activate(&RAW_UNDERRUNS, &mut clock, Some([8; 16]));
    let mut capture = Capture::new(None, Some(raw));
    capture.sample(&mut clock, Tag::Binding);
    let raw0 = capture.snapshots[0].value;
    let legacy0 = STATS.underrun_frames.load(Ordering::Relaxed);
    let ring = Ring::new();
    let mut source = RingSource {
        ring: ring.clone(),
        read: 0,
        ready: 0,
        rate: 44100,
        channels: 2,
        silence: 0,
        phase: 0,
    };
    for _ in 0..5 {
        assert_eq!(source.next(), Some(0.));
        assert_eq!(source.next(), Some(0.));
    }
    capture.sample(&mut clock, Tag::Requested);
    assert_eq!(
        capture.snapshots[1].value - raw0,
        5,
        "raw counts each unavailable frame once"
    );
    assert_eq!(
        STATS.underrun_frames.load(Ordering::Relaxed) - legacy0,
        5,
        "legacy still counts in the raw branch"
    );
    ring.eof.store(true, Ordering::Release);
    assert_eq!(source.next(), None);
    capture.sample(&mut clock, Tag::Requested);
    assert_eq!(
        capture.snapshots[2].value - raw0,
        5,
        "EOF is not an underrun"
    );
}
