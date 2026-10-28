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
    Fx, JointFinish, FxSubmit, Hud, Present, Swap, ActorPose, ActorScalesPalette, ActorNumeric,
    ActorSerial, ActorWorkerDispatch, ActorConsumers, ActorDirect, ActorFallback, ActorAttachments, FinishSort, FinishMergePack, FinishUploadCpu, FinishUploadGl, FinishUniforms, FinishPass, FinishRecycle, WorldStateBindings, WorldUniforms, WorldDraws, Input, StoryWeapons, Particles, CameraScene, RenderSetup, FirstPerson, FrameworkTail, BackendReport, Other }
const REGIONS: usize = Region::Other as usize;
const NAMES: [&str; REGIONS] = ["npc_think_movement", "player_movement", "fixed_physics", "movers", "objects",
    "audio_prepare", "prepare_acoustics", "light_selection", "world_opaque", "world_visibility",
    "world_parameters", "world_submit", "alpha_prepare", "alpha_submit", "actor_gather", "actor_collect",
    "actor_publication", "actor_submit", "fx_gather", "joint_finish", "fx_submit", "hud", "present", "swap", "actor_pose", "actor_scales_palette",
    "actor_numeric_capture", "actor_serial_skin", "actor_worker_dispatch", "actor_consumers_publication", "actor_direct_prepare", "actor_fallback_skin_layers", "actor_npc_attachments", "joint_sort", "joint_merge_pack", "joint_upload_cpu", "joint_upload_gl", "joint_uniform_prep", "joint_pass_setup_end", "joint_recycle", "world_state_bindings", "world_uniforms", "world_draws", "input", "story_weapons", "particles", "camera_scene", "render_setup", "first_person", "framework_tail", "backend_timing_report"];
#[derive(Clone, Copy)]
#[repr(usize)]
pub(crate) enum Owner { Audio, Player, Npc, Camera, Other }
const OWNERS: [&str; 5] = ["audio", "player", "npc", "camera", "other"];
const ACTORS: usize = 128;
pub(crate) const MUTABLE: u32 = 1;
pub(crate) const PUPPET: u32 = 2;
pub(crate) const FROZEN: u32 = 4;
pub(crate) const ELECTRIC: u32 = 8;
pub(crate) const AMBIENT: u32 = 16;
pub(crate) const BOUNDS: u32 = 32;
pub(crate) const TOPOLOGY: u32 = 64;
pub(crate) const PUBLICATION: u32 = 128;
const REASONS: [&str;8] = ["mutable", "puppet_capability", "frozen", "electric", "ambient_refresh",
    "arena_or_identity", "topology", "publication_fallback"];
#[derive(Clone, Copy, serde::Serialize)]
struct Actor {
    id: u64, serial_vertices: u32, worker_vertices: u32, serial_reasons: u32,
    reason_vertices: [u32;8],
}
impl Actor { const ZERO: Self = Self { id:0,serial_vertices:0,worker_vertices:0,serial_reasons:0,reason_vertices:[0;8] }; }
pub(crate) const PUBLICATION_REJECTIONS: [&str;11] = ["missing_texture", "unregistered_surface", "opacity", "empty_recipe", "unsupported_recipe", "mixed_alpha", "bank_surface_capacity", "bank_held", "bank_vertex_capacity", "bank_index_capacity", "frame_vertex_budget"];
pub(crate) const COUNTERS: [&str;7] = ["upload_requests", "upload_hits", "upload_bytes", "world_draws", "world_texture_binds", "world_sampler_uniform_sets", "world_vbo_layout_changes"];
#[derive(Clone, Copy)]
struct Row {
    sequence: u64, measured: bool, total: u64, coverage: u64,
    actors: [Actor;ACTORS], actor_count: usize, worker_skin_ns: [u64;2],
    inclusive: [u64; REGIONS], exclusive: [u64; REGIONS], calls: [u32; REGIONS], parents: [u64; REGIONS],
    sweeps: [u32; 5], sweep_ns: [u64; 5], acoustic_sweeps: u32, acoustic_sweep_ns: u64,
    static_ns:[u64;5], static_counts:[[u64;7];5],
    cache: [[u32; 3]; 3], queries: [[u32; 4]; 3], evictions: u32, errors: u32,
    serial_fallbacks: u64, rejections:[u64;11], counters:[u64;7],
}
impl Row {
    const ZERO: Self = Self { sequence: 0, measured: false, total: 0, coverage: 0,
        actors:[Actor::ZERO;ACTORS], actor_count:0, worker_skin_ns:[0;2],
        inclusive: [0;REGIONS], exclusive: [0;REGIONS], calls: [0;REGIONS], parents: [0;REGIONS],
        sweeps:[0;5], sweep_ns:[0;5], acoustic_sweeps:0, acoustic_sweep_ns:0,
        static_ns:[0;5],static_counts:[[0;7];5],
        cache:[[0;3];3], queries:[[0;4];3], evictions:0, errors:0, serial_fallbacks:0, rejections:[0;11], counters:[0;7] };
}
#[derive(Clone, Copy)]
struct Entry { scene: usize, bits: [u32;6], last: u64, prior: u64 }
impl Entry { const EMPTY: Self = Self {scene:0,bits:[0;6],last:0,prior:0}; }
struct State {
    rows: [Row;CAPACITY], current: Row, count: usize, accepted: usize,
    start: Option<Instant>, pending: bool, overflow: bool, sequence: u64,
    owner: Owner, sweep_depth: usize, scene: usize, query_class: usize,
    stack: [usize;DEPTH], children: [u64;DEPTH], depth: usize,
    table: [Entry;TABLE],
    serial_fallback_base: u64,
}
impl State {
    #[cfg(test)]
    const fn new() -> Self { Self { rows:[Row::ZERO;CAPACITY],current:Row::ZERO,count:0,accepted:0,
        start:None,pending:false,overflow:false,sequence:0,owner:Owner::Other,sweep_depth:0,scene:0,query_class:0,
        stack:[0;DEPTH],children:[0;DEPTH],depth:0,table:[Entry::EMPTY;TABLE],serial_fallback_base:0 } }
    fn boxed() -> Box<Self> {
        // Initialize bounded storage once, before gameplay. A full const State
        // would embed the enlarged actor rows in the native ELF's initializer.
        // Row/Entry and the integer arrays contain only zero-valid scalar fields;
        // initialize Option<Instant> and Owner explicitly in their own layouts.
        let mut storage=Box::<Self>::new_uninit();
        let p=storage.as_mut_ptr();
        unsafe {
            std::ptr::addr_of_mut!((*p).rows).write_bytes(0,1);
            std::ptr::addr_of_mut!((*p).current).write(Row::ZERO);
            std::ptr::addr_of_mut!((*p).count).write(0);
            std::ptr::addr_of_mut!((*p).accepted).write(0);
            std::ptr::addr_of_mut!((*p).start).write(None);
            std::ptr::addr_of_mut!((*p).pending).write(false);
            std::ptr::addr_of_mut!((*p).overflow).write(false);
            std::ptr::addr_of_mut!((*p).sequence).write(0);
            std::ptr::addr_of_mut!((*p).owner).write(Owner::Other);
            std::ptr::addr_of_mut!((*p).sweep_depth).write(0);
            std::ptr::addr_of_mut!((*p).scene).write(0);
            std::ptr::addr_of_mut!((*p).query_class).write(0);
            std::ptr::addr_of_mut!((*p).stack).write_bytes(0,1);
            std::ptr::addr_of_mut!((*p).children).write_bytes(0,1);
            std::ptr::addr_of_mut!((*p).depth).write(0);
            std::ptr::addr_of_mut!((*p).table).write_bytes(0,1);
            std::ptr::addr_of_mut!((*p).serial_fallback_base).write(0);
            storage.assume_init()
        }
    }
    fn close(&mut self, now: Instant) {
        if let Some(start) = self.start {
            if self.pending {
                self.current.total=now.duration_since(start).as_nanos() as u64;
                self.current.serial_fallbacks=crate::character::skinning::overlap::serial_fallbacks()
                    .saturating_sub(self.serial_fallback_base);
                if self.depth != 0 || self.sweep_depth != 0 { self.current.errors += 1; }
                if self.count < CAPACITY {self.rows[self.count]=self.current;self.count+=1;} else {self.overflow=true;}
            }
        }
        self.pending=false;
    }
    fn query(&mut self, class:usize, bits:[u32;6]) {
        self.query_class = class;
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
    static STATE: RefCell<Box<State>> = RefCell::new(State::boxed());
}
pub(crate) fn active() -> bool {ACTIVE.with(Cell::get)}
pub(crate) fn is_active()->bool {active()}
fn configured() -> bool {
    #[cfg(feature="vita_renderer")]
    {crate::render::vita_renderer::config().phase_census}
    #[cfg(not(feature="vita_renderer"))]
    {false}
}
pub(crate) fn initialize() {if configured() {STATE.with(|_|{});}}
pub(crate) fn set_owner(owner:Owner) {
    #[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
    crate::collision::sah_probe::owner(owner as usize);
    if active(){STATE.with(|s|s.borrow_mut().owner=owner);}
}
pub(crate) fn reset() {
    ACTIVE.with(|a|a.set(false));
    // Do not touch the large fixed table in the default-off arm.
    if configured() { STATE.with(|s| {let mut s=s.borrow_mut();s.count=0;s.accepted=0;s.start=None;
        s.pending=false;s.overflow=false;s.sequence=0;s.depth=0;s.sweep_depth=0;s.owner=Owner::Other;
        s.table.fill(Entry::EMPTY);}); }
}
pub(crate) fn begin_configured() {
    #[cfg(target_os="vita")]
    let enabled = configured() || crate::r13_survey::census_active();
    #[cfg(not(target_os="vita"))]
    let enabled = configured();
    begin(enabled);
}
#[cfg(target_os="vita")]
pub(crate) fn initialize_survey() { STATE.with(|_|{}); }
#[cfg(target_os="vita")]
pub(crate) fn export_survey(map:&str)->anyhow::Result<()> {
    let path=format!("ux0:data/alice/private/frame_profile/{map}-r13b-census.json");
    STATE.with(|s|->anyhow::Result<()> {let s=s.borrow();let mut out=std::io::BufWriter::new(std::fs::File::create(&path)?);
        write(&s,map,&mut out)?;out.flush()?;
        let mut csv=std::io::BufWriter::new(std::fs::File::create(std::path::Path::new(&path).with_extension("csv"))?);
        write_csv(&s,&mut csv)?;csv.flush()?;Ok(())})?;
    ACTIVE.with(|a|a.set(false));Ok(())
}
// The explicit survey runs in Full mode, where the benchmark clock is inert.
// Its presentation timestamp owns BOTH the CSV interval and the census row.
#[cfg(any(target_os="vita",test))]
pub(crate) fn begin_survey(now:Instant) {
    STATE.with(|s| {let mut s=s.borrow_mut();s.count=0;s.accepted=0;s.start=None;
        s.pending=false;s.overflow=false;s.sequence=0;s.depth=0;s.sweep_depth=0;
        s.table.fill(Entry::EMPTY);});
    begin_at(true,now);
}
#[cfg(any(target_os="vita",test))]
pub(crate) fn survey_presented(now:Instant,finished:bool) {
    if finished {
        STATE.with(|s|s.borrow_mut().close(now));
        ACTIVE.with(|a|a.set(false));
    } else {begin_at(true,now);}
}
fn begin(enabled:bool) {
    if !enabled {ACTIVE.with(|a|a.set(false));return;}
    begin_at(enabled,Instant::now());
}
fn begin_at(enabled:bool,now:Instant) {
    ACTIVE.with(|a|a.set(enabled));if !enabled {return;}
    STATE.with(|s| {let mut s=s.borrow_mut();s.close(now);s.sequence+=1;
        s.current=Row{sequence:s.sequence,..Row::ZERO};s.start=Some(now);s.owner=Owner::Other;
        s.depth=0;s.sweep_depth=0;
        s.serial_fallback_base=crate::character::skinning::overlap::serial_fallbacks();});
}
pub(crate) fn accept(accepted:bool,measured:bool) {
    if !active() {return;}
    STATE.with(|s|{let mut s=s.borrow_mut();
        // The generic profiler can nominate a frame before the survey supplies
        // its final attempted-frame decision. Count that frame only once.
        if accepted && !s.pending {s.accepted+=1;}
        if !accepted && s.pending {s.accepted-=1;}
        s.pending=accepted;s.current.measured=measured;});
}
pub(crate) struct Timer {start:Option<Instant>, region:usize, level:usize}
/// Hot world-loop leaves accumulate locally: two clocks per interval, one TLS
/// publication per pass, no stack/RefCell traffic per draw. The parent owns all
/// intervals; they must be disjoint and publish before its guard is dropped.
pub(crate) struct WorldLeaves {enabled:bool,level:usize,ns:[u64;3],calls:[u32;3]}
pub(crate) fn world_leaves()->WorldLeaves {
    let mut result=WorldLeaves{enabled:active(),level:0,ns:[0;3],calls:[0;3]};
    if result.enabled {STATE.with(|s| {let s=s.borrow();result.level=s.depth;});}
    result
}
impl WorldLeaves {
    pub(crate) fn start(&self)->Option<Instant> {self.enabled.then(Instant::now)}
    pub(crate) fn add(&mut self,index:usize,start:Option<Instant>) {
        if let Some(start)=start {self.ns[index]+=start.elapsed().as_nanos() as u64;self.calls[index]+=1;}
    }
}
impl Drop for WorldLeaves {fn drop(&mut self) {
    if !self.enabled {return;}
    STATE.with(|s| {let mut s=s.borrow_mut();let d=self.level;
        if s.depth!=d || d==0 || s.stack[d-1]!=Region::WorldSubmit as usize {s.current.errors+=1;return;}
        for (j,region) in [Region::WorldStateBindings,Region::WorldUniforms,Region::WorldDraws].into_iter().enumerate() {
            let i=region as usize;s.current.inclusive[i]+=self.ns[j];s.current.exclusive[i]+=self.ns[j];
            s.current.calls[i]+=self.calls[j];if self.calls[j]>0 {s.current.parents[i]|=1u64<<(Region::WorldSubmit as usize);}
            s.children[d-1]+=self.ns[j];
        }
    });
}}
pub(crate) fn span(region:Region) -> Timer {
    let mut timer=Timer{start:None,region:region as usize,level:0};
    if region==Region::Other || !active() {return timer;}
    STATE.with(|s| {let mut s=s.borrow_mut();
        // These are actor_gather subdivisions, not whole-frame pose/FX totals.
        if (Region::ActorPose as usize..=Region::ActorAttachments as usize).contains(&(region as usize)) && !s.stack[..s.depth].contains(&(Region::ActorGather as usize)) {return;}
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
pub(crate) struct OwnerScope(Option<Owner>,
    #[cfg(all(feature="sim_maps_probe",not(target_os="vita")))] usize);
pub(crate) fn owner(owner:Owner) -> OwnerScope {
    OwnerScope(if active() {Some(STATE.with(|s| {let mut s=s.borrow_mut();std::mem::replace(&mut s.owner,owner)}))} else {None},
        #[cfg(all(feature="sim_maps_probe",not(target_os="vita")))] crate::collision::sah_probe::owner(owner as usize))
}
impl Drop for OwnerScope {fn drop(&mut self){
    #[cfg(all(feature="sim_maps_probe",not(target_os="vita")))] crate::collision::sah_probe::owner(self.1);
    if let Some(owner)=self.0 {STATE.with(|s|s.borrow_mut().owner=owner);}
}}
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
/// Static misses only, before settled/dynamic/weapon completion. Cache hits do
/// not enter this bracket. These nested times never add to root coverage.
pub(crate) struct StaticTraversal {start:Option<Instant>,owner:usize}
pub(crate) fn static_traversal()->StaticTraversal {
    if !active(){return StaticTraversal{start:None,owner:0};}
    StaticTraversal{start:Some(Instant::now()),owner:STATE.with(|s|s.borrow().owner as usize)}
}
impl StaticTraversal {
    pub(crate) fn finish(self,nodes:u64,tested:u64,clipped:u64,planes:u64,certificates:u64,fallbacks:u64) {
        if let Some(t)=self.start {let ns=t.elapsed().as_nanos() as u64;
            STATE.with(|s|{let mut s=s.borrow_mut();s.current.static_ns[self.owner]+=ns;
                let row=&mut s.current.static_counts[self.owner];
                for (dst,src) in row.iter_mut().zip([1,nodes,tested,clipped,planes,certificates,fallbacks]){*dst+=src;}
            });
        }
    }
}
pub(crate) fn scene(scene:usize) {if active(){STATE.with(|s|s.borrow_mut().scene=scene);}}
pub(crate) fn query(class:usize,a:glam::Vec3,b:glam::Vec3) {
    if active(){STATE.with(|s|s.borrow_mut().query(class,[a.x.to_bits(),a.y.to_bits(),a.z.to_bits(),b.x.to_bits(),b.y.to_bits(),b.z.to_bits()]));}
}
pub(crate) fn cache_result(hit: bool, evicted: bool) {
    if active() { STATE.with(|s| { let mut s=s.borrow_mut(); let class=s.query_class;
        s.current.cache[class][if hit {0} else {1}]+=1;
        s.current.cache[class][2]+=u32::from(evicted);
    }); }
}
/// Main-thread counters only. Unknown identities use 0; overflow invalidates export.
pub(crate) fn skin(actor:Option<crate::character::skinning::identity::ActorId>,vertices:usize,reasons:u32) {
    if !active() { return; }
    STATE.with(|s| { let mut s=s.borrow_mut(); let r=&mut s.current;
        let id=actor.map_or(0,|a|a.census_id());
        let index=if let Some(i)=r.actors[..r.actor_count].iter().position(|a|a.id==id) {i}
        else {if r.actor_count==ACTORS {r.errors+=1;return;} let i=r.actor_count;r.actor_count+=1;r.actors[i].id=id;i};
        let a=&mut r.actors[index];
        if reasons==0 {a.worker_vertices+=vertices as u32;} else {
            a.serial_vertices+=vertices as u32;a.serial_reasons|=reasons;
            for i in 0..8 {if reasons&(1<<i)!=0 {a.reason_vertices[i]+=vertices as u32;}}
        }
    });
}
pub(crate) fn worker_skin(index:usize,ns:u64) {
    if active() { STATE.with(|s|s.borrow_mut().current.worker_skin_ns[index]+=ns); }
}
pub(crate) fn rejection(reason:usize) {
    if active() { STATE.with(|s|s.borrow_mut().current.rejections[reason]+=1); }
}
pub(crate) fn count(index:usize,value:u64) {
    if active() { STATE.with(|s|s.borrow_mut().current.counters[index]+=value); }
}
/// Miniquad reports after swap, outside any viewer scope. The callback runs
/// synchronously on the render thread; reject overlap instead of double counting.
#[cfg(any(target_os="vita",test))]
pub(crate) fn backend_report(ns:u64) {
    if active() { STATE.with(|s| {let mut s=s.borrow_mut();
        if s.depth!=0 {s.current.errors+=1;return;}
        let i=Region::BackendReport as usize;
        s.current.inclusive[i]+=ns;s.current.exclusive[i]+=ns;s.current.calls[i]+=1;
        s.current.parents[i]|=1u64<<REGIONS;s.current.coverage+=ns;
    }); }
}
#[cfg(target_os="vita")]
pub(crate) struct WorldCommands(bool);
#[cfg(target_os="vita")]
pub(crate) fn world_commands()->WorldCommands {
    let enabled=active();
    if enabled { unsafe { macroquad::miniquad::native::vita::begin_diagnostic_commands(); } }
    WorldCommands(enabled)
}
#[cfg(target_os="vita")]
impl Drop for WorldCommands {fn drop(&mut self) {
    if self.0 {let counts=unsafe {macroquad::miniquad::native::vita::end_diagnostic_commands()};
        if active() {STATE.with(|s| {let mut s=s.borrow_mut();for (i,n) in counts.into_iter().enumerate() {s.current.counters[i+3]+=n;}});}
    }
}}
fn write(s:&State,map:&str,out:&mut impl Write)->anyhow::Result<()> {
    anyhow::ensure!(!s.overflow && s.count==CAPACITY,"census requires exactly 60+120 accepted frames");
    anyhow::ensure!(s.rows[..60].iter().all(|r|!r.measured) && s.rows[60..].iter().all(|r|r.measured),"census warmup/measurement alignment");
    anyhow::ensure!(s.rows.iter().all(|r|r.errors==0 && r.total>=r.coverage && r.total>0 && r.exclusive.iter().sum::<u64>()==r.coverage),"census hierarchy/closure failure");
    // All allocation and encoding is after the measured window.
    let rows:Vec<_>=s.rows.iter().enumerate().map(|(i,r)|serde_json::json!({
        "accepted_frame":i+1,"sequence":r.sequence,"measured_frame":if r.measured {Some(i-59)}else{None},
        "frame_total_ns":r.total,"bracket_union_ns":r.coverage,"unbracketed_residual_ns":r.total-r.coverage,
        "inclusive_ns":&r.inclusive[..],"exclusive_ns":&r.exclusive[..],"calls":&r.calls[..],"parent_masks":&r.parents[..],
        "static_traversal_ns":r.static_ns,"static_traversal_counts":r.static_counts,
        "sweep_count":r.sweeps,"sweep_ns":r.sweep_ns,"acoustics_world_sweep_count":r.acoustic_sweeps,
        "acoustics_world_sweep_ns":r.acoustic_sweep_ns,"static_queries_total_hit1_hit4_hit16":r.queries,
        "static_cache_hit_miss_eviction":r.cache,
        "live_table_evictions":r.evictions,"hierarchy_errors":r.errors,
        "actors":&r.actors[..r.actor_count],"worker_skin_ns":r.worker_skin_ns,
        "serial_fallbacks":r.serial_fallbacks,"publication_rejections":r.rejections,"diagnostic_counters":r.counters,
        "added_brackets":r.calls[30..].iter().map(|&n|n as u64).sum::<u64>(),
    })).collect();
    let serial_fallbacks:u64=s.rows[..s.count].iter().map(|r|r.serial_fallbacks).sum();
    #[cfg(feature="vita_renderer")]
    let audio_static_cache=crate::render::vita_renderer::config().audio_static_cache;
    #[cfg(not(feature="vita_renderer"))]
    let audio_static_cache=false;
    serde_json::to_writer(&mut *out,&serde_json::json!({"schema":4,"publication_rejection_columns":PUBLICATION_REJECTIONS,"diagnostic_counter_columns":COUNTERS,"actor_capacity":ACTORS,"serial_reasons":REASONS,
        "audio_static_cache":audio_static_cache,"map":map,"regions":&NAMES[..],"owners":OWNERS,
        "static_count_columns":["queries","nodes","hulls_tested","hulls_clipped","planes","legacy_certificates","fallbacks"],
        "query_classes":["room","ambient","effects"],"rows":rows,"capacity":CAPACITY,"table_entries":TABLE,"table_probes":PROBES,
        "serial_fallbacks":serial_fallbacks,
        "parent_mask_frame_bit":REGIONS,"clock":"monotonic elapsed nanoseconds; includes preemption and API waits; no GPU timing",
        "residual":"frame_total minus union of root brackets; nested inclusive values must never be summed",
        "query_rule":"exact scene and endpoint bits; previous 1/4/16 attempted frames, excluding same-frame-only repeats; live evictions make hit opportunity a lower bound",
        "observer_threshold_ms":1.0}))?;writeln!(out)?;Ok(())
}
fn write_csv(s:&State,out:&mut impl Write)->anyhow::Result<()> {
    write!(out,"accepted_frame,sequence,measured,frame_total_ns,bracket_union_ns,unbracketed_residual_ns,added_brackets")?;
    for suffix in ["inclusive_ns","exclusive_ns","calls","parent_mask"] {for name in NAMES {write!(out,",{name}_{suffix}")?;}}
    for name in PUBLICATION_REJECTIONS {write!(out,",publication_reject_{name}")?;}
    for name in COUNTERS {write!(out,",{name}")?;}
    writeln!(out)?;
    for (i,r) in s.rows.iter().enumerate() {
        write!(out,"{},{},{},{},{},{},{}",i+1,r.sequence,r.measured,r.total,r.coverage,r.total-r.coverage,r.calls[30..].iter().map(|&n|n as u64).sum::<u64>())?;
        for values in [&r.inclusive[..],&r.exclusive[..]] {for n in values {write!(out,",{n}")?;}}
        for n in r.calls {write!(out,",{n}")?;}
        for n in r.parents {write!(out,",{n}")?;}
        for n in r.rejections {write!(out,",{n}")?;}
        for n in r.counters {write!(out,",{n}")?;}
        writeln!(out)?;
    }
    Ok(())
}
pub(crate) fn export(map:&str)->anyhow::Result<()> {
    if !active(){return Ok(());}
    #[cfg(target_os="vita")]
    let path=std::path::PathBuf::from(format!("ux0:data/alice/private/frame_profile/{map}-fullbench-census-180.json"));
    #[cfg(not(target_os="vita"))]
    let path=std::path::PathBuf::from(format!("{map}-census-180.json"));
    STATE.with(|s|->anyhow::Result<()>{let s=s.borrow();let mut out=std::io::BufWriter::new(std::fs::File::create(&path)?);write(&s,map,&mut out)?;out.flush()?;
        let mut csv=std::io::BufWriter::new(std::fs::File::create(std::path::Path::new(&path).with_extension("csv"))?);
        write_csv(&s,&mut csv)?;csv.flush()?;Ok(())})?;
    ACTIVE.with(|a|a.set(false));Ok(())
}
#[cfg(not(target_os="vita"))]
pub(crate) fn host_finish(map:&str)->anyhow::Result<()> {if active(){STATE.with(|s|s.borrow_mut().close(Instant::now()));export(map)?;}Ok(())}

#[cfg(test)]
pub(crate) fn test_begin(enabled:bool) {begin(enabled);}
#[cfg(test)]
pub(crate) fn test_survey_json()->anyhow::Result<serde_json::Value> {
    STATE.with(|s| {let s=s.borrow();assert_eq!(s.accepted,s.count);
        let mut bytes=Vec::new();write(&s,"fixture",&mut bytes)?;
        Ok(serde_json::from_slice(&bytes)?)})
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn r14_all_new_leaves_and_csv_close_with_real_timer_bodies() {
        let mut now=Instant::now();begin_survey(now);
        let allocation_scope=crate::kernel_alloc::Scope::begin();
        for frame in 0..180 {
            {let _root=span(Region::ActorGather);
             let _consumer=span(Region::ActorConsumers);
             for leaf in [Region::ActorDirect,Region::ActorFallback,Region::ActorAttachments] {drop(span(leaf));}}
            {let _root=span(Region::JointFinish);
             for leaf in [Region::FinishSort,Region::FinishMergePack,Region::FinishPass,Region::FinishRecycle] {drop(span(leaf));}
             let _cpu=span(Region::FinishUploadCpu);
             drop(span(Region::FinishUploadGl));drop(span(Region::FinishUniforms));}
            {let _root=span(Region::WorldSubmit);
             let mut leaves=world_leaves();
             for i in 0..3 {let start=leaves.start();leaves.add(i,start);}}
            for leaf in [Region::Input,Region::StoryWeapons,Region::Particles,Region::CameraScene,Region::RenderSetup,Region::FirstPerson,Region::FrameworkTail] {drop(span(leaf));}
            backend_report(7);
            for i in 0..11 {rejection(i);}
            for i in 0..7 {count(i,1);}
            accept(true,frame>=60);now+=std::time::Duration::from_secs(1);
            survey_presented(now,frame==179);
        }
        assert_eq!(allocation_scope.finish(),0,"diagnostic hot path allocated");
        println!("R14_CENSUS_STORAGE_BYTES state={} row={}",std::mem::size_of::<State>(),std::mem::size_of::<Row>());
        let v=test_survey_json().unwrap();
        for row in v["rows"].as_array().unwrap() {
            let sum:u64=row["exclusive_ns"].as_array().unwrap().iter().map(|n|n.as_u64().unwrap()).sum();
            assert_eq!(sum+row["unbracketed_residual_ns"].as_u64().unwrap(),row["frame_total_ns"].as_u64().unwrap());
            assert_eq!(row["added_brackets"],21);
            for i in 30..REGIONS {assert_eq!(row["calls"][i],1);}
        }
        STATE.with(|s| {
            let s=s.borrow();let mut bytes=Vec::new();write_csv(&s,&mut bytes).unwrap();
            let csv=std::str::from_utf8(&bytes).unwrap();let mut lines=csv.lines();
            let header:Vec<_>=lines.next().unwrap().split(',').collect();
            assert_eq!(header.len(),7+4*REGIONS+11+7);
            for (i,line) in lines.enumerate() {
                let values:Vec<_>=line.split(',').collect();assert_eq!(values.len(),header.len());
                let row=&v["rows"][i];
                for (field,index) in [("frame_total_ns",3),("bracket_union_ns",4),("unbracketed_residual_ns",5),("added_brackets",6)] {assert_eq!(values[index].parse::<u64>().unwrap(),row[field].as_u64().unwrap());}
                for (field,base) in [("inclusive_ns",7),("exclusive_ns",7+REGIONS),("calls",7+2*REGIONS),("parent_masks",7+3*REGIONS),("publication_rejections",7+4*REGIONS),("diagnostic_counters",18+4*REGIONS)] {
                    for (j,n) in row[field].as_array().unwrap().iter().enumerate() {assert_eq!(values[base+j].parse::<u64>().unwrap(),n.as_u64().unwrap());}
                }
            }
            if let Ok(dir)=std::env::var("ALICE_TIMING_TEST_OUTPUT") {
                std::fs::write(std::path::Path::new(&dir).join("r14-synthetic-census.json"),serde_json::to_vec(&v).unwrap()).unwrap();
                std::fs::write(std::path::Path::new(&dir).join("r14-synthetic-census.csv"),bytes).unwrap();
            }
        });
        begin(false);backend_report(99);count(0,99);rejection(0);
    }
    #[test]
    fn r14_backend_overlap_and_exclusive_closure_are_fail_closed() {
        begin(true);
        {let _scope=span(Region::Input);backend_report(1);}
        STATE.with(|s|assert_eq!(s.borrow().current.errors,1));begin(false);
        let mut s=State::boxed();s.count=CAPACITY;
        for (i,r) in s.rows.iter_mut().enumerate() {*r=Row{total:10,measured:i>=60,..Row::ZERO};}
        s.rows[0].exclusive[30]=1;
        assert!(write(&s,"fixture",&mut Vec::new()).is_err());
    }
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
            assert_eq!(v["schema"],4);assert_eq!(v["audio_static_cache"],false);
            assert_eq!(v["serial_fallbacks"],0);assert_eq!(v["rows"][0]["serial_fallbacks"],0);
            assert_eq!(v["regions"].as_array().unwrap().len(),NAMES.len());
            assert_eq!(v["actor_capacity"],ACTORS);assert!(v.get("static_count_columns").is_some());
            s.overflow=true;assert!(write(&s,"fixture",&mut Vec::new()).is_err());});begin(false);
    }
    #[test]
    fn survey_validation_still_rejects_count_alignment_and_hierarchy_errors() {
        let mut s=State::boxed();
        for (i,r) in s.rows.iter_mut().enumerate() {*r=Row{total:10,measured:i>=60,..Row::ZERO};}
        s.count=CAPACITY;
        assert!(write(&s,"fixture",&mut Vec::new()).is_ok());
        s.count=179;assert!(write(&s,"fixture",&mut Vec::new()).is_err());s.count=CAPACITY;
        s.overflow=true;assert!(write(&s,"fixture",&mut Vec::new()).is_err());s.overflow=false;
        s.rows[59].measured=true;assert!(write(&s,"fixture",&mut Vec::new()).is_err());s.rows[59].measured=false;
        s.rows[60].measured=false;assert!(write(&s,"fixture",&mut Vec::new()).is_err());s.rows[60].measured=true;
        s.rows[0].errors=1;assert!(write(&s,"fixture",&mut Vec::new()).is_err());s.rows[0].errors=0;
        s.rows[0].coverage=11;assert!(write(&s,"fixture",&mut Vec::new()).is_err());s.rows[0].coverage=0;
        s.rows[0].total=0;assert!(write(&s,"fixture",&mut Vec::new()).is_err());
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
