use super::*;
use std::{cell::Cell, path::PathBuf, rc::Rc};
#[derive(Clone)]
struct Fake(Rc<Cell<Result<u64, clock::Error>>>);
impl Source for Fake {
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
