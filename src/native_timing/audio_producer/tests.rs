use super::*;
#[path = "host_fixture.rs"]
mod fixture;
use fixture::*;
fn finish(o: &mut Owner, j: &mut Capture<Fake>, f: &Fake) {
    let mut c = Checked::new(f.clone());
    o.request_seal(&mut c);
    j.poll();
    o.collect(Duration::ZERO);
    o.freeze();
}
#[test]
fn contiguous_cuts_reuse_original_end_and_keep_whole_blocks_and_bits() {
    let (mut o, mut j, f, _) = setup();
    o.request_start(Some(1));
    f.0.set(Ok(2));
    j.poll();
    let scope = crate::kernel_alloc::Scope::begin();
    f.0.set(Ok(3));
    let b = j.block_begin();
    f.0.set(Ok(5));
    j.block_end(b, 8.0);
    f.0.set(Ok(6));
    let cut = j.legacy_begin();
    f.0.set(Ok(11));
    j.legacy_end(
        cut,
        Window {
            blocks: 7,
            ..Window::default()
        },
        false,
    );
    // Reset/emit work 6..11 is inside the next native interval, without a gap.
    f.0.set(Ok(12));
    let b = j.block_begin();
    f.0.set(Ok(14));
    j.block_end(b, 2.5);
    f.0.set(Ok(15));
    finish(&mut o, &mut j, &f);
    assert_eq!(scope.finish(), 0);
    assert_eq!(
        o.status, "Delivered",
        "complete bundle count/identity validation"
    );
    let d = o.completed.as_ref().unwrap();
    assert_eq!(
        d.buffers.fences.len(),
        2,
        "final partial append before acknowledgement"
    );
    assert_eq!(d.receipt.flags, 0);
    assert_eq!(
        (d.buffers.fences[0].end_us, d.buffers.fences[1].begin_us),
        (6, 6)
    );
    assert_eq!(d.buffers.blocks[0].total_ms_bits, 8.0f64.to_bits());
    assert_eq!(d.buffers.fences[0].n, 1);
    assert_eq!(d.buffers.legacy[0].window.blocks, 7);
    assert_eq!(d.receipt.legacy_queue_dropped, 1);
    assert!(d.receipt.final_partial);
    assert!(!o.bootstrap.as_ref().unwrap().loop_closed.load(Acquire));
}
#[test]
fn seal_before_start_attachment_and_missing_bundle_are_explicit() {
    let (mut o, mut j, f, _) = setup();
    finish(&mut o, &mut j, &f);
    assert_eq!(
        o.completed.as_ref().unwrap().receipt.reason,
        "SealedBeforeStart"
    );
    let (mut o, _j, _f, _) = setup();
    o.control
        .as_ref()
        .unwrap()
        .terminal
        .store(DELIVERED, Release);
    o.collect(Duration::ZERO);
    assert_eq!(o.status, "ProtocolFailure");
}
#[test]
fn late_completion_never_changes_timeout_export() {
    let (mut o, mut j, f, _) = setup();
    o.request_start(Some(1));
    f.0.set(Ok(2));
    j.poll();
    let mut c = Checked::new(f.clone());
    o.request_seal(&mut c);
    o.collect(Duration::ZERO);
    assert_eq!(o.status, "Timeout");
    f.0.set(Ok(3));
    j.poll();
    o.collect(Duration::from_millis(1));
    assert_eq!(o.status, "Timeout");
    assert!(o.completed.is_none());
    assert!(o.rx.as_ref().unwrap().pop().is_some());
}
#[test]
fn full_return_queue_retains_failed_bundle_and_never_acknowledges_success() {
    let (mut o, mut j, f, _) = setup();
    j.poll();
    let other = Completed {
        identity: o.control.as_ref().unwrap().identity(),
        buffers: Buffers::new(),
        receipt: Receipt::default(),
    };
    assert!(j.active.as_ref().unwrap().return_tx.push(other).is_ok());
    finish(&mut o, &mut j, &f);
    assert_eq!(o.status, "DeliveryFailed");
    assert!(j.failed_return.is_some());
    assert_eq!(
        o.control.as_ref().unwrap().terminal.load(Acquire),
        DELIVERY_FAILED
    );
}
#[test]
fn clocks_resolution_overflow_capacity_and_empty_intervals_fail_closed() {
    for kind in 0..5 {
        let (mut o, mut j, f, _) = setup();
        o.request_start(Some(1));
        f.0.set(Ok(2));
        j.poll();
        f.0.set(Ok(3));
        let b = j.block_begin();
        match kind {
            0 => f.0.set(Err(clock::Error::ReadFailed)),
            1 => f.0.set(Ok(1)),
            2 => {
                j.receipt.blocks_produced = u32::MAX;
                f.0.set(Ok(4));
            }
            3 => f.0.set(Ok(3)),
            _ => f.0.set(Ok(4)),
        }
        j.block_end(b, 6.0);
        f.0.set(Ok(10));
        finish(&mut o, &mut j, &f);
        let receipt = o.completed.unwrap().receipt;
        if kind <= 2 {
            assert_ne!(receipt.flags, 0);
        } else {
            assert_eq!(receipt.flags, 0);
        }
    }
    let (mut o, mut j, f, _) = setup();
    o.request_start(Some(1));
    f.0.set(Ok(2));
    j.poll();
    for _ in 0..BLOCK_CAP + 1 {
        f.0.set(Ok(f.0.get().unwrap() + 1));
        let b = j.block_begin();
        f.0.set(Ok(f.0.get().unwrap() + 1));
        j.block_end(b, 1.0);
    }
    f.0.set(Ok(f.0.get().unwrap() + 1));
    finish(&mut o, &mut j, &f);
    let d = o.completed.unwrap();
    assert_eq!(d.receipt.blocks_lost, 1);
    assert_eq!(d.buffers.blocks.len(), BLOCK_CAP);
    assert!(d.receipt.flags & LOSS != 0);
    let (mut o, mut j, f, _) = setup();
    o.request_start(Some(1));
    f.0.set(Ok(2));
    j.poll();
    f.0.set(Ok(4));
    finish(&mut o, &mut j, &f);
    assert!(o.completed.unwrap().buffers.fences[0].flags & EMPTY != 0);
}
#[test]
fn quantiles_use_legacy_order_statistics_for_twenty_and_twenty_one() {
    for n in [1, 2, 20, 21, 100, 512, 513] {
        let (mut o, mut j, f, _) = setup();
        o.request_start(Some(1));
        f.0.set(Ok(2));
        j.poll();
        for i in 0..n {
            f.0.set(Ok(3 + i * 2));
            let b = j.block_begin();
            f.0.set(Ok(4 + i * 2));
            j.block_end(b, i as f64);
        }
        f.0.set(Ok(10 + n * 2));
        finish(&mut o, &mut j, &f);
        let d = o.completed.unwrap();
        if n <= 512 {
            assert_eq!(
                d.buffers.fences[0].quantile_ms_bits,
                [
                    ((n / 2) as f64).to_bits(),
                    (((n - 1) * 95 / 100) as f64).to_bits(),
                    ((n - 1) as f64).to_bits()
                ]
            );
        } else {
            assert_ne!(d.receipt.flags & LOSS, 0);
        }
    }
}
#[test]
fn duplicate_admission_and_topology_changes_never_allocate_second_buffers() {
    let (mut o, mut j, f, b) = setup();
    let (tx, _) = queue::channel(1);
    let mut c = Checked::new(f.clone());
    let scope = crate::kernel_alloc::Scope::begin();
    let again = Owner::prepare(b.clone(), &tx, &mut c, Some([7; 16]));
    assert_eq!(scope.finish(), 0);
    assert_eq!(again.status, "DuplicateSessionClaim");
    let state = AudioState::mixer(b.universe);
    let replacement = Arc::new(Bootstrap::new(&state));
    let rejected = Owner::prepare(replacement, &tx, &mut c, Some([7; 16]));
    assert_eq!(rejected.status, "AmbiguousMixerTopology");
    state.mixer_dropped();
    o.request_start(Some(1));
    f.0.set(Ok(2));
    j.poll();
    f.0.set(Ok(3));
    finish(&mut o, &mut j, &f);
    assert_ne!(o.completed.unwrap().receipt.flags & TOPOLOGY, 0);
    let state = AudioState {
        birth: b.audio,
        universe: b.universe,
        ..AudioState::default()
    };
    let fresh = Arc::new(Bootstrap::new(&state));
    let scope = crate::kernel_alloc::Scope::begin();
    let rejected = Owner::prepare(fresh, &tx, &mut c, Some([7; 16]));
    assert_eq!(scope.finish(), 0);
    assert_eq!(rejected.status, "GlobalCaptureAlreadyClaimed");
}
#[test]
fn saturation_of_fence_and_legacy_counts_is_visible_even_for_last_drop() {
    let (mut o, mut j, f, _) = setup();
    o.request_start(Some(1));
    f.0.set(Ok(2));
    j.poll();
    for i in 0..FENCE_CAP + 1 {
        f.0.set(Ok(3 + i as u64 * 2));
        let cut = j.legacy_begin();
        f.0.set(Ok(4 + i as u64 * 2));
        j.legacy_end(cut, Window::default(), false);
    }
    f.0.set(Ok(1000));
    finish(&mut o, &mut j, &f);
    let d = o.completed.unwrap();
    assert_eq!(d.receipt.legacy_queue_dropped, 129);
    assert_eq!(d.receipt.legacy_lost, 1);
    assert_eq!(d.receipt.fences_lost, 2);
}

#[test]
fn ordinal_overflow_and_same_metrics_concurrent_claims_are_bounded() {
    let universe = Box::leak(Box::new(Universe::new()));
    universe.ordinal.store(u32::MAX, SeqCst);
    let state = AudioState::mixer(universe);
    assert!(universe.fault.load(SeqCst));
    assert_eq!(state.birth.unwrap().audio_id, 0);
    assert!(universe.topology().is_none());
    let state = Arc::new(AudioState::default());
    let barrier = Arc::new(std::sync::Barrier::new(3));
    let mut threads = Vec::new();
    for _ in 0..2 {
        let (s, b) = (state.clone(), barrier.clone());
        threads.push(std::thread::spawn(move || {
            b.wait();
            s.claim_worker().is_ok()
        }));
    }
    barrier.wait();
    assert_eq!(
        threads
            .into_iter()
            .filter_map(|h| h.join().ok())
            .filter(|v| *v)
            .count(),
        1
    );
}
#[test]
fn crossing_block_is_owned_whole_by_completion_and_empty_cut_is_explicit() {
    let (mut o, mut j, f, _) = setup();
    o.request_start(Some(1));
    f.0.set(Ok(2));
    j.poll();
    f.0.set(Ok(3));
    let begin = j.block_begin();
    f.0.set(Ok(4));
    let cut = j.legacy_begin();
    j.legacy_end(cut, Window::default(), true);
    f.0.set(Ok(5));
    j.block_end(begin, 7.0);
    f.0.set(Ok(6));
    finish(&mut o, &mut j, &f);
    let d = o.completed.unwrap();
    assert_eq!(d.receipt.flags, 0);
    assert_eq!(d.buffers.fences[0].n, 0);
    assert_eq!(d.buffers.fences[1].n, 1);
    assert_eq!(
        (d.buffers.blocks[0].begin_us, d.buffers.blocks[0].end_us),
        (3, 5)
    );
    assert_eq!(d.buffers.fences[1].quantile_ms_bits, [7.0f64.to_bits(); 3]);
}

#[test]
fn final_ack_requires_successful_bundle_publication_before_acquire() {
    struct Fixed;
    impl Source for Fixed {
        fn domain(&self) -> &'static str {
            "host-injected-fake-us"
        }
        fn read_us(&mut self) -> Result<u64, clock::Error> {
            Ok(10)
        }
    }
    let (mut owner, old, _fake, bootstrap) = setup();
    let control = owner.control.as_ref().unwrap().clone();
    let rx = old.inbox;
    let (entered, seen) = std::sync::mpsc::channel();
    let (release, gate) = std::sync::mpsc::channel();
    let mut journal = Capture::new(Fixed, bootstrap, rx);
    journal.transfer_pause = Some((entered, gate));
    let thread = std::thread::spawn(move || journal.close("WorkerStop"));
    seen.recv_timeout(Duration::from_secs(2)).unwrap();
    let terminal_before_push = control.terminal.load(Acquire);
    release.send(()).unwrap();
    thread.join().unwrap();
    assert_eq!(
        terminal_before_push, PENDING,
        "final ack published before complete bundle push"
    );
    owner.collect(Duration::ZERO);
    assert_eq!(owner.status, "Delivered");
    assert!(owner.completed.is_some());
}

#[cfg(target_os = "macos")]
fn host_thread_cpu_ns() -> u64 {
    // Local macOS SDK _time.h: CLOCK_THREAD_CPUTIME_ID=16. Test only; this is
    // neither the Vita clock adapter nor a timestamp for exported evidence.
    #[repr(C)]
    struct Timespec {
        sec: i64,
        nsec: i64,
    }
    extern "C" {
        fn clock_gettime(clock: u32, out: *mut Timespec) -> i32;
    }
    let mut t = Timespec { sec: 0, nsec: 0 };
    assert_eq!(unsafe { clock_gettime(16, &mut t) }, 0);
    assert!(t.sec >= 0 && t.nsec >= 0);
    t.sec as u64 * 1_000_000_000 + t.nsec as u64
}
#[test]
fn host_only_bounded_producer_hook_wall_cpu_and_allocation_cost() {
    let mut results = Vec::with_capacity(6);
    for enabled in [false, true] {
        for _ in 0..3 {
            let universe = Box::leak(Box::new(Universe::new()));
            let state = AudioState::mixer(universe);
            let bootstrap = Arc::new(Bootstrap::new(&state));
            let (tx, rx) = queue::channel(1);
            let mut c = Checked::new(clock::Native);
            let mut owner = if enabled {
                let o = Owner::prepare(bootstrap.clone(), &tx, &mut c, Some([7; 16]));
                o.request_start(c.read());
                Some(o)
            } else {
                None
            };
            let mut journal = Capture::new(clock::Native, bootstrap, rx);
            let mut source = rodio::buffer::SamplesBuffer::new(2, 44100, vec![0.25; 512 * 1024]);
            let scope = crate::kernel_alloc::Scope::begin();
            #[cfg(target_os = "macos")]
            let cpu = host_thread_cpu_ns();
            let start = Instant::now();
            let mut checksum = 0f32;
            for i in 0..512 {
                journal.poll();
                let native = journal.block_begin();
                let legacy = Instant::now();
                for _ in 0..1024 {
                    checksum += std::hint::black_box(source.next().unwrap());
                }
                let total = legacy.elapsed().as_secs_f64() * 1000.;
                journal.block_end(native, total);
                if i % 64 == 63 {
                    let cut = journal.legacy_begin();
                    journal.legacy_end(cut, Window::default(), true);
                }
            }
            let wall = start.elapsed().as_nanos();
            #[cfg(target_os = "macos")]
            let cpu = Some(host_thread_cpu_ns() - cpu);
            #[cfg(not(target_os = "macos"))]
            let cpu: Option<u64> = None;
            assert_eq!(
                scope.finish(),
                0,
                "producer hook allocated during bounded capture"
            );
            assert_eq!(checksum, 0.25 * 512. * 1024.);
            if let Some(o) = &mut owner {
                o.request_seal(&mut c);
                journal.poll();
                o.collect(Duration::ZERO);
                assert_eq!(o.status, "Delivered");
                assert_eq!(o.completed.as_ref().unwrap().receipt.blocks_produced, 512);
            }
            results.push((enabled, wall, cpu));
        }
    }
    println!("HOST ONLY actual producer journal hooks + 524288 SamplesBuffer samples each trial; (enabled,wall_ns,thread_cpu_ns)={results:?}; zero hot allocations. Not the full game/worker overhead or Vita estimate");
}

#[test]
fn start_publication_failure_and_combined_start_seal_remain_unqualified() {
    let universe = Box::leak(Box::new(Universe::new()));
    let state = AudioState::mixer(universe);
    let boot = Arc::new(Bootstrap::new(&state));
    let (tx, _rx) = queue::channel(1);
    let (rt, _rr) = queue::channel(1);
    let c = Arc::new(Control {
        run_id: Some([7; 16]),
        clock_id: clock::DOMAIN,
        audio_id: state.birth.unwrap().audio_id,
        worker_id: boot.worker_id,
        topology: universe.topology().unwrap(),
        request: AtomicU32::new(0),
        start_us: AtomicU64::new(0),
        start_fault: AtomicBool::new(false),
        terminal: AtomicU32::new(0),
        final_blocks: AtomicU32::new(0),
        final_fences: AtomicU32::new(0),
        final_legacy: AtomicU32::new(0),
    });
    let empty = Buffers {
        blocks: Vec::new(),
        fences: Vec::new(),
        legacy: Vec::new(),
        scratch: Vec::new(),
    };
    assert!(tx
        .push(Start {
            buffers: empty,
            control: c,
            return_tx: rt
        })
        .is_ok());
    let mut clock = Checked::new(clock::Native);
    let mut owner = Owner::prepare(boot, &tx, &mut clock, Some([7; 16]));
    assert_eq!(owner.status, "StartPublicationFailed");
    owner.collect(Duration::ZERO);
    assert!(owner.completed.is_none());
    let (mut o, mut j, f, _) = setup();
    o.request_start(Some(0));
    finish(&mut o, &mut j, &f);
    assert_eq!(o.completed.unwrap().receipt.reason, "SealedBeforeStart");
    let (mut o, mut j, f, _) = setup();
    o.request_start(Some(0));
    f.0.set(Ok(1));
    j.poll();
    finish(&mut o, &mut j, &f);
    let d = o.completed.unwrap();
    assert!(d.receipt.no_residual_interval);
    assert!(d.buffers.fences.is_empty());
}
