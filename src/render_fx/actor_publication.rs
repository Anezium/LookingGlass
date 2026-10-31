//! Two owned banks per logical surface. Only numeric array addresses reach workers.
//! Pending packets contain handles, never borrowed/reused worker output. Collection
//! is the sole publication boundary; retained GPU storage keeps its five-slot ring.
use super::*;
use crate::skeletal::SkinSurface;
use std::{cell::Cell, collections::HashMap, rc::Weak, sync::Arc};
use crate::character::skinning::identity::ActorId;
use super::actor_vertices::PublicationMode;
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct Key { pub actor: ActorId, pub surface: usize }
const UNUSED_FRAMES:u64=120;
const SURFACES:usize=2048;
const VERTICES:usize=65536;
const INDICES:usize=262144;
#[derive(Default)]
struct Bank { key:Option<Key>, last_used:u64, slots:[Option<Rc<prepared::Geometry>>;2], cpu:[Option<Arc<SkinSurface>>;2], mode:[Option<PublicationMode>;2], epoch:[u64;2], instance:[Option<u64>;2] }
/// A non-simple recipe whose raw skin a worker writes into `bank`/`slot`. Its layers are
/// built on the game thread after a successful drain, into the placeholder at `group`.
/// Game-thread only: the Rc<Surface> and captured draw values never reach a worker.
struct Deferred { group:usize, bank:usize, slot:usize, surface:Rc<Surface>, camera:Vec3, time:f32, appearance:Appearance, limit:usize, vertices:usize, dropped_layers:u64 }
#[derive(Default)]
struct State {
    banks:Vec<Bank>, lookup:HashMap<Key,usize>,
    #[cfg(test)] cursor:usize,
    /// (group, bank, slot, vertex charge) of direct groups that receive geometry in complete().
    pending:Vec<(usize,usize,usize,usize)>,
    deferred:Vec<Deferred>,
    held:Vec<Group>, held_puffs:Vec<retained::world_puffs::Puff>,
    /// True after hold() moves this frame's actor groups out of FRAME.
    /// Failed cleanup must touch that owner, not a later world group.
    staged_held:bool,
    completed:bool,
    /// Geometries whose bank Rc was dropped while another owner still held them.
    /// Host probe and tests only: memory_account() prunes it, nothing does on the Vita.
    #[cfg(any(test, all(feature="sim_maps_probe", not(target_os="vita"))))]
    evicted_referenced:Vec<Weak<prepared::Geometry>>,
    vertex_capacity:usize, index_capacity:usize, epoch:u64,
}
impl State {
    fn clear(&mut self) { *self=Self::default(); }
    fn evict(&mut self,id:usize) {
        let bank=&mut self.banks[id];
        if let Some(key)=bank.key.take() { self.lookup.remove(&key); }
        for g in bank.slots.iter().flatten() {
            #[cfg(any(test, all(feature="sim_maps_probe", not(target_os="vita"))))]
            if Rc::strong_count(g)>1 { self.evicted_referenced.push(Rc::downgrade(g)); }
            self.vertex_capacity-=g.vertices.capacity(); self.index_capacity-=g.indices.capacity();
        }
        *bank=Bank::default();
    }
    fn next_frame(&mut self) {
        self.epoch=self.epoch.checked_add(1).expect("actor publication epoch overflow");
        self.pending.clear(); self.deferred.clear(); self.held.clear(); self.held_puffs.clear();
        self.staged_held=false; self.completed=false;
        for id in 0..self.banks.len() {
            if self.banks[id].key.is_some() && self.epoch-self.banks[id].last_used>UNUSED_FRAMES { self.evict(id); }
        }
    }
    fn oldest_unused(&self)->Option<usize> {
        // Never evict a destination used this frame, or churn last frame's actors
        // merely because their draw hasn't been visited yet in this frame.
        self.banks.iter().enumerate().filter(|(_,b)|b.key.is_some() && b.last_used+1<self.epoch)
            .min_by_key(|(_,b)|b.last_used).map(|(id,_)|id)
    }
    fn reserve_key(&mut self,mesh:&Mesh,cpu:Arc<SkinSurface>,unchanged:bool,instance:u64,alpha:PublicationMode,key:Key)->Option<(usize,usize,usize)> {
        let id=if let Some(&id)=self.lookup.get(&key) {id} else {
            let id=if let Some(id)=self.banks.iter().position(|b|b.key.is_none()) {id}
                else if self.banks.len()<SURFACES {self.banks.push(Bank::default());self.banks.len()-1}
                else {let id=self.oldest_unused()?;self.evict(id);id};
            self.banks[id].key=Some(key); self.lookup.insert(key,id); id
        };
        self.banks[id].last_used=self.epoch;
        loop {
            if let Some(result)=self.reserve_at(mesh,cpu.clone(),unchanged,instance,alpha,id) {return Some(result);}
            // Held slots cannot be reclaimed; evict only unrelated unused ids.
            if self.banks[id].slots.iter().enumerate().all(|(slot,g)|g.as_ref().is_some_and(|g|Rc::strong_count(g)>1 || self.banks[id].epoch[slot]==self.epoch)) {return None;}
            self.evict(self.oldest_unused()?);
        }
    }

    #[cfg(test)]
    fn reserve(&mut self,mesh:&Mesh,cpu:Arc<SkinSurface>,unchanged:bool,alpha:PublicationMode)->Option<(usize,usize,usize)> {
        let id=self.cursor;self.cursor+=1;self.reserve_at(mesh,cpu,unchanged,0,alpha,id)
    }
    fn reserve_at(&mut self,mesh:&Mesh,cpu:Arc<SkinSurface>,unchanged:bool,instance:u64,alpha:PublicationMode,id:usize)->Option<(usize,usize,usize)> {
        let s=self;
        if id>=SURFACES {return None;}
        while s.banks.len()<=id {s.banks.push(Bank::default());}
        let slot=s.banks[id].slots.iter().enumerate().position(|(slot,g)|
            g.as_ref().is_none_or(|g|Rc::strong_count(g)==1 && (s.epoch==0 || s.banks[id].epoch[slot]!=s.epoch)))?;
        let old_capacity=s.banks[id].slots[slot].as_ref().map_or((0,0),|g|(g.vertices.capacity(),g.indices.capacity()));
        if s.vertex_capacity-old_capacity.0+old_capacity.0.max(mesh.vertices.len())>VERTICES
            || s.index_capacity-old_capacity.1+old_capacity.1.max(cpu.indices.len())>INDICES {return None;}
        let bank=&mut s.banks[id];
        let same_cpu=bank.cpu[slot].as_ref().is_some_and(|c|Arc::ptr_eq(c,&cpu));
        let geometry=bank.slots[slot].get_or_insert_with(||Rc::new(prepared::Geometry::new(Mesh {
            vertices:Vec::new(),indices:Vec::new(),texture:None })));
        let g=Rc::get_mut(geometry).unwrap();
        if g.vertices.len()!=mesh.vertices.len() {
            g.mesh.vertices.reserve_exact(mesh.vertices.len().saturating_sub(g.vertices.len()));
            g.mesh.vertices.resize(mesh.vertices.len(),Vertex::new(0.,0.,0.,0.,0.,WHITE));
        }
        // UV is the only live field untouched by the exact skin kernel. Snapshot it
        // before returning to a shared-model caller; blink/hidden edits cannot race.
        let same_uv=g.vertices.len()==mesh.vertices.len() && g.vertices.iter().zip(&mesh.vertices)
            .all(|(a,b)|a.uv.to_array().map(f32::to_bits)==b.uv.to_array().map(f32::to_bits));
        for (out,live) in g.mesh.vertices.iter_mut().zip(&mesh.vertices) {out.uv=live.uv;}

        if !same_cpu {
            g.mesh.indices.clear();g.mesh.indices.reserve_exact(cpu.indices.len());
            g.mesh.indices.extend_from_slice(&cpu.indices);
        }
        if !(unchanged && bank.instance[slot]==Some(instance) && same_cpu && same_uv && bank.mode[slot]==Some(alpha) && bank.epoch[slot]==s.epoch.saturating_sub(1)) {g.advance_owned(!same_cpu);}
        bank.instance[slot]=Some(instance);
        bank.mode[slot]=Some(alpha);bank.epoch[slot]=s.epoch;
        let output=g.mesh.vertices.as_mut_ptr() as usize;
        let new_capacity=(g.vertices.capacity(),g.indices.capacity());
        bank.cpu[slot]=Some(cpu);
        s.vertex_capacity=s.vertex_capacity-old_capacity.0+new_capacity.0;
        s.index_capacity=s.index_capacity-old_capacity.1+new_capacity.1;
        Some((id,slot,output))
    }
    /// The frame vertex budget can reject a surface after reserve_at has stored
    /// its instance token. Clear that token and keep the epoch, so this frame
    /// does not reuse the slot and the next frame cannot skip a stale upload.
    fn release_unsubmitted_token(&mut self, id: usize, slot: usize) {
        self.banks[id].instance[slot] = None;
    }
    fn frame_budget_rejects(&mut self, id: usize, slot: usize, used: usize, n: usize) -> bool {
        if used.saturating_add(n) > MAX_VERTICES {
            self.release_unsubmitted_token(id, slot);
            true
        } else { false }
    }
    /// Unsuccessful frame cleanup: a gather panic, a dropped guard or a failed collect.
    /// Every group still waiting for collect output (deferred placeholders and direct groups
    /// whose geometry arrives in complete()) is removed from its current owner, with its
    /// vertex charge and committed dropped-layer count. Before hold() that owner is FRAME.
    /// After hold() it is `held`; the remaining held groups and puffs are finished serial or
    /// world output and go back to FRAME in the order complete() would have used.
    /// After complete() this is a no-op.
    fn discard_unfilled(&mut self) {
        if self.completed { return; }
        let mut remove: Vec<usize> = self.deferred.iter().map(|d| d.group)
            .chain(self.pending.iter().map(|p| p.0)).collect();
        remove.sort_unstable();
        remove.dedup();
        let charge: usize = self.deferred.iter().map(|d| d.vertices).sum::<usize>()
            + self.pending.iter().map(|p| p.3).sum::<usize>();
        let dropped: u64 = self.deferred.iter().map(|d| d.dropped_layers).sum();
        fn strip(groups: &mut Vec<Group>, puffs: &mut [retained::world_puffs::Puff], remove: &[usize]) {
            for &index in remove.iter().rev() { groups.remove(index); }
            for puff in puffs {
                let shift = remove.iter().filter(|g| **g < puff.before_group).count();
                puff.before_group -= shift;
            }
        }
        let staged = self.staged_held;
        let (held, held_puffs) = (&mut self.held, &mut self.held_puffs);
        FRAME.with(|f| {
            let mut f = f.borrow_mut();
            if staged {
                strip(held, held_puffs, &remove);
                let world_groups = f.groups.len();
                for puff in held_puffs.iter_mut() { puff.before_group += world_groups; }
                f.groups.append(held);
                f.world_puffs.append(held_puffs);
            } else {
                let f = &mut *f;
                strip(&mut f.groups, &mut f.world_puffs, &remove);
            }
            f.vertices = f.vertices.checked_sub(charge).expect("discarded actor vertices exceed FRAME.vertices");
        });
        if dropped > 0 { retained::uncount("vita_fx_dropped_layers", dropped); }
        self.pending.clear();
        self.deferred.clear();
        self.staged_held = false;
    }
}
#[cfg(any(test, all(feature="sim_maps_probe", not(target_os="vita"))))]
/// One walked publication allocation. Payload bytes do not depend on pointer width.
/// Container headers do: host Vec is 24 B and Rc/Arc header is 16 B; Vita ABI uses 12 B and 8 B.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct MemoryAccount {
    bank_vertex_len: usize, bank_vertex_cap: usize, bank_index_len: usize, bank_index_cap: usize,
    geometry_allocs: usize, free_slots: usize, held_slots: usize,
    evicted_allocs: usize, evicted_vertex_cap: usize, evicted_index_cap: usize, evicted_payload_bytes: usize,
    lookup_len: usize, lookup_cap: usize, lookup_bytes_estimate: usize, banks_len: usize, banks_cap: usize, cpu_arcs: usize,
    pending_len: usize, pending_cap: usize, deferred_len: usize, deferred_cap: usize,
    held_len: usize, held_cap: usize, puff_len: usize, puff_cap: usize,
    held_pass_bytes: usize, held_pass_allocs: usize,
    payload_bytes: usize, container_host_bytes: usize, rc_boxes: usize,
}
#[cfg(any(test, all(feature="sim_maps_probe", not(target_os="vita"))))]
fn live_geometry(g: &Rc<prepared::Geometry>, seen: &mut HashMap<usize, ()>, into: &mut MemoryAccount, evicted: bool) {
    let ptr = Rc::as_ptr(g) as usize;
    if seen.insert(ptr, ()).is_some() { return; }
    let vc = g.vertices.capacity();
    let ic = g.indices.capacity();
    let bytes = vc * std::mem::size_of::<Vertex>() + ic * std::mem::size_of::<u16>();
    into.rc_boxes += 1;
    if evicted {
        into.evicted_allocs += 1;
        into.evicted_vertex_cap += vc;
        into.evicted_index_cap += ic;
        into.evicted_payload_bytes += bytes;
    } else {
        into.payload_bytes += bytes;
        into.geometry_allocs += 1;
        into.bank_vertex_len += g.vertices.len();
        into.bank_vertex_cap += vc;
        into.bank_index_len += g.indices.len();
        into.bank_index_cap += ic;
    }
}
#[cfg(any(test, all(feature="sim_maps_probe", not(target_os="vita"))))]
impl State {
    fn memory_account(&mut self) -> MemoryAccount {
        self.evicted_referenced.retain(|g| g.strong_count() > 0);
        let mut seen = HashMap::new();
        let mut account = MemoryAccount {
            lookup_len: self.lookup.len(), lookup_cap: self.lookup.capacity(),
            banks_len: self.banks.len(), banks_cap: self.banks.capacity(),
            pending_len: self.pending.len(), pending_cap: self.pending.capacity(),
            deferred_len: self.deferred.len(), deferred_cap: self.deferred.capacity(),
            held_len: self.held.len(), held_cap: self.held.capacity(),
            puff_len: self.held_puffs.len(), puff_cap: self.held_puffs.capacity(),
            ..MemoryAccount::zero()
        };
        account.container_host_bytes = self.banks.capacity() * std::mem::size_of::<Bank>()
            + self.pending.capacity() * std::mem::size_of::<(usize, usize, usize, usize)>()
            + self.deferred.capacity() * std::mem::size_of::<Deferred>()
            + self.held.capacity() * std::mem::size_of::<Group>()
            + self.held_puffs.capacity() * std::mem::size_of::<retained::world_puffs::Puff>();
        account.lookup_bytes_estimate = self.lookup.capacity()
            * (std::mem::size_of::<Key>() + std::mem::size_of::<usize>() + 1);
        // Held passes own the per-layer copies. A pass whose geometry Rc is Some shares the
        // bank geometry already walked above, so only its own buffers are added here.
        for group in &self.held {
            account.held_pass_bytes += group.passes.capacity() * std::mem::size_of::<Pass>();
            for pass in &group.passes {
                account.held_pass_bytes += pass.mesh.vertices.capacity() * std::mem::size_of::<Vertex>()
                    + pass.mesh.indices.capacity() * std::mem::size_of::<u16>();
                account.held_pass_allocs += usize::from(pass.mesh.vertices.capacity() > 0)
                    + usize::from(pass.mesh.indices.capacity() > 0);
            }
        }
        let mut cpu = HashMap::new();
        for bank in &self.banks {
            for slot in &bank.slots {
                match slot {
                    Some(g) => {
                        if Rc::strong_count(g) > 1 { account.held_slots += 1; }
                        live_geometry(g, &mut seen, &mut account, false);
                    }
                    None => account.free_slots += 1,
                }
            }
            for cpu_slot in bank.cpu.iter().flatten() {
                cpu.insert(Arc::as_ptr(cpu_slot) as usize, ());
            }
        }
        account.cpu_arcs = cpu.len();
        let evicted: Vec<_> = self.evicted_referenced.iter().filter_map(Weak::upgrade).collect();
        for g in &evicted { live_geometry(g, &mut seen, &mut account, true); }
        account
    }
}
#[cfg(any(test, all(feature="sim_maps_probe", not(target_os="vita"))))]
impl MemoryAccount {
    const fn zero() -> Self {
        Self {
            bank_vertex_len: 0, bank_vertex_cap: 0, bank_index_len: 0, bank_index_cap: 0,
            geometry_allocs: 0, free_slots: 0, held_slots: 0, evicted_allocs: 0, evicted_vertex_cap: 0,
            evicted_index_cap: 0, evicted_payload_bytes: 0, lookup_len: 0, lookup_cap: 0, lookup_bytes_estimate: 0,
            banks_len: 0, banks_cap: 0, cpu_arcs: 0,
            pending_len: 0, pending_cap: 0, deferred_len: 0, deferred_cap: 0, held_len: 0, held_cap: 0,
            puff_len: 0, puff_cap: 0, held_pass_bytes: 0, held_pass_allocs: 0,
            payload_bytes: 0, container_host_bytes: 0, rc_boxes: 0,
        }
    }
    /// Host heap estimate for walked publication storage. Rc header is 2 pointers.
    /// Geometry's own Vec headers sit inside `size_of::<Geometry>()`. Buffer payload is separate.
    /// Lookup buckets use one byte plus key/value per capacity slot (hashbrown layout, no SIMD
    /// group overhead). Held pass structs stay in their Vec capacity. This is not a malloc trace.
    fn host_bytes(self) -> usize {
        self.payload_bytes + self.evicted_payload_bytes + self.held_pass_bytes + self.lookup_bytes_estimate
            + self.rc_boxes * (std::mem::size_of::<usize>() * 2 + std::mem::size_of::<prepared::Geometry>())
            + self.container_host_bytes
    }
    /// Same payload and the same Rc count, with a 4-byte Vita pointer header.
    /// Container structs stay on the host figure: their padding is not retargeted.
    fn vita_abi_bytes(self) -> usize {
        self.payload_bytes + self.evicted_payload_bytes + self.held_pass_bytes + self.rc_boxes * 8
    }
}
thread_local! {
    static ACTIVE:Cell<bool>=const {Cell::new(false)};
    /// actor_deferred_layers, latched once per frame on the game thread.
    static DEFERRED:Cell<bool>=const {Cell::new(false)};
    static STATE:RefCell<State>=RefCell::default();
}
pub(crate) fn active()->bool { ACTIVE.with(Cell::get) }
#[cfg(test)]
thread_local! {
    static DEFERRED_TEST:Cell<Option<bool>>=const {Cell::new(None)};
    static DEFERRED_COMMITS:Cell<u64>=const {Cell::new(0)};
}
/// Scoped test override, restored on unwind. Worker behaviour never reads it: the
/// value is latched in begin() and only the resulting mode reaches a worker.
#[cfg(test)]
pub(crate) fn with_deferred_layers<T>(on:bool,run:impl FnOnce()->T)->T {
    struct Reset(Option<bool>);
    impl Drop for Reset { fn drop(&mut self) { DEFERRED_TEST.with(|s|s.set(self.0)); } }
    let _reset=Reset(DEFERRED_TEST.with(|s|s.replace(Some(on))));
    run()
}
/// Default off pending console acceptance. Tests: a scoped override, else
/// LOOKING_GLASS_TEST_DEFERRED_LAYERS=on runs the whole suite with it on.
pub(crate) fn deferred_layers()->bool {
    #[cfg(test)]
    { DEFERRED_TEST.with(Cell::get).unwrap_or_else(|| match std::env::var("LOOKING_GLASS_TEST_DEFERRED_LAYERS").as_deref() {
        Err(std::env::VarError::NotPresent) | Ok("off") => false,
        Ok("on") => true,
        _ => panic!("LOOKING_GLASS_TEST_DEFERRED_LAYERS must be off or on"),
    }) }
    #[cfg(not(test))]
    { crate::render::vita_renderer::config().actor_deferred_layers }
}
#[cfg(test)]
pub(crate) fn test_deferred_commits()->u64 { DEFERRED_COMMITS.with(Cell::get) }
#[cfg(test)]
pub(crate) fn test_deferred_pending()->(usize,usize,usize) {
    STATE.with(|s| {let s=s.borrow();(s.deferred.len(),s.pending.len(),s.held.len())})
}
/// Exhaust the vertex budget of the banks for the rest of this scene (tests only).
#[cfg(test)]
pub(crate) fn test_fill_vertex_capacity() { STATE.with(|s| s.borrow_mut().vertex_capacity=VERTICES); }
#[cfg(test)]
pub(crate) fn test_capacities() -> (usize, usize) {
    STATE.with(|s| { let s = s.borrow(); (s.vertex_capacity, s.index_capacity) })
}
#[cfg(test)]
pub(crate) fn test_stamp(key: Key) -> Option<(u64, PublicationMode)> {
    STATE.with(|s| {
        let s = s.borrow();
        let id = *s.lookup.get(&key)?;
        let bank = &s.banks[id];
        let slot = bank.epoch.iter().position(|epoch| *epoch == s.epoch)?;
        Some((bank.slots[slot].as_ref()?.version.vertices, bank.mode[slot]?))
    })
}
/// Hold extra references to up to `slots` existing slots of one bank, as an older
/// retained publication would. Returns the held geometry.
#[cfg(test)]
pub(crate) fn test_retain(key:Key,slots:usize)->Vec<Rc<prepared::Geometry>> {
    STATE.with(|s| {let s=s.borrow();
        s.lookup.get(&key).map_or_else(Vec::new,|&id|s.banks[id].slots.iter().flatten().take(slots).cloned().collect())})
}
/// Scene::from_parts covers every map load, including same-map save restoration.
/// The overlap owner checks Idle first: no worker can retain a destination here.
pub(crate) fn reset() {
    assert!(!active(),"actor publication reset during a frame");
    STATE.with(|s|s.borrow_mut().clear());
}

pub(crate) fn begin() {
    let c=crate::render::vita_renderer::config();
    let enabled=c.actor_single_prepare_active() && retained_active();
    ACTIVE.with(|a|a.set(enabled));
    DEFERRED.with(|d|d.set(enabled && deferred_layers()));
    if enabled { STATE.with(|s| {let mut s=s.borrow_mut();s.next_frame();}); }
}
pub(crate) fn end() {
    DEFERRED.with(|d|d.set(false));
    if ACTIVE.with(|a|a.replace(false)) {
        STATE.with(|s| {
            let mut s=s.borrow_mut();
            s.discard_unfilled();
            s.pending.clear(); s.deferred.clear(); s.held.clear(); s.held_puffs.clear(); s.staged_held=false;
        });
    }
}
/// This address is valid only between prepare and the fully drained collect. Banks
/// cannot be mutated again while any old packet/view still owns their publication.
pub(crate) fn prepare(mesh:&Mesh,cpu:Arc<SkinSurface>,unchanged:bool,instance:u64,key:Key)->Option<(usize,PublicationMode)> {
    let texture=mesh.texture.as_ref()?;
    #[cfg(test)]
    let surface=super::test_support::surface(texture);
    #[cfg(not(test))]
    let surface=None;
    let surface=match surface {Some(s)=>s,None=>SURFACE_LOOKUP.with(|s|s.borrow().get(&texture.raw_miniquad_id()).and_then(Weak::upgrade))?};
    note_eligibility(&surface);
    if SKIN_OPACITY.with(Cell::get)!=1. {return None;}
    let (time,appearance)=FRAME.with(|f| {let f=f.borrow();(f.time,f.appearance)});
    let Some((passes,alpha))=actor_cache::recipe(&surface,time,appearance) else {
        // recipe() has no side effect when it refuses; the serial fallback stays exact.
        return if DEFERRED.with(Cell::get) {prepare_deferred(mesh,cpu,unchanged,instance,key,surface,time,appearance)} else {None};
    };
    let mode=PublicationMode::recipe(alpha);
    let mut passes=Some(passes);
    let result=STATE.with(|s| {
        let mut s=s.borrow_mut();
        let (id,slot,output)=s.reserve_key(mesh,cpu,unchanged,instance,mode,key)?;
        let n=mesh.vertices.len()*passes.as_ref().unwrap().len();
        let used=FRAME.with(|f|f.borrow().vertices);
        if s.frame_budget_rejects(id,slot,used,n) {
            FRAME.with(|f|f.borrow_mut().dropped+=1);
            return None;
        }
        let group=FRAME.with(|f| {
            let mut f=f.borrow_mut();
            f.vertices+=n;let id=f.groups.len();
            let transparent=surface.transparent||f.depth_read_only;
            f.groups.push(Group {passes:passes.take().unwrap(),transparent});Some(id)
        })?;
        s.pending.push((group,id,slot,n));
        Some((output,mode))
    });
    // Full/held banks and packet limits must not discard a reusable recipe.
    if let Some(mut unused)=passes {actor_cache::recycle(&mut unused);}
    result
}
/// Eligible for deferral: a positive layer cap, at least one of the first `limit` layers
/// non-simple, and entity opacity 1. Non-simple layers with mixed vertex alpha are accepted:
/// raw output feeds the unchanged per-layer alpha code. Simple mixed-alpha stays serial.
pub(super) fn deferred_accepts(surface:&Surface,limit:usize)->bool {
    limit>0 && !surface.layers.is_empty()
        && surface.layers.iter().take(limit).any(|l|!actor_cache::simple(&l.stage))
        && SKIN_OPACITY.with(Cell::get)==1.
}
/// The serial fallback in skin_timed_produced, with clock=None and alpha=1, would build
/// `limit` passes of `mesh.vertices.len()` vertices each and submit them as one group. Here
/// the same group position, budget and dropped-layer count are committed now, and the passes
/// are built after collect from the worker's raw bytes. Every refusal returns before any
/// FRAME or counter mutation, so the serial fallback then counts exactly once.
#[allow(clippy::too_many_arguments)]
fn prepare_deferred(mesh:&Mesh,cpu:Arc<SkinSurface>,unchanged:bool,instance:u64,key:Key,surface:Rc<Surface>,time:f32,appearance:Appearance)->Option<(usize,PublicationMode)> {
    let c=crate::render::vita_renderer::config();
    let cap=super::actor_max_layers(c);
    let limit=cap.min(surface.layers.len());
    if !deferred_accepts(&surface,limit) {return None;}
    // The fallback's `fast` must hold, so pass_at takes its material-free branch, which
    // has no error path: every committed placeholder receives exactly `limit` passes.
    if !(retained_active() && c.actor_fast_path) {return None;}
    let dropped_layers=surface.layers.len().saturating_sub(cap);
    let n=mesh.vertices.len().checked_mul(limit)?;
    let (used,camera)=FRAME.with(|f| {let f=f.borrow();(f.vertices,f.camera)});
    // submit(): f.vertices.saturating_add(count) > MAX_VERTICES drops the group.
    if used.saturating_add(n)>MAX_VERTICES {return None;}
    STATE.with(|s| {
        let mut s=s.borrow_mut();
        let (bank,slot,output)=s.reserve_key(mesh,cpu,unchanged,instance,PublicationMode::Raw,key)?;
        let group=FRAME.with(|f| {
            let mut f=f.borrow_mut();
            debug_assert_eq!(f.vertices,used);
            f.vertices+=n;let group=f.groups.len();
            let transparent=surface.transparent||f.depth_read_only;
            f.groups.push(Group {passes:Vec::new(),transparent});group
        });
        s.deferred.push(Deferred {group,bank,slot,surface,camera,time,appearance,limit,vertices:n,dropped_layers:dropped_layers as u64});
        // The placeholder is committed. Refusal returned before this and left the
        // serial fallback to count the same capped layers once.
        if dropped_layers>0 {retained::count("vita_fx_dropped_layers",dropped_layers as u64);}
        #[cfg(test)]
        DEFERRED_COMMITS.with(|c|c.set(c.get()+1));
        Some((output,PublicationMode::Raw))
    })
}
/// Separate staging preserves world/actor and equal-depth packet barriers and ties.
pub(crate) fn hold() {
    STATE.with(|s| {let mut s=s.borrow_mut();FRAME.with(|f| {let mut f=f.borrow_mut();
        std::mem::swap(&mut s.held,&mut f.groups);std::mem::swap(&mut s.held_puffs,&mut f.world_puffs);
    }); s.staged_held=true; });
}
pub(crate) fn complete() {
    STATE.with(|s| {
        let mut s=s.borrow_mut();
        // All workers drained successfully. Build deferred layers before FRAME's mutable
        // borrow below: pass_at reads FRAME through retained_active().
        if !s.deferred.is_empty() {
            assert!(retained_active(),"deferred actor layers outside the direct view");
            // Records stay in `s.deferred` until every placeholder is filled, so a panic here
            // still lets end() find and discard all of this frame's deferred groups.
            #[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
            let (mut copy_bytes, mut copy_cap, mut copy_allocs) = (0u64, 0u64, 0u64);
            for n in 0..s.deferred.len() {
                let d=&s.deferred[n];
                let (group,limit,surface,camera,time,appearance)=(d.group,d.limit,d.surface.clone(),d.camera,d.time,d.appearance);
                // A read-only view of the published slot; passes own their copies.
                let geometry=s.banks[d.bank].slots[d.slot].as_ref().unwrap().clone();
                let passes=super::layer_passes(&geometry.vertices,&geometry.indices,&surface,camera,time,1.,appearance,true,None,limit);
                assert_eq!(passes.len(),limit,"deferred layer placeholder without its passes");
                #[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
                {
                    copy_bytes+=super::layer_copy_bytes(geometry.vertices.len(),geometry.indices.len(),limit);
                    for pass in &passes {
                        copy_cap+= (pass.mesh.vertices.capacity()*std::mem::size_of::<Vertex>()
                            + pass.mesh.indices.capacity()*std::mem::size_of::<u16>()) as u64;
                        copy_allocs+=u64::from(pass.mesh.vertices.capacity()>0)+u64::from(pass.mesh.indices.capacity()>0);
                    }
                }
                let placeholder=&mut s.held[group];
                debug_assert!(placeholder.passes.is_empty());
                placeholder.passes=passes;
            }
            #[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
            {
                crate::sim_maps::count("actor_deferred_surfaces",s.deferred.len() as u64);
                crate::sim_maps::count("actor_deferred_capacity_bytes",(s.deferred.capacity()*std::mem::size_of::<Deferred>()) as u64);
                crate::sim_maps::count("actor_deferred_layer_copy_bytes",copy_bytes);
                crate::sim_maps::count("actor_deferred_layer_copy_capacity_bytes",copy_cap);
                crate::sim_maps::count("actor_deferred_layer_copy_allocs",copy_allocs);
                crate::sim_maps::count("actor_layer_copy_bytes",copy_bytes);
                crate::sim_maps::count("actor_layer_copy_capacity_bytes",copy_cap);
                crate::sim_maps::count("actor_layer_copy_allocs",copy_allocs);
            }
            s.deferred.clear();
        }
        #[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
        {
            let account=s.memory_account();
            crate::sim_maps::count("actor_publication_bank_bytes",(s.vertex_capacity*std::mem::size_of::<Vertex>()+s.index_capacity*2) as u64);
            crate::sim_maps::count("actor_publication_bank_count",s.lookup.len() as u64);
            emit_memory(&account);
        }
        for n in 0..s.pending.len() {
            let (group,bank,slot,_)=s.pending[n];
            let geometry=s.banks[bank].slots[slot].as_ref().unwrap().clone();
            for pass in &mut s.held[group].passes {pass.geometry=Some(geometry.clone());}
        }
        #[cfg(feature="actor_diagnostics")]
        retained::count("actor_publication_capacity_bytes",(s.vertex_capacity*std::mem::size_of::<Vertex>()+s.index_capacity*2) as u64);
        #[cfg(feature="actor_diagnostics")]
        retained::count("actor_publication_banks",s.lookup.len() as u64);
        FRAME.with(|f| {let mut f=f.borrow_mut();let world_groups=f.groups.len();
            for puff in &mut s.held_puffs {puff.before_group+=world_groups;}
            f.groups.append(&mut s.held);f.world_puffs.append(&mut s.held_puffs);
        });
        s.completed=true;
    });
}

fn note_eligibility(surface: &Rc<Surface>) {
    #[cfg(all(feature="sim_maps_probe", not(target_os="vita")))]
    {
        let id = Rc::as_ptr(surface) as usize;
        ELIGIBILITY.with(|found| {
            let mut found = found.borrow_mut();
            if !found.contains_key(&id) { found.insert(id, eligibility_record(surface)); }
        });
    }
    #[cfg(not(all(feature="sim_maps_probe", not(target_os="vita"))))]
    { let _ = surface; }
}
#[cfg(any(test, all(feature="sim_maps_probe", not(target_os="vita"))))]
/// Stage flags and the uniform/mixed class for one actor surface. Host inventory and tests
/// both use this; it does not allocate a bank or submit a layer.
pub(crate) fn eligibility_record(surface: &Surface) -> serde_json::Value {
    let limit = super::actor_max_layers(crate::render::vita_renderer::config()).min(surface.layers.len());
    let layers: Vec<_> = surface.layers.iter().map(|layer| {
        let stage = &layer.stage;
        serde_json::json!({
            "environment": stage.environment,
            "vector_uv": stage.vector_uv.is_some(),
            "dot_alpha": stage.dot_alpha.is_some(),
            "mods": !stage.mods.is_empty(),
            "vertex_alpha": stage.vertex_alpha,
        })
    }).collect();
    let accepts = deferred_accepts(surface, limit);
    let considered: Vec<bool> = surface.layers.iter().take(limit).map(|layer| layer.stage.vertex_alpha).collect();
    let classification = if considered.is_empty() { "empty" }
        else if considered.iter().all(|alpha| *alpha == considered[0]) { "uniform" }
        else { "mixed" };
    let reason = if surface.layers.is_empty() { "empty" }
        else if limit == 0 { "cap" }
        else if SKIN_OPACITY.with(Cell::get) != 1. { "opacity" }
        else if !surface.layers.iter().take(limit).any(|layer| !actor_cache::simple(&layer.stage)) { "simple" }
        else if accepts { "deferred" }
        else { "refused" };
    serde_json::json!({
        "layers": layers, "limit": limit, "classification": classification,
        "deferred_accepts": accepts, "reason": reason,
        "environment": surface.layers.iter().any(|layer| layer.stage.environment),
    })
}
#[cfg(all(feature="sim_maps_probe", not(target_os="vita")))]
thread_local! {
    static ELIGIBILITY: RefCell<std::collections::BTreeMap<usize, serde_json::Value>> = RefCell::default();
}
#[cfg(all(feature="sim_maps_probe", not(target_os="vita")))]
pub(crate) fn emit_eligibility(map: &str) {
    ELIGIBILITY.with(|found| {
        let found = found.borrow();
        let mut surfaces: Vec<_> = found.iter().map(|(id, record)| {
            let mut record = record.clone();
            record["surface"] = serde_json::json!(id);
            record
        }).collect();
        surfaces.sort_by_key(|record| record["surface"].as_u64().unwrap_or(0));
        let environment_actors = surfaces.iter().filter(|record| record["environment"] == true).count();
        println!("ACTOR_LAYER_ELIGIBILITY {}", serde_json::json!({
            "map": map, "surfaces": surfaces, "environment_actors": environment_actors,
        }));
    });
}
#[cfg(all(feature="sim_maps_probe", not(target_os="vita")))]
fn emit_memory(account: &MemoryAccount) {
    let host = account.host_bytes();
    let vita = account.vita_abi_bytes();
    let rows = [
        ("actor_mem_payload_bytes", account.payload_bytes),
        ("actor_mem_evicted_payload_bytes", account.evicted_payload_bytes),
        ("actor_mem_held_pass_bytes", account.held_pass_bytes),
        ("actor_mem_held_pass_allocs", account.held_pass_allocs),
        ("actor_mem_lookup_bytes_estimate", account.lookup_bytes_estimate),
        ("actor_mem_container_host_bytes", account.container_host_bytes),
        ("actor_mem_host_bytes", host),
        ("actor_mem_vita_abi_bytes", vita),
        ("actor_mem_geometry_allocs", account.geometry_allocs),
        ("actor_mem_evicted_allocs", account.evicted_allocs),
        ("actor_mem_evicted_vertex_cap", account.evicted_vertex_cap),
        ("actor_mem_evicted_index_cap", account.evicted_index_cap),
        ("actor_mem_free_slots", account.free_slots),
        ("actor_mem_held_slots", account.held_slots),
        ("actor_mem_bank_vertex_len", account.bank_vertex_len),
        ("actor_mem_bank_vertex_cap", account.bank_vertex_cap),
        ("actor_mem_bank_index_len", account.bank_index_len),
        ("actor_mem_bank_index_cap", account.bank_index_cap),
        ("actor_mem_lookup_len", account.lookup_len),
        ("actor_mem_lookup_cap", account.lookup_cap),
        ("actor_mem_banks_len", account.banks_len),
        ("actor_mem_banks_cap", account.banks_cap),
        ("actor_mem_cpu_arcs", account.cpu_arcs),
        ("actor_mem_pending_len", account.pending_len),
        ("actor_mem_pending_cap", account.pending_cap),
        ("actor_mem_deferred_len", account.deferred_len),
        ("actor_mem_deferred_cap", account.deferred_cap),
        ("actor_mem_held_len", account.held_len),
        ("actor_mem_held_cap", account.held_cap),
        ("actor_mem_puff_len", account.puff_len),
        ("actor_mem_puff_cap", account.puff_cap),
        ("actor_mem_rc_boxes", account.rc_boxes),
        ("actor_mem_sizeof_vertex", std::mem::size_of::<Vertex>()),
        ("actor_mem_sizeof_geometry", std::mem::size_of::<prepared::Geometry>()),
        ("actor_mem_sizeof_bank", std::mem::size_of::<Bank>()),
        ("actor_mem_sizeof_deferred", std::mem::size_of::<Deferred>()),
        ("actor_mem_worst_bank_payload_bytes", VERTICES * std::mem::size_of::<Vertex>() + INDICES * 2),
    ];
    for (name, value) in rows { crate::sim_maps::count(name, value as u64); }
}

#[cfg(test)]
mod tests {
    use super::*;
    const O:PublicationMode=PublicationMode::Opaque255;
    fn fixture()->(Mesh,Arc<SkinSurface>) {
        let surface=crate::skeletal::Surface::new("body".into(),
            [Vec3::ZERO,Vec3::X,Vec3::Y].into_iter().map(|offset|crate::skeletal::SkinVertex {
                uv:Vec2::ONE,weights:vec![crate::skeletal::Weight {bone:0,amount:1.,offset}]}).collect(),vec![0,1,2]);
        (Mesh {vertices:vec![Vertex::new(0.,0.,0.,1.,1.,WHITE);3],indices:vec![0,1,2],texture:None},surface.cpu())
    }
    fn key(actor:ActorId,cpu:&Arc<SkinSurface>)->Key { Key {actor,surface:Arc::as_ptr(cpu) as usize} }
    #[test]
    fn npc_lod_visibility_spawn_and_despawn_keep_later_actor_banks_and_upload_revisions() {
        let (mesh,cpu)=fixture();let mut state=State::default();
        let actors:[ActorId;4]=std::array::from_fn(|_|ActorId::default());
        let mut later=None;
        // Exact, Hidden, Reuse, reappearing, spawn, despawn: first NPC's path
        // changes visitation, but the later NPC's immutable upload stamp stays warm.
        for first in [true,false,false,true,true,false] {
            state.next_frame();
            if first {state.reserve_key(&mesh,cpu.clone(),true,0,O,key(actors[0],&cpu)).unwrap();}
            if state.epoch==5 {state.reserve_key(&mesh,cpu.clone(),false,0,O,key(actors[2],&cpu)).unwrap();}
            let (id,slot,ptr)=state.reserve_key(&mesh,cpu.clone(),true,0,O,key(actors[1],&cpu)).unwrap();
            let stamp=state.banks[id].slots[slot].as_ref().unwrap().version;
            if let Some((old_id,old_ptr,old_stamp))=later {assert_eq!((id,ptr,stamp),(old_id,old_ptr,old_stamp));}
            later=Some((id,ptr,stamp));
        }
        assert_eq!(state.lookup.len(),3);
    }
    #[test]
    fn unchanged_shared_instance_moving_to_another_slot_advances_gpu_revision() {
        let (mesh,cpu)=fixture();let mut state=State::default();state.next_frame();
        let key=key(ActorId::default(),&cpu);
        let (id,a,_)=state.reserve_key(&mesh,cpu.clone(),false,11,O,key).unwrap();
        let (_,b,_)=state.reserve_key(&mesh,cpu.clone(),false,22,O,key).unwrap();
        assert_ne!(a,b);
        let old_a=state.banks[id].slots[a].as_ref().unwrap().version;
        // The first instance disappears. Instance 22 is bit-static but now gets
        // slot A, whose last GPU upload belonged to instance 11.
        state.next_frame();
        let (_,slot,_)=state.reserve_key(&mesh,cpu.clone(),true,22,O,key).unwrap();
        assert_eq!(slot,a);
        let moved=state.banks[id].slots[slot].as_ref().unwrap().version;
        assert_ne!(moved.vertices,old_a.vertices,"different instance retained a stale GPU stamp");
        state.next_frame();
        state.reserve_key(&mesh,cpu,true,22,O,key).unwrap();
        assert_eq!(state.banks[id].slots[a].as_ref().unwrap().version,moved,
            "the same unchanged instance must retain its warm upload");
    }
    #[test]
    fn repeated_actor_surface_in_one_frame_uses_disjoint_destinations() {
        let (mesh,cpu)=fixture();let mut state=State::default();state.next_frame();
        let key=key(ActorId::default(),&cpu);
        let a=state.reserve_key(&mesh,cpu.clone(),false,0,O,key).unwrap();
        let b=state.reserve_key(&mesh,cpu.clone(),false,0,O,key).unwrap();
        assert_ne!(a.2,b.2);assert!(state.reserve_key(&mesh,cpu,false,0,O,key).is_none());
    }
    #[test]
    fn unused_actor_ids_evict_at_age_and_under_capacity_pressure_without_touching_held_views() {
        let (mesh,cpu)=fixture();let mut state=State::default();state.next_frame();
        let old_key=key(ActorId::default(),&cpu);
        let (id,slot,_)=state.reserve_key(&mesh,cpu.clone(),false,0,O,old_key).unwrap();
        let held=state.banks[id].slots[slot].as_ref().unwrap().clone();
        for _ in 0..=UNUSED_FRAMES {state.next_frame();}
        assert!(state.lookup.is_empty());assert_eq!(state.vertex_capacity,0);
        assert_eq!(held.indices,[0,1,2]);assert_eq!(Arc::strong_count(&cpu),1);
        // Slot-count pressure reuses an unused id, never grows the bounded table.
        for _ in 0..SURFACES {state.reserve_key(&mesh,cpu.clone(),false,0,O,key(ActorId::default(),&cpu)).unwrap();}
        assert!(state.reserve_key(&mesh,cpu.clone(),false,0,O,key(ActorId::default(),&cpu)).is_none());
        state.next_frame();state.next_frame();
        assert!(state.reserve_key(&mesh,cpu,false,0,O,Key {actor:ActorId::default(),surface:0}).is_some());
        assert_eq!(state.lookup.len(),SURFACES);assert_eq!(state.banks.len(),SURFACES);
    }
    #[test]
    fn map_changes_and_same_map_reload_release_skins_and_restore_the_warm_budget() {
        let mut state=State::default();let mut old_cpu:Option<std::sync::Weak<SkinSurface>>=None;
        let mut held=None;let mut warm=None;
        for _map_or_reload in 0..12 {
            state.clear();
            assert_eq!((state.vertex_capacity,state.index_capacity,state.lookup.len(),state.banks.len()),(0,0,0,0));
            if let Some(old)=old_cpu.take() {assert!(old.upgrade().is_none(),"old map skin pinned");}
            // Previously submitted immutable geometry may outlive the scene reset.
            if let Some(old)=held.take() {let old:Rc<prepared::Geometry>=old;assert_eq!(old.indices,[0,1,2]);}
            let source=crate::skeletal::Surface::new("body".into(),(0..4096).map(|_|crate::skeletal::SkinVertex {
                uv:Vec2::ONE,weights:vec![crate::skeletal::Weight {bone:0,amount:1.,offset:Vec3::ZERO}]}).collect(),vec![0,1,2]);
            let cpu=source.cpu();let mesh=Mesh {vertices:cpu.bind_vertices.clone(),indices:cpu.indices.clone(),texture:None};
            state.next_frame();
            for _ in 0..5 {assert!(state.reserve_key(&mesh,cpu.clone(),false,0,O,key(ActorId::default(),&cpu)).is_some(),"unexpected serial fallback after map/reload");}
            let capacity=(state.vertex_capacity,state.index_capacity,state.lookup.len());
            if let Some(warm)=warm {assert_eq!(capacity,warm);} else {warm=Some(capacity);}
            held=state.banks[0].slots[0].clone();old_cpu=Some(Arc::downgrade(&cpu));
        }
        state.clear();assert!(old_cpu.unwrap().upgrade().is_none());
    }
    #[test]
    fn held_views_exhaust_two_banks_and_never_overwrite_old_vertices() {
        let (mut mesh,cpu)=fixture();let mut state=State::default();
        let (id,slot,ptr)=state.reserve(&mesh,cpu.clone(),false,O).unwrap();
        let old=state.banks[id].slots[slot].as_ref().unwrap().clone();
        assert_eq!(ptr,old.vertices.as_ptr() as usize);
        mesh.vertices[0].uv.x=-0.;state.cursor=0;
        let (id,slot,_)=state.reserve(&mesh,cpu.clone(),false,O).unwrap();
        let second=state.banks[id].slots[slot].as_ref().unwrap().clone();
        assert_eq!(old.vertices[0].uv.x.to_bits(),1_f32.to_bits());
        assert_eq!(second.vertices[0].uv.x.to_bits(),(-0_f32).to_bits());
        state.cursor=0;assert!(state.reserve(&mesh,cpu.clone(),false,O).is_none());
        drop(old);state.cursor=0;assert!(state.reserve(&mesh,cpu,false,O).is_some());
        assert_eq!(state.vertex_capacity,6);assert_eq!(state.index_capacity,6);
    }
    #[test]
    fn reclaimed_old_view_cannot_reuse_a_newer_inputs_revision() {
        let (mesh,cpu)=fixture();let mut state=State::default();state.epoch=1;
        state.reserve_at(&mesh,cpu.clone(),false,0,O,0).unwrap();
        let old=state.banks[0].slots[0].as_ref().unwrap().clone();let version=old.version;
        state.epoch=2;state.reserve_at(&mesh,cpu.clone(),false,0,O,0).unwrap();
        let second=state.banks[0].slots[1].as_ref().unwrap().clone();
        drop(old);state.epoch=3;
        state.reserve_at(&mesh,cpu,true,0,O,0).unwrap();
        assert_ne!(state.banks[0].slots[0].as_ref().unwrap().version.vertices,version.vertices);
        assert_eq!(second.vertices[0].uv,Vec2::ONE);
    }
    #[test]
    fn cold_and_evicted_bank_transition_allocations_have_fixed_bounds() {
        let (mesh,cpu)=fixture();let mut state=State::default();state.next_frame();
        let key=key(ActorId::default(),&cpu);
        let cold=crate::kernel_alloc::Scope::begin();
        state.reserve_key(&mesh,cpu.clone(),false,1,O,key).unwrap();
        let cold=cold.finish();assert!(cold<=5,"cold metadata plus one slot: {cold}");
        let warm=crate::kernel_alloc::Scope::begin();
        for _ in 0..120 {state.next_frame();state.reserve_key(&mesh,cpu.clone(),true,1,O,key).unwrap();}
        assert_eq!(warm.finish(),0);
        for _ in 0..=UNUSED_FRAMES {state.next_frame();}
        let reentry=crate::kernel_alloc::Scope::begin();
        state.reserve_key(&mesh,cpu,false,2,O,key).unwrap();
        let reentry=reentry.finish();assert!(reentry<=3,"one replacement slot: {reentry}");
        println!("publication transition allocations: cold={cold}, warm=0, reentry={reentry}; max vertices={VERTICES}, indices={INDICES}, banks={SURFACES}, bank_bytes={}",std::mem::size_of::<Bank>());
    }
    #[test]
    fn warmed_storage_is_bounded_and_map_replacement_copies_new_topology() {
        let (mesh,cpu)=fixture();let mut state=State::default();
        state.reserve(&mesh,cpu.clone(),false,O).unwrap();
        let scope=crate::kernel_alloc::Scope::begin();
        for _ in 0..128 {state.cursor=0;assert!(state.reserve(&mesh,cpu.clone(),false,O).is_some());}
        assert_eq!(scope.finish(),0);assert_eq!(state.banks.len(),1);
        state.cursor=0;let version=state.banks[0].slots[0].as_ref().unwrap().version;
        state.reserve(&mesh,cpu.clone(),true,O).unwrap();
        assert_eq!(state.banks[0].slots[0].as_ref().unwrap().version,version);
        let (_,new_cpu)=fixture();state.cursor=0;
        let before=state.banks[0].slots[0].as_ref().unwrap().version;
        state.reserve(&mesh,new_cpu,false,O).unwrap();
        assert_ne!(state.banks[0].slots[0].as_ref().unwrap().version.topology,before.topology);
        state.cursor=SURFACES;assert!(state.reserve(&mesh,cpu.clone(),false,O).is_none());
        state.cursor=0;state.vertex_capacity=VERTICES;
        let mut larger=mesh;larger.vertices.push(Vertex::new(0.,0.,0.,0.,0.,WHITE));
        assert!(state.reserve(&larger,cpu,false,O).is_none());
    }
    #[test]
    fn identical_transform_token_dropped_by_the_frame_vertex_limit_cannot_skip_a_stale_slot() {
        let (mesh,cpu)=fixture();let mut state=State::default();state.next_frame();
        let key=key(ActorId::default(),&cpu);
        // Previous frame: token 11 owns slot 0 and its GPU revision.
        let (id,slot0,_)=state.reserve_key(&mesh,cpu.clone(),false,11,O,key).unwrap();
        let stale=state.banks[id].slots[slot0].as_ref().unwrap().version;
        state.next_frame();
        // First identical-transform instance claims token 11 and would skip.
        // The frame vertex limit then drops the reserved surface.
        let (id,slot,_)=state.reserve_key(&mesh,cpu.clone(),true,11,O,key).unwrap();
        assert_eq!(slot,slot0);
        assert_eq!(state.banks[id].slots[slot].as_ref().unwrap().version,stale);
        assert!(state.frame_budget_rejects(id,slot,MAX_VERTICES,1),"one more vertex overflows the frame");
        assert!(state.banks[id].instance[slot].is_none());
        assert_eq!(state.banks[id].epoch[slot],state.epoch,"the dropped slot stays reserved for this frame");
        // The dropped draw was not recorded, so the second instance claims token 11 again.
        let (_,other,_)=state.reserve_key(&mesh,cpu.clone(),false,11,O,key).unwrap();
        assert_ne!(other,slot0);
        state.next_frame();
        let (_,slot,_)=state.reserve_key(&mesh,cpu.clone(),true,11,O,key).unwrap();
        assert_eq!(slot,slot0);
        assert_ne!(state.banks[id].slots[slot].as_ref().unwrap().version.vertices,stale.vertices,
            "the shared token must upload over the other instance's slot");
    }
    #[test]
    fn deferred_layers_accessor_follows_scoped_override_and_restores_on_unwind() {
        let ambient=deferred_layers();
        assert_eq!(ambient,std::env::var("LOOKING_GLASS_TEST_DEFERRED_LAYERS").as_deref()==Ok("on"));
        for on in [false,true] {
            assert_eq!(with_deferred_layers(on,deferred_layers),on);
            assert_eq!(with_deferred_layers(on,||with_deferred_layers(!on,deferred_layers)),!on);
            assert_eq!(with_deferred_layers(on,deferred_layers),on);
            let failed=std::panic::catch_unwind(||with_deferred_layers(on,||panic!("inside override")));
            assert!(failed.is_err());assert_eq!(deferred_layers(),ambient,"override must restore on unwind");
        }
    }
    fn test_surface(stages:Vec<Stage>)->(Texture2D,Rc<Surface>) {
        let base=super::super::test_support::texture(77);
        let layers=stages.into_iter().map(|s|(Stage {images:vec!["a".into()],..s},vec![super::super::test_support::texture(78)])).collect();
        (base.clone(),super::super::test_support::register(&base,layers,false))
    }
    #[test]
    fn deferral_needs_a_non_simple_layer_within_the_cap_positive_cap_and_full_opacity() {
        let env=Stage {environment:true,..Default::default()};
        let (_,simple_only)=test_surface(vec![Stage::default(),Stage {vertex_alpha:true,..Default::default()}]);
        let (_,late)=test_surface(vec![Stage::default(),env.clone()]);
        let (_,early)=test_surface(vec![env.clone(),Stage::default()]);
        assert!(!deferred_accepts(&simple_only,2),"simple mixed alpha stays serial");
        assert!(deferred_accepts(&late,2) && !deferred_accepts(&late,1));
        assert!(deferred_accepts(&early,1) && !deferred_accepts(&early,0));
        super::super::with_skin_opacity(0.5,||assert!(!deferred_accepts(&early,2)));
        super::super::test_support::clear();
    }
    #[test]
    fn deferred_refusal_mutates_no_frame_counter_and_the_serial_fallback_counts_once() {
        use super::super::test_support as ts;
        let three=vec![Stage {environment:true,..Default::default()},Stage::default(),Stage::default()];
        let (base,surface)=test_surface(three);
        let (mesh,cpu)=fixture();let mesh=Mesh {texture:Some(base),..mesh};
        let key=key(ActorId::default(),&cpu);
        with_deferred_layers(true,|| {
            for refusal in 0..3 {
                reset();ts::begin_direct(Vec3::ZERO,Vec3::Y,1.,Appearance::default());retained::host_reset();
                begin();assert!(active());
                match refusal {
                    0=>ts::set_frame_vertices(ts::MAX-5),     // budget: 3 vertices x 2 layers
                    1=>STATE.with(|s|s.borrow_mut().vertex_capacity=VERTICES), // bank capacity
                    _=>{assert!(prepare(&mesh,cpu.clone(),false,1,key).is_some());assert!(prepare(&mesh,cpu.clone(),false,2,key).is_some());} // both slots used
                }
                let before=ts::snapshot();
                assert!(prepare(&mesh,cpu.clone(),false,3,key).is_none(),"refusal {refusal}");
                assert_eq!(ts::snapshot(),before,"a refused deferral changed FRAME or counters");
                // The serial fallback then counts the capped layer and any budget drop once.
                super::super::skin(&mesh);
                let after=ts::snapshot();
                assert_eq!(after.dropped_layers,before.dropped_layers+1);
                assert_eq!(after.dropped,before.dropped+usize::from(refusal==0));
                end();ts::end_frame();
            }
            reset();
        });
        drop(surface);ts::clear();
    }
    #[test]
    fn deferred_commit_counts_capped_layers_once_and_reserves_the_fallback_group() {
        use super::super::test_support as ts;
        let (base,surface)=test_surface(vec![Stage {dot_alpha:Some(Vec2::ONE),..Default::default()},Stage::default(),Stage::default()]);
        let (mesh,cpu)=fixture();let mesh=Mesh {texture:Some(base),..mesh};
        let [off,on]=[false,true].map(|on| with_deferred_layers(on,|| {
            reset();ts::begin_direct(Vec3::ZERO,Vec3::Y,1.,Appearance::default());retained::host_reset();begin();
            let direct=prepare(&mesh,cpu.clone(),false,1,key(ActorId::default(),&cpu));
            if direct.is_none() {super::super::skin(&mesh);}
            let snapshot=ts::snapshot();
            end();
            let cleaned=ts::snapshot();
            ts::end_frame();reset();
            (direct.map(|d|d.1),snapshot,cleaned)
        }));
        assert_eq!(off.0,None);assert_eq!(on.0,Some(PublicationMode::Raw));
        assert_eq!((on.1.vertices,on.1.dropped,on.1.dropped_layers),(off.1.vertices,off.1.dropped,off.1.dropped_layers));
        assert_eq!((on.1.groups.len(),off.1.groups.len()),(1,1));
        assert!(on.1.groups[0].1.is_empty(),"placeholder until complete or failed cleanup");
        assert!(on.2.groups.is_empty(),"failed cleanup left the unfilled placeholder");
        assert_eq!((on.2.vertices,on.2.dropped_layers),(0,0));
        assert!(!off.2.groups.is_empty(),"serial output must survive the same guard drop");
        drop(surface);ts::clear();
    }
    #[test]
    fn memory_account_counts_each_rc_once_and_keeps_evicted_geometry_while_referenced() {
        let (mesh,cpu)=fixture();
        let mut state=State::default();
        state.next_frame();
        let key=key(ActorId::default(),&cpu);
        let (id,slot,_)=state.reserve_key(&mesh,cpu.clone(),false,0,O,key).unwrap();
        let held=state.banks[id].slots[slot].as_ref().unwrap().clone();
        let account=state.memory_account();
        assert_eq!(account.geometry_allocs,1);
        assert_eq!(account.rc_boxes,1);
        assert_eq!((account.bank_vertex_len,account.bank_index_len),(3,3));
        assert!(account.bank_vertex_cap>=account.bank_vertex_len);
        assert!(account.bank_index_cap>=account.bank_index_len);
        assert_eq!(account.cpu_arcs,1);
        assert_eq!(account.held_slots,1);
        assert!(account.payload_bytes>=3*std::mem::size_of::<Vertex>()+6);
        assert!(account.host_bytes()>account.payload_bytes);
        assert_eq!(account.vita_abi_bytes(),account.payload_bytes+8);
        for _ in 0..=UNUSED_FRAMES {state.next_frame();}
        let evicted=state.memory_account();
        assert_eq!(evicted.geometry_allocs,0);
        assert_eq!(evicted.evicted_allocs,1);
        assert_eq!(evicted.rc_boxes,1,"the surviving allocation is counted once");
        assert_eq!(evicted.payload_bytes,0);
        assert_eq!(evicted.bank_vertex_cap,0,"evicted storage is not bank-live");
        assert!(evicted.evicted_payload_bytes>=3*std::mem::size_of::<Vertex>()+6);
        assert_eq!(evicted.evicted_payload_bytes,evicted.evicted_vertex_cap*std::mem::size_of::<Vertex>()
            +evicted.evicted_index_cap*std::mem::size_of::<u16>());
        assert_eq!(held.indices,[0,1,2]);
        drop(held);
        let gone=state.memory_account();
        assert_eq!((gone.evicted_allocs,gone.rc_boxes,gone.payload_bytes),(0,0,0));
    }
    #[test]
    fn eligibility_record_names_stage_flags_and_uniform_or_mixed_alpha() {
        let env=Stage {environment:true,vertex_alpha:true,..Default::default()};
        let dot=Stage {dot_alpha:Some(Vec2::ONE),mods:vec![crate::materials::TcMod::Rotate(1.)],..Default::default()};
        let (_,mixed)=test_surface(vec![env.clone(),Stage {vertex_alpha:false,vector_uv:Some((Vec3::X,Vec3::Y)),..Default::default()}]);
        let record=eligibility_record(&mixed);
        assert_eq!(record["deferred_accepts"],true);
        assert_eq!(record["classification"],"mixed");
        assert_eq!(record["reason"],"deferred");
        assert_eq!(record["environment"],true);
        assert_eq!(record["layers"][0]["environment"],true);
        assert_eq!(record["layers"][0]["vertex_alpha"],true);
        assert_eq!(record["layers"][1]["vector_uv"],true);
        let (_,simple)=test_surface(vec![Stage {vertex_alpha:true,..Default::default()},Stage {vertex_alpha:true,..Default::default()}]);
        let record=eligibility_record(&simple);
        assert_eq!(record["deferred_accepts"],false);
        assert_eq!(record["classification"],"uniform");
        assert_eq!(record["reason"],"simple");
        let dot_record=eligibility_record(&test_surface(vec![dot]).1);
        assert_eq!(dot_record["layers"][0]["dot_alpha"],true);
        assert_eq!(dot_record["layers"][0]["mods"],true);
        super::super::test_support::clear();
    }
}
