//! Count allocation entry calls made by the measured kernel thread, excluding dispatch.
#![cfg_attr(not(target_os = "vita"), allow(dead_code))]
#[cfg(not(target_os = "vita"))]
mod imp {
    use std::{
        alloc::{GlobalAlloc, Layout, System},
        cell::Cell,
    };
    thread_local! {static ACTIVE:Cell<bool>=const {Cell::new(false)}; static CALLS:Cell<u64>=const {Cell::new(0)};}
    fn count() {
        let _ = ACTIVE.try_with(|a| {
            if a.get() {
                let _ = CALLS.try_with(|n| n.set(n.get() + 1));
            }
        });
    }
    struct Allocator;
    #[global_allocator]
    static ALLOCATOR: Allocator = Allocator;
    unsafe impl GlobalAlloc for Allocator {
        unsafe fn alloc(&self, l: Layout) -> *mut u8 {
            count();
            System.alloc(l)
        }
        unsafe fn alloc_zeroed(&self, l: Layout) -> *mut u8 {
            count();
            System.alloc_zeroed(l)
        }
        unsafe fn realloc(&self, p: *mut u8, l: Layout, n: usize) -> *mut u8 {
            count();
            System.realloc(p, l, n)
        }
        unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
            System.dealloc(p, l)
        }
    }
    pub fn begin() {
        CALLS.with(|n| n.set(0));
        ACTIVE.with(|a| assert!(!a.replace(true)));
    }
    pub fn finish() -> u64 {
        ACTIVE.with(|a| a.set(false));
        CALLS.with(Cell::get)
    }
}
#[cfg(target_os = "vita")]
mod imp {
    use std::sync::atomic::{AtomicI32, AtomicU32, Ordering::Relaxed};
    static THREAD: AtomicI32 = AtomicI32::new(-1);
    static CALLS: AtomicU32 = AtomicU32::new(0);
    extern "C" {
        fn sceKernelGetThreadId() -> i32;
        fn __real_malloc(size: usize) -> *mut core::ffi::c_void;
        fn __real_calloc(n: usize, size: usize) -> *mut core::ffi::c_void;
        fn __real_realloc(p: *mut core::ffi::c_void, size: usize) -> *mut core::ffi::c_void;
    }
    fn count() {
        let id = THREAD.load(Relaxed);
        if id >= 0 && unsafe { sceKernelGetThreadId() } == id {
            CALLS.fetch_add(1, Relaxed);
        }
    }
    #[no_mangle]
    pub unsafe extern "C" fn __wrap_malloc(size: usize) -> *mut core::ffi::c_void {
        count();
        __real_malloc(size)
    }
    #[no_mangle]
    pub unsafe extern "C" fn __wrap_calloc(n: usize, size: usize) -> *mut core::ffi::c_void {
        count();
        __real_calloc(n, size)
    }
    #[no_mangle]
    pub unsafe extern "C" fn __wrap_realloc(
        p: *mut core::ffi::c_void,
        size: usize,
    ) -> *mut core::ffi::c_void {
        count();
        __real_realloc(p, size)
    }
    pub fn begin() {
        CALLS.store(0, Relaxed);
        assert_eq!(THREAD.swap(unsafe { sceKernelGetThreadId() }, Relaxed), -1);
    }
    pub fn finish() -> u64 {
        THREAD.store(-1, Relaxed);
        CALLS.load(Relaxed) as u64
    }
}
pub(crate) struct Scope;
impl Scope {
    pub(crate) fn begin() -> Self {
        imp::begin();
        Self
    }
    pub(crate) fn finish(self) -> u64 {
        let n = imp::finish();
        std::mem::forget(self);
        n
    }
}
impl Drop for Scope {
    fn drop(&mut self) {
        imp::finish();
    }
}
#[cfg(test)]
mod tests {
    #[test]
    fn counts_only_allocations_in_the_current_kernel_scope() {
        let s = super::Scope::begin();
        let x = std::hint::black_box(vec![0u8; 33]);
        assert!(s.finish() >= 1);
        drop(x);
        assert_eq!(super::Scope::begin().finish(), 0);
    }
}
