use super::*;
use crate::native_timing::{
    audio_producer as d,
    clock::{self, Checked, Native},
};
use std::sync::mpsc;
fn universe() -> &'static d::Universe {
    Box::leak(Box::new(d::Universe::new()))
}
fn wait(mut yes: impl FnMut() -> bool) {
    let t = Instant::now();
    while !yes() {
        assert!(
            t.elapsed() < Duration::from_secs(3),
            "bounded worker deadline"
        );
        thread::yield_now();
    }
}
struct Blocked {
    entered: mpsc::Sender<()>,
    release: mpsc::Receiver<()>,
    calls: usize,
    panic: bool,
}
impl Iterator for Blocked {
    type Item = f32;
    fn next(&mut self) -> Option<f32> {
        if self.calls % SAMPLES == 0 {
            self.entered.send(()).unwrap();
            self.release.recv_timeout(Duration::from_secs(4)).unwrap();
            if self.panic {
                panic!("injected source panic")
            }
        }
        self.calls += 1;
        Some(0.25)
    }
}
impl PcmSource for Blocked {
    fn pcm_block(&mut self, epoch: u32) -> Block {
        let mut block = Block::silence(epoch);
        for sample in &mut block.samples { *sample = self.next().unwrap_or(0.); }
        block
    }
}
impl Source for Blocked {
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
#[test]
fn real_worker_seals_after_block_and_keeps_playing_without_diagnostic_writes() {
    #[cfg(feature = "native_timing")]
    let _music_serial = native_music_test_guard();
    let (control, mut mixer) = mixer_with(universe());
    let sink = control.sink();
    sink.append(rodio::buffer::SamplesBuffer::new(
        2,
        44100,
        vec![0.25; 44100 * 2],
    ));
    // The genuine Mixer and manual Callback exercise the normal worker route.
    let mut config = block::settings();
    config.log_thread = true;
    config.cushion = 1;
    let (worker, mut callback) = buffered_with(mixer, control.metrics.clone(), config).unwrap();
    wait(|| worker.bootstrap.entry.get().is_some());
    // Process actual PCM before the Session attaches; it must not appear as
    // reconstructed native journal history.
    callback.by_ref().take(SAMPLES).for_each(drop);
    for _ in 0..2 {
        wait(|| callback.ready.has_pending());
        assert!(callback.by_ref().take(SAMPLES).all(|v| v == 0.25));
    }
    let mut clock = Checked::new(Native);
    let mut owner = worker.prepare_native(&mut clock, Some([7; 16]));
    owner.request_start(clock.read());
    for _ in 0..2 {
        wait(|| callback.ready.has_pending());
        assert!(callback.by_ref().take(SAMPLES).all(|v| v == 0.25));
    }
    owner.request_seal(&mut clock);
    owner.collect(Duration::from_secs(2));
    owner.freeze();
    assert_eq!(owner.status, "Delivered");
    let before = owner.completed.as_ref().unwrap().buffers.blocks.len();
    assert!(before > 0);
    assert!(!worker.bootstrap.loop_closed.load(Ordering::Acquire));
    for _ in 0..4 {
        wait(|| callback.ready.has_pending());
        assert!(callback.by_ref().take(SAMPLES).all(|v| v == 0.25));
    }
    assert_eq!(
        owner.completed.as_ref().unwrap().buffers.blocks.len(),
        before
    );
    let b = worker.bootstrap.clone();
    drop(worker);
    assert!(b.loop_closed.load(Ordering::Acquire));
    assert_eq!(b.joined.load(Ordering::Acquire), 1);
    assert_eq!(b.reporter.load(Ordering::Acquire), 2);
}
#[test]
fn blocked_source_timeout_late_bundle_and_panic_have_explicit_outcomes() {
    #[cfg(feature = "native_timing")]
    let _music_serial = native_music_test_guard();
    for panic in [false, true] {
        let (control, _mixer) = mixer_with(universe());
        let mut config = block::settings();
        config.cushion = 1;
        let (et, er) = mpsc::channel();
        let (rt, rr) = mpsc::channel();
        let source = Blocked {
            entered: et,
            release: rr,
            calls: 0,
            panic,
        };
        let (worker, mut cb) = buffered_with(source, control.metrics.clone(), config).unwrap();
        wait(|| worker.bootstrap.entry.get().is_some());
        let mut c = Checked::new(Native);
        let mut owner = worker.prepare_native(&mut c, Some([7; 16]));
        owner.request_start(c.read());
        cb.by_ref().take(SAMPLES).for_each(drop);
        er.recv_timeout(Duration::from_secs(2)).unwrap();
        owner.request_seal(&mut c);
        owner.collect(Duration::from_millis(5));
        assert_eq!(owner.status, "Timeout");
        rt.send(()).unwrap();
        wait(|| {
            worker
                .bootstrap
                .control
                .get()
                .unwrap()
                .terminal
                .load(Ordering::Acquire)
                != d::PENDING
        });
        owner.collect(Duration::from_secs(1));
        assert_eq!(owner.status, "Timeout");
        assert!(owner.completed.is_none());
        let b = worker.bootstrap.clone();
        drop(worker);
        assert_eq!(b.joined.load(Ordering::Acquire), if panic { 2 } else { 1 });
        assert_eq!(b.exit.load(Ordering::Acquire), if panic { 3 } else { 1 });
    }
}
#[test]
fn policy_failure_and_spawn_failure_close_reporter_and_reject_late_attach() {
    #[cfg(feature = "native_timing")]
    let _music_serial = native_music_test_guard();
    for spawn in [false, true] {
        let (control, _mixer) = mixer_with(universe());
        let mut cfg = block::settings();
        cfg.log_thread = true;
        let hooks = TestHooks {
            policy_fail: !spawn,
            spawn_fail: spawn,
            ..TestHooks::default()
        };
        let result = buffered_body(
            rodio::buffer::SamplesBuffer::new(2, 44100, vec![0.; 2048]),
            control.metrics.clone(),
            cfg,
            Native,
            hooks,
        );
        if spawn {
            assert!(result.is_err());
            let b = control.metrics.native.bootstrap.get().unwrap();
            assert_eq!(b.exit.load(Ordering::Acquire), 4);
        } else {
            let (worker, _) = result.unwrap();
            wait(|| worker.bootstrap.loop_closed.load(Ordering::Acquire));
            let mut c = Checked::new(Native);
            assert_eq!(
                worker.prepare_native(&mut c, Some([7; 16])).status,
                "PolicyRejected"
            );
            drop(worker);
        }
    }
}
#[test]
fn same_metrics_worker_claim_precedes_prefill_and_topology_is_conservative() {
    #[cfg(feature = "native_timing")]
    let _music_serial = native_music_test_guard();
    let (control, _mixer) = mixer_with(universe());
    let source = || rodio::buffer::SamplesBuffer::new(2, 44100, vec![0.; 2048]);
    let (worker, _) = buffered(source(), control.metrics.clone()).unwrap();
    let before = control.metrics.queued_samples.load(Ordering::Acquire);
    let error = buffered(source(), control.metrics.clone()).err().unwrap();
    assert!(error.to_string().contains("Metrics already has a worker"));
    assert_eq!(
        control.metrics.queued_samples.load(Ordering::Acquire),
        before
    );
    drop(worker);
}
#[test]
fn slow_reporter_drains_after_producer_closure_and_final_queue_drop_is_retained() {
    #[cfg(feature = "native_timing")]
    let _music_serial = native_music_test_guard();
    let (control, _mixer) = mixer_with(universe());
    let mut cfg = block::settings();
    cfg.log_thread = true;
    cfg.cushion = 1;
    let (gate, rx) = mpsc::channel();
    let (seen, counts) = mpsc::channel();
    let hooks = TestHooks {
        interval: Duration::ZERO,
        reporter_gate: Some(rx),
        reporter_seen: Some(seen),
        ..TestHooks::default()
    };
    let (worker, mut cb) = buffered_body(
        rodio::buffer::SamplesBuffer::new(2, 44100, vec![0.5; 44100 * 2]),
        control.metrics.clone(),
        cfg,
        Native,
        hooks,
    )
    .unwrap();
    wait(|| worker.bootstrap.entry.get().is_some());
    let mut c = Checked::new(Native);
    let mut owner = worker.prepare_native(&mut c, Some([7; 16]));
    owner.request_start(c.read());
    for _ in 0..16 {
        cb.by_ref().take(SAMPLES).for_each(drop);
        wait(|| cb.ready.has_pending());
    }
    owner.request_seal(&mut c);
    owner.collect(Duration::from_secs(2));
    assert_eq!(owner.status, "Delivered");
    assert!(
        owner
            .completed
            .as_ref()
            .unwrap()
            .receipt
            .legacy_queue_dropped
            > 0
    );
    // Publish ordinary stop while the reporter is held. It cannot claim closure
    // from stop; producer joins before the reporter is released and drains.
    worker.stop.store(true, Ordering::Release);
    wait(|| worker.bootstrap.loop_closed.load(Ordering::Acquire));
    gate.send(()).unwrap();
    assert_eq!(counts.recv_timeout(Duration::from_secs(2)).unwrap(), 8);
    drop(worker);
}

#[derive(Clone)]
struct StepClock {
    value: Arc<std::sync::atomic::AtomicU64>,
    reads: Arc<AtomicUsize>,
}
impl clock::Source for StepClock {
    fn domain(&self) -> &'static str {
        "host-injected-fake-us"
    }
    fn read_us(&mut self) -> Result<u64, clock::Error> {
        self.reads.fetch_add(1, Ordering::Relaxed);
        Ok(self.value.fetch_add(100, Ordering::SeqCst))
    }
}
struct StackSource {
    source: rodio::buffer::SamplesBuffer<f32>,
    send: Option<mpsc::Sender<usize>>,
}
impl Iterator for StackSource {
    type Item = f32;
    fn next(&mut self) -> Option<f32> {
        if let Some(tx) = self.send.take() {
            #[cfg(target_os = "macos")]
            let actual = {
                extern "C" {
                    fn pthread_self() -> *mut std::ffi::c_void;
                    fn pthread_get_stacksize_np(t: *mut std::ffi::c_void) -> usize;
                }
                unsafe { pthread_get_stacksize_np(pthread_self()) }
            };
            #[cfg(not(target_os = "macos"))]
            let actual = 0;
            tx.send(actual).unwrap();
        }
        self.source.next()
    }
}
impl Source for StackSource {
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
#[test]
fn actual_worker_fake_clock_and_config_false_have_bounded_clocks_and_stack() {
    #[cfg(feature = "native_timing")]
    let _music_serial = native_music_test_guard();
    for enabled in [false, true] {
        let (control, _mixer) = mixer_with(universe());
        let mut cfg = block::settings();
        cfg.cushion = 1;
        let fake = StepClock {
            value: Arc::new(std::sync::atomic::AtomicU64::new(10)),
            reads: Arc::new(AtomicUsize::new(0)),
        };
        let (tx, rx) = mpsc::channel();
        let source = StackSource {
            source: rodio::buffer::SamplesBuffer::new(2, 44100, vec![0.5; 44100 * 2]),
            send: Some(tx),
        };
        let hooks = TestHooks {
            stack: 16 * 1024,
            ..TestHooks::default()
        };
        let (worker, mut cb) =
            buffered_body(tests::Plain(source), control.metrics.clone(), cfg, fake.clone(), hooks).unwrap();
        wait(|| worker.bootstrap.entry.get().is_some());
        let mut c = Checked::new(Native);
        let mut owner = if enabled {
            let o = worker.prepare_native(&mut c, Some([7; 16]));
            o.request_start(c.read());
            Some(o)
        } else {
            None
        };
        cb.by_ref().take(SAMPLES).for_each(drop);
        let stack = rx.recv_timeout(Duration::from_secs(2)).unwrap();
        if cfg!(target_os = "macos") {
            assert!(
                stack > 0 && stack <= 32 * 1024,
                "actual host worker stack exceeds charged reserve"
            );
        }
        wait(|| cb.ready.has_pending());
        cb.by_ref().take(SAMPLES).for_each(drop);
        if let Some(o) = &mut owner {
            o.request_seal(&mut c);
            o.collect(Duration::from_secs(2));
            assert_eq!(o.status, "Delivered");
            assert_eq!(
                o.completed.as_ref().unwrap().receipt.flags & d::CLOCK,
                d::CLOCK,
                "injected producer domain must fail closed against actual Native birth/control"
            );
        }
        let reads = fake.reads.load(Ordering::Relaxed);
        for _ in 0..4 {
            wait(|| cb.ready.has_pending());
            assert!(cb.by_ref().take(SAMPLES).all(|v| v == 0.5));
        }
        assert_eq!(
            fake.reads.load(Ordering::Relaxed),
            reads,
            "closed or disabled producer must perform no new diagnostic clocks"
        );
        if !enabled {
            assert_eq!(reads, 0);
        }
        drop(worker);
        println!("HOST actual producer bounded_stack={stack} diagnostic_config={enabled} clock_reads={reads}; no device/stack/RAM qualification");
    }
}

#[test]
fn reporter_must_not_exit_before_producers_last_emit() {
    #[cfg(feature = "native_timing")]
    let _music_serial = native_music_test_guard();
    let (control, _mixer) = mixer_with(universe());
    let mut cfg = block::settings();
    cfg.log_thread = true;
    cfg.cushion = 1;
    let (et, er) = mpsc::channel();
    let (rt, rr) = mpsc::channel();
    let (gate, rx) = mpsc::channel();
    let (seen, counts) = mpsc::channel();
    let (pass, passed) = mpsc::channel();
    let hooks = TestHooks {
        interval: Duration::ZERO,
        reporter_gate: Some(rx),
        reporter_seen: Some(seen),
        reporter_pass: Some(pass),
        ..TestHooks::default()
    };
    let (worker, mut cb) = buffered_body(
        Blocked {
            entered: et,
            release: rr,
            calls: 0,
            panic: false,
        },
        control.metrics.clone(),
        cfg,
        Native,
        hooks,
    )
    .unwrap();
    wait(|| worker.bootstrap.entry.get().is_some());
    let mut c = Checked::new(Native);
    let mut owner = worker.prepare_native(&mut c, Some([7; 16]));
    owner.request_start(c.read());
    cb.by_ref().take(SAMPLES).for_each(drop);
    er.recv_timeout(Duration::from_secs(2)).unwrap();
    worker.stop.store(true, Ordering::Release);
    gate.send(()).unwrap();
    let (stopped_early, first_drain) = passed.recv_timeout(Duration::from_secs(2)).unwrap();
    // The source is still inside its first block. Now permit the FINAL report.
    rt.send(()).unwrap();
    wait(|| worker.bootstrap.loop_closed.load(Ordering::Acquire));
    let printed = counts.recv_timeout(Duration::from_secs(2)).unwrap();
    owner.collect(Duration::from_secs(1));
    let reason = owner.completed.as_ref().map(|v| v.receipt.reason);
    drop(worker);
    assert!(
        !stopped_early,
        "reporter observed stop instead of producer closure"
    );
    assert_eq!(
        printed,
        first_drain + 1,
        "reporter lost producer's last report"
    );
    assert_eq!(reason, Some("WorkerStop"));
}

#[test]
fn reporter_panic_is_distinct_from_loop_closure_and_join_receipt() {
    #[cfg(feature = "native_timing")]
    let _music_serial = native_music_test_guard();
    let (control, _mixer) = mixer_with(universe());
    let mut cfg = block::settings();
    cfg.log_thread = true;
    let hooks = TestHooks {
        reporter_panic: true,
        ..TestHooks::default()
    };
    let (worker, _cb) = buffered_body(
        rodio::buffer::SamplesBuffer::new(2, 44100, vec![0.; 2048]),
        control.metrics.clone(),
        cfg,
        Native,
        hooks,
    )
    .unwrap();
    wait(|| worker.bootstrap.reporter.load(Ordering::Acquire) == 3);
    assert_eq!(worker.bootstrap.reporter_join.load(Ordering::Acquire), 0);
    let b = worker.bootstrap.clone();
    drop(worker);
    assert_eq!(b.reporter_join.load(Ordering::Acquire), 2);
    assert_eq!(b.joined.load(Ordering::Acquire), 1);
}

// Ownership and lifecycle regressions.
#[test]
fn mixer_drop_and_replacement_change_live_topology() {
    let u = universe();
    let (control, mixer) = mixer_with(u);
    let live = u.topology().expect("one live mixer");
    drop(mixer);
    assert!(
        u.topology().is_none(),
        "dropped mixer must leave no live topology"
    );
    let (_c2, _m2) = mixer_with(u);
    assert_ne!(
        u.topology(),
        Some(live),
        "replacement must change the topology epoch"
    );
    drop(control);
}
#[test]
fn legacy_emit_outcomes_match_cumulative_drop_counter() {
    #[cfg(feature = "native_timing")]
    let _music_serial = native_music_test_guard();
    let (control, _mixer) = mixer_with(universe());
    let mut cfg = block::settings();
    cfg.log_thread = true;
    cfg.cushion = 1;
    let (gate, rx) = mpsc::channel();
    let hooks = TestHooks {
        interval: Duration::ZERO,
        reporter_gate: Some(rx),
        ..TestHooks::default()
    };
    let (worker, mut cb) = buffered_body(
        rodio::buffer::SamplesBuffer::new(2, 44100, vec![0.5; 44100 * 2]),
        control.metrics.clone(),
        cfg,
        Native,
        hooks,
    )
    .unwrap();
    wait(|| worker.bootstrap.entry.get().is_some());
    let mut c = Checked::new(Native);
    let mut owner = worker.prepare_native(&mut c, Some([7; 16]));
    owner.request_start(c.read());
    for _ in 0..16 {
        cb.by_ref().take(SAMPLES).for_each(drop);
        wait(|| cb.ready.has_pending());
    }
    owner.request_seal(&mut c);
    owner.collect(Duration::from_secs(2));
    assert_eq!(owner.status, "Delivered");
    let legacy = &owner.completed.as_ref().unwrap().buffers.legacy;
    assert!(legacy.iter().any(|l| !l.emitted) && legacy.iter().any(|l| l.emitted));
    for w in legacy.windows(2) {
        assert_eq!(
            w[1].window.dropped_reports,
            w[0].window.dropped_reports + u32::from(!w[0].emitted),
            "retained emit outcome disagrees with the legacy drop counter"
        );
    }
    gate.send(()).unwrap();
    drop(worker);
}
/// Genuine Mixer/buffered_body/reporter journal with the Native clock, paced
/// consumption and short legacy intervals; serialized for the Python checker.
#[test]
fn genuine_worker_journal_is_written_for_the_strict_checker() {
    #[cfg(feature = "native_timing")]
    let _music_serial = native_music_test_guard();
    let (control, mixer) = mixer_with(universe());
    control.sink().append(rodio::buffer::SamplesBuffer::new(
        2,
        44100,
        vec![0.25; 44100 * 8],
    ));
    let mut cfg = block::settings();
    cfg.log_thread = true;
    cfg.cushion = 2;
    let hooks = TestHooks {
        interval: Duration::from_millis(25),
        ..TestHooks::default()
    };
    let (worker, mut cb) =
        buffered_body(mixer, control.metrics.clone(), cfg, Native, hooks).unwrap();
    wait(|| worker.bootstrap.entry.get().is_some());
    let mut c = Checked::new(Native);
    let mut owner = worker.prepare_native(&mut c, Some([7; 16]));
    owner.request_start(c.read());
    let t = Instant::now();
    while t.elapsed() < Duration::from_millis(700) {
        wait(|| cb.ready.has_pending());
        cb.by_ref().take(SAMPLES).for_each(drop);
        thread::sleep(Duration::from_millis(2));
    }
    owner.request_seal(&mut c);
    owner.collect(Duration::from_secs(2));
    owner.freeze();
    assert_eq!(owner.status, "Delivered");
    let d = owner.completed.as_ref().unwrap();
    assert_eq!(d.receipt.flags, 0);
    assert!(
        d.buffers.legacy.len() >= 4,
        "several genuine legacy windows"
    );
    let out = std::path::PathBuf::from(std::env::var("ALICE_TIMING_TEST_OUTPUT").unwrap())
        .join("genuine-worker");
    std::fs::create_dir_all(&out).unwrap();
    std::fs::write(out.join("owner.json"), serde_json::to_vec(&owner).unwrap()).unwrap();
    drop(worker);
}
