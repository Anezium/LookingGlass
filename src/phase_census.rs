//! Opt-in, main-thread elapsed census. No hot-path allocation, I/O, GL calls or locks.
//! Inclusive brackets and their exclusive portions are accumulated separately;
//! root union coverage, not a sum of inclusive children, defines the residual.
#![allow(dead_code)]
use std::{cell::{Cell, RefCell}, io::Write, time::Instant};
const CAPACITY: usize = 180;
const TABLE: usize = 4096;
const PROBES: usize = 16;
const DEPTH: usize = 32;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(usize)]
pub(crate) enum Region { Npc, PlayerMovement, FixedPhysics, Movers, Objects, AudioPrepare,
    Acoustics, Lights, WorldOpaque, WorldVisibility, WorldParameters, WorldSubmit,
    AlphaPrepare, AlphaSubmit, ActorGather, ActorCollect, ActorPublication, ActorSubmit,
    Fx, JointFinish, FxSubmit, Hud, Present, Swap, Other }
const REGIONS: usize = Region::Other as usize;
const NAMES: [&str; REGIONS] = ["npc_think_movement", "player_movement", "fixed_physics", "movers", "objects",
    "audio_prepare", "prepare_acoustics", "light_selection", "world_opaque", "world_visibility",
    "world_parameters", "world_submit", "alpha_prepare", "alpha_submit", "actor_gather", "actor_collect",
    "actor_publication", "actor_submit", "fx_gather", "joint_finish", "fx_submit", "hud", "present", "swap"];
#[derive(Clone, Copy)]
#[repr(usize)]
pub(crate) enum Owner { Audio, Player, Npc, Camera, Other }
const OWNERS: [&str; 5] = ["audio", "player", "npc", "camera", "other"];
#[derive(Clone, Copy)]
struct Row {
    sequence: u64, measured: bool, total: u64, coverage: u64,
    inclusive: [u64; REGIONS], exclusive: [u64; REGIONS], calls: [u32; REGIONS], parents: [u32; REGIONS],
    sweeps: [u32; 5], sweep_ns: [u64; 5], acoustic_sweeps: u32, acoustic_sweep_ns: u64,
    queries: [[u32; 4]; 3], evictions: u32, errors: u32,
}
impl Row {
    const ZERO: Self = Self { sequence: 0, measured: false, total: 0, coverage: 0,
        inclusive: [0;REGIONS], exclusive: [0;REGIONS], calls: [0;REGIONS], parents: [0;REGIONS],
        sweeps:[0;5], sweep_ns:[0;5], acoustic_sweeps:0, acoustic_sweep_ns:0,
        queries:[[0;4];3], evictions:0, errors:0 };
}
#[derive(Clone, Copy)]
struct Entry { scene: usize, bits: [u32;6], last: u64, prior: u64 }
impl Entry { const EMPTY: Self = Self {scene:0,bits:[0;6],last:0,prior:0}; }
struct State {
    rows: [Row;CAPACITY], current: Row, count: usize, accepted: usize,
    start: Option<Instant>, pending: bool, overflow: bool, sequence: u64,
    owner: Owner, sweep_depth: usize, scene: usize,
    stack: [usize;DEPTH], children: [u64;DEPTH], depth: usize,
    table: [Entry;TABLE],
}
impl State {
    const fn new() -> Self { Self { rows:[Row::ZERO;CAPACITY],current:Row::ZERO,count:0,accepted:0,
        start:None,pending:false,overflow:false,sequence:0,owner:Owner::Other,sweep_depth:0,scene:0,
        stack:[0;DEPTH],children:[0;DEPTH],depth:0,table:[Entry::EMPTY;TABLE] } }
    fn close(&mut self, now: Instant) {
        if let Some(start) = self.start {
            if self.pending {
                self.current.total=now.duration_since(start).as_nanos() as u64;
                if self.depth != 0 || self.sweep_depth != 0 { self.current.errors += 1; }
                if self.count < CAPACITY {self.rows[self.count]=self.current;self.count+=1;} else {self.overflow=true;}
            }
        }
        self.pending=false;
    }
    fn query(&mut self, class:usize, bits:[u32;6]) {
        let mut hash=(self.scene as u64).wrapping_mul(0x9e3779b97f4a7c15);
        for b in bits {hash=(hash ^ b as u64).wrapping_mul(0x100000001b3);}
        let first=(hash as usize) & (TABLE-1);
        let mut slot=first;
        let mut oldest=u64::MAX;
        let mut found=false;
        for probe in 0..PROBES {
            let i=(first+probe)&(TABLE-1);let e=self.table[i];
            if e.last != 0 && e.scene==self.scene && e.bits==bits {slot=i;found=true;break;}
            if e.last<oldest {oldest=e.last;slot=i;}
        }
        let e=&mut self.table[slot];
        let previous=if found {if e.last==self.sequence {e.prior} else {e.last}} else {0};
        let q=&mut self.current.queries[class];q[0]+=1;
        if previous>0 {let age=self.sequence-previous;for (i,k) in [1,4,16].into_iter().enumerate() {q[i+1]+=u32::from(age<=k);}}
        if found {if e.last != self.sequence {e.prior=e.last;e.last=self.sequence;}}
        else {
            // A live eviction loses history and can only create false misses.
            if e.last>0 && self.sequence-e.last<=16 {self.current.evictions+=1;}
            *e=Entry{scene:self.scene,bits,last:self.sequence,prior:0};
        }
    }
}
thread_local! {
    static ACTIVE: Cell<bool> = const { Cell::new(false) };
    static STATE: RefCell<State> = const { RefCell::new(State::new()) };
}
fn active() -> bool {ACTIVE.with(Cell::get)}
fn configured() -> bool {
    #[cfg(feature="vita_renderer")]
    {crate::render::vita_renderer::config().phase_census}
    #[cfg(not(feature="vita_renderer"))]
    {false}
}
pub(crate) fn initialize() {if configured() {STATE.with(|_|{});}}
pub(crate) fn set_owner(owner:Owner) {if active(){STATE.with(|s|s.borrow_mut().owner=owner);}}
pub(crate) fn reset() {
    ACTIVE.with(|a|a.set(false));
    // Do not touch the large fixed table in the default-off arm.
    if configured() { STATE.with(|s| {let mut s=s.borrow_mut();s.count=0;s.accepted=0;s.start=None;
        s.pending=false;s.overflow=false;s.sequence=0;s.depth=0;s.sweep_depth=0;s.owner=Owner::Other;
        s.table.fill(Entry::EMPTY);}); }
}
pub(crate) fn begin_configured() { begin(configured()); }
fn begin(enabled:bool) {
    ACTIVE.with(|a|a.set(enabled));if !enabled {return;}
    let now=Instant::now();
    STATE.with(|s| {let mut s=s.borrow_mut();s.close(now);s.sequence+=1;
        s.current=Row{sequence:s.sequence,..Row::ZERO};s.start=Some(now);s.owner=Owner::Other;
        s.depth=0;s.sweep_depth=0;});
}
pub(crate) fn accept(accepted:bool,measured:bool) {
    if !active() {return;}
    STATE.with(|s|{let mut s=s.borrow_mut();s.pending=accepted;s.current.measured=measured;
        if accepted {s.accepted+=1;}});
}
pub(crate) struct Timer {start:Option<Instant>, region:usize, level:usize}
pub(crate) fn span(region:Region) -> Timer {
    let mut timer=Timer{start:None,region:region as usize,level:0};
    if region==Region::Other || !active() {return timer;}
    STATE.with(|s| {let mut s=s.borrow_mut();
        if s.depth==DEPTH {s.current.errors+=1;return;}
        timer.level=s.depth;let d=s.depth;s.stack[d]=region as usize;s.children[d]=0;s.depth+=1;
        let parent=if d==0 {REGIONS} else {s.stack[d-1]};s.current.parents[region as usize]|=1<<parent;
        timer.start=Some(Instant::now());
    });timer
}
impl Drop for Timer {
    fn drop(&mut self) {if let Some(start)=self.start {
        let elapsed=start.elapsed().as_nanos() as u64;
        STATE.with(|s| {let mut s=s.borrow_mut();let d=self.level;
            if s.depth != d+1 || s.stack[d]!=self.region {s.current.errors+=1;return;}
            s.depth=d;let child=s.children[d];
            if child>elapsed {s.current.errors+=1;}
            s.current.inclusive[self.region]+=elapsed;s.current.exclusive[self.region]+=elapsed.saturating_sub(child);
            s.current.calls[self.region]+=1;
            if d==0 {s.current.coverage+=elapsed;} else {s.children[d-1]+=elapsed;}
        });
    }}
}
pub(crate) struct OwnerScope(Option<Owner>);
pub(crate) fn owner(owner:Owner) -> OwnerScope {
    OwnerScope(if active() {Some(STATE.with(|s| {let mut s=s.borrow_mut();std::mem::replace(&mut s.owner,owner)}))} else {None})
}
impl Drop for OwnerScope {fn drop(&mut self){if let Some(owner)=self.0 {STATE.with(|s|s.borrow_mut().owner=owner);}}}
pub(crate) struct Sweep {start:Option<Instant>, entered:bool, owner:usize, acoustic:bool}
pub(crate) fn sweep() -> Sweep {
    let mut result=Sweep{start:None,entered:false,owner:0,acoustic:false};if !active(){return result;}
    STATE.with(|s| {let mut s=s.borrow_mut();result.entered=true;
        if s.sweep_depth==0 {result.owner=s.owner as usize;
            result.acoustic=s.stack[..s.depth].contains(&(Region::Acoustics as usize));result.start=Some(Instant::now());}
        s.sweep_depth+=1;
    });result
}
impl Drop for Sweep {fn drop(&mut self){if !self.entered{return;}let ns=self.start.map(|t|t.elapsed().as_nanos() as u64);
    STATE.with(|s|{let mut s=s.borrow_mut();s.sweep_depth-=1;if let Some(ns)=ns {
        s.current.sweeps[self.owner]+=1;s.current.sweep_ns[self.owner]+=ns;
        if self.acoustic {s.current.acoustic_sweeps+=1;s.current.acoustic_sweep_ns+=ns;}
    }});
}}
pub(crate) fn scene(scene:usize) {if active(){STATE.with(|s|s.borrow_mut().scene=scene);}}
pub(crate) fn query(class:usize,a:glam::Vec3,b:glam::Vec3) {
    if active(){STATE.with(|s|s.borrow_mut().query(class,[a.x.to_bits(),a.y.to_bits(),a.z.to_bits(),b.x.to_bits(),b.y.to_bits(),b.z.to_bits()]));}
}
fn write(s:&State,map:&str,out:&mut impl Write)->anyhow::Result<()> {
    anyhow::ensure!(!s.overflow && s.count==CAPACITY,"census requires exactly 60+120 accepted frames");
    anyhow::ensure!(s.rows[..60].iter().all(|r|!r.measured) && s.rows[60..].iter().all(|r|r.measured),"census warmup/measurement alignment");
    anyhow::ensure!(s.rows.iter().all(|r|r.errors==0 && r.total>=r.coverage && r.total>0),"census hierarchy/closure failure");
    // All allocation and encoding is after the measured window.
    let rows:Vec<_>=s.rows.iter().enumerate().map(|(i,r)|serde_json::json!({
        "accepted_frame":i+1,"sequence":r.sequence,"measured_frame":if r.measured {Some(i-59)}else{None},
        "frame_total_ns":r.total,"bracket_union_ns":r.coverage,"unbracketed_residual_ns":r.total-r.coverage,
        "inclusive_ns":r.inclusive,"exclusive_ns":r.exclusive,"calls":r.calls,"parent_masks":r.parents,
        "sweep_count":r.sweeps,"sweep_ns":r.sweep_ns,"acoustics_world_sweep_count":r.acoustic_sweeps,
        "acoustics_world_sweep_ns":r.acoustic_sweep_ns,"static_queries_total_hit1_hit4_hit16":r.queries,
        "live_table_evictions":r.evictions,"hierarchy_errors":r.errors,
    })).collect();
    serde_json::to_writer(&mut *out,&serde_json::json!({"schema":1,"map":map,"regions":NAMES,"owners":OWNERS,
        "query_classes":["room","ambient","effects"],"rows":rows,"capacity":CAPACITY,"table_entries":TABLE,"table_probes":PROBES,
        "parent_mask_frame_bit":REGIONS,"clock":"monotonic elapsed nanoseconds; includes preemption and API waits; no GPU timing",
        "residual":"frame_total minus union of root brackets; nested inclusive values must never be summed",
        "query_rule":"exact scene and endpoint bits; previous 1/4/16 attempted frames, excluding same-frame-only repeats; live evictions make hit opportunity a lower bound",
        "observer_threshold_ms":1.0}))?;writeln!(out)?;Ok(())
}
pub(crate) fn export(map:&str)->anyhow::Result<()> {
    if !active(){return Ok(());}
    #[cfg(target_os="vita")]
    let path=std::path::PathBuf::from(format!("ux0:data/alice/private/frame_profile/{map}-fullbench-census-180.json"));
    #[cfg(not(target_os="vita"))]
    let path=std::path::PathBuf::from(format!("{map}-census-180.json"));
    STATE.with(|s|->anyhow::Result<()>{let s=s.borrow();let mut out=std::io::BufWriter::new(std::fs::File::create(path)?);write(&s,map,&mut out)?;out.flush()?;Ok(())})?;
    ACTIVE.with(|a|a.set(false));Ok(())
}
#[cfg(not(target_os="vita"))]
pub(crate) fn host_finish(map:&str)->anyhow::Result<()> {if active(){STATE.with(|s|s.borrow_mut().close(Instant::now()));export(map)?;}Ok(())}

#[cfg(test)]
pub(crate) fn test_begin(enabled:bool) {begin(enabled);}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_keys_previous_windows_same_frame_scene_and_eviction() {
        let mut s=State::new();s.scene=1;s.sequence=1;s.query(0,[0;6]);s.query(0,[0;6]);
        assert_eq!(s.current.queries[0],[2,0,0,0]);
        s.sequence=2;s.current=Row::ZERO;s.query(1,[0;6]);s.query(1,[0;6]);
        assert_eq!(s.current.queries[1],[2,2,2,2]);
        s.sequence=6;s.current=Row::ZERO;s.query(2,[0;6]);assert_eq!(s.current.queries[2],[1,0,1,1]);
        s.sequence=22;s.current=Row::ZERO;s.query(0,[0;6]);assert_eq!(s.current.queries[0],[1,0,0,1]);
        s.sequence=39;s.current=Row::ZERO;s.query(0,[0;6]);assert_eq!(s.current.queries[0],[1,0,0,0]);
        s.sequence=40;s.scene=2;s.current=Row::ZERO;s.query(0,[0;6]);assert_eq!(s.current.queries[0],[1,0,0,0]);
        s.sequence=41;s.current=Row::ZERO;s.query(0,[0x80000000,0,0,0,0,0]);assert_eq!(s.current.queries[0],[1,0,0,0]);
        for i in 0..10000 {s.query(0,[i,1,2,3,4,5]);}assert!(s.current.evictions>0);
    }
    #[test]
    fn nested_union_owner_sweeps_and_hot_allocations() {
        begin(true);scene(1);
        let scope=crate::kernel_alloc::Scope::begin();
        {let _a=span(Region::AudioPrepare);let _o=owner(Owner::Audio);
            {let _k=span(Region::Acoustics);let _s=sweep();{let _nested=sweep();}
                query(0,glam::Vec3::ZERO,glam::Vec3::ONE);}}
        assert_eq!(scope.finish(),0);
        STATE.with(|s|{let s=s.borrow();assert_eq!(s.current.sweeps,[1,0,0,0,0]);
            assert_eq!(s.current.acoustic_sweeps,1);assert_eq!(s.current.errors,0);
            assert_eq!(s.current.coverage,s.current.inclusive[Region::AudioPrepare as usize]);
            assert_eq!(s.current.exclusive[Region::AudioPrepare as usize]+s.current.inclusive[Region::Acoustics as usize],s.current.coverage);});
        begin(false);
    }
    #[test]
    fn accepted_window_final_boundary_and_export_rejection() {
        begin(true);
        STATE.with(|s|{let mut s=s.borrow_mut();s.count=0;s.accepted=0;s.pending=false;});
        accept(false,false);begin(true);
        for i in 0..180 {
            STATE.with(|s|s.borrow_mut().start=Some(Instant::now()-std::time::Duration::from_micros(1)));
            accept(true,i>=60);begin(true);}
        STATE.with(|s|{let mut s=s.borrow_mut();assert_eq!(s.count,180);assert!(!s.overflow);
            let mut bytes=Vec::new();write(&s,"fixture",&mut bytes).unwrap();
            let v:serde_json::Value=serde_json::from_slice(&bytes).unwrap();
            assert_eq!(v["rows"][60]["measured_frame"],1);assert_eq!(v["rows"][179]["measured_frame"],120);
            s.overflow=true;assert!(write(&s,"fixture",&mut Vec::new()).is_err());});begin(false);
    }
    #[test]
    fn observer_cost_microbenchmark() {
        // Fixed workload and alternating rounds: includes 28 region brackets,
        // 200 nested API sweep calls, 24 exact-key lookups and frame bookkeeping.
        let mut results=Vec::new();
        for round in 0..8 {let enabled=matches!(round%4,1|2);let start=Instant::now();
            for _ in 0..1000 {begin(enabled);scene(1);
                for _ in 0..28 {std::hint::black_box(span(Region::Objects));}
                for _ in 0..100 {let _s=sweep();std::hint::black_box(sweep());}
                for _ in 0..24 {query(0,glam::Vec3::ZERO,glam::Vec3::ONE);}
            }results.push((enabled,start.elapsed().as_nanos() as f64/1000.));}
        begin(false);println!("CENSUS_MICROBENCH_NS_PER_FRAME {:?}",results);
        println!("CENSUS_FIXED_STORAGE_BYTES {}",std::mem::size_of::<State>());
    }
}
