//! Attempts, not successful effects. No clock/IO/allocation or retry loop.
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering::SeqCst};
static ENABLED: AtomicBool = AtomicBool::new(false);
static EPOCH: AtomicU32 = AtomicU32::new(0);
static WRAPPED: AtomicBool = AtomicBool::new(false);
// Charge the actual three statics, including the sticky overflow flag. This is
// payload accounting, not linker alignment or native process RAM.
pub const STATIC_BYTES: usize = std::mem::size_of_val(&ENABLED)
    + std::mem::size_of_val(&EPOCH)
    + std::mem::size_of_val(&WRAPPED);
const _: () = assert!(STATIC_BYTES == 6);
pub fn enable() {
    ENABLED.store(true, SeqCst);
}
pub fn attempt() {
    if ENABLED.load(SeqCst) && EPOCH.fetch_add(1, SeqCst) == u32::MAX {
        WRAPPED.store(true, SeqCst);
    }
}
pub fn snapshot() -> (u32, bool) {
    (EPOCH.load(SeqCst), WRAPPED.load(SeqCst))
}
