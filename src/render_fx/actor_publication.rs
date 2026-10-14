//! Two owned banks per logical surface. Only numeric array addresses reach workers.
//! Pending packets contain handles, never borrowed/reused worker output. Collection
//! is the sole publication boundary; retained GPU storage keeps its five-slot ring.
use super::*;
use crate::skeletal::SkinSurface;
use std::{cell::Cell, sync::Arc};
const SURFACES:usize=2048;
const VERTICES:usize=65536;
const INDICES:usize=262144;
#[derive(Default)]
struct Bank { slots:[Option<Rc<prepared::Geometry>>;2], cpu:[Option<Arc<SkinSurface>>;2], alpha:[Option<bool>;2], epoch:[u64;2] }
#[derive(Default)]
struct State {
    banks:Vec<Bank>, cursor:usize, pending:Vec<(usize,usize,usize)>,
    held:Vec<Group>, held_puffs:Vec<retained::world_puffs::Puff>, completed:bool, vertex_capacity:usize, index_capacity:usize, epoch:u64,
}
impl State {
    #[cfg(test)]
    fn reserve(&mut self,mesh:&Mesh,cpu:Arc<SkinSurface>,unchanged:bool,alpha:bool)->Option<(usize,usize,usize)> {
        let id=self.cursor;self.cursor+=1;self.reserve_at(mesh,cpu,unchanged,alpha,id)
    }
    fn reserve_at(&mut self,mesh:&Mesh,cpu:Arc<SkinSurface>,unchanged:bool,alpha:bool,id:usize)->Option<(usize,usize,usize)> {
        let s=self;
        if id>=SURFACES {return None;}
        while s.banks.len()<=id {s.banks.push(Bank::default());}
        let slot=s.banks[id].slots.iter().position(|g|g.as_ref().is_none_or(|g|Rc::strong_count(g)==1))?;
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
        crate::actor_prep_profile::count(crate::actor_prep_profile::Count::CopiedBytes,mesh.vertices.len()*8);
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
pub(crate) fn begin() {
    let c=crate::render::vita_renderer::config();
    let enabled=c.actor_single_prepare && c.skin_overlap && c.actor_fast_path
        && c.actor_layer_cache && c.fx_prepare_cache && !c.actor_layer_reuse && retained_active();
    ACTIVE.with(|a|a.set(enabled));
    if enabled { STATE.with(|s| {let mut s=s.borrow_mut();s.cursor=0;s.epoch=s.epoch.checked_add(1).expect("actor publication epoch overflow");s.pending.clear();s.held.clear();s.held_puffs.clear();s.completed=false;}); }
}
pub(crate) fn end() {
    if ACTIVE.with(|a|a.replace(false)) { STATE.with(|s| {let mut s=s.borrow_mut();s.pending.clear();s.held.clear();s.held_puffs.clear();}); }
}
/// This address is valid only between prepare and the fully drained collect. Banks
/// cannot be mutated again while any old packet/view still owns their publication.
pub(crate) fn prepare(mesh:&Mesh,cpu:Arc<SkinSurface>,unchanged:bool,id:usize)->Option<(usize,bool)> {
    let _span=crate::actor_prep_profile::actor_span(crate::actor_prep_profile::Phase::Simple);
    let texture=mesh.texture.as_ref()?;
    let surface=SURFACE_LOOKUP.with(|s|s.borrow().get(&texture.raw_miniquad_id()).and_then(Weak::upgrade))?;
    if SKIN_OPACITY.with(Cell::get)!=1. {return None;}
    let (time,appearance)=FRAME.with(|f| {let f=f.borrow();(f.time,f.appearance)});
    let (passes,alpha)=actor_cache::recipe(&surface,time,appearance)?;
    STATE.with(|s| {
        let mut s=s.borrow_mut();
        let (id,slot,output)=s.reserve_at(mesh,cpu,unchanged,alpha,id)?;
        let group=FRAME.with(|f| {
            let mut f=f.borrow_mut();let n=mesh.vertices.len()*passes.len();
            if f.vertices.saturating_add(n)>MAX_VERTICES {f.dropped+=1;return None;}
            f.vertices+=n;let id=f.groups.len();
            let transparent=surface.transparent||f.depth_read_only;
            f.groups.push(Group {passes,transparent});Some(id)
        })?;
        s.pending.push((group,id,slot));
        Some((output,alpha))
    })
}
/// Separate staging preserves world/actor and equal-depth packet barriers and ties.
pub(crate) fn hold() {
    STATE.with(|s| {let mut s=s.borrow_mut();FRAME.with(|f| {let mut f=f.borrow_mut();
        std::mem::swap(&mut s.held,&mut f.groups);std::mem::swap(&mut s.held_puffs,&mut f.world_puffs);
    });});
}
pub(crate) fn complete() {
    let _span=crate::actor_prep_profile::span(crate::actor_prep_profile::Phase::Simple);
    STATE.with(|s| {
        let mut s=s.borrow_mut();
        for n in 0..s.pending.len() {
            let (group,bank,slot)=s.pending[n];
            let geometry=s.banks[bank].slots[slot].as_ref().unwrap().clone();
            for pass in &mut s.held[group].passes {pass.geometry=Some(geometry.clone());}
        }
        retained::count("actor_publication_capacity_bytes",(s.vertex_capacity*std::mem::size_of::<Vertex>()+s.index_capacity*2) as u64);
        retained::count("actor_publication_banks",s.banks.len() as u64);
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
        state.cursor=0;state.epoch=1;let version=state.banks[0].slots[0].as_ref().unwrap().version;
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
