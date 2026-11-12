//! Always-on scene-boundary diagnostics. No per-frame allocation or formatting.
//! Native peak is an allocator/phase-boundary sample, not a continuous RAM trace.
use serde::Serialize;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering::Relaxed};
static ACTIVE: AtomicBool = AtomicBool::new(false);
static PEAK: AtomicUsize = AtomicUsize::new(0);
static PEAK_ARENA: AtomicUsize = AtomicUsize::new(0);
static ALLOCATIONS: AtomicUsize = AtomicUsize::new(0);
#[derive(Debug, Serialize)]
pub(crate) struct Snapshot {
    heap_used: Option<usize>, heap_free: Option<usize>, heap_arena: Option<usize>,
    vgl_ram_free: Option<usize>, vgl_vram_free: Option<usize>, vgl_phycont_free: Option<usize>,
}
fn sample() -> Snapshot {
    #[cfg(target_os = "vita")]
    {
        #[repr(C)]
        struct MallInfo { arena: usize, ordblks: usize, smblks: usize, hblks: usize,
            hblkhd: usize, usmblks: usize, fsmblks: usize, uordblks: usize,
            fordblks: usize, keepcost: usize }
        extern "C" { fn mallinfo() -> MallInfo; fn vglMemFree(kind: i32) -> usize; }
        unsafe {
            let h = mallinfo();
            Snapshot { heap_used: Some(h.uordblks), heap_free: Some(h.fordblks), heap_arena: Some(h.arena),
                vgl_ram_free: Some(vglMemFree(1)), vgl_vram_free: Some(vglMemFree(0)), vgl_phycont_free: Some(vglMemFree(2)) }
        }
    }
    #[cfg(not(target_os = "vita"))]
    Snapshot { heap_used: Some(crate::kernel_alloc::live_bytes()), heap_free: None, heap_arena: None,
        vgl_ram_free: None, vgl_vram_free: None, vgl_phycont_free: None }
}
// Allocation observers need only mallinfo. Query vitaGL's unsynchronized pool
// counters at the main-thread boundary snapshots, never on allocating workers.
#[cfg(target_os = "vita")]
fn heap_sample() -> (usize, usize) {
    #[repr(C)]
    struct MallInfo { arena: usize, ordblks: usize, smblks: usize, hblks: usize,
        hblkhd: usize, usmblks: usize, fsmblks: usize, uordblks: usize,
        fordblks: usize, keepcost: usize }
    // This function is reachable from allocator wrappers. The linked newlib
    // implementation ignores reent; bypass lazy __getreent initialization.
    extern "C" { fn _mallinfo_r(r:*mut core::ffi::c_void) -> MallInfo; }
    let h = unsafe { _mallinfo_r(core::ptr::null_mut()) };
    (h.uordblks, h.arena)
}
pub(crate) fn observe() {
    if !ACTIVE.load(Relaxed) { return; }
    #[cfg(target_os = "vita")]
    let (used, arena) = heap_sample();
    #[cfg(not(target_os = "vita"))]
    let (used, arena) = (crate::kernel_alloc::live_bytes(), 0);
    PEAK.fetch_max(used, Relaxed);
    PEAK_ARENA.fetch_max(arena, Relaxed);
}
/// One branch outside transitions. Bound mallinfo walks to one per 128 allocation
/// calls during a load, plus every startup phase and the release/install boundaries.
#[cfg(target_os = "vita")]
pub(crate) fn allocation(size: usize) {
    if ACTIVE.load(Relaxed) && (size >= 65536 || ALLOCATIONS.fetch_add(1, Relaxed) % 128 == 0) { observe(); }
}
#[cfg(not(target_os = "vita"))]
pub(crate) fn allocation(size: usize) {
    let _ = size;
    if ACTIVE.load(Relaxed) { PEAK.fetch_max(crate::kernel_alloc::live_bytes(), Relaxed); }
}
struct Record {
    kind: String, from: String, to: String, before: Snapshot,
    released: Option<Snapshot>, finished: bool,
}
thread_local! {
    static CURRENT: std::cell::RefCell<std::rc::Weak<std::cell::RefCell<Record>>> = Default::default();
}
pub(crate) struct Transition(std::rc::Rc<std::cell::RefCell<Record>>);
impl Transition {
    pub fn begin(kind: &str, from: &str, to: &str) -> Self {
        crate::frame_profile::exclude_transition_frame();
        // A later operation supersedes an unfinished diagnostic bracket. Close
        // it before resetting the counters; dropping its stale guard is harmless.
        CURRENT.with(|slot| {
            if let Some(previous) = slot.borrow().upgrade() {
                if !previous.borrow().finished {
                    previous.borrow_mut().finish(false);
                    eprintln!("[lg/transition] unfinished operation superseded by {kind}");
                }
            }
        });
        let before = sample();
        PEAK.store(before.heap_used.unwrap_or(0), Relaxed);
        PEAK_ARENA.store(before.heap_arena.unwrap_or(0), Relaxed);
        ALLOCATIONS.store(0, Relaxed);
        // A begin record survives a failure inside destination construction.
        eprintln!("[lg/transition-memory] {}", serde_json::json!({
            "event":"begin", "kind":kind,"from":from,"to":to,"before":before}));
        let result = Self(std::rc::Rc::new(std::cell::RefCell::new(Record {
            kind: kind.into(), from: from.into(), to: to.into(), before,
            released: None, finished: false })));
        CURRENT.with(|slot| *slot.borrow_mut() = std::rc::Rc::downgrade(&result.0));
        #[cfg(not(target_os="vita"))]
        crate::kernel_alloc::transition_begin();
        ACTIVE.store(true, Relaxed);
        result
    }
    pub fn destination(&mut self, to: &str) { self.0.borrow_mut().to = to.into(); }
    pub fn after_release(&mut self) { observe(); self.0.borrow_mut().released = Some(sample()); }
    pub fn finish(&mut self, success: bool) { self.0.borrow_mut().finish(success); }
}
impl Record {
    pub fn finish(&mut self, success: bool) {
        if self.finished { return; }
        observe();
        ACTIVE.store(false, Relaxed);
        let after = sample();
        #[cfg(not(target_os="vita"))]
        let census = Some(crate::kernel_alloc::transition_finish());
        #[cfg(target_os="vita")]
        let census: Option<[usize;4]> = None;
        eprintln!("[lg/transition-memory] {}", serde_json::json!({
            "event":"end", "kind":self.kind,"from":self.from,"to":self.to,"success":success,
            "before":self.before,"released":self.released,"after":after,
            "peak_heap_used":PEAK.load(Relaxed),"peak_heap_arena":PEAK_ARENA.load(Relaxed),
            "peak_kind":if cfg!(target_os="vita") {"sampled-newlib"} else {"host-rust-requests"},
            "host_requests_and_newlib32_upper_bound":census}));
        self.finished = true;
    }
}
impl Drop for Transition { fn drop(&mut self) { self.finish(false); } }

/// Drop resources first, then retire Macroquad's texture queue and vitaGL's
/// four-frame deferred free ring. A fifth swap waits for its previous GC job.
/// Draw without outgoing scene references or simulation advance. Natural exits
/// preserve their original loading indicator throughout retirement and the build.
pub(crate) async fn released() {
    released_with(|| macroquad::prelude::clear_background(macroquad::prelude::BLACK)).await;
}
pub(crate) async fn released_with(mut draw: impl FnMut()) {
    crate::render_fx::prune_surfaces();
    if legacy_overlap() { return; }
    #[cfg(target_os="vita")]
    let frames = 6;
    #[cfg(not(target_os="vita"))]
    let frames = 1;
    for _ in 0..frames {
        draw();
        macroquad::prelude::next_frame().await;
    }
    observe();
}

/// The overlap reference and scripted inputs are absent from console builds.
pub(crate) fn legacy_overlap() -> bool {
    #[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
    { std::env::var_os("LOOKING_GLASS_TRANSITION_LEGACY").is_some() }
    #[cfg(not(all(feature="sim_maps_probe",not(target_os="vita"))))]
    { false }
}
pub(crate) fn host_probe() -> bool {
    #[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
    { std::env::var_os("LOOKING_GLASS_TRANSITION_ACTION").is_some() || std::env::var_os("LOOKING_GLASS_TRANSITION_PLAN").is_some() }
    #[cfg(not(all(feature="sim_maps_probe",not(target_os="vita"))))]
    { false }
}
pub(crate) fn host_action(frame: u64) -> String {
    #[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
    {
        if let Ok(plan) = std::env::var("LOOKING_GLASS_TRANSITION_PLAN") {
            static LAST_PLAN_FRAME: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(u64::MAX);
            let plan: Vec<(u64, String, String)> = serde_json::from_str(&plan).expect("host transition plan");
            if let Some((_, action, destination)) = plan.into_iter().find(|p| p.0 == frame) {
                if LAST_PLAN_FRAME.swap(frame, Relaxed) == frame { return String::new(); }
                std::env::set_var("LOOKING_GLASS_TRANSITION_DESTINATION", destination);
                return action;
            }
            return String::new();
        }
        static FIRED: AtomicBool = AtomicBool::new(false);
        if frame == 2 && !FIRED.swap(true, Relaxed) {
            return std::env::var("LOOKING_GLASS_TRANSITION_ACTION").unwrap_or_default();
        }
    }
    let _ = frame;
    String::new()
}
/// One-shot failures in real builders, armed only after startup. Absent on Vita.
pub(crate) fn fault(stage: &str) -> anyhow::Result<()> {
    #[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
    {
        static FAILED: AtomicBool = AtomicBool::new(false);
        let requested = std::env::var("LOOKING_GLASS_TRANSITION_FAIL_BUILD").unwrap_or_default();
        let live_operation = CURRENT.with(|slot| slot.borrow().upgrade().is_some_and(|r| {
            let r = r.borrow(); !r.finished && r.kind != "startup-entry"
        }));
        if live_operation && (requested == stage || (requested == "1" && stage == "npcs"))
            && !FAILED.swap(true, Relaxed) {
            eprintln!("[lg/transition-fault] host transition fault at {stage}");
            anyhow::bail!("host transition fault at {stage}");
        }
    }
    let _ = stage;
    Ok(())
}
/// A separate fault proves destination failure followed by recovery failure.
/// Neither this hook nor its environment controls exist in the console build.
pub(crate) fn recovery_fault() -> anyhow::Result<()> {
    #[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
    {
        static FAILED: AtomicBool = AtomicBool::new(false);
        let requested = std::env::var("LOOKING_GLASS_TRANSITION_FAIL_RECOVERY").unwrap_or_default();
        if requested == "always" || (requested == "once" && !FAILED.swap(true, Relaxed)) {
            eprintln!("[lg/transition-fault] host outgoing recovery fault");
            anyhow::bail!("host outgoing recovery fault");
        }
    }
    Ok(())
}

#[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
thread_local! {
    static RECOVERY_KEY: std::cell::Cell<Option<macroquad::prelude::KeyCode>> = const { std::cell::Cell::new(None) };
}
#[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
pub(crate) fn host_recovery_key() -> Option<macroquad::prelude::KeyCode> {
    RECOVERY_KEY.with(std::cell::Cell::get)
}
#[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
pub(crate) fn host_recovery_clear_key() { RECOVERY_KEY.with(|key| key.set(None)); }
/// Host-only scripted UI edges, consumed through the real Input::ui path.
/// The permanently failing builder remains armed through the first Enter.
#[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
pub(crate) fn host_recovery_screen(attempt: usize, frame: usize) {
    use macroquad::prelude::*;
    RECOVERY_KEY.with(|key| key.set(None));
    let Ok(mode) = std::env::var("LOOKING_GLASS_RECOVERY_UI") else { return; };
    let action = if frame == 3 {
        match mode.as_str() {
            "retry" => {
                if attempt >= 3 { std::env::remove_var("LOOKING_GLASS_TRANSITION_FAIL_RECOVERY"); }
                RECOVERY_KEY.with(|key| key.set(Some(KeyCode::Enter)));
                "enter"
            }
            "quit" => {
                RECOVERY_KEY.with(|key| key.set(Some(KeyCode::Escape)));
                "escape"
            }
            _ => panic!("unknown host recovery UI mode"),
        }
    } else { "wait" };
    println!("TRANSITION_RECOVERY {}", serde_json::json!({"attempt":attempt,"screen_frame":frame,
        "action":action,"fault_armed":std::env::var("LOOKING_GLASS_TRANSITION_FAIL_RECOVERY").ok()}));
}
#[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
pub(crate) fn host_recovery_capture(attempt: usize, frame: usize) {
    // Read the submitted screen before swapping/clearing the framebuffer.
    if frame == 1 {
        if let Ok(directory) = std::env::var("LOOKING_GLASS_RECOVERY_CAPTURE") {
            let path = std::path::Path::new(&directory).join(format!("screen-{attempt}.png"));
            macroquad::prelude::get_screen_data().export_png(path.to_str().unwrap());
        }
    }
}
pub(crate) fn host_destination() -> String {
    #[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
    { std::env::var("LOOKING_GLASS_TRANSITION_DESTINATION").unwrap_or_default() }
    #[cfg(not(all(feature="sim_maps_probe",not(target_os="vita"))))]
    { String::new() }
}
pub(crate) fn host_start_chapter() -> Option<String> {
    #[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
    { std::env::var("LOOKING_GLASS_START_CHAPTER").ok() }
    #[cfg(not(all(feature="sim_maps_probe",not(target_os="vita"))))]
    { None }
}
pub(crate) fn phase(scope: &str, label: &str) {
    observe();
    #[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
    if ACTIVE.load(Relaxed) && host_probe() {
        println!("TRANSITION_CENSUS {}", serde_json::json!({"scope":scope,"phase":label,
            "live_rust_request_bytes":crate::kernel_alloc::live_bytes(),
            "requests_and_newlib32_upper_bound":crate::kernel_alloc::transition_finish()}));
    }
    let _ = (scope,label);
}

#[cfg(test)]
mod tests {
    #[test]
    fn census_includes_transient_requests_and_final_release() {
        use super::*;
        let mut t = Transition::begin("unit", "out", "in");
        let baseline = sample().heap_used.unwrap();
        let bytes = std::hint::black_box(vec![0u8; 1024 * 1024]);
        assert!(PEAK.load(Relaxed) >= baseline + bytes.len());
        drop(bytes);
        t.after_release();
        assert!(t.0.borrow().released.as_ref().unwrap().heap_used.unwrap() < PEAK.load(Relaxed));
        t.finish(true);
        assert!(!ACTIVE.load(Relaxed));
        t.finish(true); // idempotent; Drop cannot emit another completion.
    }
    #[test]
    fn superseded_guard_closes_once_and_cannot_close_its_successor() {
        use super::*;
        let mut outer = Transition::begin("new-game", "hedge3", "gvillage");
        let mut inner = Transition::begin("death-retry", "hedge3", "hedge3");
        assert!(outer.0.borrow().finished);
        outer.finish(true);
        drop(outer);
        assert!(ACTIVE.load(Relaxed));
        assert!(!inner.0.borrow().finished);
        inner.finish(true);
        assert!(!ACTIVE.load(Relaxed));
    }
}
