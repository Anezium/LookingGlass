//! Standalone HOST allocation regression for the actual exporter.
//! Compile with the locked host dependencies; put the binary/output outside source.
//! Measures requested heap bytes, not allocator overhead, stack or device RAM.
#![allow(dead_code)]
#[path = "mod.rs"]
mod native_timing;
// The actual exporter receives an explicit owned root below. Session startup is
// not used in this standalone process; prevent accidentally exercising a shim.
mod preferences {pub fn path(_: &str)->std::path::PathBuf {panic!("Session path is outside this exporter-only probe")}}
use native_timing::*;
#[path = "audio_producer/host_fixture.rs"]
mod host_fixture;
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering::Relaxed};

struct Meter;
#[repr(C)]
struct Header {
    tracked: bool,
}
static ON: AtomicBool = AtomicBool::new(false);
static CURRENT: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);
static ALLOCS: AtomicUsize = AtomicUsize::new(0);
unsafe impl GlobalAlloc for Meter {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let (combined, offset) = Layout::new::<Header>().extend(layout).unwrap();
        let base = unsafe { System.alloc(combined.pad_to_align()) };
        if base.is_null() {
            return base;
        }
        let tracked = ON.load(Relaxed);
        unsafe { base.cast::<Header>().write(Header { tracked }) };
        if tracked {
            ALLOCS.fetch_add(1, Relaxed);
            let current = CURRENT.fetch_add(layout.size(), Relaxed) + layout.size();
            PEAK.fetch_max(current, Relaxed);
        }
        unsafe { base.add(offset) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        let (combined, offset) = Layout::new::<Header>().extend(layout).unwrap();
        let base = unsafe { ptr.sub(offset) };
        if unsafe { (*base.cast::<Header>()).tracked } {
            CURRENT.fetch_sub(layout.size(), Relaxed);
        }
        unsafe { System.dealloc(base, combined.pad_to_align()) };
    }
    unsafe fn realloc(&self, ptr: *mut u8, old: Layout, new_size: usize) -> *mut u8 {
        let new = unsafe { self.alloc(Layout::from_size_align(new_size, old.align()).unwrap()) };
        if !new.is_null() {
            unsafe { std::ptr::copy_nonoverlapping(ptr, new, old.size().min(new_size)) };
            unsafe { self.dealloc(ptr, old) };
        }
        new
    }
}
#[global_allocator]
static ALLOCATOR: Meter = Meter;
struct Fake(u64);
impl clock::Source for Fake {
    fn domain(&self) -> &'static str {
        "host-injected-fake-us"
    }
    fn read_us(&mut self) -> Result<u64, clock::Error> {
        Ok(self.0)
    }
}
fn main() {
    let output = std::env::args().nth(1).expect("new owned output root");
    let root = std::path::PathBuf::from(&output);
    // All metadata hashes present with maximum decimal byte width. All bounded
    // datasets populated, including maximum escaped map/entry lengths.
    // Production Session owns Box<Recorder>; retain that layout on the export thread.
    let mut r = Box::new(Recorder::new(
        Fake(0),
        Some([255; 16]),
        Metadata {
            source_sha256: Some([255; 32]),
            build_sha256: Some([255; 32]),
            game_config_sha256: Some([255; 32]),
            startup_save_sha256: Some([255; 32]),
            recorder_config_sha256: [255; 32],
        },
    ));
    for _ in 0..WORLD_CAP - 1 {
        r.ready(
            &"\0".repeat(64),
            Some(&"\0".repeat(128)),
            Reason::StartSave,
            State::default(),
        );
    }
    for i in 0..FRAME_CAP - 1 {
        r.clock.source.0 = i as u64 * 10_000;
        r.begin(State::default());
        r.cpu_begin();
        r.simulation(0.01, 0.01, 0.01);
        r.end(State::default(), 0);
    }
    r.request_stop();
    r.clock.source.0 += 10_000;
    r.begin(State::default());
    while r.events.len() < EVENT_CAP - 1 {
        r.event(Milestone::Recovery, OPERATION);
    }
    r.raw_requested = true;
    let pcm = std::sync::OnceLock::new();
    let music = std::sync::OnceLock::new();
    r.bind_raw_counters(Some(&pcm), Some(&music));
    for _ in 0..audio_counters::SNAPSHOT_CAP / 2 - 1 {
        r.sample_raw_counters(audio_counters::Tag::Requested);
    }
    r.audio_producer = Some(Box::new(host_fixture::filled(8191, 127, 127)));
    r.audio_producer
        .as_mut()
        .unwrap()
        .associate_counters(r.raw_counters.as_ref());
    let audio = r.audio_producer.as_ref().unwrap();
    let buffers = &audio.completed.as_ref().unwrap().buffers;
    println!("HOST actual layout NativeRecorder={} FakeRecorder={} Block={} Fence={} Legacy={} Buffers={} Control={} Bootstrap={} AudioState={} Universe={} Owner={} CaptureNative={} Start={} OptionStart={} Completed={} OptionCompleted={} StartChannel={} CompletedChannel={} producer_stack_reserve=32768 bootstrap_reserve={} fixed_reserve={} capacities={:?} rows={:?}",std::mem::size_of::<Recorder<clock::Native>>(),std::mem::size_of::<Recorder<Fake>>(),std::mem::size_of::<audio_producer::Block>(),std::mem::size_of::<audio_producer::Fence>(),std::mem::size_of::<audio_producer::Legacy>(),std::mem::size_of::<audio_producer::Buffers>(),std::mem::size_of::<audio_producer::Control>(),std::mem::size_of::<audio_producer::Bootstrap>(),std::mem::size_of::<audio_producer::AudioState>(),std::mem::size_of::<audio_producer::Universe>(),std::mem::size_of::<audio_producer::Owner>(),std::mem::size_of::<audio_producer::Capture<clock::Native>>(),std::mem::size_of::<audio_producer::Start>(),std::mem::size_of::<Option<audio_producer::Start>>(),std::mem::size_of::<audio_producer::Completed>(),std::mem::size_of::<Option<audio_producer::Completed>>(),transfer::allocation_bytes::<audio_producer::Start>(1),transfer::allocation_bytes::<audio_producer::Completed>(1),audio_producer::BOOTSTRAP_RESERVE,audio_producer::FIXED_RESERVE,buffers.capacities(),[buffers.blocks.len(),buffers.fences.len(),buffers.legacy.len()]);
    let mut plan=schedule::Schedule::new(schedule::Config{map:"\0".repeat(64),ready_offset_us:600_000_000});
    plan.ready(&"\0".repeat(64),1,Some(0));
    for i in 0..schedule::TOKEN_CAP {plan.submit(i as u64*schedule::POLL_US,i as u64+1,1,0,State::default());}
    r.schedule=Some(plan);
    let storage = r.actual_storage_bytes();
    // Run export on a bounded HOST stack. On macOS verify the actual pthread
    // allocation (rather than assuming Builder's requested minimum is exact).
    let (dir, peak, count, retained, actual_stack) = std::thread::Builder::new()
        .stack_size(16 * 1024)
        .spawn(move || {
            #[cfg(target_os = "macos")]
            let actual_stack = {
                extern "C" {
                    fn pthread_self() -> *mut std::ffi::c_void;
                    fn pthread_get_stacksize_np(thread: *mut std::ffi::c_void) -> usize;
                }
                unsafe { pthread_get_stacksize_np(pthread_self()) }
            };
            #[cfg(not(target_os = "macos"))]
            let actual_stack = 0; // explicitly unavailable on other hosts
            ON.store(true, Relaxed);
            let dir = r.export(&root).unwrap();
            ON.store(false, Relaxed);
            (
                dir,
                PEAK.load(Relaxed),
                ALLOCS.load(Relaxed),
                CURRENT.load(Relaxed),
                actual_stack,
            )
        })
        .unwrap()
        .join()
        .unwrap();
    let manifest = std::fs::read(dir.join("manifest.json")).unwrap();
    let value: serde_json::Value = serde_json::from_slice(&manifest).unwrap();
    assert_eq!(value["frame_count"], FRAME_CAP - 1);
    assert_eq!(value["world_count"], WORLD_CAP - 1);
    assert_eq!(value["event_count"], EVENT_CAP - 1);
    assert_eq!(
        value["audio_producer_storage_bytes"],
        audio_producer::MAX_STORAGE
    );
    assert!(manifest.len() < 16 * 1024);
    // Reserve bounds requested HOST heap plus the ENTIRE measured export-thread
    // stack, including fixed/hash/serializer scratch. Allocator bookkeeping and
    // OS/thread-runtime storage are not process RAM measurements.
    assert!(peak <= 48 * 1024, "export heap reserve exceeded: {peak}");
    if cfg!(target_os = "macos") {
        assert!(actual_stack > 0 && actual_stack <= 32 * 1024);
        assert!(
            peak + actual_stack + STATIC_AUX_BYTES <= AUX_RESERVE,
            "auxiliary reserve exceeded"
        );
    }
    assert!(storage + AUX_RESERVE <= BUDGET);
    println!("HOST export_heap_peak={peak} actual_thread_stack={actual_stack} allocations={count} retained_path={retained} manifest_bytes={} storage={storage} aux={AUX_RESERVE} total_reserved={} limit={BUDGET}; native stack/device RAM UNTESTED", manifest.len(), storage + AUX_RESERVE);
    let storage_statics = SESSION_CLAIM_BYTES + audio_producer::STATIC_BYTES + CLOCK_STATIC_RESERVE_BYTES;
    assert_eq!(STATIC_AUX_BYTES, RAW_SLOT_BYTES + operations::STATIC_BYTES + overlay::STATIC_BYTES);
    assert_eq!(operations::STATIC_BYTES, 6);
    assert!(storage_statics <= audio_producer::BOOTSTRAP_RESERVE);
    println!("HOST static_payload glyph_table={} operations={} raw_slots={} session_claim={} universe={} clock_origin={} storage_statics_already_charged={storage_statics} aux_statics={STATIC_AUX_BYTES} static_total={} aux_used={} aux_limit={AUX_RESERVE}; probe allocator/test-only statics excluded; linker padding and native RAM UNMEASURED", overlay::STATIC_BYTES, operations::STATIC_BYTES, RAW_SLOT_BYTES, SESSION_CLAIM_BYTES, audio_producer::STATIC_BYTES, CLOCK_STATIC_RESERVE_BYTES, storage_statics + STATIC_AUX_BYTES, peak + actual_stack + STATIC_AUX_BYTES);
    println!("HOST raw_snapshot_size={} raw_max_storage={MAX_RAW_STORAGE} raw_slots_in_aux={RAW_SLOT_BYTES}", std::mem::size_of::<audio_counters::Snapshot>());
}
