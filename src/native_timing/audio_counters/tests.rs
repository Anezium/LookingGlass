use super::*;
use crate::native_timing::{Metadata, Recorder};
struct Clock {
    now: u64,
    fault: bool,
}
impl Source for Clock {
    fn domain(&self) -> &'static str {
        "host-injected-fake-us"
    }
    fn read_us(&mut self) -> Result<u64, super::super::clock::Error> {
        if self.fault {
            return Err(super::super::clock::Error::ReadFailed);
        }
        let now = self.now;
        self.now += 1;
        Ok(now)
    }
}
fn clock() -> Checked<Clock> {
    Checked::new(Clock {
        now: 10,
        fault: false,
    })
}
fn raw(c: &mut Checked<Clock>) -> Arc<Raw> {
    Raw::activate(&OnceLock::new(), c, Some([3; 16]))
}
#[test]
fn lifetime_activation_and_each_field_have_distinct_brackets() {
    let mut c = clock();
    let a = raw(&mut c);
    let b = raw(&mut c);
    let mut capture = Capture::new(Some(a), Some(b));
    capture.sample(&mut c, Tag::Binding);
    assert_eq!(capture.snapshots.len(), 2);
    assert_eq!(capture.snapshots[0].pre_us, Some(13));
    assert_eq!(capture.snapshots[0].post_us, Some(14));
    assert_eq!(capture.snapshots[1].pre_us, Some(16));
    assert_eq!(capture.snapshots[1].post_us, Some(17));
    assert!(capture.flush_subset.is_none());
}
#[test]
fn increments_on_each_side_of_each_read_keep_real_atomic_value_and_bracket() {
    for point in [
        ReadPoint::BeforePre,
        ReadPoint::AfterPre,
        ReadPoint::AfterValue,
        ReadPoint::AfterPost,
    ] {
        let mut c = clock();
        let r = raw(&mut c);
        let before = r.read(&mut c, 1, Field::PcmStarvedBlocks, Tag::Requested);
        let snap = r.read_with(&mut c, 2, Field::PcmStarvedBlocks, Tag::Requested, |at| {
            if std::mem::discriminant(&point) == std::mem::discriminant(&at) {
                r.event();
            }
        });
        let after = r.read(&mut c, 3, Field::PcmStarvedBlocks, Tag::Requested);
        assert_eq!(before.value, 0);
        assert_eq!(after.value, 1);
        assert_eq!(
            snap.value,
            u32::from(matches!(point, ReadPoint::BeforePre | ReadPoint::AfterPre))
        );
        assert!(before.post_us <= snap.pre_us && snap.post_us <= after.pre_us);
    }
}
#[test]
fn wrap_in_flight_and_completed_wrap_cannot_masquerade_as_zero() {
    let mut c = clock();
    let r = raw(&mut c);
    r.value.store(u32::MAX, SeqCst);
    r.writers.fetch_add(1, SeqCst);
    r.value.fetch_add(1, SeqCst);
    let busy = r.read(&mut c, 1, Field::PcmStarvedBlocks, Tag::Requested);
    assert_eq!(busy.value, 0);
    assert!(busy.busy);
    assert!(!busy.wrapped);
    r.wrapped.store(true, SeqCst);
    r.writers.fetch_sub(1, SeqCst);
    let fault = r.read(&mut c, 2, Field::PcmStarvedBlocks, Tag::Requested);
    assert!(!fault.busy);
    assert!(fault.wrapped);
    r.event();
    assert!(
        r.read(&mut c, 3, Field::PcmStarvedBlocks, Tag::Requested)
            .wrapped
    );
    // A real event crossing MAX also publishes the sticky fault.
    let r = raw(&mut c);
    r.value.store(u32::MAX, SeqCst);
    r.event();
    assert!(
        r.read(&mut c, 1, Field::MusicUnderrunFrames, Tag::Requested)
            .wrapped
    );
}
#[test]
fn faults_backward_missing_identity_and_saturation_stay_visible() {
    let mut c = clock();
    let a = raw(&mut c);
    let mut capture = Capture::new(Some(a), None);
    for _ in 0..SNAPSHOT_CAP + 3 {
        capture.sample(&mut c, Tag::Requested);
    }
    assert_eq!(capture.snapshots.len(), SNAPSHOT_CAP);
    assert_eq!(capture.lost, 3);
    assert_eq!(capture.attempted, SNAPSHOT_CAP as u64 + 3);
    assert_eq!(capture.snapshots[0].ordinal, 1);
    let r = raw(&mut c);
    c.source.now = 0;
    assert!(r
        .read(&mut c, 1, Field::PcmStarvedBlocks, Tag::Requested)
        .pre_us
        .is_none());
    assert_eq!(c.fault, Some(super::super::clock::Error::Backward));
    let mut c = clock();
    let r = raw(&mut c);
    c.source.fault = true;
    assert!(r
        .read(&mut c, 1, Field::PcmStarvedBlocks, Tag::Requested)
        .post_us
        .is_none());
    let r = Raw::activate(&OnceLock::new(), &mut clock(), None);
    assert!(r.binding.lifetime_id.is_none());
}
#[test]
fn activation_is_nonresetting_multiple_lifetimes_and_disabled_binding_does_no_event_work() {
    let pcm = OnceLock::new();
    let music = OnceLock::new();
    let mut r = Recorder::new(
        Clock {
            now: 0,
            fault: false,
        },
        Some([7; 16]),
        Metadata::default(),
    );
    r.bind_raw_counters(Some(&pcm), Some(&music));
    assert!(pcm.get().is_none() && music.get().is_none() && r.raw_counters.is_none());
    r.raw_requested = true;
    r.bind_raw_counters(Some(&pcm), Some(&music));
    pcm.get().unwrap().event();
    music.get().unwrap().event();
    let id = pcm.get().unwrap().binding.lifetime_id;
    r.sample_raw_counters(Tag::Requested);
    assert_eq!(r.raw_counters.as_ref().unwrap().snapshots[2].value, 1);
    r.bind_raw_counters(Some(&pcm), Some(&music));
    assert_eq!(pcm.get().unwrap().binding.lifetime_id, id);
    assert_eq!(r.raw_counters.as_ref().unwrap().snapshots.len(), 4);
    // New playback slot is a different counter lifetime; old total stays intact.
    let next = OnceLock::new();
    let another = Raw::activate(&next, &mut r.clock, Some([9; 16]));
    assert_ne!(another.binding.lifetime_id, id);
    assert_eq!(another.value.load(SeqCst), 0);
    assert_eq!(pcm.get().unwrap().value.load(SeqCst), 1);
}
#[test]
fn atomic_readers_and_event_writers_stress_without_shared_mutable_recorder() {
    let mut c = clock();
    let r = raw(&mut c);
    let worker = r.clone();
    let join = std::thread::spawn(move || {
        for _ in 0..100_000 {
            worker.event();
        }
    });
    let mut previous = 0;
    for n in 1..=100_000 {
        let s = r.read(&mut c, n, Field::PcmStarvedBlocks, Tag::Requested);
        assert!(s.value >= previous);
        assert!(!s.wrapped);
        previous = s.value;
    }
    join.join().unwrap();
    assert_eq!(r.value.load(SeqCst), 100_000);
}

#[test]
fn legacy_increment_is_inside_busy_bracket_and_late_writer_cannot_hide() {
    let mut c = clock();
    let r = raw(&mut c);
    r.event_with(|| {
        let during = r.read(&mut c, 1, Field::PcmStarvedBlocks, Tag::Requested);
        assert!(during.busy, "legacy increment outside busy bracket");
    });
    assert_eq!(r.value.load(SeqCst), 1);
    let snapshot = r.read_with(
        &mut c,
        2,
        Field::PcmStarvedBlocks,
        Tag::Requested,
        |point| {
            if matches!(point, ReadPoint::AfterValue) {
                r.writers.fetch_add(1, SeqCst);
            }
        },
    );
    assert!(snapshot.busy, "late in-flight writer must fail closed");
    r.writers.fetch_sub(1, SeqCst);
}
