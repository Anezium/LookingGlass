//! Two owned banks per logical surface. Only numeric array addresses reach workers.
//! Pending packets contain handles, never borrowed/reused worker output. Collection
//! is the sole publication boundary; retained GPU storage keeps its five-slot ring.
use super::*;
use crate::skeletal::SkinSurface;
use std::{cell::Cell, collections::HashMap, rc::Weak, sync::Arc};
use crate::character::skinning::identity::ActorId;
use super::actor_vertices::PublicationMode;
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct Key { pub actor: ActorId, pub surface: usize, pub occurrence: u16 }
const UNUSED_FRAMES:u64=120;
/// Draws of one actor surface in a frame that get their own bank when
/// actor_publication_occurrences is on. Later draws use the serial path.
const MAX_OCCURRENCES:u16=4;
const SURFACES:usize=2048;
const VERTICES:usize=65536;
const INDICES:usize=262144;
#[cfg(any(test,feature="actor_diagnostics",all(feature="sim_maps_probe",not(target_os="vita"))))]
const MEMORY_ROWS:usize=36;
#[derive(Default)]
struct Bank { key:Option<Key>, last_used:u64, slots:[Option<Rc<prepared::Geometry>>;2], cpu:[Option<Arc<SkinSurface>>;2], mode:[Option<PublicationMode>;2], epoch:[u64;2], instance:[Option<u64>;2],
    /// The live vertex array whose UVs each slot last copied, and the edit count of that copy.
    uv_live:[usize;2], uv_edits:[u32;2] }
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
    /// vita_fx_dropped_layers charged by direct groups still waiting in `pending`.
    direct_capped:u64,
    /// Draw index per (actor, surface) in this frame; cleared, not freed, each frame.
    occurrences:HashMap<(ActorId,usize),u16>, occurrences_on:bool,
    /// actor_publication_uv_reuse, latched per frame. `uv_edits` counts announced live UV edits.
    uv_reuse_on:bool, uv_edits:u32,
    /// Reused UVs that disagreed with the live mesh. Tests and the host probe compare; the Vita never does.
    #[cfg(any(test,all(feature="sim_maps_probe",not(target_os="vita"))))]
    uv_stale:u64,
    /// Geometries whose bank Rc was dropped while another owner still held them.
    /// Host probe and tests only: memory_account() prunes it, nothing does on the Vita.
    #[cfg(any(test, feature="actor_diagnostics", all(feature="sim_maps_probe", not(target_os="vita"))))]
    evicted_referenced:Vec<Weak<prepared::Geometry>>,
    vertex_capacity:usize, index_capacity:usize, epoch:u64,
}
impl State {
    fn clear(&mut self) { *self=Self::default(); }
    fn evict(&mut self,id:usize) {
        let bank=&mut self.banks[id];
        if let Some(key)=bank.key.take() { self.lookup.remove(&key); }
        for g in bank.slots.iter().flatten() {
            #[cfg(any(test, feature="actor_diagnostics", all(feature="sim_maps_probe", not(target_os="vita"))))]
            if Rc::strong_count(g)>1 { self.evicted_referenced.push(Rc::downgrade(g)); }
            self.vertex_capacity-=g.vertices.capacity(); self.index_capacity-=g.indices.capacity();
        }
        *bank=Bank::default();
    }
    fn next_frame(&mut self) {
        self.epoch=self.epoch.checked_add(1).expect("actor publication epoch overflow");
        self.pending.clear(); self.deferred.clear(); self.held.clear(); self.held_puffs.clear();
        self.staged_held=false; self.completed=false; self.direct_capped=0;
        self.occurrences.clear();
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
    /// The n-th eligible draw of one actor surface in this frame selects bank n.
    /// The index is consumed before reservation, so a refused reservation or
    /// budget rejection still advances it and call order maps to a fixed bank.
    fn occurrence(&mut self,key:Key)->Option<Key> {
        if !self.occurrences_on {return Some(key);}
        let n=self.occurrences.entry((key.actor,key.surface)).or_insert(0);
        if *n>=MAX_OCCURRENCES {
            #[cfg(any(feature="actor_diagnostics",all(feature="sim_maps_probe",not(target_os="vita"))))]
            retained::count("actor_publication_occurrence_limit",1);
            return None;
        }
        let key=Key {occurrence:*n,..key};
        *n+=1;
        Some(key)
    }
    /// Host-layout inventory for diagnostics. Each geometry allocation is listed
    /// once, under one owner/lifetime class: a bank slot that is free, current
    /// (reserved this frame) or held (an earlier frame's output still referenced
    /// outside the bank), or evicted (no longer bank-owned, kept alive by an
    /// external holder). Bank-owned capacity is what VERTICES/INDICES bound; the
    /// evicted, container, map and header rows are outside those caps. HashMap
    /// rows are capacity*entry lower bounds without control bytes. The external
    /// rows are aliases: FRAME groups and CPU handles reference allocations that
    /// are already listed above, so they name holders without adding bytes to a
    /// class. Not Vita ABI.
    #[cfg(any(test,feature="actor_diagnostics",all(feature="sim_maps_probe",not(target_os="vita"))))]
    fn memory(&mut self)->[(&'static str,u64);MEMORY_ROWS] {
        use std::mem::size_of;
        fn map<K,V>(m:&HashMap<K,V>)->usize {m.capacity()*size_of::<(K,V)>()}
        fn bytes(g:&prepared::Geometry)->usize {g.vertices.capacity()*size_of::<Vertex>()+g.indices.capacity()*2}
        // An Rc allocation holds two counts and the value; each non-empty Vec is one more allocation.
        fn allocations(g:&prepared::Geometry)->usize {1+usize::from(g.vertices.capacity()>0)+usize::from(g.indices.capacity()>0)}
        self.evicted_referenced.retain(|g|g.strong_count()>0);
        let mut class=[[0usize;3];4];
        for bank in &self.banks {
            for (slot,g) in bank.slots.iter().enumerate() {
                let Some(g)=g else {continue};
                let c=if bank.epoch[slot]==self.epoch {1} else if Rc::strong_count(g)>1 {2} else {0};
                class[c][0]+=1;class[c][1]+=bytes(g);class[c][2]+=allocations(g);
            }
        }
        for g in self.evicted_referenced.iter().filter_map(std::rc::Weak::upgrade) {
            class[3][0]+=1;class[3][1]+=bytes(&g);class[3][2]+=allocations(&g);
        }
        let geometries=class.iter().map(|c|c[0]).sum::<usize>();
        let nested=self.held.iter().map(|g|g.passes.capacity()).sum::<usize>();
        let cpu=self.banks.iter().flat_map(|b|b.cpu.iter().flatten()).count();
        // External holders: FRAME groups and CPU handles alias allocations listed
        // above. Count them by unique address so the rows name holders, not bytes.
        let mut bank_ptrs=std::collections::HashSet::new();
        for g in self.banks.iter().flat_map(|b|b.slots.iter().flatten()) {bank_ptrs.insert(Rc::as_ptr(g) as usize);}
        for g in self.evicted_referenced.iter().filter_map(std::rc::Weak::upgrade) {bank_ptrs.insert(Rc::as_ptr(&g) as usize);}
        let (frame_groups,frame_refs,frame_bytes,frame_outside)=FRAME.with(|f| {
            let f=f.borrow();
            let mut refs=std::collections::HashMap::<usize,usize>::new();
            for group in &f.groups {
                for pass in &group.passes {
                    #[cfg(feature="vita_renderer")]
                    if let Some(g)=&pass.geometry {refs.insert(Rc::as_ptr(g) as usize,bytes(g));}
                    #[cfg(not(feature="vita_renderer"))]
                    let _=pass;
                }
            }
            let outside=refs.keys().filter(|p|!bank_ptrs.contains(p)).count();
            (f.groups.len(),refs.len(),refs.values().sum::<usize>(),outside)
        });
        let mut cpu_ptrs=std::collections::HashSet::new();
        for c in self.banks.iter().flat_map(|b|b.cpu.iter().flatten()) {cpu_ptrs.insert(Arc::as_ptr(c) as usize);}
        let cpu_bytes=cpu_ptrs.len()*(2*size_of::<usize>()+size_of::<SkinSurface>());
        [("actor_publication_mem_owned_bytes",(self.vertex_capacity*size_of::<Vertex>()+self.index_capacity*2) as u64),
         ("actor_publication_mem_banks",self.lookup.len() as u64),
         ("actor_publication_mem_bank_table_bytes",(self.banks.capacity()*size_of::<Bank>()) as u64),
         ("actor_publication_mem_maps_bytes",(map(&self.lookup)+map(&self.occurrences)) as u64),
         ("actor_publication_mem_pending_held_bytes",(self.pending.capacity()*size_of::<(usize,usize,usize,usize)>()
            +self.deferred.capacity()*size_of::<Deferred>()
            +self.held.capacity()*size_of::<Group>()+self.held_puffs.capacity()*size_of::<retained::world_puffs::Puff>()) as u64),
         ("actor_publication_mem_evicted_referenced_bytes",class[3][1] as u64),
         ("actor_publication_mem_slots_free",class[0][0] as u64),
         ("actor_publication_mem_slots_free_bytes",class[0][1] as u64),
         ("actor_publication_mem_slots_current",class[1][0] as u64),
         ("actor_publication_mem_slots_current_bytes",class[1][1] as u64),
         ("actor_publication_mem_slots_held",class[2][0] as u64),
         ("actor_publication_mem_slots_held_bytes",class[2][1] as u64),
         ("actor_publication_mem_evicted_referenced",class[3][0] as u64),
         ("actor_publication_mem_geometry_allocations",class.iter().map(|c|c[2]).sum::<usize>() as u64),
         ("actor_publication_mem_geometry_header_bytes",(geometries*(2*size_of::<usize>()+size_of::<prepared::Geometry>())) as u64),
         ("actor_publication_mem_banks_len",self.banks.len() as u64),
         ("actor_publication_mem_banks_capacity",self.banks.capacity() as u64),
         ("actor_publication_mem_lookup_len",self.lookup.len() as u64),
         ("actor_publication_mem_lookup_capacity",self.lookup.capacity() as u64),
         ("actor_publication_mem_occurrences_len",self.occurrences.len() as u64),
         ("actor_publication_mem_occurrences_capacity",self.occurrences.capacity() as u64),
         ("actor_publication_mem_pending_len",self.pending.len() as u64),
         ("actor_publication_mem_pending_capacity",self.pending.capacity() as u64),
         ("actor_publication_mem_held_groups_len",self.held.len() as u64),
         ("actor_publication_mem_held_groups_capacity",self.held.capacity() as u64),
         ("actor_publication_mem_held_pass_capacity",nested as u64),
         ("actor_publication_mem_held_pass_bytes",(nested*size_of::<Pass>()) as u64),
         ("actor_publication_mem_evicted_weak_capacity",self.evicted_referenced.capacity() as u64),
         ("actor_publication_mem_evicted_weak_bytes",(self.evicted_referenced.capacity()*size_of::<std::rc::Weak<prepared::Geometry>>()) as u64),
         ("actor_publication_mem_cpu_handles",cpu as u64),
         ("actor_publication_mem_cpu_arc_allocations",cpu_ptrs.len() as u64),
         ("actor_publication_mem_cpu_arc_referenced_bytes",cpu_bytes as u64),
         ("actor_publication_mem_external_frame_groups",frame_groups as u64),
         ("actor_publication_mem_external_frame_geometry_refs",frame_refs as u64),
         ("actor_publication_mem_external_frame_geometry_bytes",frame_bytes as u64),
         ("actor_publication_mem_external_frame_not_bank_owned",frame_outside as u64)]
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
        let resized=g.vertices.len()!=mesh.vertices.len();
        if resized {
            g.mesh.vertices.reserve_exact(mesh.vertices.len().saturating_sub(g.vertices.len()));
            g.mesh.vertices.resize(mesh.vertices.len(),Vertex::new(0.,0.,0.,0.,0.,WHITE));
        }
        // UV is the only live field untouched by the exact skin kernel. Snapshot it
        // before returning to a shared-model caller; blink/hidden edits cannot race.
        // With actor_publication_uv_reuse a slot that already holds this live array's UVs
        // skips both walks: production writes a live skeletal mesh's position, normal,
        // color and texture, never its UVs (see note_live_uv_edit). Workers keep them too.
        let live=mesh.vertices.as_ptr() as usize;
        let uv_held=s.uv_reuse_on && !resized && same_cpu && bank.uv_live[slot]==live && bank.uv_edits[slot]==s.uv_edits;
        let same_uv=if uv_held {
            crate::sim_maps::count("actor_publication_uv_reused_vertices",mesh.vertices.len() as u64);
            #[cfg(any(test,all(feature="sim_maps_probe",not(target_os="vita"))))]
            if !g.vertices.iter().zip(&mesh.vertices).all(|(a,b)|a.uv.to_array().map(f32::to_bits)==b.uv.to_array().map(f32::to_bits)) {
                s.uv_stale+=1;
                #[cfg(all(not(test),feature="sim_maps_probe",not(target_os="vita")))]
                assert!(crate::sim_maps::probe::negative_control().is_some(),"actor_publication_uv_reuse held a stale UV");
            }
            true
        } else {
            let same_uv=g.vertices.len()==mesh.vertices.len() && g.vertices.iter().zip(&mesh.vertices)
                .all(|(a,b)|a.uv.to_array().map(f32::to_bits)==b.uv.to_array().map(f32::to_bits));
            for (out,live) in g.mesh.vertices.iter_mut().zip(&mesh.vertices) {out.uv=live.uv;}
            bank.uv_live[slot]=if s.uv_reuse_on {live} else {0};
            bank.uv_edits[slot]=s.uv_edits;
            same_uv
        };

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
        let dropped = dropped + self.direct_capped;
        if dropped > 0 { retained::uncount("vita_fx_dropped_layers", dropped); }
        self.direct_capped = 0;
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
    geometry_allocs: usize, free_slots: usize, held_slots: usize, external_slots: usize,
    held_reference_count: usize,
    evicted_allocs: usize, evicted_vertex_cap: usize, evicted_index_cap: usize, evicted_payload_bytes: usize,
    lookup_len: usize, lookup_cap: usize, lookup_bytes_estimate: usize, banks_len: usize, banks_cap: usize, cpu_arcs: usize,
    pending_len: usize, pending_cap: usize, deferred_len: usize, deferred_cap: usize,
    held_len: usize, held_cap: usize, puff_len: usize, puff_cap: usize,
    /// Per-layer vertex/index buffer allocations owned by held passes (not Pass Vec storage).
    held_buffer_bytes: usize, held_buffer_allocs: usize,
    /// Pass/container allocations: the Vec<Pass> allocation of each held group.
    held_container_bytes: usize, held_container_allocs: usize,
    /// Held buffer + container bytes, kept as the host total component.
    held_pass_bytes: usize,
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
        // bank geometry already walked above, so only its own buffers are added here. Pass Vec
        // storage is a separate container allocation at host size, never a Vita layout.
        let mut held_refs: HashMap<usize, usize> = HashMap::new();
        for group in &self.held {
            let container = group.passes.capacity() * std::mem::size_of::<Pass>();
            account.held_container_bytes += container;
            account.held_container_allocs += usize::from(group.passes.capacity() > 0);
            for pass in &group.passes {
                if let Some(g) = &pass.geometry {
                    *held_refs.entry(Rc::as_ptr(g) as usize).or_default() += 1;
                    account.held_reference_count += 1;
                }
                account.held_buffer_bytes += pass.mesh.vertices.capacity() * std::mem::size_of::<Vertex>()
                    + pass.mesh.indices.capacity() * std::mem::size_of::<u16>();
                account.held_buffer_allocs += usize::from(pass.mesh.vertices.capacity() > 0)
                    + usize::from(pass.mesh.indices.capacity() > 0);
            }
        }
        account.held_pass_bytes = account.held_buffer_bytes + account.held_container_bytes;
        let mut cpu = HashMap::new();
        for bank in &self.banks {
            for slot in &bank.slots {
                match slot {
                    Some(g) => {
                        // Separate an actual held-pass reference from any other owner (FRAME
                        // during the append window, or an external retained publication).
                        if Rc::strong_count(g) > 1 {
                            if held_refs.contains_key(&(Rc::as_ptr(g) as usize)) { account.held_slots += 1; }
                            else { account.external_slots += 1; }
                        }
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
            geometry_allocs: 0, free_slots: 0, held_slots: 0, external_slots: 0, held_reference_count: 0, evicted_allocs: 0, evicted_vertex_cap: 0,
            evicted_index_cap: 0, evicted_payload_bytes: 0, lookup_len: 0, lookup_cap: 0, lookup_bytes_estimate: 0,
            banks_len: 0, banks_cap: 0, cpu_arcs: 0,
            pending_len: 0, pending_cap: 0, deferred_len: 0, deferred_cap: 0, held_len: 0, held_cap: 0,
            puff_len: 0, puff_cap: 0, held_buffer_bytes: 0, held_buffer_allocs: 0,
            held_container_bytes: 0, held_container_allocs: 0, held_pass_bytes: 0,
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
    /// Explicitly scoped mixed-width subtotal, **not** a Vita ABI total: walked bank and
    /// evicted payload, held per-layer buffers, and an 8-byte Rc header per counted Rc box.
    /// It deliberately excludes `held_container_bytes` (host-sized Pass Vec storage),
    /// `container_host_bytes` (host-sized banks/pending/deferred/held/puff Vecs) and the
    /// host-sized Geometry boxes inside the Rc allocation. Those need target struct sizes
    /// that a host build cannot observe, so they are reported separately, never relabelled.
    fn mixed_width_abi_subtotal_bytes(self) -> usize {
        self.payload_bytes + self.evicted_payload_bytes + self.held_buffer_bytes + self.rc_boxes * 8
    }
    /// Every observed field with its scoped counter name. Ownership classes stay separate:
    /// live bank-owned geometry, held-referenced slots, reusable free slots and evicted
    /// geometry still held by another owner are never added into one another.
    fn rows(self) -> Vec<(&'static str, u64)> {
        vec![
            ("actor_mem_payload_bytes", self.payload_bytes as u64),
            ("actor_mem_evicted_payload_bytes", self.evicted_payload_bytes as u64),
            ("actor_mem_held_buffer_bytes", self.held_buffer_bytes as u64),
            ("actor_mem_held_buffer_allocs", self.held_buffer_allocs as u64),
            ("actor_mem_held_container_bytes", self.held_container_bytes as u64),
            ("actor_mem_held_container_allocs", self.held_container_allocs as u64),
            ("actor_mem_held_pass_bytes", self.held_pass_bytes as u64),
            ("actor_mem_mixed_width_abi_subtotal_bytes", self.mixed_width_abi_subtotal_bytes() as u64),
            ("actor_mem_lookup_bytes_estimate", self.lookup_bytes_estimate as u64),
            ("actor_mem_container_host_bytes", self.container_host_bytes as u64),
            ("actor_mem_host_bytes", self.host_bytes() as u64),
            ("actor_mem_geometry_allocs", self.geometry_allocs as u64),
            ("actor_mem_live_allocs", self.geometry_allocs as u64),
            ("actor_mem_held_referenced_allocs", self.held_slots as u64),
            ("actor_mem_external_referenced_slots", self.external_slots as u64),
            ("actor_mem_held_reference_count", self.held_reference_count as u64),
            ("actor_mem_reusable_slots", self.free_slots as u64),
            ("actor_mem_evicted_allocs", self.evicted_allocs as u64),
            ("actor_mem_evicted_referenced_allocs", self.evicted_allocs as u64),
            ("actor_mem_evicted_vertex_cap", self.evicted_vertex_cap as u64),
            ("actor_mem_evicted_index_cap", self.evicted_index_cap as u64),
            ("actor_mem_free_slots", self.free_slots as u64),
            ("actor_mem_held_slots", self.held_slots as u64),
            ("actor_mem_bank_vertex_len", self.bank_vertex_len as u64),
            ("actor_mem_bank_vertex_cap", self.bank_vertex_cap as u64),
            ("actor_mem_bank_index_len", self.bank_index_len as u64),
            ("actor_mem_bank_index_cap", self.bank_index_cap as u64),
            ("actor_mem_lookup_len", self.lookup_len as u64),
            ("actor_mem_lookup_cap", self.lookup_cap as u64),
            ("actor_mem_banks_len", self.banks_len as u64),
            ("actor_mem_banks_cap", self.banks_cap as u64),
            ("actor_mem_cpu_arcs", self.cpu_arcs as u64),
            ("actor_mem_pending_len", self.pending_len as u64),
            ("actor_mem_pending_cap", self.pending_cap as u64),
            ("actor_mem_deferred_len", self.deferred_len as u64),
            ("actor_mem_deferred_cap", self.deferred_cap as u64),
            ("actor_mem_held_len", self.held_len as u64),
            ("actor_mem_held_cap", self.held_cap as u64),
            ("actor_mem_puff_len", self.puff_len as u64),
            ("actor_mem_puff_cap", self.puff_cap as u64),
            ("actor_mem_rc_boxes", self.rc_boxes as u64),
            ("actor_mem_sizeof_vertex", std::mem::size_of::<Vertex>() as u64),
            ("actor_mem_sizeof_geometry", std::mem::size_of::<prepared::Geometry>() as u64),
            ("actor_mem_sizeof_bank", std::mem::size_of::<Bank>() as u64),
            ("actor_mem_sizeof_deferred", std::mem::size_of::<Deferred>() as u64),
            ("actor_mem_bank_payload_bound_bytes", (VERTICES * std::mem::size_of::<Vertex>() + INDICES * 2) as u64),
        ]
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
/// Default on since r22. Tests: a scoped override, else the shipped default;
/// LOOKING_GLASS_TEST_DEFERRED_LAYERS=off runs the whole suite with it off.
pub(crate) fn deferred_layers()->bool {
    #[cfg(test)]
    { DEFERRED_TEST.with(Cell::get).unwrap_or_else(|| match std::env::var("LOOKING_GLASS_TEST_DEFERRED_LAYERS").as_deref() {
        Ok("off") => false,
        Err(std::env::VarError::NotPresent) | Ok("on") => true,
        _ => panic!("LOOKING_GLASS_TEST_DEFERRED_LAYERS must be off or on"),
    }) }
    #[cfg(not(test))]
    { crate::render::vita_renderer::config().actor_deferred_layers }
}
#[cfg(test)]
thread_local! { static UV_REUSE_TEST:Cell<Option<bool>>=const {Cell::new(None)}; }
#[cfg(test)]
pub(crate) fn with_publication_uv_reuse<T>(on:bool,run:impl FnOnce()->T)->T {
    struct Reset(Option<bool>);
    impl Drop for Reset { fn drop(&mut self) { UV_REUSE_TEST.with(|s|s.set(self.0)); } }
    let _reset=Reset(UV_REUSE_TEST.with(|s|s.replace(Some(on))));
    run()
}
/// Default on since r25. Tests: a scoped override, else the shipped default;
/// LOOKING_GLASS_TEST_PUBLICATION_UV_REUSE=off runs the whole suite with it off.
pub(crate) fn publication_uv_reuse()->bool {
    #[cfg(test)]
    { UV_REUSE_TEST.with(|s|s.get()).unwrap_or_else(||match std::env::var("LOOKING_GLASS_TEST_PUBLICATION_UV_REUSE").as_deref() {
        Ok("off") => false,
        Err(std::env::VarError::NotPresent) | Ok("on") => true,
        _ => panic!("LOOKING_GLASS_TEST_PUBLICATION_UV_REUSE must be off or on"),
    }) }
    #[cfg(not(test))]
    { crate::render::vita_renderer::config().actor_publication_uv_reuse }
}
/// The contract of actor_publication_uv_reuse: code that writes a live skeletal mesh's
/// UVs after load calls this before the mesh's next draw. Production has no such writer
/// (the audit is in tools/perf_contracts/host_flag_exact_gather.py, and tools/check_source.py
/// fails on a .uv write in a new file); test fixtures do. A first writer must promote this.
#[cfg(test)]
pub(crate) fn note_live_uv_edit() { STATE.with(|s| {let mut s=s.borrow_mut();s.uv_edits=s.uv_edits.wrapping_add(1);}); }
/// This frame's publication epoch. One per frame in which single preparation runs.
pub(crate) fn frame_epoch()->u64 { STATE.with(|s|s.borrow().epoch) }
#[cfg(test)]
thread_local! { static OCCURRENCES_TEST:Cell<Option<bool>>=const {Cell::new(None)}; }
#[cfg(test)]
pub(crate) fn with_publication_occurrences<T>(on:bool,run:impl FnOnce()->T)->T {
    struct Reset(Option<bool>);
    impl Drop for Reset { fn drop(&mut self) { OCCURRENCES_TEST.with(|s|s.set(self.0)); } }
    let _reset=Reset(OCCURRENCES_TEST.with(|s|s.replace(Some(on))));
    run()
}
/// Default on since r22. Tests: a scoped override, else the shipped default;
/// LOOKING_GLASS_TEST_PUBLICATION_OCCURRENCES=off runs the whole suite with it off.
pub(crate) fn publication_occurrences()->bool {
    #[cfg(test)]
    { OCCURRENCES_TEST.with(|s|s.get()).unwrap_or_else(||match std::env::var("LOOKING_GLASS_TEST_PUBLICATION_OCCURRENCES").as_deref() {
        Ok("off") => false,
        Err(std::env::VarError::NotPresent) | Ok("on") => true,
        _ => panic!("LOOKING_GLASS_TEST_PUBLICATION_OCCURRENCES must be off or on"),
    }) }
    #[cfg(not(test))]
    { crate::render::vita_renderer::config().actor_publication_occurrences }
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
    // Latched once per frame: every prepare in this frame uses the same value.
    if enabled { STATE.with(|s| {let mut s=s.borrow_mut();s.occurrences_on=publication_occurrences();s.uv_reuse_on=publication_uv_reuse();s.next_frame();}); }
}
pub(crate) fn end() {
    DEFERRED.with(|d|d.set(false));
    if ACTIVE.with(|a|a.replace(false)) {
        STATE.with(|s| {
            let mut s=s.borrow_mut();
            s.discard_unfilled();
            s.pending.clear(); s.deferred.clear(); s.held.clear(); s.held_puffs.clear(); s.staged_held=false; s.direct_capped=0;
            #[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
            emit_memory_phase(&mut s, "end-cleared", 0);
        });
    }
}
/// This address is valid only between prepare and the fully drained collect. Banks
/// cannot be mutated again while any old packet/view still owns their publication.
///
/// A refusal returns None and the caller runs the original serial consumer, which
/// owns the refused draw's FRAME.dropped and vita_fx_dropped_layers charges. A kept
/// direct draw takes the capped-layer charge the serial cache would have taken.
/// Occurrence banks apply to direct recipe draws only; deferred draws keep one key.
pub(crate) fn prepare(mesh:&Mesh,cpu:Arc<SkinSurface>,unchanged:bool,instance:u64,key:Key)->Option<(usize,PublicationMode)> {
    let texture=mesh.texture.as_ref();
    #[cfg(test)]
    let surface=texture.and_then(super::test_support::surface);
    #[cfg(not(test))]
    let surface=None;
    let surface=surface.or_else(||texture.and_then(|t|SURFACE_LOOKUP.with(|s|s.borrow().get(&t.raw_miniquad_id()).and_then(Weak::upgrade))));
    #[cfg(test)]
    let surface=surface.or_else(super::test_surface);
    let surface=surface?;
    note_eligibility(&surface);
    if SKIN_OPACITY.with(Cell::get)!=1. {return None;}
    let (time,appearance)=FRAME.with(|f| {let f=f.borrow();(f.time,f.appearance)});
    let Some((passes,alpha))=actor_cache::recipe(&surface,time,appearance) else {
        // recipe() has no side effect when it refuses; the serial fallback stays exact.
        return if DEFERRED.with(Cell::get) {prepare_deferred(mesh,cpu,unchanged,instance,key,surface,time,appearance)} else {None};
    };
    let capped=surface.layers.len()-passes.len();
    let mode=PublicationMode::recipe(alpha);
    let mut passes=Some(passes);
    let result=STATE.with(|s| {
        let mut s=s.borrow_mut();
        let key=s.occurrence(key)?;
        let (id,slot,output)=s.reserve_key(mesh,cpu,unchanged,instance,mode,key)?;
        let n=mesh.vertices.len()*passes.as_ref().unwrap().len();
        let used=FRAME.with(|f|f.borrow().vertices);
        if s.frame_budget_rejects(id,slot,used,n) {return None;}
        let group=FRAME.with(|f| {
            let mut f=f.borrow_mut();
            f.vertices+=n;let id=f.groups.len();
            let transparent=surface.transparent||f.depth_read_only;
            f.groups.push(Group {passes:passes.take().unwrap(),transparent});Some(id)
        })?;
        if capped>0 {
            retained::count("vita_fx_dropped_layers",capped as u64);
            s.direct_capped+=capped as u64;
        }
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
        // Host-only phase evidence: hold-returned is the gather/hold population before any
        // deferred construction. The pre-clear Deferred length is preserved for later samples.
        #[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
        let pre_clear_deferred = s.deferred.len();
        #[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
        emit_memory_phase(&mut s, "hold-returned", pre_clear_deferred);
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
                // layer_passes already counted this call's capacity and allocations into
                // actor_layer_copy_capacity_bytes/allocs; adding copy_cap here would count the
                // same deferred copies twice in the all-helper totals.
            }
            s.deferred.clear();
        }
        #[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
        emit_memory_phase(&mut s, "deferred-built", pre_clear_deferred);
        for n in 0..s.pending.len() {
            let (group,bank,slot,_)=s.pending[n];
            let geometry=s.banks[bank].slots[slot].as_ref().unwrap().clone();
            for pass in &mut s.held[group].passes {pass.geometry=Some(geometry.clone());}
        }
        // Direct attachment is the inclusive held population: pending geometry Rcs are now
        // shared with their held passes, so held_slots authenticates real references rather
        // than a pre-attachment zero. The default counters are emitted at this phase.
        #[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
        {
            let account=s.memory_account();
            crate::sim_maps::count("actor_publication_bank_bytes",(s.vertex_capacity*std::mem::size_of::<Vertex>()+s.index_capacity*2) as u64);
            crate::sim_maps::count("actor_publication_bank_count",s.lookup.len() as u64);
            emit_memory(&account);
        }
        #[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
        emit_memory_phase(&mut s, "direct-attached", pre_clear_deferred);
        #[cfg(feature="actor_diagnostics")]
        retained::count("actor_publication_capacity_bytes",(s.vertex_capacity*std::mem::size_of::<Vertex>()+s.index_capacity*2) as u64);
        #[cfg(feature="actor_diagnostics")]
        retained::count("actor_publication_banks",s.lookup.len() as u64);
        #[cfg(any(feature="actor_diagnostics",all(feature="sim_maps_probe",not(target_os="vita"))))]
        {
            let memory=s.memory();
            for (name,value) in memory {retained::count(name,value);}
            // Per-frame inventory for the host oracle; independent of the counter switch.
            #[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
            if crate::sim_maps::probe::skin_proof() {
                println!("PUBLICATION_MEMORY {}",serde_json::json!({"epoch":s.epoch,"rows":memory.iter().map(|(k,v)|(k.to_string(),*v)).collect::<std::collections::BTreeMap<_,_>>()}));
            }
        }
        FRAME.with(|f| {let mut f=f.borrow_mut();let world_groups=f.groups.len();
            for puff in &mut s.held_puffs {puff.before_group+=world_groups;}
            f.groups.append(&mut s.held);f.world_puffs.append(&mut s.held_puffs);
        });
        s.completed=true;
        s.direct_capped=0;
        #[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
        emit_memory_phase(&mut s, "frame-appended", pre_clear_deferred);
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
    for (name, value) in account.rows() { crate::sim_maps::count(name, value); }
}

/// Phase-authenticated account sample. The default counters are emitted at direct-attached;
/// this line preserves hold-returned (pre-clear Deferred), deferred-built, direct-attached,
/// frame-appended and end-cleared populations separately, so a single zero cannot be read as
/// the lifetime peak. `pre_clear_deferred_len` is the Deferred length before complete() clears it.
#[cfg(all(feature="sim_maps_probe", not(target_os="vita")))]
fn emit_memory_phase(state: &mut State, phase: &'static str, pre_clear_deferred_len: usize) {
    let account = state.memory_account();
    let mut record = serde_json::json!({
        "frame": crate::sim_maps::probe::frame(),
        "phase": phase,
        "pre_clear_deferred_len": pre_clear_deferred_len,
        "scope": "state-owned banks/pending/deferred/held only; FRAME-owned groups, CPU/surface owners, allocator overhead and observer scratch are out of scope",
    });
    for (name, value) in account.rows() {
        record[name.trim_start_matches("actor_mem_")] = serde_json::json!(value);
    }
    println!("ACTOR_MEMORY_PHASE {record}");
}

#[cfg(test)]
pub(super) fn pinned_slots()->Vec<Rc<prepared::Geometry>> {
    STATE.with(|s|s.borrow().banks.iter().flat_map(|b|b.slots.iter().filter_map(|g|g.clone())).collect())
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
    fn key(actor:ActorId,cpu:&Arc<SkinSurface>)->Key { Key {actor,surface:Arc::as_ptr(cpu) as usize,occurrence:0} }
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
        assert!(state.reserve_key(&mesh,cpu,false,0,O,Key {actor:ActorId::default(),surface:0,occurrence:0}).is_some());
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
        assert_eq!(ambient,std::env::var("LOOKING_GLASS_TEST_DEFERRED_LAYERS").as_deref()!=Ok("off"));
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
        assert_eq!(account.held_slots,0);
        assert_eq!(account.external_slots,1,"a retained publication is an external holder");
        assert_eq!(account.held_reference_count,0);
        assert!(account.payload_bytes>=3*std::mem::size_of::<Vertex>()+6);
        assert!(account.host_bytes()>account.payload_bytes);
        assert_eq!(account.held_buffer_bytes,0);
        assert_eq!(account.held_container_bytes,0);
        assert_eq!(account.mixed_width_abi_subtotal_bytes(),account.payload_bytes+8);
        assert_eq!(account.held_pass_bytes,account.held_buffer_bytes+account.held_container_bytes);
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
    fn memory_account_separates_buffer_container_and_mixed_width_subtotals() {
        let account=MemoryAccount { payload_bytes:1000, evicted_payload_bytes:200, held_buffer_bytes:300,
            held_buffer_allocs:4, held_container_bytes:500, held_container_allocs:3, held_pass_bytes:800,
            lookup_bytes_estimate:7, container_host_bytes:11, rc_boxes:2, ..MemoryAccount::zero() };
        assert_eq!(account.host_bytes(),1000+200+800+7+11+2*(2*std::mem::size_of::<usize>()+std::mem::size_of::<prepared::Geometry>()));
        // The mixed-width subtotal excludes the host-sized Pass Vec container and every
        // host container, so it cannot be read as a Vita ABI total.
        assert_eq!(account.mixed_width_abi_subtotal_bytes(),1000+200+300+16);
        assert!(account.mixed_width_abi_subtotal_bytes()<account.host_bytes());
        let rows:std::collections::BTreeMap<_,_>=account.rows().into_iter().collect();
        assert_eq!(rows["actor_mem_held_buffer_allocs"],4);
        assert_eq!(rows["actor_mem_held_container_allocs"],3);
        assert_eq!(rows["actor_mem_live_allocs"],0);
        assert_eq!(rows["actor_mem_held_referenced_allocs"],0);
        assert_eq!(rows["actor_mem_external_referenced_slots"],0);
        assert_eq!(rows["actor_mem_held_reference_count"],0);
        assert_eq!(rows["actor_mem_reusable_slots"],0);
        assert_eq!(rows["actor_mem_evicted_referenced_allocs"],0);
        assert_eq!(rows["actor_mem_bank_payload_bound_bytes"],(VERTICES*std::mem::size_of::<Vertex>()+INDICES*2) as u64);
        assert_eq!(rows["actor_mem_mixed_width_abi_subtotal_bytes"],1000+200+300+16);
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

    // Occurrence banks (actor_publication_occurrences).
    fn surface(vertices:usize,indices:Vec<u16>)->(Mesh,Arc<SkinSurface>) {
        let source=crate::skeletal::Surface::new("body".into(),(0..vertices).map(|_|crate::skeletal::SkinVertex {
            uv:Vec2::ONE,weights:vec![crate::skeletal::Weight {bone:0,amount:1.,offset:Vec3::ZERO}]}).collect(),indices);
        let cpu=source.cpu();
        (Mesh {vertices:cpu.bind_vertices.clone(),indices:cpu.indices.clone(),texture:None},cpu)
    }
    fn on()->State { State {occurrences_on:true,..Default::default()} }
    /// prepare() after its eligibility checks: occurrence, reservation, frame budget.
    fn claim(state:&mut State,mesh:&Mesh,cpu:&Arc<SkinSurface>,unchanged:bool,instance:u64,key:Key,used:usize)->Option<(usize,usize,usize)> {
        let key=state.occurrence(key)?;
        let (id,slot,output)=state.reserve_key(mesh,cpu.clone(),unchanged,instance,O,key)?;
        if state.frame_budget_rejects(id,slot,used,mesh.vertices.len()) {return None;}
        Some((id,slot,output))
    }
    fn geometry(state:&State,(id,slot,_):(usize,usize,usize))->Rc<prepared::Geometry> { state.banks[id].slots[slot].clone().unwrap() }
    fn occurrence_of(state:&State,id:usize)->u16 { state.banks[id].key.unwrap().occurrence }
    fn pass()->Pass {
        Pass {_game_thread:crate::game_thread::GameThread::default(),geometry:None,effect:false,blend_index:0,depth_equal:false,
            #[cfg(target_os="vita")] world_static:None,
            #[cfg(target_os="vita")] world_geometry_counted:false,
            mesh:Mesh {vertices:Vec::new(),indices:Vec::new(),texture:None},material:None,lit:false,clamp:false,cutout:0.,
            cutout_reference:0.,gain:Vec4::ONE,fog:0.,model:true,appearance:Appearance::default(),filter_fade:0.}
    }
    /// The thread-local frame path of prepare() after its texture/recipe checks.
    fn publish(mesh:&Mesh,cpu:&Arc<SkinSurface>,key:Key)->Option<usize> {
        STATE.with(|s| {
            let mut s=s.borrow_mut();
            let key=s.occurrence(key)?;
            let (id,slot,output)=s.reserve_key(mesh,cpu.clone(),false,0,O,key)?;
            let used=FRAME.with(|f|f.borrow().vertices);
            if s.frame_budget_rejects(id,slot,used,mesh.vertices.len()) {return None;}
            let group=FRAME.with(|f| {let mut f=f.borrow_mut();f.vertices+=mesh.vertices.len();
                f.groups.push(Group {passes:vec![pass()],transparent:false});f.groups.len()-1});
            s.pending.push((group,id,slot,mesh.vertices.len()));
            Some(output)
        })
    }
    /// Retained direct frame for begin(); always restores the thread-local state.
    struct Retained;
    fn retained()->Retained {
        FRAME.with(|f| {let mut f=f.borrow_mut();f.active=true;f.direct=true;});
        Retained
    }
    impl Drop for Retained {
        fn drop(&mut self) {
            ACTIVE.with(|a|a.set(false));STATE.with(|s|s.borrow_mut().clear());
            FRAME.with(|f|*f.borrow_mut()=Frame::default());
        }
    }
    #[test]
    fn publication_occurrence_switch_scoped_override_restores_on_unwind_and_env_selects_the_suite_arm() {
        let suite=match std::env::var("LOOKING_GLASS_TEST_PUBLICATION_OCCURRENCES").as_deref() {
            Ok("off") => false, Err(std::env::VarError::NotPresent) | Ok("on") => true, other => panic!("{other:?}") };
        println!("publication occurrence suite arm: {}",if suite {"on"} else {"off"});
        assert_eq!(publication_occurrences(),suite);
        for on in [false,true] {
            assert_eq!(with_publication_occurrences(on,publication_occurrences),on);
            assert_eq!(with_publication_occurrences(!on,||with_publication_occurrences(on,publication_occurrences)),on);
            assert_eq!(publication_occurrences(),suite);
        }
        let unwound=std::panic::catch_unwind(||with_publication_occurrences(!suite,|| {
            assert_eq!(publication_occurrences(),!suite);panic!("unwind through the override");
        }));
        assert!(unwound.is_err());assert_eq!(publication_occurrences(),suite);
        // The test Config is a frozen default (on); the switch never reads it in tests.
        assert!(crate::render::vita_renderer::config().actor_publication_occurrences);
    }
    // actor_publication_uv_reuse.
    fn uv_state(on:bool)->State { State {uv_reuse_on:on,occurrences_on:true,..Default::default()} }
    /// One frame's publication, with the previous output still in flight, so the two slots
    /// alternate as they do in play. Returns (vertices, whether the upload stamp moved, stale count).
    struct UvArm {state:State,actor:ActorId,held:Option<Rc<prepared::Geometry>>,stamps:[Option<u64>;2]}
    impl UvArm {
        fn new(on:bool)->Self { Self {state:State {uv_reuse_on:on,occurrences_on:true,..Default::default()},actor:ActorId::default(),held:None,stamps:[None;2]} }
        fn frame(&mut self,mesh:&Mesh,cpu:&Arc<SkinSurface>)->(Vec<Vertex>,bool,u64) {
            self.state.next_frame();
            let (id,slot,_)=self.state.reserve_key(mesh,cpu.clone(),true,0,PublicationMode::Opaque255,key(self.actor,cpu)).unwrap();
            let g=self.state.banks[id].slots[slot].as_ref().unwrap().clone();
            let moved=self.stamps[slot].replace(g.version.vertices)!=Some(g.version.vertices);
            let vertices=g.vertices.clone();
            self.held=Some(g);
            (vertices,moved,self.state.uv_stale)
        }
    }
    #[test]
    fn uv_reuse_publishes_the_same_vertices_and_upload_stamps_as_the_full_walks() {
        let (mesh,cpu)=fixture();
        let (mut on,mut off)=(UvArm::new(true),UvArm::new(false));
        for frame in 0..8 {
            let a=on.frame(&mesh,&cpu);
            let b=off.frame(&mesh,&cpu);
            assert_eq!(a.0.iter().map(|v|v.uv).collect::<Vec<_>>(),b.0.iter().map(|v|v.uv).collect::<Vec<_>>(),"frame {frame}");
            assert_eq!(a.1,b.1,"upload stamp in frame {frame}");
            assert_eq!(a.2,0);
        }
        let live=mesh.vertices.as_ptr() as usize;
        assert_eq!(on.state.banks.len(),1);
        assert_eq!(on.state.banks[0].uv_live,[live;2],"both slots hold the live UVs");
        assert_eq!(off.state.banks[0].uv_live,[0;2],"the off path keeps no memo");
    }
    #[test]
    fn uv_reuse_negative_control_an_unannounced_edit_goes_stale_and_is_counted() {
        let (mut mesh,cpu)=fixture();
        let (mut on,mut off)=(UvArm::new(true),UvArm::new(false));
        for _ in 0..4 {on.frame(&mesh,&cpu);off.frame(&mesh,&cpu);}
        mesh.vertices[0].uv=Vec2::new(-3.,9.);
        let a=on.frame(&mesh,&cpu);
        let b=off.frame(&mesh,&cpu);
        assert_eq!(b.0[0].uv,Vec2::new(-3.,9.));
        assert_ne!(a.0[0].uv,b.0[0].uv,"the lever must differ here, or this test proves nothing");
        assert_eq!(a.2,1);assert!(b.1,"the off arm moved its upload stamp");
    }
    #[test]
    fn uv_reuse_full_walks_return_after_an_announced_edit_a_new_mesh_or_a_new_surface() {
        let (mut mesh,cpu)=fixture();
        let (mut on,mut off)=(UvArm::new(true),UvArm::new(false));
        for _ in 0..4 {on.frame(&mesh,&cpu);off.frame(&mesh,&cpu);}
        // Announced edit: each slot repeats the walks once.
        mesh.vertices[0].uv=Vec2::new(5.,6.);on.state.uv_edits+=1;
        for _ in 0..4 {
            let a=on.frame(&mesh,&cpu);let b=off.frame(&mesh,&cpu);
            assert_eq!(a.0[0].uv,Vec2::new(5.,6.));assert_eq!(a.0[0].uv,b.0[0].uv);assert_eq!(a.1,b.1);assert_eq!(a.2,0);
        }
        // A second live array with other UVs, same surface: the address differs.
        let mut other=crate::render_fx::copy_mesh(&mesh);other.vertices[0].uv=Vec2::new(7.,8.);
        for _ in 0..4 {
            let a=on.frame(&other,&cpu);let b=off.frame(&other,&cpu);
            assert_eq!(a.0[0].uv,Vec2::new(7.,8.));assert_eq!(a.0[0].uv,b.0[0].uv);assert_eq!(a.1,b.1);assert_eq!(a.2,0);
        }
        // A new surface in the same bank slot.
        let (_,cpu2)=fixture();
        let k=key(on.actor,&cpu);
        on.state.next_frame();
        let id=on.state.lookup[&k];on.held=None;
        let (_,slot,_)=on.state.reserve_at(&mesh,cpu2.clone(),true,0,PublicationMode::Opaque255,id).unwrap();
        assert_eq!(on.state.banks[id].slots[slot].as_ref().unwrap().vertices[0].uv,Vec2::new(5.,6.));
        assert_eq!(on.state.uv_stale,0);
        // Evicting the bank forgets the memo with it.
        on.held=None;on.state.evict(id);
        let a=on.frame(&mesh,&cpu);
        assert_eq!(a.0[0].uv,Vec2::new(5.,6.));assert_eq!(a.2,0);
    }
    #[test]
    fn third_same_key_draw_publishes_only_with_occurrences_on() {
        let (mesh,cpu)=fixture();let actor=ActorId::default();
        for on in [false,true] {
            let _frame=retained();
            with_publication_occurrences(on,begin);
            assert!(active());
            let outputs:Vec<_>=(0..3).map(|_|publish(&mesh,&cpu,key(actor,&cpu))).collect();
            assert!(outputs[0].is_some() && outputs[1].is_some());
            assert_eq!(outputs[2].is_some(),on,"third draw of one actor surface, occurrences={on}");
            let used:Vec<_>=outputs.iter().flatten().collect();
            assert!(used.iter().enumerate().all(|(n,a)|used[n+1..].iter().all(|b|a!=b)),"duplicate destination");
            STATE.with(|s| {let s=s.borrow();
                assert_eq!(s.lookup.len(),if on {3} else {1});
                // Off never touches the occurrence map, not even its capacity.
                if on {assert_eq!(s.occurrences[&(actor,key(actor,&cpu).surface)],3);} else {assert_eq!(s.occurrences.capacity(),0);}
            });
            end();
        }
    }
    #[test]
    fn occurrence_switch_is_latched_once_per_frame() {
        let (mesh,cpu)=fixture();let actor=ActorId::default();let _frame=retained();
        with_publication_occurrences(true,begin);
        // Flipping the source mid-frame keeps this frame's latched value.
        let outputs:Vec<_>=with_publication_occurrences(false,||(0..3).map(|_|publish(&mesh,&cpu,key(actor,&cpu))).collect());
        assert!(outputs.iter().all(Option::is_some));
        end();
        with_publication_occurrences(false,begin);
        let outputs:Vec<_>=with_publication_occurrences(true,||(0..3).map(|_|publish(&mesh,&cpu,key(actor,&cpu))).collect());
        assert!(outputs[2].is_none(),"the frame latched off");
        STATE.with(|s|assert!(s.borrow().occurrences.is_empty()));
        end();
    }
    #[test]
    fn occurrences_of_one_actor_surface_use_distinct_banks_and_destinations() {
        let (mesh,cpu)=fixture();let mut state=on();state.next_frame();let actor=ActorId::default();
        let claims:Vec<_>=(0..3).map(|_|claim(&mut state,&mesh,&cpu,false,0,key(actor,&cpu),0).unwrap()).collect();
        for (n,a) in claims.iter().enumerate() {
            assert_eq!(occurrence_of(&state,a.0),n as u16);
            for b in &claims[n+1..] {assert_ne!(a.0,b.0,"bank");assert_ne!(a.2,b.2,"destination");}
        }
        // A different actor with the same model surface keeps its own occurrence count.
        let other=claim(&mut state,&mesh,&cpu,false,0,key(ActorId::default(),&cpu),0).unwrap();
        assert_eq!(occurrence_of(&state,other.0),0);assert!(claims.iter().all(|c|c.0!=other.0));
        state.next_frame();assert!(state.occurrences.is_empty());assert!(state.occurrences.capacity()>=2);
    }
    #[test]
    fn previous_frame_references_move_every_occurrence_to_its_alternate_slot() {
        let (mut mesh,cpu)=fixture();let mut state=on();let actor=ActorId::default();
        let mut held:Vec<(Rc<prepared::Geometry>,f32)>=Vec::new();let mut previous:Vec<(usize,usize)>=Vec::new();
        for frame in 0..6 {
            state.next_frame();
            for v in &mut mesh.vertices {v.uv.x=frame as f32;}
            let claims:Vec<_>=(0..3).map(|_|claim(&mut state,&mesh,&cpu,false,0,key(actor,&cpu),0).unwrap()).collect();
            for c in &claims {
                assert!(held.iter().all(|(g,_)|g.vertices.as_ptr() as usize!=c.2),"referenced output returned");
            }
            for ((id,slot,_),(old_id,old_slot)) in claims.iter().zip(&previous) {assert_eq!(id,old_id);assert_ne!(slot,old_slot);}
            for (g,marker) in &held {assert!(g.vertices.iter().all(|v|v.uv.x==*marker),"referenced output overwritten");}
            // The previous frame's outputs stay referenced (in-flight) until this frame's replace them.
            held=claims.iter().map(|&c|(geometry(&state,c),frame as f32)).collect();
            previous=claims.iter().map(|&(id,slot,_)|(id,slot)).collect();
        }
    }
    #[test]
    fn both_slots_held_refuse_only_that_occurrence_and_its_index_is_consumed() {
        let (mesh,cpu)=fixture();let mut state=on();let actor=ActorId::default();let k=key(actor,&cpu);
        let mut pinned=Vec::new();
        for _ in 0..2 {
            state.next_frame();
            let claims:Vec<_>=(0..3).map(|_|claim(&mut state,&mesh,&cpu,false,0,k,0).unwrap()).collect();
            pinned.push(geometry(&state,claims[1]));
        }
        state.next_frame();
        let first=claim(&mut state,&mesh,&cpu,false,0,k,0).unwrap();
        assert!(claim(&mut state,&mesh,&cpu,false,0,k,0).is_none(),"occurrence 1 has both slots referenced");
        let third=claim(&mut state,&mesh,&cpu,false,0,k,0).unwrap();
        assert_eq!((occurrence_of(&state,first.0),occurrence_of(&state,third.0)),(0,2),"the refusal consumed index 1");
        let fourth=claim(&mut state,&mesh,&cpu,false,0,k,0).unwrap();assert_eq!(occurrence_of(&state,fourth.0),3);
        assert_eq!(state.occurrences[&(actor,k.surface)],4);
        drop(pinned);state.next_frame();
        assert!((0..4).all(|_|claim(&mut state,&mesh,&cpu,false,0,k,0).is_some()),"released slots are reusable");
    }
    #[test]
    fn fourth_eligible_occurrence_publishes_and_fifth_refuses_without_touching_banks() {
        let (mesh,cpu)=fixture();let mut state=on();state.next_frame();let actor=ActorId::default();let k=key(actor,&cpu);
        let claims:Vec<_>=(0..4).map(|_|claim(&mut state,&mesh,&cpu,false,0,k,0)).collect();
        assert!(claims.iter().all(Option::is_some),"four occurrences with resources available");
        let ids:std::collections::BTreeSet<_>=claims.iter().flatten().map(|c|c.0).collect();assert_eq!(ids.len(),4);
        let before=(state.lookup.len(),state.banks.len(),state.vertex_capacity,state.index_capacity);
        assert!(claim(&mut state,&mesh,&cpu,false,0,k,0).is_none(),"fifth occurrence");
        assert!(claim(&mut state,&mesh,&cpu,false,0,k,0).is_none(),"sixth occurrence");
        assert_eq!((state.lookup.len(),state.banks.len(),state.vertex_capacity,state.index_capacity),before);
        assert_eq!(state.occurrences[&(actor,k.surface)],MAX_OCCURRENCES);
        assert!(state.lookup.keys().all(|k|k.occurrence<MAX_OCCURRENCES));
    }
    #[test]
    fn budget_and_capacity_refusals_consume_the_occurrence_and_fall_back() {
        let actor=ActorId::default();
        // Frame vertex budget: reserved, then refused with its token released.
        let (mesh,cpu)=fixture();let mut state=on();state.next_frame();let k=key(actor,&cpu);
        assert!(claim(&mut state,&mesh,&cpu,true,7,k,MAX_VERTICES).is_none());
        let refused=state.lookup[&Key {occurrence:0,..k}];
        assert!(state.banks[refused].instance.iter().all(Option::is_none),"token released");
        assert!(state.banks[refused].epoch.contains(&state.epoch),"the dropped slot stays reserved for this frame");
        let next=claim(&mut state,&mesh,&cpu,false,0,k,0).unwrap();assert_eq!(occurrence_of(&state,next.0),1);
        // Vertex capacity: a second large occurrence exceeds VERTICES.
        let (big,big_cpu)=surface(40_000,vec![0,1,2]);let mut state=on();state.next_frame();let k=key(actor,&big_cpu);
        assert!(claim(&mut state,&big,&big_cpu,false,0,k,0).is_some());
        assert!(claim(&mut state,&big,&big_cpu,false,0,k,0).is_none(),"vertex cap");
        assert!(claim(&mut state,&big,&big_cpu,false,0,k,0).is_none(),"vertex cap");
        assert_eq!(state.occurrences[&(actor,k.surface)],3);assert!(state.vertex_capacity<=VERTICES);
        // Index capacity: a second occurrence with 150,000 indices exceeds INDICES.
        let (wide,wide_cpu)=surface(3,[0,1,2].repeat(50_000));let mut state=on();state.next_frame();let k=key(actor,&wide_cpu);
        assert!(claim(&mut state,&wide,&wide_cpu,false,0,k,0).is_some());
        assert!(claim(&mut state,&wide,&wide_cpu,false,0,k,0).is_none(),"index cap");
        assert_eq!(state.occurrences[&(actor,k.surface)],2);assert!(state.index_capacity<=INDICES);
        // SURFACES full of banks used in this or the previous frame.
        let (mesh,cpu)=fixture();let mut state=on();state.next_frame();
        for surface in 0..SURFACES/4 {
            for _ in 0..4 {claim(&mut state,&mesh,&cpu,false,0,Key {actor,surface,occurrence:0},0).unwrap();}
        }
        assert_eq!(state.banks.len(),SURFACES);
        let late=Key {actor,surface:SURFACES,occurrence:0};
        assert!(claim(&mut state,&mesh,&cpu,false,0,late,0).is_none(),"every bank used this frame");
        assert_eq!(state.occurrences[&(actor,SURFACES)],1);
        state.next_frame();
        assert!(claim(&mut state,&mesh,&cpu,false,0,late,0).is_none(),"every bank used last frame");
        state.next_frame();
        assert!(claim(&mut state,&mesh,&cpu,false,0,late,0).is_some(),"unused banks are evicted under pressure");
        assert_eq!(state.banks.len(),SURFACES);
    }
    #[test]
    fn alternating_occurrence_order_never_skips_an_upload_or_overwrites_a_held_output() {
        let (mesh,cpu)=fixture();let mut state=on();let actor=ActorId::default();let k=key(actor,&cpu);
        // (instance token, last version) per (bank, slot); identical UVs isolate the instance stamp.
        let mut stamps:HashMap<(usize,usize),(u64,u64,prepared::Version)>=HashMap::new();
        let mut held:Vec<(Rc<prepared::Geometry>,prepared::Version)>=Vec::new();
        let (mut changed,mut warm)=(0,0);
        for frame in 0..8_u64 {
            state.next_frame();
            // Held outputs alternate each bank's slot every frame; a period of four
            // frames puts the other instance into a slot it did not upload last time.
            let order:&[u64]=if frame%4<2 {&[1,2]} else {&[2,1]};
            let mut current=Vec::new();
            for &instance in order {
                let c=claim(&mut state,&mesh,&cpu,frame>0,instance,k,0).unwrap();
                assert!(held.iter().all(|(g,_)|g.vertices.as_ptr() as usize!=c.2),"held output reused");
                let version=state.banks[c.0].slots[c.1].as_ref().unwrap().version;
                if let Some(&(old_instance,old_epoch,old_version))=stamps.get(&(c.0,c.1)) {
                    if old_instance!=instance {assert_ne!(version.vertices,old_version.vertices,"stale upload of another instance");changed+=1;}
                    else if old_epoch+1==state.epoch {assert_eq!(version,old_version);warm+=1;}
                }
                stamps.insert((c.0,c.1),(instance,state.epoch,version));
                current.push((geometry(&state,c),version));
            }
            for (g,version) in &held {assert_eq!(g.version,*version,"held output re-stamped");}
            held=current;
        }
        assert!(changed>0,"order changes moved instances between slots");
        println!("alternating order: {changed} instance changes advanced, {warm} warm reuses");
    }
    #[test]
    fn hidden_and_spawned_occurrences_evict_unused_banks_without_freeing_held_geometry() {
        let (mut mesh,cpu)=fixture();let mut state=on();let actor=ActorId::default();let k=key(actor,&cpu);
        state.next_frame();
        let claims:Vec<_>=(0..3).map(|_|claim(&mut state,&mesh,&cpu,false,0,k,0).unwrap()).collect();
        let held=geometry(&state,claims[2]);let held_ptr=held.vertices.as_ptr() as usize;
        assert_eq!(state.memory()[5],("actor_publication_mem_evicted_referenced_bytes",0));
        // Two occurrences disappear; their banks age out while one output stays referenced.
        for frame in 0..=UNUSED_FRAMES {
            state.next_frame();for v in &mut mesh.vertices {v.uv.y=frame as f32;}
            claim(&mut state,&mesh,&cpu,false,0,k,0).unwrap();
            if frame<UNUSED_FRAMES {assert_eq!(state.lookup.len(),3,"unused banks keep ownership until their age limit");}
        }
        assert_eq!(state.lookup.len(),1);assert_eq!(state.lookup.keys().next().unwrap().occurrence,0);
        assert!(held.vertices.iter().all(|v|v.uv==Vec2::ONE) && held.indices==[0,1,2],"evicted geometry was freed or written");
        let memory=state.memory();
        assert_eq!(memory[5],("actor_publication_mem_evicted_referenced_bytes",(held.vertices.capacity()*std::mem::size_of::<Vertex>()+held.indices.capacity()*2) as u64));
        assert_eq!(memory[0].1,((state.vertex_capacity*std::mem::size_of::<Vertex>()+state.index_capacity*2) as u64));
        // Spawn: four occurrences in one frame reuse the freed bank ids, never the held destination.
        state.next_frame();
        let spawned:Vec<_>=(0..4).map(|_|claim(&mut state,&mesh,&cpu,false,0,k,0).unwrap()).collect();
        assert!(spawned.iter().all(|c|c.2!=held_ptr));assert_eq!(state.lookup.len(),4);assert_eq!(state.banks.len(),4);
        drop(held);assert_eq!(state.memory()[5].1,0);
    }
    #[test]
    fn reset_after_a_frame_clears_occurrences_and_refuses_during_a_frame() {
        let (mesh,cpu)=fixture();let actor=ActorId::default();let _frame=retained();
        with_publication_occurrences(true,begin);
        assert!((0..3).all(|_|publish(&mesh,&cpu,key(actor,&cpu)).is_some()));
        assert!(std::panic::catch_unwind(reset).is_err(),"reset while active");
        end();reset();
        STATE.with(|s| {let s=s.borrow();
            assert!(s.occurrences.is_empty() && !s.occurrences_on && s.lookup.is_empty() && s.banks.is_empty());
            assert_eq!((s.vertex_capacity,s.index_capacity,s.pending.len(),s.held.len()),(0,0,0,0));
        });
    }
    #[test]
    fn warmed_occurrence_slots_allocate_nothing_in_later_frames() {
        let (mesh,cpu)=fixture();let mut state=on();let actor=ActorId::default();let k=key(actor,&cpu);
        let mut held:Vec<Rc<prepared::Geometry>>=Vec::with_capacity(MAX_OCCURRENCES as usize);
        let frame=|state:&mut State,held:&mut Vec<Rc<prepared::Geometry>>| {
            state.next_frame();
            let claims:[_;4]=std::array::from_fn(|_|claim(state,&mesh,&cpu,true,9,k,0).unwrap());
            held.clear();held.extend(claims.iter().map(|&c|geometry(state,c)));
        };
        // Frame 1 creates the banks and slot A; frame 2 creates slot B while A is held.
        let mut warmup=Vec::new();
        for _ in 0..3 {let scope=crate::kernel_alloc::Scope::begin();frame(&mut state,&mut held);warmup.push(scope.finish());}
        assert!(warmup[0]>0 && warmup[1]>0,"the alternate slot is cold until used: {warmup:?}");
        assert_eq!(warmup[2],0,"{warmup:?}");
        assert!(state.banks.iter().all(|b|b.slots.iter().all(Option::is_some)),"every slot warmed");
        let scope=crate::kernel_alloc::Scope::begin();
        for _ in 0..120 {frame(&mut state,&mut held);}
        assert_eq!(scope.finish(),0,"warmed occurrence frames allocate");
        assert!(state.occurrences.is_empty() || state.occurrences.values().all(|&n|n==MAX_OCCURRENCES));
        println!("occurrence warmup allocations per frame: {warmup:?}, then 0 over 120 frames");
    }
    #[test]
    fn worker_panic_leaves_no_pending_or_held_publication_for_the_next_frame() {
        use crate::character::skinning::overlap;
        let (mesh,cpu)=fixture();let actor=ActorId::default();let k=key(actor,&cpu);
        overlap::startup(2,false);let _frame=retained();
        with_publication_occurrences(true,|| {
            let failed=std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let mut frame=overlap::Frame::begin();
                assert!(active());
                assert!((0..3).all(|_|publish(&mesh,&cpu,k).is_some()));
                overlap::inject_worker_panic(|| {frame.dispatch();frame.collect();});
            }));
            assert!(failed.is_err(),"the worker failure propagates");
            assert!(!active());
            STATE.with(|s| {let s=s.borrow();
                assert!(s.pending.is_empty() && s.held.is_empty() && s.held_puffs.is_empty() && !s.completed);
                assert!(s.banks.iter().flat_map(|b|b.slots.iter().flatten()).all(|g|Rc::strong_count(g)==1),"an unpublished output escaped");
            });
            let mut frame=overlap::Frame::begin();
            assert!((0..3).all(|_|publish(&mesh,&cpu,k).is_some()),"the next frame reserves again");
            frame.dispatch();frame.collect();
            STATE.with(|s|assert!(s.borrow().completed));
            let published=FRAME.with(|f|f.borrow().groups.iter().flat_map(|g|&g.passes).filter(|p|p.geometry.is_some()).count());
            assert_eq!(published,3);
            drop(frame);
        });
    }
    #[test]
    fn publication_memory_figures_cover_occurrence_banks_and_their_caps() {
        use std::mem::size_of;
        let vertex=size_of::<Vertex>();
        println!("publication memory: host size_of::<Vertex>()={vertex} B, Bank={} B, Key={} B, occurrence entry={} B, pending entry={} B, Group={} B",
            size_of::<Bank>(),size_of::<Key>(),size_of::<((ActorId,usize),u16)>(),size_of::<(usize,usize,usize,usize)>(),size_of::<Group>());
        // Two extra occurrences, two slots each, at the contract's actor vertex counts.
        for (map,vertices) in [("skool2",1294_usize),("centipede1",1434)] {
            println!("publication memory sketch, not a measured heap delta: {map} two extra occurrences x2 slots: {} B at host {vertex} B/vertex, {} B at a 40 B/vertex layout sketch, plus 2 B per index",
                2*2*vertices*vertex,2*2*vertices*40);
        }
        // Exhaustion: every bank of four occurrences, owned capacity at the vertex cap.
        let (mesh,cpu)=surface(4096,vec![0,1,2]);let mut state=on();let actor=ActorId::default();
        let mut held=Vec::new();
        for _ in 0..2 {
            state.next_frame();held.clear();
            for surface in 0..8 {for _ in 0..4 {
                if let Some(c)=claim(&mut state,&mesh,&cpu,false,0,Key {actor,surface,occurrence:0},0) {held.push(geometry(&state,c));}
            }}
        }
        let memory=state.memory();
        assert!(state.vertex_capacity<=VERTICES && state.index_capacity<=INDICES);
        assert!(memory[0].1<=(VERTICES*vertex+INDICES*2) as u64,"bank-owned bytes exceed the caps");
        println!("publication memory: bank-owned cap check {memory:?}; vertex/index cap {} B. This is not a total heap bound.",VERTICES*vertex+INDICES*2);
    }
    /// Q1 F4. Owner/lifetime inventory at cold, warm (both slots), overlap,
    /// release, exhaustion and external-holder eviction. Every geometry allocation
    /// appears in exactly one class, and the classes add up to bank-owned bytes.
    #[test]
    fn publication_memory_inventory_lists_each_allocation_once_by_owner_and_lifetime() {
        use std::collections::{BTreeMap,BTreeSet};
        use std::mem::size_of;
        fn rows(state:&mut State)->BTreeMap<&'static str,u64> {
            let rows:BTreeMap<_,_>=state.memory().into_iter().collect();
            // Independent identity check: no allocation is listed twice.
            let mut seen=BTreeSet::new();
            for g in state.banks.iter().flat_map(|b|b.slots.iter().flatten()) {
                assert!(seen.insert(Rc::as_ptr(g) as usize),"a bank slot repeats an allocation");
                if g.vertices.capacity()>0 {assert!(seen.insert(g.vertices.as_ptr() as usize));}
            }
            for g in state.evicted_referenced.iter().filter_map(std::rc::Weak::upgrade) {
                assert!(seen.insert(Rc::as_ptr(&g) as usize),"evicted geometry is still bank-owned");
            }
            assert_eq!(rows["actor_publication_mem_slots_free_bytes"]+rows["actor_publication_mem_slots_current_bytes"]
                +rows["actor_publication_mem_slots_held_bytes"],rows["actor_publication_mem_owned_bytes"],"classes cover bank-owned bytes once");
            rows
        }
        fn show(stage:&str,calls:Option<u64>,rows:&BTreeMap<&'static str,u64>) {
            println!("PUBLICATION_INVENTORY {}",serde_json::json!({"stage":stage,"allocation_calls":calls,"rows":rows,
                "layout":"host","vita_abi_measured":false}));
        }
        println!("publication inventory host layout: Vertex {} B, Geometry {} B, Rc allocation {} B, Bank {} B, Pass {} B",
            size_of::<Vertex>(),size_of::<prepared::Geometry>(),2*size_of::<usize>()+size_of::<prepared::Geometry>(),size_of::<Bank>(),size_of::<Pass>());
        let (mesh,cpu)=fixture();let mut state=on();let k=key(ActorId::default(),&cpu);
        let scope=crate::kernel_alloc::Scope::begin();state.next_frame();let calls=scope.finish();
        let r=rows(&mut state);show("cold",Some(calls),&r);
        assert_eq!((calls,r["actor_publication_mem_geometry_allocations"],r["actor_publication_mem_owned_bytes"]),(0,0,0));
        // Slot A, then slot B while A is still referenced by an outside holder.
        let scope=crate::kernel_alloc::Scope::begin();let a=claim(&mut state,&mesh,&cpu,false,1,k,0).unwrap();let calls=scope.finish();
        let holder=geometry(&state,a);
        let r=rows(&mut state);show("warm-slot-a",Some(calls),&r);
        assert!(calls>0);assert_eq!((r["actor_publication_mem_slots_current"],r["actor_publication_mem_geometry_allocations"]),(1,3));
        state.next_frame();
        let scope=crate::kernel_alloc::Scope::begin();let b=claim(&mut state,&mesh,&cpu,true,1,k,0).unwrap();let calls=scope.finish();
        assert_ne!(a.1,b.1);
        let r=rows(&mut state);show("warm-slot-b",Some(calls),&r);
        assert!(calls>0,"the alternate slot is cold until used");
        assert_eq!((r["actor_publication_mem_slots_current"],r["actor_publication_mem_slots_held"]),(1,1));
        // Both slots hold a handle to the same CPU surface allocation: two handles,
        // one allocation, counted once in the referenced-bytes row.
        assert_eq!((r["actor_publication_mem_cpu_handles"],r["actor_publication_mem_cpu_arc_allocations"]),(2,1),
            "CPU handles name one shared surface allocation");
        assert_eq!(r["actor_publication_mem_cpu_arc_referenced_bytes"],(2*size_of::<usize>()+size_of::<SkinSurface>()) as u64);
        assert_eq!((r["actor_publication_mem_external_frame_groups"],r["actor_publication_mem_external_frame_geometry_refs"]),(0,0),
            "no FRAME group is holding an unpublished protocol here");
        // A FRAME group that references a bank slot names the same allocation: the
        // external rows grow, the unique class bytes do not.
        FRAME.with(|f| f.borrow_mut().groups.push(Group {passes:vec![Pass {geometry:Some(holder.clone()),..pass()}],transparent:false}));
        let r=rows(&mut state);show("frame-holder",None,&r);
        assert_eq!((r["actor_publication_mem_external_frame_groups"],r["actor_publication_mem_external_frame_geometry_refs"],
            r["actor_publication_mem_external_frame_not_bank_owned"]),(1,1,0),"a FRAME holder aliases a bank-owned allocation");
        assert_eq!(r["actor_publication_mem_external_frame_geometry_bytes"],
            (holder.vertices.capacity()*size_of::<Vertex>()+holder.indices.capacity()*2) as u64);
        FRAME.with(|f| f.borrow_mut().groups.clear());
        drop(holder);let holder=geometry(&state,b);
        // Both slots warm: an overlapping frame allocates nothing.
        state.next_frame();
        let scope=crate::kernel_alloc::Scope::begin();let c=claim(&mut state,&mesh,&cpu,true,1,k,0).unwrap();let calls=scope.finish();
        assert_eq!(c.1,a.1);
        let r=rows(&mut state);show("overlap",Some(calls),&r);
        assert_eq!(calls,0,"zero additional allocations once both slots are warm");
        assert_eq!((r["actor_publication_mem_slots_current"],r["actor_publication_mem_slots_held"],r["actor_publication_mem_slots_free"]),(1,1,0));
        drop(holder);state.next_frame();
        let r=rows(&mut state);show("released",None,&r);
        assert_eq!((r["actor_publication_mem_slots_free"],r["actor_publication_mem_slots_held"]),(2,0));
        // Nested pass vectors owned by held groups are their own row.
        state.held.push(Group {passes:Vec::with_capacity(4),transparent:false});
        let r=rows(&mut state);show("held-group",None,&r);
        assert_eq!(r["actor_publication_mem_held_pass_capacity"],4);
        assert_eq!(r["actor_publication_mem_held_pass_bytes"],(4*size_of::<Pass>()) as u64);
        state.held.clear();
        // Exhaustion: distinct surfaces until the vertex cap refuses.
        let (big,big_cpu)=surface(4096,vec![0,1,2]);let mut state=on();let actor=ActorId::default();
        state.next_frame();
        let mut holders=Vec::new();
        let scope=crate::kernel_alloc::Scope::begin();
        let mut fitted=0;
        while let Some(c)=claim(&mut state,&big,&big_cpu,false,0,Key {actor,surface:fitted,occurrence:0},0) {holders.push(geometry(&state,c));fitted+=1;}
        let calls=scope.finish();
        let r=rows(&mut state);show("exhaustion",Some(calls),&r);
        assert_eq!(fitted,VERTICES/4096);
        assert!(r["actor_publication_mem_owned_bytes"]<=(VERTICES*size_of::<Vertex>()+INDICES*2) as u64);
        // The holders outlive their banks: aged eviction drops bank ownership only.
        for _ in 0..=UNUSED_FRAMES {state.next_frame();}
        let r=rows(&mut state);show("evicted-held",None,&r);
        assert_eq!((r["actor_publication_mem_owned_bytes"],r["actor_publication_mem_evicted_referenced"]),(0,fitted as u64));
        let evicted=r["actor_publication_mem_evicted_referenced_bytes"];
        // A second fill while the first holders live: bank-owned stays under the
        // cap, but bank-owned plus externally held bytes exceeds it.
        let mut second=Vec::new();
        let mut n=0;
        while let Some(c)=claim(&mut state,&big,&big_cpu,false,0,Key {actor,surface:1000+n,occurrence:0},0) {second.push(geometry(&state,c));n+=1;}
        let r=rows(&mut state);show("refill-with-external-holders",None,&r);
        let cap=(VERTICES*size_of::<Vertex>()+INDICES*2) as u64;
        assert!(r["actor_publication_mem_owned_bytes"]<=cap);
        assert!(r["actor_publication_mem_owned_bytes"]+evicted>cap,"external holders keep bytes outside the bank cap");
        println!("publication inventory: bank-owned bytes stay under {cap} B; geometry an external holder keeps after eviction is outside that cap, so total retained bytes are bounded only by how long holders live");
        drop((holders,second));
        let r=rows(&mut state);show("holders-dropped",None,&r);
        assert_eq!(r["actor_publication_mem_evicted_referenced"],0);
    }
}
