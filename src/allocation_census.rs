//! Default-off process-heap brackets and payload cross-checks. No allocator replacement.
//! Same-thread nested brackets partition NET process heap changes, not ownership.
//! Global totals include worker scopes. Cross-thread overlap can count drift more
//! than once; payload inventories disambiguate that limitation.
use std::{cell::RefCell, marker::PhantomData, rc::Rc,
    sync::atomic::{AtomicBool, AtomicI32, AtomicU32, AtomicUsize, Ordering::Relaxed}};
static ENABLED: AtomicBool = AtomicBool::new(false);
const N: usize = 10;
const NAMES: [&str;N] = ["scene_bsp", "world_render", "textures_materials", "collision",
    "entities_npc", "scripts", "audio", "audio_decode", "film_decode", "ui_fixed"];
#[derive(Clone, Copy, Default)]
struct State { children: [i64;32], depth: usize, generation: u64, seen: bool }
// Only Scope accesses TLS, outside allocator entry points. Results from every
// thread are published globally; no TLS is read by snapshot or the peak guard.
thread_local! {static STATE:RefCell<State>=RefCell::new(State::default());}
static NET: [AtomicI32;N] = [const { AtomicI32::new(0) };N];
static CALLS: [AtomicU32;N] = [const { AtomicU32::new(0) };N];
static BASELINE: AtomicUsize = AtomicUsize::new(0);
static OPEN: AtomicUsize = AtomicUsize::new(0);
static THREADS: AtomicUsize = AtomicUsize::new(0);
static ANOMALIES: AtomicUsize = AtomicUsize::new(0);
fn anomaly() { ANOMALIES.fetch_add(1,Relaxed); }
fn publish(category:usize,delta:i64) {
    if let (Some(net),Some(calls))=(NET.get(category),CALLS.get(category)) {
        // Saturate rather than wrap an unusually long/churn-heavy observation.
        let clipped=delta.clamp(i32::MIN as i64,i32::MAX as i64) as i32;
        let previous=net.fetch_update(Relaxed,Relaxed,|n|Some(n.saturating_add(clipped))).unwrap_or_else(|n|n);
        calls.fetch_add(1,Relaxed);
        if clipped as i64!=delta || previous.checked_add(clipped).is_none() {anomaly();}
    } else {anomaly();}
}
pub(crate) fn enabled()->bool { ENABLED.load(Relaxed) }
pub(crate) fn used()->usize {
    #[cfg(target_os="vita")] {
        #[repr(C)] struct MallInfo { arena:usize,ordblks:usize,smblks:usize,hblks:usize,hblkhd:usize,
            usmblks:usize,fsmblks:usize,uordblks:usize,fordblks:usize,keepcost:usize }
        extern "C" {fn _mallinfo_r(r:*mut core::ffi::c_void)->MallInfo;}
        // Linked _mallinfo_r ignores its reent argument. Avoid mallinfo's
        // __getreent lazy initialization, reclamation and diagnostic paths.
        unsafe { _mallinfo_r(core::ptr::null_mut()).uordblks }
    }
    #[cfg(not(target_os="vita"))] {crate::kernel_alloc::live_bytes()}
}
pub(crate) fn enable() { BASELINE.store(used(),Relaxed);
    #[cfg(target_os="vita")] peak::start();
    ENABLED.store(true,Relaxed); }
pub(crate) struct Scope { start:Option<usize>, category:usize, depth:usize, generation:u64,
    // A bracket must close on its opening thread, including across awaits.
    _thread:PhantomData<Rc<()>> }
impl Scope {
    pub(crate) fn new(category:usize)->Self {
        let mut result=Self{start:None,category,depth:0,generation:0,_thread:PhantomData};
        if !enabled() {return result;}
        if category>=N {anomaly();return result;}
        // Initialize bracket TLS before sampling; allocator observers never use it.
        let position=STATE.try_with(|s|{
            let Ok(mut s)=s.try_borrow_mut() else {anomaly();return None;};
            let depth=s.depth;
            let Some(child)=s.children.get_mut(depth) else {anomaly();return None;};
            *child=0;s.depth+=1;
            if !s.seen {s.seen=true;THREADS.fetch_add(1,Relaxed);}
            OPEN.fetch_add(1,Relaxed);Some((depth,s.generation))
        }).ok().flatten();
        if let Some((depth,generation))=position {
            result.start=Some(used());result.depth=depth;result.generation=generation;
        }
        result
    }
}
impl Drop for Scope {fn drop(&mut self) {if let Some(start)=self.start {
    let delta=used() as i64-start as i64;
    let _=STATE.try_with(|s|{
        let Ok(mut s)=s.try_borrow_mut() else {anomaly();return;};
        if s.generation!=self.generation {anomaly();return;}
        if s.depth!=self.depth+1 {
            // Invalidate all outstanding brackets in this thread. Stale drops
            // cannot damage later scopes or fabricate a partition.
            anomaly();OPEN.fetch_sub(s.depth,Relaxed);s.depth=0;
            s.generation=s.generation.wrapping_add(1);return;
        }
        let Some(child)=s.children.get(self.depth).copied() else {anomaly();return;};
        s.depth-=1;OPEN.fetch_sub(1,Relaxed);publish(self.category,delta-child);
        if let Some(parent)=self.depth.checked_sub(1).and_then(|d|s.children.get_mut(d)) {
            *parent+=delta;
        }
    });
}}}
pub(crate) fn snapshot(label:&str) {
    if !enabled(){return;}
    let used=used();let baseline=BASELINE.load(Relaxed);
    let net=NET.each_ref().map(|n|n.load(Relaxed) as i64);
    let calls=CALLS.each_ref().map(|n|n.load(Relaxed));
    let attributed:i64=net.iter().sum();
    println!("[lg/r13b-memory] {}",serde_json::json!({"event":"partition","label":label,
        "heap_used":used,"baseline_bytes":baseline,"categories":NAMES,"exclusive_net_bytes":net,
        "calls":calls,"open_bracket_depth":OPEN.load(Relaxed),"bracket_threads":THREADS.load(Relaxed),
        "aggregation":"all_threads",
        "scope_anomalies":ANOMALIES.load(Relaxed),"residual_bytes":used as i64-baseline as i64-attributed,
        "peak_contention_fallbacks":peak_fallbacks(),
        "scope":"all-thread aggregate of signed process-heap brackets; concurrent overlapping brackets may double-count drift; residual is signed closure, not unowned live bytes; not live object ownership; nonzero anomalies invalidate attribution",
        "peak_bounds":peak_bounds(),"heap_hard_upper_bound_bytes":192*1024*1024,"transient_peak":"transition allocator/phase samples are LOWER bounds; 192 MiB fixed newlib heap is the unconditional upper bound"}));
}
pub(crate) fn payload(label:&str,fields:serde_json::Value) {
    if enabled(){println!("[lg/r13b-memory] {}",serde_json::json!({"event":"payload","label":label,"fields":fields}));}
}
#[cfg(test)] mod tests {
    use super::*;
    #[test] fn all_threads_publish_and_malformed_scopes_never_panic() {
        assert!(!enabled());assert!(Scope::new(0).start.is_none());enable();
        let before=CALLS[6].load(Relaxed);
        std::thread::spawn(||{let _worker=Scope::new(6);}).join().unwrap();
        assert_eq!(CALLS[6].load(Relaxed),before+1);
        let outer=Scope::new(0);let inner=Scope::new(1);drop(outer);drop(inner);
        assert_eq!(OPEN.load(Relaxed),0);assert!(ANOMALIES.load(Relaxed)>=2);
        let before=CALLS[2].load(Relaxed);drop(Scope::new(2));
        assert_eq!(CALLS[2].load(Relaxed),before+1);
        let mut scopes=Vec::new();for _ in 0..40 {scopes.push(Scope::new(3));}
        while let Some(scope)=scopes.pop(){drop(scope);}
        assert_eq!(OPEN.load(Relaxed),0);drop(Scope::new(N));
        assert!(THREADS.load(Relaxed)>=2);ENABLED.store(false,Relaxed);
    }
}

fn peak_bounds()->[usize;2] {
    #[cfg(target_os="vita")] {peak::bounds()}
    #[cfg(not(target_os="vita"))] {[0,0]}
}
fn peak_fallbacks()->usize {
    #[cfg(target_os="vita")] {peak::fallbacks()}
    #[cfg(not(target_os="vita"))] {0}
}
#[cfg(any(target_os="vita",test))]
mod owner_lock {
    use std::sync::atomic::{AtomicI32,Ordering::{Acquire,Relaxed,Release}};
    pub struct Owner(AtomicI32);
    impl Owner {
        pub const fn new()->Self {Self(AtomicI32::new(0))}
        // Some(true): outer owner; Some(false): nested owner; None: contention.
        pub fn enter(&self,id:i32)->Option<bool> {
            match self.0.compare_exchange(0,id,Acquire,Relaxed) {
                Ok(_)=>Some(true),Err(owner) if owner==id=>Some(false),Err(_)=>None,
            }
        }
        pub fn release(&self) {self.0.store(0,Release);}
    }
    #[test] fn owner_reentry_and_contention() {
        let lock=Owner::new();assert_eq!(lock.enter(1),Some(true));
        assert_eq!(lock.enter(1),Some(false));assert_eq!(lock.enter(2),None);
        std::thread::scope(|s|{s.spawn(||assert_eq!(lock.enter(2),None)).join().unwrap();});
        lock.release();assert_eq!(lock.enter(2),Some(true));lock.release();
    }
}
/// Opt-in aggregate upper envelope, not an allocation trace. Serialize allocator
/// entry points only during uncontended parts of this diagnostic. Contention
/// never waits behind libc locks: widen to the fixed heap ceiling instead.
/// Between mallinfo samples, retain every requested
/// chunk, without credit for frees. Extra 64 B covers newlib splitting/rounding;
/// memalign additionally reserves its full alignment. Realloc retains old+new.
#[cfg(target_os="vita")]
pub(crate) mod peak {
    use super::*;
    use std::sync::atomic::AtomicUsize;
    // Zero is not a valid Vita thread UID. Ownership and lock acquisition are
    // one atomic operation, so even same-thread asynchronous re-entry is safe.
    static OWNER:super::owner_lock::Owner=super::owner_lock::Owner::new();
    static BASE:AtomicUsize=AtomicUsize::new(0);
    static GROWTH:AtomicUsize=AtomicUsize::new(0);
    static CALLS:AtomicUsize=AtomicUsize::new(0);
    static LOWER:AtomicUsize=AtomicUsize::new(0);
    static UPPER:AtomicUsize=AtomicUsize::new(0);
    static FALLBACKS:AtomicUsize=AtomicUsize::new(0);
    pub fn start(){let u=used();BASE.store(u,Relaxed);LOWER.store(u,Relaxed);UPPER.store(u,Relaxed);}
    pub fn bounds()->[usize;2] {[LOWER.load(Relaxed),UPPER.load(Relaxed).min(192*1024*1024)]}
    pub fn fallbacks()->usize {FALLBACKS.load(Relaxed)}
    pub struct Guard {active:bool, sample:bool, nested:bool}
    impl Guard {
        // Keep the complete observer visible at each C allocator boundary,
        // including calloc's saturated-overflow branch. An outlined Rust call
        // can retain an unwind landing pad even when its body cannot panic.
        // The strict linked-ELF closure gate must still qualify every callee.
        #[inline(always)]
        pub fn enter(request:usize,alignment:usize,allocation:bool)->Self {
            if !enabled(){return Self{active:false,sample:false,nested:false};}
            extern "C" {fn sceKernelGetThreadId()->i32;}
            let id=unsafe{sceKernelGetThreadId()};
            let ownership=if id>0 {OWNER.enter(id)} else {None};
            match ownership {
                Some(true)=>{},
                Some(false)=>return Self{active:false,sample:false,nested:true},
                // Never introduce lock-order inversion with newlib reent or
                // allocator locks. This bound stays latched for the run.
                None=>{FALLBACKS.fetch_add(1,Relaxed);UPPER.store(192*1024*1024,Relaxed);
                    return Self{active:false,sample:false,nested:false};}
            }
            let sample=allocation && (request>=65536 || CALLS.fetch_add(1,Relaxed)%128==0);
            if allocation {
                let n=request.saturating_add(alignment).saturating_add(64);
                let previous=GROWTH.fetch_update(Relaxed,Relaxed,|v|Some(v.saturating_add(n))).unwrap_or_else(|v|v);
                let growth=previous.saturating_add(n);
                UPPER.fetch_max(BASE.load(Relaxed).saturating_add(growth),Relaxed);
            }
            Self{active:true,sample,nested:false}
        }
    }
    impl Drop for Guard {fn drop(&mut self){
        if self.nested {return;}
        if self.active {
        if self.sample {let u=used();LOWER.fetch_max(u,Relaxed);UPPER.fetch_max(u,Relaxed);BASE.store(u,Relaxed);GROWTH.store(0,Relaxed);}
        OWNER.release();
    }}}
}
