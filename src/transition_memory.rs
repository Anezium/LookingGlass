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
pub(crate) fn observe() {
    if !ACTIVE.load(Relaxed) { return; }
    let s = sample();
    PEAK.fetch_max(s.heap_used.unwrap_or(0), Relaxed);
    PEAK_ARENA.fetch_max(s.heap_arena.unwrap_or(0), Relaxed);
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
pub(crate) struct Transition {
    kind: String, from: String, to: String, before: Snapshot,
    released: Option<Snapshot>, finished: bool,
}
impl Transition {
    pub fn begin(kind: &str, from: &str, to: &str) -> Self {
        crate::frame_profile::exclude_transition_frame();
        assert!(!ACTIVE.load(Relaxed), "nested scene transition");
        let before = sample();
        PEAK.store(before.heap_used.unwrap_or(0), Relaxed);
        PEAK_ARENA.store(before.heap_arena.unwrap_or(0), Relaxed);
        ALLOCATIONS.store(0, Relaxed);
        // A begin record survives a failure inside destination construction.
        eprintln!("[lg/transition-memory] {}", serde_json::json!({
            "event":"begin", "kind":kind,"from":from,"to":to,"before":before}));
        let result = Self { kind: kind.into(), from: from.into(), to: to.into(), before,
            released: None, finished: false };
        #[cfg(not(target_os="vita"))]
        crate::kernel_alloc::transition_begin();
        ACTIVE.store(true, Relaxed);
        result
    }
    pub fn destination(&mut self, to: &str) { self.to = to.into(); }
    pub fn after_release(&mut self) { observe(); self.released = Some(sample()); }
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
/// Draw only black here: no outgoing scene references or simulation advance.
pub(crate) async fn released() {
    if legacy_overlap() { return; }
    #[cfg(target_os="vita")]
    let frames = 6;
    #[cfg(not(target_os="vita"))]
    let frames = 1;
    for _ in 0..frames {
        macroquad::prelude::clear_background(macroquad::prelude::BLACK);
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
    { std::env::var_os("LOOKING_GLASS_TRANSITION_ACTION").is_some() }
    #[cfg(not(all(feature="sim_maps_probe",not(target_os="vita"))))]
    { false }
}
pub(crate) fn host_action(frame: u64) -> String {
    #[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
    {
        static FIRED: AtomicBool = AtomicBool::new(false);
        if frame == 2 && !FIRED.swap(true, Relaxed) {
            return std::env::var("LOOKING_GLASS_TRANSITION_ACTION").unwrap_or_default();
        }
    }
    let _ = frame;
    String::new()
}
pub(crate) fn host_destination() -> String {
    #[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
    { std::env::var("LOOKING_GLASS_TRANSITION_DESTINATION").unwrap_or_default() }
    #[cfg(not(all(feature="sim_maps_probe",not(target_os="vita"))))]
    { String::new() }
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
        assert!(t.released.as_ref().unwrap().heap_used.unwrap() < PEAK.load(Relaxed));
        t.finish(true);
        assert!(!ACTIVE.load(Relaxed));
        t.finish(true); // idempotent; Drop cannot emit another completion.
    }
}
