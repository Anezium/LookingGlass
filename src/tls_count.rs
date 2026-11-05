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
//!
//! `tls_fast` lives here too, since it is the same wrapper. A lookup costs one
//! `sceKernelGetTLSAddr` import plus the walk, and the game thread has about 55 keys. List nodes
//! never move or go away while their thread lives: `pte_osTlsSetValue` writes `value` in place and
//! `pte_osThreadDelete` frees them when the thread ends (read from vita_osal.o). So the game thread
//! can remember each key's node once and read `value` straight from it. Lookups from the game
//! thread are told apart by the stack pointer, which needs no thread-local storage.
#![allow(dead_code)]
use std::sync::atomic::{AtomicU8, AtomicU32, AtomicUsize, Ordering::Relaxed};

const COUNT: u8 = 1;
const FAST: u8 = 2;
const SHADOW: u8 = 4;
const SABOTAGE: u8 = 8;
/// Bits above. Zero is the off path: one load, one branch and a tail call.
static MODE: AtomicU8 = AtomicU8::new(0);
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

pub(crate) fn counting() -> bool { MODE.load(Relaxed) & COUNT != 0 }
/// Counting takes the plain walk, so a counted window shows what `tls_fast` removes, not its cost.
pub(crate) fn set_counting(on: bool) {
    if on { MODE.fetch_or(COUNT, Relaxed); } else { MODE.fetch_and(!COUNT, Relaxed); }
}
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

/// Parsed `tls_fast` config value. `shadow` returns the real result and counts disagreements with
/// the remembered node. `shadow_sabotage` looks at a wrong node on purpose, to show the count works.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FastMode { Off, On, Shadow, ShadowSabotage }
impl FastMode {
    pub(crate) fn parse(value: &str) -> Option<Self> {
        match value { "off" => Some(Self::Off), "on" => Some(Self::On), "shadow" => Some(Self::Shadow),
            "shadow_sabotage" => Some(Self::ShadowSabotage), _ => None }
    }
    fn bits(self) -> u8 {
        match self { Self::Off => 0, Self::On => FAST, Self::Shadow => FAST | SHADOW, Self::ShadowSabotage => FAST | SHADOW | SABOTAGE }
    }
    fn name(self) -> &'static str {
        match self { Self::Off => "off", Self::On => "on", Self::Shadow => "shadow", Self::ShadowSabotage => "shadow_sabotage" }
    }
}

/// A power of two. Keys are random 32-bit numbers (`pte_osTlsAlloc` calls `sceKernelGetRandomNumber`),
/// so their low bits index the table directly.
const FAST_SLOTS: usize = 256;
/// Past this the table stops growing, so a probe always ends on an empty slot.
const FAST_LIMIT: u32 = 192;
struct FastSlot { key: AtomicU32, node: AtomicUsize }
struct FastTable { slots: [FastSlot; FAST_SLOTS], handles: [AtomicUsize; FAST_SLOTS], used: AtomicU32 }
impl FastTable {
    const fn new() -> Self {
        Self { slots: [const { FastSlot { key: AtomicU32::new(0), node: AtomicUsize::new(0) } }; FAST_SLOTS],
            handles: [const { AtomicUsize::new(0) }; FAST_SLOTS], used: AtomicU32::new(0) }
    }
    /// The remembered node of a key, or zero. A node address is never zero, an OS key may be.
    #[inline(always)]
    fn find(&self, key: u32) -> usize {
        let mut index = key as usize & (FAST_SLOTS - 1);
        loop {
            let slot = &self.slots[index];
            let node = slot.node.load(Relaxed);
            if node == 0 || slot.key.load(Relaxed) == key { return node; }
            index = (index + 1) & (FAST_SLOTS - 1);
        }
    }
    /// One writer, the game thread. False when the table is full: the caller keeps using the walk.
    fn insert(&self, key: u32, node: usize, handle: usize) -> bool {
        if self.used.load(Relaxed) >= FAST_LIMIT { return false; }
        let mut index = key as usize & (FAST_SLOTS - 1);
        while self.slots[index].node.load(Relaxed) != 0 { index = (index + 1) & (FAST_SLOTS - 1); }
        self.slots[index].key.store(key, Relaxed);
        self.slots[index].node.store(node, Relaxed);
        self.handles[index].store(handle, Relaxed);
        self.used.store(self.used.load(Relaxed) + 1, Relaxed);
        true
    }
    /// Sabotage only: the next remembered node after this key's own, which is some other key's.
    fn other_than(&self, node: usize) -> usize {
        self.slots.iter().map(|slot| slot.node.load(Relaxed)).find(|&other| other != 0 && other != node).unwrap_or(node)
    }
}
static FAST_TABLE: FastTable = FastTable::new();
static FAST_STACK_LOW: AtomicUsize = AtomicUsize::new(0);
static FAST_STACK_SIZE: AtomicUsize = AtomicUsize::new(0);
static FAST_OVERFLOW: AtomicU32 = AtomicU32::new(0);
static FAST_UNSET: AtomicU32 = AtomicU32::new(0);
static FAST_CHECKS: AtomicU32 = AtomicU32::new(0);
static FAST_MISMATCHES: AtomicU32 = AtomicU32::new(0);
static FAST_STARTED: AtomicU8 = AtomicU8::new(0);

/// The node holding `key` in a list, or null. Same walk as `walk`, for the node instead of the value.
#[inline(always)]
unsafe fn node_of(head: *const *mut Node, key: u32) -> *mut Node {
    let mut node = unsafe { *head };
    while !node.is_null() {
        if unsafe { (*node).key } == key { return node; }
        node = unsafe { (*node).next };
    }
    core::ptr::null_mut()
}

/// A lookup the table cannot answer yet: find the node by walking, remember it. A key with no node
/// on this thread reads as null, like the real function, and stays out of the table until it is set.
#[cold]
#[inline(never)]
unsafe fn fast_fill(table: &FastTable, head: *const *mut Node, key: u32, handle: usize) -> *mut core::ffi::c_void {
    let node = unsafe { node_of(head, key) };
    if node.is_null() {
        FAST_UNSET.store(FAST_UNSET.load(Relaxed).wrapping_add(1), Relaxed);
        return core::ptr::null_mut();
    }
    if !table.insert(key, node as usize, handle) { FAST_OVERFLOW.store(FAST_OVERFLOW.load(Relaxed).wrapping_add(1), Relaxed); }
    unsafe { (*node).value }
}

/// Compare what the table would answer with the real answer. One writer, the game thread.
fn shadow_check(table: &FastTable, sabotage: bool, key: u32, real: *mut core::ffi::c_void) {
    let mut node = table.find(key);
    if node == 0 { return; }
    if sabotage { node = table.other_than(node); }
    let remembered = unsafe { (*(node as *const Node)).value };
    FAST_CHECKS.store(FAST_CHECKS.load(Relaxed).wrapping_add(1), Relaxed);
    if remembered != real { FAST_MISMATCHES.store(FAST_MISMATCHES.load(Relaxed).wrapping_add(1), Relaxed); }
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
        let mode = MODE.load(Relaxed);
        if mode == 0 { return unsafe { __real_pthread_getspecific(key) }; }
        if mode & COUNT != 0 {
            // Read first: nothing before this line can call out, so lr still holds the return address.
            let pc: usize;
            unsafe { core::arch::asm!("mov {}, lr", out(reg) pc, options(nomem, nostack, preserves_flags)); }
            return unsafe { counted(key, pc) };
        }
        let sp: usize;
        unsafe { core::arch::asm!("mov {}, sp", out(reg) sp, options(nomem, nostack, preserves_flags)); }
        // Another thread's stack is never inside the game thread's, and a null handle reads as null.
        if sp.wrapping_sub(FAST_STACK_LOW.load(Relaxed)) >= FAST_STACK_SIZE.load(Relaxed) || key.is_null() {
            return unsafe { __real_pthread_getspecific(key) };
        }
        let os_key = unsafe { *(key as *const u32) };
        if mode != FAST { return unsafe { shadowed(key, os_key, mode) }; }
        let node = FAST_TABLE.find(os_key);
        if node != 0 { return unsafe { (*(node as *const Node)).value }; }
        unsafe { fast_fill(&FAST_TABLE, vitasdk_get_tls_data(0), os_key, key as usize) }
    }
    #[cold]
    #[inline(never)]
    unsafe fn shadowed(key: *const core::ffi::c_void, os_key: u32, mode: u8) -> *mut core::ffi::c_void {
        let real = unsafe { __real_pthread_getspecific(key) };
        if FAST_TABLE.find(os_key) == 0 { let _ = unsafe { fast_fill(&FAST_TABLE, vitasdk_get_tls_data(0), os_key, key as usize) }; }
        shadow_check(&FAST_TABLE, mode & SABOTAGE != 0, os_key, real);
        real
    }
    /// Per lookup, over every remembered key, in the order the table holds them. Runs after the
    /// measured window, so its cost lands in no measured frame.
    pub(super) fn bench() -> (usize, f64, f64) {
        const ROUNDS: u32 = 200;
        let handles: Vec<(usize, u32)> = FAST_TABLE.handles.iter().zip(&FAST_TABLE.slots)
            .filter_map(|(handle, slot)| { let handle = handle.load(Relaxed); (handle != 0).then(|| (handle, slot.key.load(Relaxed))) }).collect();
        let total = (handles.len() as u32 * ROUNDS).max(1) as f64;
        let start = std::time::Instant::now();
        for _ in 0..ROUNDS { for (handle, _) in &handles {
            std::hint::black_box(unsafe { __real_pthread_getspecific(*handle as *const core::ffi::c_void) });
        } }
        let real = start.elapsed().as_nanos() as f64 / total;
        let start = std::time::Instant::now();
        for _ in 0..ROUNDS { for (handle, _) in &handles {
            std::hint::black_box(unsafe { __wrap_pthread_getspecific(*handle as *const core::ffi::c_void) });
        } }
        (handles.len(), real, start.elapsed().as_nanos() as f64 / total)
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

/// Call once on the game thread, first thing. A stack the kernel reports wrongly refuses the lever,
/// which then costs one load and one branch per lookup and changes nothing.
pub(crate) fn start_fast(mode: FastMode) {
    if mode == FastMode::Off { return; }
    #[cfg(target_os = "vita")]
    {
        let probe = 0u8;
        let here = &probe as *const u8 as usize;
        let stack = crate::skin_worker::current_stack();
        let inside = stack.is_some_and(|(low, size)| here.wrapping_sub(low) < size);
        println!("[lg/tls-fast] start mode={} stack={stack:x?} inside={inside}", mode.name());
        let Some((low, size)) = stack.filter(|_| inside) else { return; };
        FAST_STACK_LOW.store(low, Relaxed);
        FAST_STACK_SIZE.store(size, Relaxed);
        FAST_STARTED.store(mode.bits(), Relaxed);
        MODE.fetch_or(mode.bits(), Relaxed);
    }
}
/// The line after the survey window, when the lever asked to run. Shows the keys the table holds,
/// what it could not hold, and on the console the cost per lookup with and without it.
pub(crate) fn fast_line(map: &str, mode: FastMode) -> Option<String> {
    if mode == FastMode::Off { return None; }
    #[cfg(target_os = "vita")]
    let (benched, real, fast) = if MODE.load(Relaxed) & FAST != 0 { vita::bench() } else { (0, 0., 0.) };
    #[cfg(not(target_os = "vita"))]
    let (benched, real, fast) = (0usize, 0f64, 0f64);
    Some(format!("[lg/tls-fast] map={map} mode={} running={} keys={} overflow={} unset={} checks={} mismatches={} benched={benched} real_ns={real:.0} fast_ns={fast:.0}",
        mode.name(), FAST_STARTED.load(Relaxed) != 0, FAST_TABLE.used.load(Relaxed), FAST_OVERFLOW.load(Relaxed),
        FAST_UNSET.load(Relaxed), FAST_CHECKS.load(Relaxed), FAST_MISMATCHES.load(Relaxed)))
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
    fn list(keys: &[u32], values: &mut [u8]) -> Vec<Node> {
        let mut nodes: Vec<Node> = keys.iter().enumerate().map(|(i, &key)| Node { next: core::ptr::null_mut(), key,
            value: &mut values[i] as *mut u8 as *mut core::ffi::c_void }).collect();
        for i in 0..keys.len().saturating_sub(1) { let next: *mut Node = &mut nodes[i + 1]; nodes[i].next = next; }
        nodes
    }
    #[test]
    fn fast_mode_names_and_bits() {
        for (name, mode) in [("off", FastMode::Off), ("on", FastMode::On), ("shadow", FastMode::Shadow), ("shadow_sabotage", FastMode::ShadowSabotage)] {
            assert_eq!(FastMode::parse(name), Some(mode));
            assert_eq!(mode.name(), name);
        }
        assert_eq!(FastMode::parse("true"), None);
        assert_eq!(FastMode::Off.bits(), 0);
        assert_eq!(FastMode::On.bits(), FAST, "the hot path compares against exactly this");
    }
    #[test]
    fn fast_table_answers_what_the_walk_answers_and_follows_the_value() {
        let _lock = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        // Keys 7, 263 and 519 share a home slot, and 0 is a legal key.
        let keys = [7u32, 263, 519, 0, 0xDEAD_BEEF];
        let mut values = [0u8; 8];
        let mut nodes = list(&keys, &mut values);
        let first: *mut Node = &mut nodes[0];
        let head: *const *mut Node = &first;
        let table = FastTable::new();
        for &key in &keys { assert_eq!(table.find(key), 0, "an empty table knows nothing"); }
        for (i, &key) in keys.iter().enumerate() {
            let value = unsafe { fast_fill(&table, head, key, 0x100 + i) };
            assert_eq!(value, unsafe { walk(head, key) }.0);
        }
        for (i, &key) in keys.iter().enumerate() {
            assert_eq!(table.find(key), &nodes[i] as *const Node as usize, "key {key}");
        }
        assert_eq!(table.find(775), 0, "same home slot, never inserted");
        // A write to a node reaches the next lookup: the table holds the node, not the value.
        let mut other = 0u8;
        nodes[2].value = &mut other as *mut u8 as *mut core::ffi::c_void;
        assert_eq!(unsafe { (*(table.find(519) as *const Node)).value }, nodes[2].value);
    }
    #[test]
    fn fast_fill_reads_null_for_a_key_this_thread_never_set_and_stays_out_of_the_table() {
        let _lock = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let mut values = [0u8; 2];
        let mut nodes = list(&[1, 2], &mut values);
        let first: *mut Node = &mut nodes[0];
        let head: *const *mut Node = &first;
        let table = FastTable::new();
        let before = FAST_UNSET.load(Relaxed);
        assert!(unsafe { fast_fill(&table, head, 3, 0x100) }.is_null());
        assert_eq!(FAST_UNSET.load(Relaxed), before.wrapping_add(1));
        assert_eq!(table.used.load(Relaxed), 0);
        let empty: *mut Node = core::ptr::null_mut();
        assert!(unsafe { fast_fill(&table, &empty, 1, 0x100) }.is_null());
    }
    #[test]
    fn a_full_fast_table_keeps_answering_and_never_spins() {
        let _lock = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let table = FastTable::new();
        for key in 0..FAST_LIMIT { assert!(table.insert(key, 0x1000 + key as usize * 16, 0x100)); }
        assert!(!table.insert(FAST_LIMIT, 0x9000, 0x100));
        assert_eq!(table.used.load(Relaxed), FAST_LIMIT);
        assert_eq!(table.find(FAST_LIMIT), 0, "an unknown key ends on an empty slot");
        assert_eq!(table.find(5), 0x1000 + 5 * 16);
        // A key that lost out still reads right through the walk.
        let mut value = 0u8;
        let mut nodes = list(&[FAST_LIMIT], core::slice::from_mut(&mut value));
        let first: *mut Node = &mut nodes[0];
        let before = FAST_OVERFLOW.load(Relaxed);
        assert_eq!(unsafe { fast_fill(&table, &first, FAST_LIMIT, 0x100) }, nodes[0].value);
        assert_eq!(FAST_OVERFLOW.load(Relaxed), before.wrapping_add(1));
    }
    #[test]
    fn the_shadow_check_counts_a_wrong_node_only_under_sabotage() {
        let _lock = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let mut values = [0u8; 2];
        let mut nodes = list(&[1, 2], &mut values);
        let first: *mut Node = &mut nodes[0];
        let head: *const *mut Node = &first;
        let table = FastTable::new();
        for key in [1, 2] { unsafe { fast_fill(&table, head, key, 0x100) }; }
        let (checks, mismatches) = (FAST_CHECKS.load(Relaxed), FAST_MISMATCHES.load(Relaxed));
        shadow_check(&table, false, 1, nodes[0].value);
        shadow_check(&table, false, 2, nodes[1].value);
        shadow_check(&table, false, 9, core::ptr::null_mut());
        assert_eq!(FAST_CHECKS.load(Relaxed), checks.wrapping_add(2), "an unknown key checks nothing");
        assert_eq!(FAST_MISMATCHES.load(Relaxed), mismatches);
        shadow_check(&table, true, 1, nodes[0].value);
        assert_eq!(FAST_MISMATCHES.load(Relaxed), mismatches.wrapping_add(1), "the wrong node shows up");
    }
    #[test]
    fn buckets_cover_every_length_once() {
        let edges = [(0, 0), (1, 0), (2, 1), (3, 2), (4, 2), (5, 3), (8, 3), (9, 4), (16, 4), (17, 5), (32, 5), (33, 6), (64, 6), (65, 7), (u32::MAX, 7)];
        for (steps, b) in edges { assert_eq!(bucket(steps), b, "{steps}"); }
    }
}
