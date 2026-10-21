//! Attempts, not successful effects. No clock/IO/allocation or retry loop.
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering::SeqCst};
static ENABLED: AtomicBool = AtomicBool::new(false);
static EPOCH: AtomicU32 = AtomicU32::new(0);
static WRAPPED: AtomicBool = AtomicBool::new(false);
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
