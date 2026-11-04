//! D2 (plan step A7): opt-in count of `pthread_getspecific` lookups and key-list steps.
//!
//! On the Vita every `thread_local!` access ends in `pthread_getspecific`, which calls
//! `vitasdk_get_tls_data(0)` (one `sceKernelGetTLSAddr` import) and then walks a
//! per-thread singly linked list of `{next, key, value}` nodes until the key matches
//! (`pte_osTlsGetValue` in vitasdk's libpthread `vita_osal.o`, read from its disassembly).
//! The Vita link wraps the symbol (`build.rs`). With counting off the wrapper tail-calls
//! the real function. With counting on it repeats the same walk, returns the same value
//! and records how many nodes it visited. Main-thread lookups are told apart by the
//! per-thread list head address, recorded once on the main thread at startup.
//!
//! The wrapper must stay allocation-free, panic-free and must not use thread-local
//! storage: it runs inside every TLS access, including those of allocator callers.
//! Main-thread counters have a single writer (load + store); other threads use
//! fetch_add. All counters are u32 and wrap; readers take wrapping differences.
#![allow(dead_code)]
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering::Relaxed};

static COUNTING: AtomicBool = AtomicBool::new(false);
static MAIN_HEAD: AtomicUsize = AtomicUsize::new(0);
static MAIN_LOOKUPS: AtomicU32 = AtomicU32::new(0);
static MAIN_STEPS: AtomicU32 = AtomicU32::new(0);
static MAIN_MISSES: AtomicU32 = AtomicU32::new(0);
static MAIN_MAX_STEPS: AtomicU32 = AtomicU32::new(0);
/// Main-thread walk lengths: 1, 2, 3-4, 5-8, 9-16, 17-32, 33-64, 65 or more nodes.
static MAIN_HISTOGRAM: [AtomicU32; 8] = [const { AtomicU32::new(0) }; 8];
static OTHER_LOOKUPS: AtomicU32 = AtomicU32::new(0);
static OTHER_STEPS: AtomicU32 = AtomicU32::new(0);

/// One node of the vitasdk pthread-embedded TLS list (offsets 0, 4, 8).
#[repr(C)]
pub(crate) struct Node {
    next: *mut Node,
    key: u32,
    value: *mut core::ffi::c_void,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Snapshot {
    pub(crate) main_lookups: u32,
    pub(crate) main_steps: u32,
    pub(crate) main_misses: u32,
    pub(crate) other_lookups: u32,
    pub(crate) other_steps: u32,
}
impl Snapshot {
    pub(crate) const ZERO: Self = Self { main_lookups: 0, main_steps: 0, main_misses: 0, other_lookups: 0, other_steps: 0 };
    pub(crate) fn since(self, base: Self) -> Self {
        Self {
            main_lookups: self.main_lookups.wrapping_sub(base.main_lookups),
            main_steps: self.main_steps.wrapping_sub(base.main_steps),
            main_misses: self.main_misses.wrapping_sub(base.main_misses),
            other_lookups: self.other_lookups.wrapping_sub(base.other_lookups),
            other_steps: self.other_steps.wrapping_sub(base.other_steps),
        }
    }
}

pub(crate) fn counting() -> bool { COUNTING.load(Relaxed) }
pub(crate) fn set_counting(on: bool) { COUNTING.store(on, Relaxed); }
pub(crate) fn snapshot() -> Snapshot {
    Snapshot {
        main_lookups: MAIN_LOOKUPS.load(Relaxed),
        main_steps: MAIN_STEPS.load(Relaxed),
        main_misses: MAIN_MISSES.load(Relaxed),
        other_lookups: OTHER_LOOKUPS.load(Relaxed),
        other_steps: OTHER_STEPS.load(Relaxed),
    }
}
pub(crate) fn histogram() -> [u32; 8] { std::array::from_fn(|i| MAIN_HISTOGRAM[i].load(Relaxed)) }
pub(crate) fn max_steps() -> u32 { MAIN_MAX_STEPS.load(Relaxed) }
/// Main thread only: restart the max and the histogram for a new window.
pub(crate) fn reset_window_shape() {
    MAIN_MAX_STEPS.store(0, Relaxed);
    for h in &MAIN_HISTOGRAM { h.store(0, Relaxed); }
}

/// Who asks. A bounded table of call sites, by return address, main thread only, while counting.
/// A site is the instruction after the call into `pthread_getspecific`, which ends up in the
/// `thread_local!` it serves or in the function that inlined it. Addresses resolve against the ELF
/// offline: the line also prints this function's own address, so the load offset is known.
const SITES: usize = 256;
#[derive(Default)]
struct Site { pc: AtomicUsize, lookups: AtomicU32, steps: AtomicU32 }
static SITE_TABLE: [Site; SITES] = [const { Site { pc: AtomicUsize::new(0), lookups: AtomicU32::new(0), steps: AtomicU32::new(0) } }; SITES];
static SITE_OVERFLOW: AtomicU32 = AtomicU32::new(0);
#[inline(always)]
pub(crate) fn record_site(head: usize, pc: usize, steps: u32) {
    if head == 0 || head != MAIN_HEAD.load(Relaxed) { return; }
    let mut slot = (pc >> 1).wrapping_mul(0x9E37_79B1) % SITES;
    for _ in 0..SITES {
        let site = &SITE_TABLE[slot];
        let known = site.pc.load(Relaxed);
        if known == pc || known == 0 {
            if known == 0 { site.pc.store(pc, Relaxed); }
            site.lookups.store(site.lookups.load(Relaxed).wrapping_add(1), Relaxed);
            site.steps.store(site.steps.load(Relaxed).wrapping_add(steps), Relaxed);
            return;
        }
        slot = (slot + 1) % SITES;
    }
    SITE_OVERFLOW.store(SITE_OVERFLOW.load(Relaxed).wrapping_add(1), Relaxed);
}
/// Forget the warmup, like the shape counters.
pub(crate) fn reset_sites() {
    for site in &SITE_TABLE { site.pc.store(0, Relaxed); site.lookups.store(0, Relaxed); site.steps.store(0, Relaxed); }
    SITE_OVERFLOW.store(0, Relaxed);
}
/// One line after the window. Top sites by steps, the thing a walk pays for.
pub(crate) fn sites_line(map: &str) -> String {
    let mut rows: Vec<(u32, u32, usize)> = SITE_TABLE.iter().filter(|s| s.pc.load(Relaxed) != 0)
        .map(|s| (s.steps.load(Relaxed), s.lookups.load(Relaxed), s.pc.load(Relaxed))).collect();
    rows.sort_unstable_by(|a, b| b.cmp(a));
    let listed: Vec<String> = rows.iter().take(48).map(|(steps, lookups, pc)| format!("{pc:#x}:{lookups}:{steps}")).collect();
    format!("[lg/tls-sites] map={map} self={:#x} sites={} overflow={} pc_lookups_steps={}", sites_line as fn(&str) -> String as usize, rows.len(), SITE_OVERFLOW.load(Relaxed), listed.join(","))
}

/// The exact `pte_osTlsGetValue` loop: first node from the head word, compare the key
/// at +4, follow +0. Returns (value or null, nodes visited, found).
#[inline(always)]
pub(crate) unsafe fn walk(head: *const *mut Node, key: u32) -> (*mut core::ffi::c_void, u32, bool) {
    let mut node = unsafe { *head };
    let mut steps = 0u32;
    while !node.is_null() {
        steps = steps.wrapping_add(1);
        if unsafe { (*node).key } == key { return (unsafe { (*node).value }, steps, true); }
        node = unsafe { (*node).next };
    }
    (core::ptr::null_mut(), steps, false)
}

#[inline(always)]
fn bucket(steps: u32) -> usize {
    match steps { 0..=1 => 0, 2 => 1, 3..=4 => 2, 5..=8 => 3, 9..=16 => 4, 17..=32 => 5, 33..=64 => 6, _ => 7 }
}

#[inline(always)]
pub(crate) fn record(head: usize, steps: u32, found: bool) {
    if head != 0 && head == MAIN_HEAD.load(Relaxed) {
        MAIN_LOOKUPS.store(MAIN_LOOKUPS.load(Relaxed).wrapping_add(1), Relaxed);
        MAIN_STEPS.store(MAIN_STEPS.load(Relaxed).wrapping_add(steps), Relaxed);
        if !found { MAIN_MISSES.store(MAIN_MISSES.load(Relaxed).wrapping_add(1), Relaxed); }
        if steps > MAIN_MAX_STEPS.load(Relaxed) { MAIN_MAX_STEPS.store(steps, Relaxed); }
        if let Some(h) = MAIN_HISTOGRAM.get(bucket(steps)) { h.store(h.load(Relaxed).wrapping_add(1), Relaxed); }
    } else {
        OTHER_LOOKUPS.fetch_add(1, Relaxed);
        OTHER_STEPS.fetch_add(steps, Relaxed);
    }
}

#[cfg(target_os = "vita")]
mod vita {
    use super::*;
    extern "C" {
        fn __real_pthread_getspecific(key: *const core::ffi::c_void) -> *mut core::ffi::c_void;
        fn vitasdk_get_tls_data(thid: i32) -> *mut *mut Node;
    }
    /// Link-time replacement for every `pthread_getspecific` reference (build.rs).
    /// `pthread_getspecific(NULL)` returns NULL; otherwise the key handle's first
    /// word is the OS key that the list stores at +4 (libpthread `pthread_getspecific.o`).
    /// The off path is one load, one branch and a tail call; counting stays out of line.
    #[no_mangle]
    pub unsafe extern "C" fn __wrap_pthread_getspecific(key: *const core::ffi::c_void) -> *mut core::ffi::c_void {
        if COUNTING.load(Relaxed) {
            // Read first: nothing before this line can call out, so lr still holds the return address.
            let pc: usize;
            unsafe { core::arch::asm!("mov {}, lr", out(reg) pc, options(nomem, nostack, preserves_flags)); }
            return unsafe { counted(key, pc) };
        }
        unsafe { __real_pthread_getspecific(key) }
    }
    #[cold]
    #[inline(never)]
    unsafe fn counted(key: *const core::ffi::c_void, pc: usize) -> *mut core::ffi::c_void {
        if key.is_null() { return core::ptr::null_mut(); }
        let os_key = unsafe { *(key as *const u32) };
        let head = unsafe { vitasdk_get_tls_data(0) };
        let (value, steps, found) = unsafe { walk(head, os_key) };
        record(head as usize, steps, found);
        record_site(head as usize, pc, steps);
        value
    }
    pub(super) fn main_head() -> usize { unsafe { vitasdk_get_tls_data(0) as usize } }
}

/// Call once on the main thread before any survey window.
pub(crate) fn register_main() {
    #[cfg(target_os = "vita")]
    MAIN_HEAD.store(vita::main_head(), Relaxed);
}

#[cfg(test)]
pub(crate) fn test_register_main(head: usize) { MAIN_HEAD.store(head, Relaxed); }
/// Serializes host tests that touch the process-wide counters or window shape.
#[cfg(test)]
pub(crate) static TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[cfg(test)]
mod tests {
    use super::*;
    // The list layout and the walk are the parts the Vita cannot unit-test; the
    // counters are process-global, so this is the only test that records.
    #[test]
    fn walk_matches_the_vitasdk_loop_and_records_main_and_other_threads() {
        let _lock = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let mut values = [0u8; 4];
        let mut nodes: Vec<Node> = (0..4).map(|i| Node { next: core::ptr::null_mut(), key: 10 + i as u32,
            value: &mut values[i] as *mut u8 as *mut core::ffi::c_void }).collect();
        for i in 0..3 { let next: *mut Node = &mut nodes[i + 1]; nodes[i].next = next; }
        let first: *mut Node = &mut nodes[0];
        let head: *const *mut Node = &first;
        for (key, steps) in [(10, 1), (11, 2), (13, 4)] {
            let (value, n, found) = unsafe { walk(head, key) };
            assert!(found); assert_eq!(n, steps);
            assert_eq!(value, &mut values[(key - 10) as usize] as *mut u8 as *mut core::ffi::c_void);
        }
        let (value, n, found) = unsafe { walk(head, 99) };
        assert!(value.is_null() && !found && n == 4, "a miss visits every node and returns null");
        let empty: *mut Node = core::ptr::null_mut();
        assert_eq!(unsafe { walk(&empty, 10) }.1, 0);

        test_register_main(head as usize);
        reset_window_shape();
        let before = snapshot();
        for (steps, found) in [(1, true), (2, true), (4, true), (4, false), (70, true)] { record(head as usize, steps, found); }
        std::thread::spawn(move || record(12345, 7, true)).join().unwrap();
        let d = snapshot().since(before);
        assert_eq!((d.main_lookups, d.main_steps, d.main_misses), (5, 81, 1));
        assert_eq!((d.other_lookups, d.other_steps), (1, 7));
        assert_eq!(max_steps(), 70);
        assert_eq!(histogram(), [1, 1, 2, 0, 0, 0, 0, 1]);
        let wrapped = Snapshot { main_lookups: 3, ..Snapshot::ZERO }.since(Snapshot { main_lookups: u32::MAX, ..Snapshot::ZERO });
        assert_eq!(wrapped.main_lookups, 4, "wrapping difference");
    }
    #[test]
    fn sites_are_counted_by_address_on_the_main_thread_and_printed_by_steps() {
        let _lock = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        test_register_main(0x5000);
        reset_sites();
        for _ in 0..3 { record_site(0x5000, 0x8100_0011, 10); }
        record_site(0x5000, 0x8100_0021, 40);
        record_site(0x6000, 0x8100_0031, 99); // Another thread.
        let line = sites_line("skool2");
        assert!(line.starts_with("[lg/tls-sites] map=skool2 self=0x"), "{line}");
        assert!(line.contains("sites=2 overflow=0"), "{line}");
        assert!(line.ends_with("pc_lookups_steps=0x81000021:1:40,0x81000011:3:30"), "{line}");
        // A full table counts what it cannot place and keeps what it has.
        reset_sites();
        for i in 0..(SITES + 2) { record_site(0x5000, 0x1000 + i * 2, 1); }
        assert!(sites_line("m").contains(&format!("sites={SITES} overflow=2")));
        // The warmup is forgotten.
        reset_sites();
        assert!(sites_line("m").contains("sites=0 overflow=0"));
    }
    #[test]
    fn buckets_cover_every_length_once() {
        let edges = [(0, 0), (1, 0), (2, 1), (3, 2), (4, 2), (5, 3), (8, 3), (9, 4), (16, 4), (17, 5), (32, 5), (33, 6), (64, 6), (65, 7), (u32::MAX, 7)];
        for (steps, b) in edges { assert_eq!(bucket(steps), b, "{steps}"); }
    }
}
