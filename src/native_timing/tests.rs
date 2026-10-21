use super::*;
use std::{cell::Cell, path::PathBuf, rc::Rc};
#[derive(Clone)]
struct Fake(Rc<Cell<Result<u64, clock::Error>>>);
impl Source for Fake {
    fn domain(&self) -> &'static str {
        "host-injected-fake-us"
    }
    fn read_us(&mut self) -> Result<u64, clock::Error> {
        self.0.get()
    }
}
fn fake() -> (Recorder<Fake>, Fake) {
    let clock = Fake(Rc::new(Cell::new(Ok(0))));
    (
        Recorder::new(clock.clone(), Some([7; 16]), Metadata::default()),
        clock,
    )
}
fn state(ticks: u64) -> State {
    State::observe(3.5, 7.25, ticks, [1., 2., 3., 0.2, -0.1])
}
fn frame(r: &mut Recorder<Fake>, before: State, after: State, reasons: u32) {
    r.begin(before);
    r.cpu_begin();
    r.simulation(0.06, 0.05, 0.025);
    r.end(after, reasons);
}
fn root(name: &str) -> PathBuf {
    let p = std::env::var_os("ALICE_TIMING_TEST_OUTPUT")
        .expect("tests require explicit owned output directory");
    PathBuf::from(p).join(name)
}
#[test]
fn pending_attributes_whole_interval_to_previous_begin_and_never_export_time() {
    let (mut r, c) = fake();
    r.ready("tower2", None, Reason::Fresh, state(5));
    c.0.set(Ok(10));
    frame(&mut r, state(5), state(11), 0);
    r.measured(Some(73));
    assert!(r.frames.is_empty());
    c.0.set(Ok(90));
    r.begin(state(11));
    assert_eq!(r.frames[0].begin_us, Some(10));
    assert_eq!(r.frames[0].end_us, Some(90));
    assert_eq!(r.frames[0].measured_id, Some(73));
    assert_eq!(
        r.frames[0].simulation.unwrap().elapsed_bits,
        0.06f32.to_bits()
    );
    let pending = r.pending.unwrap();
    c.0.set(Ok(1000));
    let dir = r.export(&root("pending")).unwrap();
    let m: serde_json::Value =
        serde_json::from_slice(&std::fs::read(dir.join("manifest.json")).unwrap()).unwrap();
    assert_eq!(m["pending_count"], 1);
    assert!(!m["stopped_at_actual_begin"].as_bool().unwrap());
    assert_eq!(r.pending.unwrap().begin_us, pending.begin_us);
    assert_eq!(r.pending.unwrap().end_us, None);
}
#[test]
fn all_install_reasons_increment_same_map_and_preserve_restored_clock_baselines() {
    let (mut r, c) = fake();
    for (i, reason) in [
        Reason::Fresh,
        Reason::StartSave,
        Reason::Load,
        Reason::Retry,
        Reason::ChapterOrConsole,
        Reason::Natural,
    ]
    .into_iter()
    .enumerate()
    {
        c.0.set(Ok((i as u64) * 1000));
        r.before_install();
        r.ready("tower2", Some("same-entry"), reason, state(250));
        frame(&mut r, state(250), state(251), 0);
        assert_eq!(r.generation, i as u64 + 1);
        assert_eq!(r.worlds[i].baseline, state(250));
    }
    assert!(r.frames.iter().all(|f| f.reasons & INSTALL != 0));
    let generation = r.generation;
    r.discontinuity(Milestone::LoadAttempt, OPERATION);
    // Real failed preparation has no before_install/ready call.
    r.event(Milestone::LoadFailed, OPERATION);
    assert_eq!(r.generation, generation);
    assert!(r.pending.unwrap().reasons & OPERATION != 0);
}
#[test]
fn modal_pause_cinema_skip_and_frozen_frames_cannot_compress_viewer_ids() {
    let (mut r, c) = fake();
    r.ready("tower2", None, Reason::Fresh, state(0));
    for (i, reasons) in [
        0,
        PAUSED | INACTIVE,
        CINEMA,
        0,
        FROZEN_SIM,
        UNFOCUSED | INACTIVE,
    ]
    .into_iter()
    .enumerate()
    {
        c.0.set(Ok(i as u64 * 100));
        frame(&mut r, state(0), state(0), reasons);
    }
    r.discontinuity(Milestone::SkipComplete, SKIP);
    r.discontinuity(Milestone::ModalEnter, MODAL);
    r.request_stop();
    c.0.set(Ok(10000));
    r.begin(state(0));
    assert!(r.stopped);
    assert!(r.pending.is_none());
    assert_eq!(
        r.frames.iter().map(|f| f.viewer_seq).collect::<Vec<_>>(),
        vec![1, 2, 3, 4, 5, 6]
    );
    assert!(r.frames[1].reasons & PAUSED != 0);
    assert!(r.frames[2].reasons & CINEMA != 0);
    assert!(r.frames[5].reasons & (SKIP | MODAL | OPERATION) != 0);
    assert_eq!(r.frames[5].end_us, Some(10000));
}
#[test]
fn checked_clock_latches_errors_backwards_overflow_and_never_resets() {
    use clock::Error::*;
    assert_eq!(clock::timespec_us(12, 345_678_999), Ok(12_345_678));
    assert_eq!(clock::timespec_us(-1, 0), Err(InvalidTimespec));
    assert_eq!(clock::timespec_us(0, 1_000_000_000), Err(InvalidTimespec));
    assert_eq!(clock::timespec_us(i64::MAX, 0), Err(Overflow));
    for error in [ReadFailed, InvalidTimespec, Overflow] {
        let (mut r, c) = fake();
        r.begin(state(0));
        c.0.set(Err(error));
        r.begin(state(0));
        c.0.set(Ok(300));
        r.begin(state(0));
        assert_eq!(r.clock.fault, Some(error));
        assert!(r.frames[0].end_us.is_none());
        assert!(r.frames[1].begin_us.is_none());
    }
    let (mut r, c) = fake();
    c.0.set(Ok(100));
    r.begin(state(0));
    c.0.set(Ok(99));
    r.begin(state(0));
    assert_eq!(r.clock.fault, Some(Backward));
    r.viewer_seq = u64::MAX;
    c.0.set(Ok(200));
    r.begin(state(0));
    assert!(r.arithmetic_overflow);
}
#[test]
fn fixed_budget_no_hot_allocations_no_overwrite_and_disabled_closure_not_called() {
    let (mut r, c) = fake();
    r.ready("tower2", None, Reason::Fresh, state(0));
    let caps = (
        r.frames.capacity(),
        r.events.capacity(),
        r.worlds.capacity(),
    );
    let scope = crate::kernel_alloc::Scope::begin();
    for i in 0..FRAME_CAP + 11 {
        c.0.set(Ok(i as u64 * 10));
        frame(&mut r, state(1), state(2), 0);
    }
    let allocations = scope.finish();
    assert_eq!(allocations, 0);
    assert_eq!(r.frames.len(), FRAME_CAP);
    assert_eq!(r.lost_frames, 10);
    assert_eq!(r.frames[0].viewer_seq, 1);
    assert_eq!(
        caps,
        (
            r.frames.capacity(),
            r.events.capacity(),
            r.worlds.capacity()
        )
    );
    assert!(r.actual_storage_bytes() + AUX_RESERVE <= BUDGET);
    let mut session = Session { recorder: None };
    let scope = crate::kernel_alloc::Scope::begin();
    for _ in 0..10000 {
        session.with(|_| panic!("disabled path evaluated"));
    }
    assert_eq!(scope.finish(), 0);
    println!(
        "HOST storage={} aux={} budget={} Frame={} World={} Event={} capacities={caps:?}",
        r.actual_storage_bytes(),
        AUX_RESERVE,
        BUDGET,
        std::mem::size_of::<Frame>(),
        std::mem::size_of::<World>(),
        std::mem::size_of::<Event>()
    );
}
#[test]
fn event_world_loss_and_unknown_identity_are_explicit() {
    let mut r = Recorder::new(Native, None, Metadata::default());
    assert!(r.invalid_identity);
    for _ in 0..WORLD_CAP + 2 {
        r.ready("tower2", None, Reason::Load, state(0));
    }
    for _ in 0..EVENT_CAP + 2 {
        r.event(Milestone::Status, 0);
    }
    assert_eq!(r.worlds.len(), WORLD_CAP);
    assert_eq!(r.lost_worlds, 2);
    assert!(r.lost_events > 0);
    r.ready(&"x".repeat(65), None, Reason::Fresh, state(0));
    assert!(r.invalid_identity);
}
#[test]
fn production_writer_partial_failures_and_repeated_snapshots_never_overwrite() {
    use super::export::Stage;
    for (index, stage) in [Stage::Data, Stage::BeforeManifest, Stage::Manifest]
        .into_iter()
        .enumerate()
    {
        let (mut r, _) = fake();
        let path = root(&format!("failure-{index}"));
        assert!(r.export_inner(&path, Some(stage)).is_err());
        let child = std::fs::read_dir(path)
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        let manifest = child.join("manifest.json");
        if stage == Stage::Manifest {
            assert!(
                serde_json::from_slice::<serde_json::Value>(&std::fs::read(manifest).unwrap())
                    .is_err()
            );
        } else {
            assert!(!manifest.exists());
        }
    }
    let (mut r, _) = fake();
    let path = root("repeated");
    let a = r.export(&path).unwrap();
    let old = std::fs::read(a.join("records.json")).unwrap();
    let b = r.export(&path).unwrap();
    assert_ne!(a, b);
    let m: serde_json::Value =
        serde_json::from_slice(&std::fs::read(b.join("manifest.json")).unwrap()).unwrap();
    assert_eq!(m["repeated_cumulative_export"], true);
    assert_eq!(m["capture_ordinal"], 2);
    assert_eq!(old, std::fs::read(a.join("records.json")).unwrap());
    r.export_attempts = 0;
    assert!(r.export(&path).is_err()); // same filename collision
    let a = export::random_id().unwrap();
    let b = export::random_id().unwrap();
    assert_ne!(a, b);
}
#[test]
fn emit_actual_recorder_fixtures_for_offline_parser_and_bounded_host_overhead() {
    for (name, step, last) in [
        ("exact", 10_000, 50_020_000),
        ("straddle", 3_000_000, 54_000_000),
    ] {
        let (mut r, c) = fake();
        r.ready("tower2", Some("Tower2_Start"), Reason::StartSave, state(60));
        let started = std::time::Instant::now();
        for b in (0..last).step_by(step as usize) {
            c.0.set(Ok(b));
            frame(&mut r, state(60), state(61), 0);
        }
        r.request_stop();
        c.0.set(Ok(last));
        r.begin(state(61));
        println!(
            "HOST fakeclock recorder {name} {} frames {}ns; no Vita extrapolation",
            r.frames.len(),
            started.elapsed().as_nanos()
        );
        r.export(&root(name)).unwrap();
    }
    let (mut r, c) = fake();
    r.ready("tower2", None, Reason::Fresh, state(0));
    // First status/operation interval is wholly before settle. One actual very
    // long frame crosses both window edges and remains whole, N=1 SMALL.
    for (b, reasons) in [(0, 0), (1_000_000, 0), (10_000_000, 0), (70_000_000, 0)] {
        c.0.set(Ok(b));
        frame(&mut r, state(0), state(0), reasons);
    }
    r.request_stop();
    c.0.set(Ok(71_000_000));
    r.begin(state(0));
    r.export(&root("long")).unwrap();
}

#[test]
fn observations_leave_actual_physics_powers_and_simulation_bits_unchanged() {
    use crate::movement::{Controls, FixedClock, Player};
    use macroquad::prelude::{vec3, Vec2};
    let world =
        crate::collision::World::fixture(&[(vec3(-1000., -1000., -100.), vec3(1000., 1000., 0.))]);
    let simulate = |record: bool| {
        let (mut r, c) = fake();
        let mut player = Player::new(vec3(0., 0., 0.1));
        let mut clock = FixedClock::default();
        let mut powers = crate::powerups::State {
            stopped: 0.035,
            recharge: 1.,
            ..Default::default()
        };
        let (mut environment, mut pickup) = (23.75f32, 48.5f32);
        r.ready(
            "fixture",
            None,
            Reason::StartSave,
            State::observe(environment, pickup, clock.ticks, [0.; 5]),
        );
        for (i, elapsed) in [0.0f32, 0.001, 0.016, 0.060, 0.250, -0.001, 0.033, 0.008]
            .into_iter()
            .cycle()
            .take(512)
            .enumerate()
        {
            let before = State::observe(
                environment,
                pickup,
                clock.ticks,
                [player.feet.x, player.feet.y, player.feet.z, 0., 0.],
            );
            if record {
                c.0.set(Ok(i as u64 * 100_000));
                r.begin(before);
                r.cpu_begin();
            }
            // Unchanged production get_frame_time cap and actual powers/FixedClock.
            let dt = elapsed.clamp(0., 0.05);
            let world_dt = powers.world_dt(dt);
            if record {
                r.simulation(elapsed, dt, world_dt);
            }
            environment += world_dt;
            pickup += dt;
            powers.update(dt);
            clock.advance(
                world_dt as f64,
                &world,
                &mut player,
                Controls {
                    wish: Vec2::X,
                    ..Default::default()
                },
            );
            if i % 43 == 0 {
                clock.pause();
            }
            if record {
                r.end(
                    State::observe(
                        environment,
                        pickup,
                        clock.ticks,
                        [player.feet.x, player.feet.y, player.feet.z, 0., 0.],
                    ),
                    0,
                );
            }
        }
        (
            serde_json::to_vec(&player).unwrap(),
            environment.to_bits(),
            pickup.to_bits(),
            clock.ticks,
            serde_json::to_vec(&powers).unwrap(),
        )
    };
    assert_eq!(simulate(false), simulate(true));
}

#[test]
fn bounded_host_real_clock_cost_and_disabled_control() {
    use std::{hint::black_box, time::Instant};
    const N: usize = 2048;
    let mut disabled_ns = Vec::new();
    let mut enabled_ns = Vec::new();
    for _ in 0..3 {
        let mut off = Session { recorder: None };
        let start = Instant::now();
        for _ in 0..N {
            black_box(&mut off).with(|_| panic!("disabled observer"));
        }
        disabled_ns.push(start.elapsed().as_nanos() as u64);
        let mut r = Recorder::new(Native, Some([8; 16]), Metadata::default());
        r.ready("host-cost", None, Reason::Fresh, state(0));
        let start = Instant::now();
        for _ in 0..N {
            let r = black_box(&mut r);
            r.begin(state(0));
            r.cpu_begin();
            r.simulation(0.016, 0.016, 0.016);
            r.end(state(1), 0);
        }
        enabled_ns.push(start.elapsed().as_nanos() as u64);
    }
    let value = serde_json::json!({"scope":"HOST ONLY bounded loop costs; no game/Vita/qualification extrapolation",
        "iterations_per_trial":N,"trials":3,"disabled_total_ns":disabled_ns,"enabled_real_clock_total_ns":enabled_ns,
        "enabled_includes":"state copies, 3 common-clock reads, ring append, eligibility and steady status checks",
        "warmup":"one ready event initializes the shared host clock before each enabled trial"});
    let path = root("host-cost");
    std::fs::create_dir_all(&path).unwrap();
    std::fs::write(
        path.join("HOST-OVERHEAD.json"),
        serde_json::to_vec_pretty(&value).unwrap(),
    )
    .unwrap();
    println!("HOST ONLY {value}");
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum IoFault {
    None,
    Write,
    Flush,
    Sync,
    InterruptedOnce,
    ZeroWrite,
}
#[derive(Default)]
struct IoCounts {
    writes: usize,
    flushes: usize,
    syncs: usize,
    bytes: usize,
}
struct ObservedFile {
    file: std::fs::File,
    counts: Rc<std::cell::RefCell<IoCounts>>,
    fault: IoFault,
    short: usize,
}
impl std::io::Write for ObservedFile {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.counts.borrow_mut().writes += 1;
        match self.fault {
            IoFault::Write => return Err(std::io::Error::other("underlying write failure")),
            IoFault::ZeroWrite => return Ok(0),
            IoFault::InterruptedOnce => {
                self.fault = IoFault::None;
                return Err(std::io::ErrorKind::Interrupted.into());
            }
            _ => {}
        }
        let n = self.file.write(&bytes[..bytes.len().min(self.short)])?;
        self.counts.borrow_mut().bytes += n;
        Ok(n)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.counts.borrow_mut().flushes += 1;
        if self.fault == IoFault::Flush {
            return Err(std::io::Error::other("underlying flush failure"));
        }
        self.file.flush()
    }
}
impl export::DurableWrite for ObservedFile {
    fn sync_all(&self) -> std::io::Result<()> {
        let mut counts = self.counts.borrow_mut();
        assert!(counts.flushes > 0, "sync before buffer flush");
        counts.syncs += 1;
        if self.fault == IoFault::Sync {
            return Err(std::io::Error::other("underlying sync failure"));
        }
        self.file.sync_all()
    }
}
fn observed_file(
    path: &std::path::Path,
    counts: Rc<std::cell::RefCell<IoCounts>>,
    fault: IoFault,
    short: usize,
) -> std::io::Result<ObservedFile> {
    Ok(ObservedFile {
        file: std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)?,
        counts,
        fault,
        short,
    })
}
fn verify_receipt(dir: &std::path::Path) -> (usize, serde_json::Value) {
    use sha2::{Digest, Sha256};
    let raw = std::fs::read(dir.join("records.json")).unwrap();
    let m: serde_json::Value =
        serde_json::from_slice(&std::fs::read(dir.join("manifest.json")).unwrap()).unwrap();
    assert_eq!(m["records_bytes"], raw.len());
    assert_eq!(m["records_sha256"], format!("{:x}", Sha256::digest(&raw)));
    assert!(serde_json::from_slice::<serde_json::Value>(&raw).is_ok());
    (raw.len(), m)
}
#[test]
fn buffered_full_capture_bounds_underlying_writes_and_preserves_exact_receipt() {
    let (mut r, c) = fake();
    r.metadata = Metadata {
        source_sha256: Some([255; 32]),
        build_sha256: Some([255; 32]),
        game_config_sha256: Some([255; 32]),
        startup_save_sha256: Some([255; 32]),
        recorder_config_sha256: [255; 32],
    };
    r.ready("tower2", Some("Tower2_Start"), Reason::StartSave, state(60));
    for i in 0..FRAME_CAP - 1 {
        c.0.set(Ok(i as u64 * 10_000));
        frame(&mut r, state(60), state(61), 0);
    }
    r.request_stop();
    c.0.set(Ok(FRAME_CAP as u64 * 10_000));
    r.begin(state(61));
    r.audio_producer = Some(Box::new(producer_fixture::filled(8191, 127, 127)));
    let counts = Rc::new(std::cell::RefCell::new(IoCounts::default()));
    let dir = r
        .export_with(&root("buffered-full"), None, |p| {
            observed_file(p, counts.clone(), IoFault::None, usize::MAX)
        })
        .unwrap();
    let (bytes, manifest) = verify_receipt(&dir);
    let counts = counts.borrow();
    assert_eq!(counts.bytes, bytes);
    // Streaming JSON tokens should coalesce into full 32KiB writes plus one tail.
    // The +1 permits the serializer to cross a buffer boundary without growth.
    assert!(counts.writes <= bytes.div_ceil(export::EXPORT_BUFFER) + 1);
    assert!(
        counts.writes < 256,
        "unbuffered JSON token writes regressed"
    );
    assert_eq!((counts.flushes, counts.syncs), (1, 1));
    assert_eq!(manifest["frame_count"], FRAME_CAP - 1);
    assert!(std::fs::metadata(dir.join("manifest.json")).unwrap().len() < 16 * 1024);
    assert!(r.actual_storage_bytes() + AUX_RESERVE <= BUDGET);
    assert_eq!(export::EXPORT_BUFFER, 32 * 1024);
    println!(
        "HOST buffered full export bytes={bytes} underlying_writes={} buffer={} aux={AUX_RESERVE}",
        counts.writes,
        export::EXPORT_BUFFER
    );
}
#[test]
fn buffered_short_writes_interrupted_retry_and_tail_flush_hash_exact_bytes() {
    use std::io::Write;
    for (name, fault) in [
        ("short", IoFault::None),
        ("interrupted", IoFault::InterruptedOnce),
    ] {
        let path = root(&format!("buffer-{name}"));
        std::fs::create_dir_all(&path).unwrap();
        let file = path.join("payload");
        let counts = Rc::new(std::cell::RefCell::new(IoCounts::default()));
        let sink = observed_file(&file, counts.clone(), fault, 7).unwrap();
        let mut writer = export::HashWriter::buffered(sink);
        let payload = vec![173; export::EXPORT_BUFFER * 2 + 37];
        // Exercise both short direct writes and a pending tail at finish.
        writer.write_all(&payload).unwrap();
        let (bytes, hash) = writer.finish().unwrap();
        let actual = std::fs::read(file).unwrap();
        assert_eq!(actual, payload);
        assert_eq!(bytes, payload.len() as u64);
        use sha2::{Digest, Sha256};
        assert_eq!(hash, format!("{:x}", Sha256::digest(&payload)));
        assert_eq!(counts.borrow().bytes, payload.len());
        assert_eq!(counts.borrow().syncs, 1);
    }
}
#[test]
fn real_buffered_write_flush_and_sync_failures_never_publish_manifest() {
    for fault in [
        IoFault::Write,
        IoFault::Flush,
        IoFault::Sync,
        IoFault::ZeroWrite,
    ] {
        let (mut r, _) = fake();
        let path = root(&format!("buffer-failure-{fault:?}"));
        let counts = Rc::new(std::cell::RefCell::new(IoCounts::default()));
        let error = r
            .export_with(&path, None, |p| observed_file(p, counts.clone(), fault, 19))
            .unwrap_err();
        let child = std::fs::read_dir(&path)
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        assert!(!child.join("manifest.json").exists());
        let counts = counts.borrow();
        assert!(counts.writes > 0);
        match fault {
            IoFault::Write => {
                assert!(error.to_string().contains("write failure"));
                assert_eq!(counts.syncs, 0);
            }
            IoFault::Flush => {
                assert!(error.to_string().contains("flush failure"));
                assert_eq!(counts.syncs, 0);
            }
            IoFault::Sync => {
                assert!(error.to_string().contains("sync failure"));
                assert_eq!(counts.syncs, 1);
            }
            IoFault::ZeroWrite => {
                assert_eq!(
                    error.downcast_ref::<std::io::Error>().unwrap().kind(),
                    std::io::ErrorKind::WriteZero
                );
                assert_eq!(counts.syncs, 0);
            }
            _ => unreachable!(),
        }
        drop(counts);
        // Retry preserves failed output and is explicitly cumulative/unqualified.
        let retry = r.export(&path).unwrap();
        assert_ne!(retry, child);
        assert_eq!(verify_receipt(&retry).1["repeated_cumulative_export"], true);
        assert!(child.join("records.json").exists());
    }
}
#[test]
fn recovery_invalidates_in_place_reset_without_inventing_generation() {
    for (name, recovery_us) in [
        ("recovery-settle", 15_000_000),
        ("recovery-window", 25_000_000),
    ] {
        let (mut r, c) = fake();
        r.ready("tower2", None, Reason::Fresh, state(0));
        for b in (0..50_020_000).step_by(10_000) {
            c.0.set(Ok(b));
            r.begin(state(0));
            r.cpu_begin();
            r.simulation(0.01, 0.01, 0.01);
            if b == recovery_us {
                let operation = r.operation;
                r.discontinuity(Milestone::Recovery, OPERATION);
                assert_eq!(r.generation, 1);
                assert_eq!(r.worlds.len(), 1);
                assert_eq!(r.operation, operation + 1);
                assert!(r.pending.unwrap().reasons & OPERATION != 0);
            }
            r.end(state(1), 0);
        }
        r.request_stop();
        c.0.set(Ok(50_020_000));
        r.begin(state(1));
        r.export(&root(name)).unwrap();
    }
}
#[test]
fn postseal_event_loss_remains_explicit_and_first_menu_quit_stays_pending() {
    let (mut r, c) = fake();
    r.ready("tower2", None, Reason::Fresh, state(0));
    frame(&mut r, state(0), state(1), 0);
    r.discontinuity(Milestone::ModalEnter, MODAL);
    r.request_stop();
    assert!(!r.stopped);
    assert!(r.pending.is_some());
    // Only a real subsequent begin can seal; Quit/export cannot supply it.
    c.0.set(Ok(1000));
    r.begin(state(1));
    let frame_count = r.frames.len();
    for _ in 0..EVENT_CAP {
        r.discontinuity(Milestone::Recovery, OPERATION);
    }
    assert!(r.lost_events > 0);
    assert_eq!(r.frames.len(), frame_count);
    assert!(r.stopped && r.pending.is_none());
    r.export(&root("postseal-loss")).unwrap();
}

#[test]
fn raw_counter_extension_exports_actual_reads_not_synthetic_window_counters() {
    for (name, positive, missing) in [
        ("raw-zero", false, false),
        ("raw-positive", true, false),
        ("raw-missing", false, true),
    ] {
        let (mut r, c) = fake();
        // Activation follows already-started audio in production. Never backdate
        // it or reconstruct counters from the old reset-per-report values.
        let pcm = std::sync::OnceLock::new();
        let music = std::sync::OnceLock::new();
        r.raw_requested = true;
        r.bind_raw_counters(Some(&pcm), if missing { None } else { Some(&music) });
        r.ready("tower2", Some("Tower2_Start"), Reason::StartSave, state(60));
        for b in (0..50_020_000).step_by(10_000) {
            c.0.set(Ok(b));
            if positive && b == 25_000_000 {
                pcm.get().unwrap().event();
            }
            frame(&mut r, state(60), state(61), 0);
        }
        r.request_stop();
        c.0.set(Ok(50_020_000));
        r.begin(state(61));
        r.sample_raw_counters(audio_counters::Tag::AfterViewer);
        assert!(r.actual_storage_bytes() + AUX_RESERVE <= BUDGET);
        r.export(&root(name)).unwrap();
    }
}

#[test]
fn parseable_orphan_after_drop_retry_still_has_no_manifest() {
    // Fail only the FIRST final buffer flush, then permit BufWriter::Drop to
    // retry it. This intentionally leaves complete parseable orphan records.
    struct OnceFail {
        file: std::fs::File,
        fail: bool,
    }
    impl std::io::Write for OnceFail {
        fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
            if self.fail {
                self.fail = false;
                Err(std::io::Error::other("first flush"))
            } else {
                self.file.write(b)
            }
        }
        fn flush(&mut self) -> std::io::Result<()> {
            self.file.flush()
        }
    }
    impl export::DurableWrite for OnceFail {
        fn sync_all(&self) -> std::io::Result<()> {
            self.file.sync_all()
        }
    }
    let (mut r, _) = fake();
    let path = root("parseable-orphan");
    assert!(r
        .export_with(&path, None, |p| Ok(OnceFail {
            file: std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(p)?,
            fail: true
        }))
        .is_err());
    let dir = std::fs::read_dir(&path)
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let raw = std::fs::read(dir.join("records.json")).unwrap();
    assert!(serde_json::from_slice::<serde_json::Value>(&raw).is_ok());
    assert!(!dir.join("manifest.json").exists());
}

#[path = "audio_producer/host_fixture.rs"]
mod producer_fixture;
#[test]
fn requested_rows_and_complete_producer_journal_use_actual_export_transaction() {
    use audio_producer::*;
    let ramp20: Vec<f64> = (0..20).map(|i| 1.0 + i as f64 * 0.25).collect();
    let ramp21: Vec<f64> = (0..21).map(|i| i as f64 * 0.25).collect();
    let mut high95 = vec![1.0; 18];
    high95.extend([9.0, 9.5]);
    let mut highmax = vec![1.0; 20];
    highmax.push(9.0);
    let zeros: Vec<f64> = (0..20)
        .map(|i| if i % 2 == 0 { 0.0 } else { -0.0 })
        .collect();
    for (name, span, values, quantiles, raw) in [
        ("audio-exact", 1_000_000u64, vec![6.0], [6.0; 3], true),
        ("audio-straddle", 3_000_000, vec![6.0], [6.0; 3], true),
        ("audio-n20-index", 3_000_000, ramp20, [3.5, 5.5, 5.75], true),
        ("audio-n21-index", 3_000_000, ramp21, [2.5, 4.75, 5.0], true),
        ("audio-p95-fail", 3_000_000, high95, [1.0, 9.0, 9.5], true),
        ("audio-max-pass", 3_000_000, highmax, [1.0, 1.0, 9.0], true),
        ("audio-signed-zero", 3_000_000, zeros, [0.0; 3], true),
        ("audio-no-raw", 3_000_000, vec![6.0], [6.0; 3], false),
    ] {
        let (mut r, c) = fake();
        let pcm = std::sync::OnceLock::new();
        let music = std::sync::OnceLock::new();
        r.raw_requested = raw;
        r.bind_raw_counters(Some(&pcm), Some(&music));
        let (mut owner, mut journal, f, _) = producer_fixture::setup();
        owner.request_start(Some(1_000_000));
        f.0.set(Ok(1_000_000));
        journal.poll();
        let mut b = 1_000_000;
        while b + span < 59_000_000 {
            let step = span / (values.len() as u64 + 1);
            for (i, value) in values.iter().enumerate() {
                f.0.set(Ok(b + 1 + i as u64 * step));
                let begin = journal.block_begin();
                f.0.set(Ok(b + (i as u64 + 1) * step));
                journal.block_end(begin, *value);
            }
            f.0.set(Ok(b + span));
            let cut = journal.legacy_begin();
            f.0.set(Ok(b + span + 1));
            journal.legacy_end(
                cut,
                audio_types::Window {
                    blocks: values.len(),
                    mix: quantiles,
                    ..Default::default()
                },
                true,
            );
            b += span;
        }
        f.0.set(Ok(b + 10));
        let begin = journal.block_begin();
        f.0.set(Ok(59_999_000));
        journal.block_end(begin, 7.0);
        f.0.set(Ok(60_000_000));
        let mut fc = clock::Checked::new(f.clone());
        owner.request_seal(&mut fc);
        journal.poll();
        owner.collect(std::time::Duration::ZERO);
        owner.freeze();
        assert_eq!(owner.status, "Delivered");
        for fence in &owner.completed.as_ref().unwrap().buffers.fences {
            if fence.legacy_id != 0 {
                assert_eq!(fence.n as usize, values.len());
                assert_eq!(
                    fence.quantile_ms_bits,
                    quantiles.map(f64::to_bits),
                    "explicit fixture order statistics"
                );
            }
        }
        owner.associate_counters(r.raw_counters.as_ref());
        r.audio_producer = Some(Box::new(owner));
        r.ready("tower2", None, Reason::Fresh, state(0));
        for b in (0..60_020_000).step_by(10_000) {
            c.0.set(Ok(b));
            frame(&mut r, state(0), state(1), 0);
        }
        r.request_stop();
        c.0.set(Ok(60_020_000));
        r.begin(state(1));
        r.sample_raw_counters(audio_counters::Tag::AfterViewer);
        r.export(&root(name)).unwrap();
    }
    let (mut r, c) = fake();
    let pcm = std::sync::OnceLock::new();
    let music = std::sync::OnceLock::new();
    r.raw_requested = true;
    r.bind_raw_counters(Some(&pcm), Some(&music));
    r.ready("tower2", None, Reason::Fresh, state(0));
    for b in (0..50_020_000).step_by(10_000) {
        c.0.set(Ok(b));
        if b == 19_000_000 || b == 51_000_000 {
            r.sample_raw_counters(audio_counters::Tag::Requested);
        }
        if b == 25_000_000 {
            pcm.get().unwrap().event();
            r.sample_raw_counters(audio_counters::Tag::Requested);
        }
        frame(&mut r, state(0), state(1), 0);
    }
    c.0.set(Ok(51_000_000));
    r.sample_raw_counters(audio_counters::Tag::Requested);
    r.request_stop();
    r.begin(state(1));
    r.sample_raw_counters(audio_counters::Tag::AfterViewer);
    r.export(&root("raw-requested")).unwrap();
}
#[test]
fn actual_session_after_viewer_export_samples_each_raw_field() {
    let mut r = Box::new(Recorder::new(
        clock::Native,
        Some([7; 16]),
        Metadata::default(),
    ));
    r.raw_requested = true;
    let pcm = std::sync::OnceLock::new();
    let music = std::sync::OnceLock::new();
    r.bind_raw_counters(Some(&pcm), Some(&music));
    let mut session = Session { recorder: Some(r) };
    let path = root("session-afterviewer");
    session.export_after_viewer_to(&path).unwrap();
    let dir = std::fs::read_dir(path)
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let data: serde_json::Value =
        serde_json::from_slice(&std::fs::read(dir.join("records.json")).unwrap()).unwrap();
    let rows = data["raw_counters"]["snapshots"].as_array().unwrap();
    assert_eq!(rows.len(), 4, "AfterViewer field reads missing");
    assert!(rows[2..].iter().all(|row| row["tag"] == "AfterViewer"));
}

#[test]
fn scheduled_actual_recorder_waits_without_capture_then_seals_before_human_quit() {
    let (mut r, c) = fake();
    r.schedule = Some(schedule::Schedule::new(schedule::Config {
        map: "tower2".into(),
        ready_offset_us: 40_000_000,
    }));
    r.ready("tower2", None, Reason::StartSave, state(0));
    // Real viewer loops continue before the native-age acquisition; no frame/audio journal yet.
    for i in 0..100 {
        c.0.set(Ok(i * 10_000));
        frame(&mut r, state(i), state(i + 1), 0);
    }
    assert!(r.frames.is_empty() && r.pending.is_none() && !r.producer_start_sent);
    let t = r
        .schedule
        .as_mut()
        .unwrap()
        .submit(1_000_000, r.viewer_seq, 1, r.operation, state(100))
        .unwrap();
    r.schedule.as_mut().unwrap().acknowledge(
        schedule::Ack {
            run_id: [7; 16],
            token: t.into(),
        },
        r.run_id,
        2_000_000,
        1,
        r.operation,
    );
    for i in 100..=7200 {
        c.0.set(Ok(i * 10_000));
        frame(&mut r, state(i), state(i + 1), 0);
    }
    assert!(r.stopped && r.pending.is_none());
    assert_eq!(r.capture_stop_us, Some(72_000_000));
    assert_eq!(r.frames.first().unwrap().begin_us, Some(28_000_000));
    assert_eq!(r.frames.len(), 4400);
    assert!(r.schedule.as_ref().unwrap().fault.is_none());
    // Human may spend minutes on menus after capture: no additional acquisition or deadline slide.
    c.0.set(Ok(900_000_000));
    r.begin(state(7200));
    assert_eq!(r.frames.len(), 4400);
    r.export(&root("scheduled")).unwrap();
}
#[test]
fn input_after_review_before_acquisition_invalidates_anchor_without_moving_window() {
    let (mut r, c) = fake();
    r.schedule = Some(schedule::Schedule::new(schedule::Config {
        map: "tower2".into(),
        ready_offset_us: 40_000_000,
    }));
    r.ready("tower2", None, Reason::Fresh, state(0));
    let t = r
        .schedule
        .as_mut()
        .unwrap()
        .submit(1, 1, 1, 0, state(0))
        .unwrap();
    r.schedule.as_mut().unwrap().acknowledge(
        schedule::Ack {
            run_id: [7; 16],
            token: t.into(),
        },
        r.run_id,
        2,
        1,
        0,
    );
    r.schedule_input(true);
    c.0.set(Ok(10));
    r.begin(state(0));
    assert_eq!(
        r.schedule.as_ref().unwrap().fault,
        Some(schedule::Fault::Interrupted)
    );
    assert_eq!(r.schedule.as_ref().unwrap().start_us, Some(40_000_000));
}
#[test]
fn scheduled_actual_modal_or_input_interrupt_does_not_wait_for_viewer_exit_to_seal() {
    let (mut r, c) = fake();
    r.schedule = Some(schedule::Schedule::new(schedule::Config {
        map: "tower2".into(),
        ready_offset_us: 40_000_000,
    }));
    r.ready("tower2", None, Reason::Fresh, state(0));
    let t = r
        .schedule
        .as_mut()
        .unwrap()
        .submit(1, 1, 1, 0, state(0))
        .unwrap();
    r.schedule.as_mut().unwrap().acknowledge(
        schedule::Ack {
            run_id: [7; 16],
            token: t.into(),
        },
        r.run_id,
        2,
        1,
        0,
    );
    c.0.set(Ok(28_000_000));
    frame(&mut r, state(0), state(1), 0);
    r.schedule_input(true);
    c.0.set(Ok(28_010_000));
    r.begin(state(1));
    assert!(r.stopped);
    assert_eq!(
        r.schedule.as_ref().unwrap().fault,
        Some(schedule::Fault::Interrupted)
    );
}

#[test]
fn actual_session_schedule_config_and_bounded_ack_file_are_consumed_only_before_deadline() {
    let dir = root("session-schedule");
    std::fs::create_dir_all(&dir).unwrap();
    let previous = std::env::var_os("LOOKING_GLASS_SETTINGS_DIR");
    std::env::set_var("LOOKING_GLASS_SETTINGS_DIR", &dir);
    struct Reset(Option<std::ffi::OsString>);
    impl Drop for Reset {
        fn drop(&mut self) {
            match &self.0 {
                Some(v) => std::env::set_var("LOOKING_GLASS_SETTINGS_DIR", v),
                None => std::env::remove_var("LOOKING_GLASS_SETTINGS_DIR"),
            };
            SESSION_CLAIM.store(false, std::sync::atomic::Ordering::SeqCst);
        }
    }
    let _reset = Reset(previous);
    std::fs::write(dir.join("native-timing-config.json"),br#"{"enabled":true,"audio_raw_counters":true,"audio_producer_capture":true,"schedule":{"map":"tower2","ready_offset_us":40000000}}"#).unwrap();
    let mut session = Session::open().unwrap();
    let r = session.recorder.as_mut().unwrap();
    r.ready("tower2", None, Reason::StartSave, state(0));
    r.begin(state(0));
    let label = session.schedule_anchor(state(0)).unwrap();
    assert!(label.as_str().starts_with("NT "));
    let r = session.recorder.as_ref().unwrap();
    let token = r.schedule.as_ref().unwrap().latest().unwrap();
    let request = serde_json::json!({"run_id":r.run_id,"token":{"ordinal":token.ordinal,"viewer_seq":token.viewer_seq,"submitted_us":token.submitted_us,"generation":token.generation,"operation":token.operation,"auth":token.auth}});
    std::fs::write(
        dir.join("native-timing-review.json"),
        serde_json::to_vec(&request).unwrap(),
    )
    .unwrap();
    session.poll_schedule_review();
    let p = session
        .recorder
        .as_ref()
        .unwrap()
        .schedule
        .as_ref()
        .unwrap();
    assert_eq!(p.reviewed_token, Some(token));
    assert!(p.fault.is_none());
    // Once accepted, the file is never opened again; later corruption cannot alter the acknowledged token.
    std::fs::write(dir.join("native-timing-review.json"), vec![b'!'; 2000]).unwrap();
    session.poll_schedule_review();
    assert!(session
        .recorder
        .as_ref()
        .unwrap()
        .schedule
        .as_ref()
        .unwrap()
        .fault
        .is_none());
    // A separate actual Session/Recorder fails closed on the bounded oversized request.
    let mut r = Recorder::new(Native, Some([7; 16]), Metadata::default());
    r.schedule = Some(schedule::Schedule::new(schedule::Config {
        map: "tower2".into(),
        ready_offset_us: 40_000_000,
    }));
    r.ready("tower2", None, Reason::Fresh, state(0));
    let mut other = Session {
        recorder: Some(Box::new(r)),
    };
    other.poll_schedule_review();
    assert_eq!(
        other
            .recorder
            .as_ref()
            .unwrap()
            .schedule
            .as_ref()
            .unwrap()
            .fault,
        Some(schedule::Fault::InvalidReview)
    );
}
#[test]
fn operation_attempt_epoch_invalidates_scheduled_actual_interval() {
    operations::enable();
    let (mut r, c) = fake();
    r.schedule = Some(schedule::Schedule::new(schedule::Config {
        map: "tower2".into(),
        ready_offset_us: 40_000_000,
    }));
    r.ready("tower2", None, Reason::Fresh, state(0));
    let t = r
        .schedule
        .as_mut()
        .unwrap()
        .submit(1, 1, 1, 0, state(0))
        .unwrap();
    r.schedule.as_mut().unwrap().acknowledge(
        schedule::Ack {
            run_id: [7; 16],
            token: t.into(),
        },
        r.run_id,
        2,
        1,
        0,
    );
    c.0.set(Ok(28_000_000));
    frame(&mut r, state(0), state(1), 0);
    operations::attempt();
    c.0.set(Ok(28_010_000));
    r.begin(state(1));
    assert!(r.stopped);
    assert_eq!(
        r.schedule.as_ref().unwrap().fault,
        Some(schedule::Fault::Interrupted)
    );
    assert_ne!(r.frames[0].reasons & OPERATION, 0);
}
