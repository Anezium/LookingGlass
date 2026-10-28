//! Default-off process-heap brackets and payload cross-checks. No allocator replacement.
//! Nested brackets partition NET process heap changes, not ownership. Audio workers
//! can allocate/free concurrently; payload inventories disambiguate that limitation.
use std::{cell::RefCell, sync::atomic::{AtomicBool, Ordering::Relaxed}};
static ENABLED: AtomicBool = AtomicBool::new(false);
const N: usize = 10;
const NAMES: [&str;N] = ["scene_bsp", "world_render", "textures_materials", "collision",
    "entities_npc", "scripts", "audio", "audio_decode", "film_decode", "ui_fixed"];
#[derive(Clone, Copy, Default)]
struct State { net: [i64;N], calls: [u64;N], children: [i64;32], depth: usize, baseline: usize }
thread_local! {static STATE:RefCell<State>=RefCell::new(State::default());}
pub(crate) fn enabled()->bool { ENABLED.load(Relaxed) }
pub(crate) fn used()->usize {
    #[cfg(target_os="vita")] {
        #[repr(C)] struct MallInfo { fields:[usize;10] }
        extern "C" {fn mallinfo()->MallInfo;}
        unsafe { mallinfo().fields[7] }
    }
    #[cfg(not(target_os="vita"))] {crate::kernel_alloc::live_bytes()}
}
pub(crate) fn enable() { STATE.with(|s|s.borrow_mut().baseline=used());ENABLED.store(true,Relaxed);
    #[cfg(target_os="vita")] peak::start(); }
pub(crate) struct Scope { start:Option<usize>, category:usize, depth:usize }
impl Scope {
    pub(crate) fn new(category:usize)->Self {
        if !enabled() {return Self{start:None,category,depth:0};}
        let start=used();let depth=STATE.with(|s|{let mut s=s.borrow_mut();let d=s.depth;
            assert!(d<32);s.children[d]=0;s.depth+=1;d});
        Self{start:Some(start),category,depth}
    }
}
impl Drop for Scope {fn drop(&mut self) {if let Some(start)=self.start {
    let delta=used() as i64-start as i64;
    STATE.with(|s|{let mut s=s.borrow_mut();assert_eq!(s.depth,self.depth+1);s.depth-=1;
        let exclusive=delta-s.children[self.depth];s.net[self.category]+=exclusive;s.calls[self.category]+=1;
        if self.depth>0 {s.children[self.depth-1]+=delta;}});
}}}
pub(crate) fn snapshot(label:&str) {
    if !enabled(){return;}
    let used=used();let s=STATE.with(|s|*s.borrow());
    let attributed:i64=s.net.iter().sum();
    println!("[lg/r13-memory] {}",serde_json::json!({"event":"partition","label":label,
        "heap_used":used,"baseline_bytes":s.baseline,"categories":NAMES,"exclusive_net_bytes":s.net,
        "calls":s.calls,"residual_bytes":used as i64-s.baseline as i64-attributed,
        "scope":"signed net process-heap bracket deltas; concurrent worker drift included; not live object ownership",
        "peak_bounds":peak_bounds(),"heap_hard_upper_bound_bytes":192*1024*1024,"transient_peak":"transition allocator/phase samples are LOWER bounds; 192 MiB fixed newlib heap is the unconditional upper bound"}));
}
pub(crate) fn payload(label:&str,fields:serde_json::Value) {
    if enabled(){println!("[lg/r13-memory] {}",serde_json::json!({"event":"payload","label":label,"fields":fields}));}
}
#[cfg(test)] mod tests {
    use super::*;
    #[test] fn disabled_scopes_do_not_initialize_or_sample() {assert!(!enabled());let s=Scope::new(0);assert!(s.start.is_none());}
}

fn peak_bounds()->[usize;2] {
    #[cfg(target_os="vita")] {peak::bounds()}
    #[cfg(not(target_os="vita"))] {[0,0]}
}
/// Opt-in aggregate upper envelope, not an allocation trace. Serialize allocator
/// entry points only during this diagnostic so the sampled baseline cannot miss
/// another monitored operation. Between mallinfo samples, retain every requested
/// chunk, without credit for frees. Extra 64 B covers newlib splitting/rounding;
/// memalign additionally reserves its full alignment. Realloc retains old+new.
#[cfg(target_os="vita")]
pub(crate) mod peak {
    use super::*;
    use std::sync::atomic::AtomicUsize;
    static LOCK:AtomicBool=AtomicBool::new(false);
    static BASE:AtomicUsize=AtomicUsize::new(0);
    static GROWTH:AtomicUsize=AtomicUsize::new(0);
    static CALLS:AtomicUsize=AtomicUsize::new(0);
    static LOWER:AtomicUsize=AtomicUsize::new(0);
    static UPPER:AtomicUsize=AtomicUsize::new(0);
    pub fn start(){let u=used();BASE.store(u,Relaxed);LOWER.store(u,Relaxed);UPPER.store(u,Relaxed);}
    pub fn bounds()->[usize;2] {[LOWER.load(Relaxed),UPPER.load(Relaxed).min(192*1024*1024)]}
    pub struct Guard {active:bool, sample:bool}
    impl Guard {
        pub fn enter(request:usize,alignment:usize,allocation:bool)->Self {
            if !enabled(){return Self{active:false,sample:false};}
            while LOCK.compare_exchange(false,true,std::sync::atomic::Ordering::Acquire,Relaxed).is_err(){std::hint::spin_loop();}
            let sample=allocation && (request>=65536 || CALLS.fetch_add(1,Relaxed)%128==0);
            if allocation {
                let n=request.saturating_add(alignment).saturating_add(64);
                let growth=GROWTH.fetch_add(n,Relaxed).saturating_add(n);
                UPPER.fetch_max(BASE.load(Relaxed).saturating_add(growth),Relaxed);
            }
            Self{active:true,sample}
        }
    }
    impl Drop for Guard {fn drop(&mut self){if self.active {
        if self.sample {let u=used();LOWER.fetch_max(u,Relaxed);UPPER.fetch_max(u,Relaxed);BASE.store(u,Relaxed);GROWTH.store(0,Relaxed);}
        LOCK.store(false,std::sync::atomic::Ordering::Release);
    }}}
}
