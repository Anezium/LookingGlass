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
    // Independent host load ledger. Sizes are allocator requests, plus the
    // r3 disassembly-derived 32-bit newlib minimum chunk model. Realloc peaks
    // conservatively retain old+new until the successful operation returns.
    thread_local! {static HEAP:Cell<Option<[u64;5]>>=const{Cell::new(None)};}
    fn heap_alloc(size:usize) {let _=HEAP.try_with(|h|if let Some(mut v)=h.get(){
        let n=size as u64;let chunk=((n+11)&!7).max(16);
        v[0]+=n;v[1]=v[1].max(v[0]);v[2]+=chunk;v[3]=v[3].max(v[2]);v[4]+=1;h.set(Some(v));
    });}
    fn heap_free(size:usize) {let _=HEAP.try_with(|h|if let Some(mut v)=h.get(){
        let n=size as u64;v[0]=v[0].checked_sub(n).expect("load ledger freed pre-scope allocation");
        v[2]-=((n+11)&!7).max(16);h.set(Some(v));
    });}
    pub fn heap_begin(){HEAP.with(|h|assert!(h.replace(Some([0;5])).is_none()));}
    pub fn heap_finish()->[u64;5]{HEAP.with(|h|h.replace(None).unwrap())}
    use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};
    static LIVE: AtomicUsize = AtomicUsize::new(0);
    static CHUNKS: AtomicUsize = AtomicUsize::new(0);
    static HIGH: AtomicUsize = AtomicUsize::new(0);
    static CHUNK_HIGH: AtomicUsize = AtomicUsize::new(0);
    fn chunk(n: usize) -> usize { ((n + 11) & !7).max(16) }
    fn live_alloc(n: usize) {
        let live = LIVE.fetch_add(n, Relaxed) + n;
        let chunks = CHUNKS.fetch_add(chunk(n), Relaxed) + chunk(n);
        HIGH.fetch_max(live, Relaxed); CHUNK_HIGH.fetch_max(chunks, Relaxed);
        crate::transition_memory::allocation(n);
    }
    fn live_free(n: usize) { LIVE.fetch_sub(n, Relaxed); CHUNKS.fetch_sub(chunk(n), Relaxed); }
    pub fn live_bytes() -> usize { LIVE.load(Relaxed) }
    pub fn transition_begin() { HIGH.store(live_bytes(), Relaxed); CHUNK_HIGH.store(CHUNKS.load(Relaxed), Relaxed); }
    pub fn transition_finish() -> [usize;4] { [LIVE.load(Relaxed), HIGH.load(Relaxed), CHUNKS.load(Relaxed), CHUNK_HIGH.load(Relaxed)] }
    struct Allocator;
    #[global_allocator]
    static ALLOCATOR: Allocator = Allocator;
    unsafe impl GlobalAlloc for Allocator {
        unsafe fn alloc(&self, l: Layout) -> *mut u8 {
            count();
            let p=System.alloc(l);if !p.is_null(){heap_alloc(l.size());live_alloc(l.size());}p
        }
        unsafe fn alloc_zeroed(&self, l: Layout) -> *mut u8 {
            count();
            let p=System.alloc_zeroed(l);if !p.is_null(){heap_alloc(l.size());live_alloc(l.size());}p
        }
        unsafe fn realloc(&self, p: *mut u8, l: Layout, n: usize) -> *mut u8 {
            count();
            let out=System.realloc(p,l,n);if !out.is_null(){heap_alloc(n);live_alloc(n);heap_free(l.size());live_free(l.size());}out
        }
        unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
            heap_free(l.size());live_free(l.size());System.dealloc(p, l)
        }
    }
    pub fn begin() {
        CALLS.with(|n| n.set(0));
        ACTIVE.with(|a| assert!(!a.replace(true)));
    }
    pub fn calls() -> u64 { CALLS.with(Cell::get) }
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
        let _r13 = crate::allocation_census::peak::Guard::enter(size,0,true);
        let out = __real_malloc(size);
        crate::transition_memory::allocation(size);
        out
    }
    #[no_mangle]
    pub unsafe extern "C" fn __wrap_calloc(n: usize, size: usize) -> *mut core::ffi::c_void {
        count();
        let _r13 = crate::allocation_census::peak::Guard::enter(n.saturating_mul(size),0,true);
        let out = __real_calloc(n, size);
        crate::transition_memory::allocation(n.saturating_mul(size));
        out
    }
    #[no_mangle]
    pub unsafe extern "C" fn __wrap_realloc(
        p: *mut core::ffi::c_void,
        size: usize,
    ) -> *mut core::ffi::c_void {
        count();
        let _r13 = crate::allocation_census::peak::Guard::enter(size,0,true);
        let out = __real_realloc(p, size);
        crate::transition_memory::allocation(size);
        out
    }
    extern "C" {
        fn __real_free(p:*mut core::ffi::c_void);
        fn __real_memalign(alignment:usize,size:usize)->*mut core::ffi::c_void;
    }
    #[no_mangle]
    pub unsafe extern "C" fn __wrap_free(p:*mut core::ffi::c_void) {
        let _r13=crate::allocation_census::peak::Guard::enter(0,0,false);
        __real_free(p);
    }
    #[no_mangle]
    pub unsafe extern "C" fn __wrap_memalign(alignment:usize,size:usize)->*mut core::ffi::c_void {
        let _r13=crate::allocation_census::peak::Guard::enter(size,alignment,true);
        __real_memalign(alignment,size)
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
#[cfg(not(target_os="vita"))]
pub(crate) fn heap_begin(){imp::heap_begin();}
#[cfg(not(target_os="vita"))]
pub(crate) fn heap_finish()->[u64;5]{imp::heap_finish()}
pub(crate) struct Scope;
impl Scope {
    #[cfg(not(target_os="vita"))]
    pub(crate) fn calls_so_far() -> u64 {imp::calls()}
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

#[cfg(not(target_os="vita"))]
pub(crate) use imp::{live_bytes,transition_begin,transition_finish};
