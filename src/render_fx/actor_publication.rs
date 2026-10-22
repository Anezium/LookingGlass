//! Two owned banks per logical surface. Only numeric array addresses reach workers.
//! Pending packets contain handles, never borrowed/reused worker output. Collection
//! is the sole publication boundary; retained GPU storage keeps its five-slot ring.
use super::*;
use crate::skeletal::SkinSurface;
use std::{cell::Cell, collections::HashMap, sync::Arc};
use crate::character::skinning::identity::ActorId;
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct Key { pub actor: ActorId, pub surface: usize }
const UNUSED_FRAMES:u64=120;
const SURFACES:usize=2048;
const VERTICES:usize=65536;
const INDICES:usize=262144;
#[derive(Default)]
struct Bank { key:Option<Key>, last_used:u64, slots:[Option<Rc<prepared::Geometry>>;2], cpu:[Option<Arc<SkinSurface>>;2], alpha:[Option<bool>;2], epoch:[u64;2] }
#[derive(Default)]
struct State {
    banks:Vec<Bank>, lookup:HashMap<Key,usize>,
    #[cfg(test)] cursor:usize,
    pending:Vec<(usize,usize,usize)>,
    held:Vec<Group>, held_puffs:Vec<retained::world_puffs::Puff>, completed:bool, vertex_capacity:usize, index_capacity:usize, epoch:u64,
}
impl State {
    fn clear(&mut self) { *self=Self::default(); }
    fn evict(&mut self,id:usize) {
        let bank=&mut self.banks[id];
        if let Some(key)=bank.key.take() { self.lookup.remove(&key); }
        for g in bank.slots.iter().flatten() {
            self.vertex_capacity-=g.vertices.capacity(); self.index_capacity-=g.indices.capacity();
        }
        *bank=Bank::default();
    }
    fn next_frame(&mut self) {
        self.epoch=self.epoch.checked_add(1).expect("actor publication epoch overflow");
        self.pending.clear(); self.held.clear(); self.held_puffs.clear(); self.completed=false;
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
    fn reserve_key(&mut self,mesh:&Mesh,cpu:Arc<SkinSurface>,unchanged:bool,alpha:bool,key:Key)->Option<(usize,usize,usize)> {
        let id=if let Some(&id)=self.lookup.get(&key) {id} else {
            let id=if let Some(id)=self.banks.iter().position(|b|b.key.is_none()) {id}
                else if self.banks.len()<SURFACES {self.banks.push(Bank::default());self.banks.len()-1}
                else {let id=self.oldest_unused()?;self.evict(id);id};
            self.banks[id].key=Some(key); self.lookup.insert(key,id); id
        };
        self.banks[id].last_used=self.epoch;
        loop {
            if let Some(result)=self.reserve_at(mesh,cpu.clone(),unchanged,alpha,id) {return Some(result);}
            // Held slots cannot be reclaimed; evict only unrelated unused ids.
            if self.banks[id].slots.iter().enumerate().all(|(slot,g)|g.as_ref().is_some_and(|g|Rc::strong_count(g)>1 || self.banks[id].epoch[slot]==self.epoch)) {return None;}
            self.evict(self.oldest_unused()?);
        }
    }

    #[cfg(test)]
    fn reserve(&mut self,mesh:&Mesh,cpu:Arc<SkinSurface>,unchanged:bool,alpha:bool)->Option<(usize,usize,usize)> {
        let id=self.cursor;self.cursor+=1;self.reserve_at(mesh,cpu,unchanged,alpha,id)
    }
    fn reserve_at(&mut self,mesh:&Mesh,cpu:Arc<SkinSurface>,unchanged:bool,alpha:bool,id:usize)->Option<(usize,usize,usize)> {
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
        if !(unchanged && same_cpu && same_uv && bank.alpha[slot]==Some(alpha) && bank.epoch[slot]==s.epoch.saturating_sub(1)) {g.advance_owned(!same_cpu);}
        bank.alpha[slot]=Some(alpha);bank.epoch[slot]=s.epoch;
        let output=g.mesh.vertices.as_mut_ptr() as usize;
        let new_capacity=(g.vertices.capacity(),g.indices.capacity());
        bank.cpu[slot]=Some(cpu);
        s.vertex_capacity=s.vertex_capacity-old_capacity.0+new_capacity.0;
        s.index_capacity=s.index_capacity-old_capacity.1+new_capacity.1;
        Some((id,slot,output))
    }
}
thread_local! {
    static ACTIVE:Cell<bool>=const {Cell::new(false)};
    static STATE:RefCell<State>=RefCell::default();
}
pub(crate) fn active()->bool { ACTIVE.with(Cell::get) }
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
    if enabled { STATE.with(|s| {let mut s=s.borrow_mut();s.next_frame();}); }
}
pub(crate) fn end() {
    if ACTIVE.with(|a|a.replace(false)) { STATE.with(|s| {let mut s=s.borrow_mut();s.pending.clear();s.held.clear();s.held_puffs.clear();}); }
}
/// This address is valid only between prepare and the fully drained collect. Banks
/// cannot be mutated again while any old packet/view still owns their publication.
pub(crate) fn prepare(mesh:&Mesh,cpu:Arc<SkinSurface>,unchanged:bool,key:Key)->Result<(usize,bool,usize), &'static str> {
    let texture=mesh.texture.as_ref().ok_or("skin_reject_texture")?;
    let surface=SURFACE_LOOKUP.with(|s|s.borrow().get(&texture.raw_miniquad_id()).and_then(Weak::upgrade)).ok_or("skin_reject_registration")?;
    if SKIN_OPACITY.with(Cell::get)!=1. {return Err("skin_reject_opacity");}
    let (time,appearance)=FRAME.with(|f| {let f=f.borrow();(f.time,f.appearance)});
    let (passes,alpha)=actor_cache::recipe(&surface,time,appearance).ok_or_else(||actor_cache::publication_rejection(&surface))?;
    let layer_count=passes.len();
    STATE.with(|s| {
        let mut s=s.borrow_mut();
        let (id,slot,output)=s.reserve_key(mesh,cpu,unchanged,alpha,key).ok_or("skin_reject_bank")?;
        let group=FRAME.with(|f| {
            let mut f=f.borrow_mut();let n=mesh.vertices.len()*passes.len();
            if f.vertices.saturating_add(n)>MAX_VERTICES {f.dropped+=1;return None;}
            f.vertices+=n;let id=f.groups.len();
            let transparent=surface.transparent||f.depth_read_only;
            f.groups.push(Group {passes,transparent});Some(id)
        }).ok_or("skin_reject_frame_budget")?;
        s.pending.push((group,id,slot));
        Ok((output,alpha,layer_count))
    })
}
/// Separate staging preserves world/actor and equal-depth packet barriers and ties.
pub(crate) fn hold() {
    STATE.with(|s| {let mut s=s.borrow_mut();FRAME.with(|f| {let mut f=f.borrow_mut();
        std::mem::swap(&mut s.held,&mut f.groups);std::mem::swap(&mut s.held_puffs,&mut f.world_puffs);
    });});
}
pub(crate) fn complete() {
    STATE.with(|s| {
        let mut s=s.borrow_mut();
        for n in 0..s.pending.len() {
            let (group,bank,slot)=s.pending[n];
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

#[cfg(test)]
mod tests {
    use super::*;
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
            if first {state.reserve_key(&mesh,cpu.clone(),true,false,key(actors[0],&cpu)).unwrap();}
            if state.epoch==5 {state.reserve_key(&mesh,cpu.clone(),false,false,key(actors[2],&cpu)).unwrap();}
            let (id,slot,ptr)=state.reserve_key(&mesh,cpu.clone(),true,false,key(actors[1],&cpu)).unwrap();
            let stamp=state.banks[id].slots[slot].as_ref().unwrap().version;
            if let Some((old_id,old_ptr,old_stamp))=later {assert_eq!((id,ptr,stamp),(old_id,old_ptr,old_stamp));}
            later=Some((id,ptr,stamp));
        }
        assert_eq!(state.lookup.len(),3);
    }
    #[test]
    fn repeated_actor_surface_in_one_frame_uses_disjoint_destinations() {
        let (mesh,cpu)=fixture();let mut state=State::default();state.next_frame();
        let key=key(ActorId::default(),&cpu);
        let a=state.reserve_key(&mesh,cpu.clone(),false,false,key).unwrap();
        let b=state.reserve_key(&mesh,cpu.clone(),false,false,key).unwrap();
        assert_ne!(a.2,b.2);assert!(state.reserve_key(&mesh,cpu,false,false,key).is_none());
    }
    #[test]
    fn unused_actor_ids_evict_at_age_and_under_capacity_pressure_without_touching_held_views() {
        let (mesh,cpu)=fixture();let mut state=State::default();state.next_frame();
        let old_key=key(ActorId::default(),&cpu);
        let (id,slot,_)=state.reserve_key(&mesh,cpu.clone(),false,false,old_key).unwrap();
        let held=state.banks[id].slots[slot].as_ref().unwrap().clone();
        for _ in 0..=UNUSED_FRAMES {state.next_frame();}
        assert!(state.lookup.is_empty());assert_eq!(state.vertex_capacity,0);
        assert_eq!(held.indices,[0,1,2]);assert_eq!(Arc::strong_count(&cpu),1);
        // Slot-count pressure reuses an unused id, never grows the bounded table.
        for _ in 0..SURFACES {state.reserve_key(&mesh,cpu.clone(),false,false,key(ActorId::default(),&cpu)).unwrap();}
        assert!(state.reserve_key(&mesh,cpu.clone(),false,false,key(ActorId::default(),&cpu)).is_none());
        state.next_frame();state.next_frame();
        assert!(state.reserve_key(&mesh,cpu,false,false,Key {actor:ActorId::default(),surface:0}).is_some());
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
            for _ in 0..5 {assert!(state.reserve_key(&mesh,cpu.clone(),false,false,key(ActorId::default(),&cpu)).is_some(),"unexpected serial fallback after map/reload");}
            let capacity=(state.vertex_capacity,state.index_capacity,state.lookup.len());
            if let Some(warm)=warm {assert_eq!(capacity,warm);} else {warm=Some(capacity);}
            held=state.banks[0].slots[0].clone();old_cpu=Some(Arc::downgrade(&cpu));
        }
        state.clear();assert!(old_cpu.unwrap().upgrade().is_none());
    }
    #[test]
    fn held_views_exhaust_two_banks_and_never_overwrite_old_vertices() {
        let (mut mesh,cpu)=fixture();let mut state=State::default();
        let (id,slot,ptr)=state.reserve(&mesh,cpu.clone(),false,false).unwrap();
        let old=state.banks[id].slots[slot].as_ref().unwrap().clone();
        assert_eq!(ptr,old.vertices.as_ptr() as usize);
        mesh.vertices[0].uv.x=-0.;state.cursor=0;
        let (id,slot,_)=state.reserve(&mesh,cpu.clone(),false,false).unwrap();
        let second=state.banks[id].slots[slot].as_ref().unwrap().clone();
        assert_eq!(old.vertices[0].uv.x.to_bits(),1_f32.to_bits());
        assert_eq!(second.vertices[0].uv.x.to_bits(),(-0_f32).to_bits());
        state.cursor=0;assert!(state.reserve(&mesh,cpu.clone(),false,false).is_none());
        drop(old);state.cursor=0;assert!(state.reserve(&mesh,cpu,false,false).is_some());
        assert_eq!(state.vertex_capacity,6);assert_eq!(state.index_capacity,6);
    }
    #[test]
    fn reclaimed_old_view_cannot_reuse_a_newer_inputs_revision() {
        let (mesh,cpu)=fixture();let mut state=State::default();state.epoch=1;
        state.reserve_at(&mesh,cpu.clone(),false,false,0).unwrap();
        let old=state.banks[0].slots[0].as_ref().unwrap().clone();let version=old.version;
        state.epoch=2;state.reserve_at(&mesh,cpu.clone(),false,false,0).unwrap();
        let second=state.banks[0].slots[1].as_ref().unwrap().clone();
        drop(old);state.epoch=3;
        state.reserve_at(&mesh,cpu,true,false,0).unwrap();
        assert_ne!(state.banks[0].slots[0].as_ref().unwrap().version.vertices,version.vertices);
        assert_eq!(second.vertices[0].uv,Vec2::ONE);
    }
    #[test]
    fn warmed_storage_is_bounded_and_map_replacement_copies_new_topology() {
        let (mesh,cpu)=fixture();let mut state=State::default();
        state.reserve(&mesh,cpu.clone(),false,false).unwrap();
        let scope=crate::kernel_alloc::Scope::begin();
        for _ in 0..128 {state.cursor=0;assert!(state.reserve(&mesh,cpu.clone(),false,false).is_some());}
        assert_eq!(scope.finish(),0);assert_eq!(state.banks.len(),1);
        state.cursor=0;let version=state.banks[0].slots[0].as_ref().unwrap().version;
        state.reserve(&mesh,cpu.clone(),true,false).unwrap();
        assert_eq!(state.banks[0].slots[0].as_ref().unwrap().version,version);
        let (_,new_cpu)=fixture();state.cursor=0;
        let before=state.banks[0].slots[0].as_ref().unwrap().version;
        state.reserve(&mesh,new_cpu,false,false).unwrap();
        assert_ne!(state.banks[0].slots[0].as_ref().unwrap().version.topology,before.topology);
        state.cursor=SURFACES;assert!(state.reserve(&mesh,cpu.clone(),false,false).is_none());
        state.cursor=0;state.vertex_capacity=VERTICES;
        let mut larger=mesh;larger.vertices.push(Vertex::new(0.,0.,0.,0.,0.,WHITE));
        assert!(state.reserve(&larger,cpu,false,false).is_none());
    }
}
