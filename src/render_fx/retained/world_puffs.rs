//! Direct quad publications after the exact joint-alpha order is known.
//! Non-particle packets, invalid meshes and authored multi-pass layers are barriers.
use super::*;
use crate::materials::{Blend, Stage};
use std::rc::Rc;

pub(crate) enum PuffVertices {Legacy([Vertex;4]),Fused(crate::particles::quad::NumericQuad)}
impl PuffVertices {
    pub(crate) fn center(&self)->Vec3 {match self {Self::Legacy(v)=>center_vertices(v),Self::Fused(n)=>n.center()}}
    #[cfg(test)]
    fn array(&self)->[Vertex;4] {match self {Self::Legacy(v)=>*v,Self::Fused(n)=>std::array::from_fn(|i|n.vertex(i))}}
    #[cfg(test)]
    fn legacy_mut(&mut self)->&mut [Vertex;4] {match self {Self::Legacy(v)=>v,_=>panic!("legacy test fixture")}}
}
pub(crate) struct Puff {
    pub vertices: PuffVertices,
    pub texture: Option<Texture2D>,
    pub blend: Blend,
    pub before_group: usize,
}
fn normalized(blend: Blend) -> Blend { if blend == Blend::Add { Blend::AlphaAdd } else { blend } }
fn puff_key(p: &Puff) -> PacketKey {
    let stage = Stage::default();
    PacketKey { texture: p.texture.as_ref().map(Texture2D::raw_miniquad_id), state: [
        crate::render::blend_index(normalized(p.blend)) as u32, 0, 0, u32::from(stage.clamp),
        (stage.alpha_test as f32).to_bits(), stage.alpha_reference.to_bits(),
        1f32.to_bits(), 1f32.to_bits(), 1f32.to_bits(), 1f32.to_bits(),
        super::super::fog(normalized(p.blend)).to_bits(), 1, 0, 0, 0, 0, 1, 0, 0, 0,
    ] }
}
fn meta(source: Source, groups: &[super::super::Group], puffs: &[Puff]) -> Option<(PacketKey, usize, usize)> {
    match source {
        Source::Puff(id) => Some((puff_key(&puffs[id]), 4, 6)),
        Source::Actor(id) => {
            let g = &groups[id];
            let p = g.passes.first()?;
            (g.passes.len() == 1 && p.effect && p.geometry.as_ref()
                .map_or_else(|| valid_mesh(p.geometry()), |g| g.valid))
                .then(|| (packet_key(p), p.geometry().vertices.len(), p.geometry().indices.len()))
        }
        Source::World(_) => None,
    }
}
enum Vertices<'a> {Slice(&'a [Vertex]),Fused(&'a crate::particles::quad::NumericQuad)}
impl Vertices<'_> {
    fn len(&self)->usize {match self {Self::Slice(v)=>v.len(),Self::Fused(_)=>4}}
    fn same(&self,old:&[Vertex])->bool {match self {Self::Slice(v)=>vertices_equal(old,v),Self::Fused(n)=>n.same(old)}}
    fn append(&self,target:&mut Vec<Vertex>) {match self {Self::Slice(v)=>target.extend_from_slice(v),Self::Fused(n)=>n.append(target)}}
}
fn vertices<'a>(source: Source, groups: &'a [super::super::Group], puffs: &'a [Puff]) -> Vertices<'a> {
    match source {
        Source::Actor(id) => Vertices::Slice(&groups[id].passes[0].geometry().vertices),
        Source::Puff(id) => match &puffs[id].vertices {PuffVertices::Legacy(v)=>Vertices::Slice(v),PuffVertices::Fused(n)=>Vertices::Fused(n)},
        Source::World(_) => unreachable!(),
    }
}
fn indices(source: Source, groups: &[super::super::Group]) -> &[u16] {
    match source {
        Source::Actor(id) => &groups[id].passes[0].geometry().indices,
        Source::Puff(_) => &[0, 1, 2, 0, 2, 3],
        Source::World(_) => unreachable!(),
    }
}
struct Batch {
    members: Vec<Source>,
    geometry: Rc<super::super::prepared::Geometry>,
    output: usize,
}
impl Default for Batch {
    fn default() -> Self {
        Self { members: Vec::new(), output: 0,
            geometry: Rc::new(super::super::prepared::Geometry::new(Mesh {
                vertices: Vec::new(), indices: Vec::new(), texture: None })) }
    }
}
#[derive(Default)]
pub(crate) struct Bank {
    batches: Vec<Batch>,
    candidates: Vec<(usize, usize, usize, PacketKey)>,
    kept: Vec<(f32, Source, Option<usize>)>,
    live: usize,
}
impl Bank {
    /// Same first-fit material policy and 60000/180000 bounds as merge_cached.
    /// Only effect packets enter this plan; actor merging remains in its old path.
    fn plan(&mut self, groups: &[super::super::Group], puffs: &[Puff], list: &[(f32, Source)], material: bool) {
        self.live = 0;
        self.candidates.clear(); self.kept.clear();
        for &(depth, source) in list {
            let Some((key, vc, ic)) = meta(source, groups, puffs) else {
                self.candidates.clear(); self.kept.push((depth, source, None)); continue;
            };
            if !material && self.candidates.len() > 1 {
                let last = self.candidates.pop().unwrap(); self.candidates.clear(); self.candidates.push(last);
            }
            let found = self.candidates.iter().position(|c| c.3 == key && c.1 + vc <= 60000 && c.2 + ic <= 180000);
            let target = if let Some(i) = found {
                let c = &mut self.candidates[i]; c.1 += vc; c.2 += ic; c.0
            } else {
                if !material { self.candidates.clear(); }
                let id = self.live; self.live += 1;
                if self.batches.len() <= id { self.batches.push(Batch::default()); }
                self.batches[id].members.clear();
                self.candidates.push((id, vc, ic, key));
                self.kept.push((depth, source, Some(id)));
                id
            };
            self.batches[target].members.push(source);
        }
    }
    fn pack(batch: &mut Batch, groups: &[super::super::Group], puffs: &[Puff]) -> Rc<super::super::prepared::Geometry> {
        if Rc::get_mut(&mut batch.geometry).is_none() { batch.geometry = Batch::default().geometry; }
        let g = Rc::get_mut(&mut batch.geometry).unwrap();
        let mut vb = 0; let mut ib = 0;
        let mut same_vertices = true; let mut same_indices = true;
        for &source in &batch.members {
            let v = vertices(source, groups, puffs); let i = indices(source, groups);
            same_vertices &= g.vertices.get(vb..vb+v.len()).is_some_and(|old| v.same(old));
            same_indices &= g.indices.get(ib..ib+i.len()).is_some_and(|old|
                old.iter().zip(i).all(|(&a,&b)| a as usize == b as usize + vb));
            vb += v.len(); ib += i.len();
        }
        same_vertices &= g.vertices.len() == vb;
        same_indices &= g.indices.len() == ib;
        assert!(vb <= 60000 && ib <= 180000);
        if !same_vertices { g.mesh.vertices.clear(); }
        if !same_indices { g.mesh.indices.clear(); }
        let mut base = 0;
        for &source in &batch.members {
            let v = vertices(source, groups, puffs);
            if !same_vertices {
                v.append(&mut g.mesh.vertices);
                if matches!(source,Source::Puff(_)) {count("puff_final_vertex_write_bytes",(4*std::mem::size_of::<Vertex>()) as u64);}
            }
            if !same_indices { g.mesh.indices.extend(indices(source, groups).iter().map(|&i| i + base as u16)); }
            base += v.len();
        }
        if !same_vertices || !same_indices {
            let old = g.version;
            let mesh = std::mem::replace(&mut g.mesh, Mesh { vertices: Vec::new(), indices: Vec::new(), texture: None });
            *g = super::super::prepared::Geometry::merged(mesh, Some(old), same_indices);
            if same_vertices { g.version.vertices = old.vertices; }
        }
        batch.geometry.clone()
    }
    pub(super) fn build(&mut self, groups: &mut Vec<super::super::Group>, puffs: &[Puff], list: &mut Vec<(f32, Source)>, material: bool) {
        self.plan(groups, puffs, list, material);
        let mut produced = 0;
        for batch in &mut self.batches[..self.live] {
            let first = batch.members[0];
            if batch.members.iter().any(|s| matches!(s, Source::Puff(_))) { produced += 1; }
            // A lone pre-existing effect already has its correct publication.
            if batch.members.len() == 1 && matches!(first, Source::Actor(_)) {
                if let Source::Actor(id) = first { batch.output = id; }
                continue;
            }
            let geometry = Self::pack(batch, groups, puffs);
            let mut p = match first {
                Source::Actor(id) => groups[id].passes.pop().unwrap(),
                Source::Puff(id) => {
                    let puff = &puffs[id];
                    super::super::pass(Mesh { vertices: Vec::new(), indices: Vec::new(), texture: puff.texture.clone() },
                        &Stage { blend: normalized(puff.blend), ..Default::default() }, true, false,
                        super::super::Appearance::default()).expect("direct puff pass")
                }
                Source::World(_) => unreachable!(),
            };
            p.effect = true; p.geometry = Some(geometry);
            for &source in &batch.members {
                if let Source::Actor(id) = source { groups[id].passes.clear(); }
            }
            if let Source::Actor(id) = first {
                groups[id].passes.push(p); batch.output = id;
            } else {
                let mut passes = FRAME.with(|f| f.borrow_mut().group_pool.pop().unwrap_or_default());
                passes.push(p); batch.output = groups.len();
                groups.push(super::super::Group { passes, transparent: true });
            }
        }
        list.clear();
        list.extend(self.kept.iter().map(|&(depth, source, batch)|
            (depth, batch.map_or(source, |b| Source::Actor(self.batches[b].output)))));
        count("vita_world_puff_batches", produced);
        count("vita_world_puff_quads", puffs.len() as u64);
        count("vita_world_puff_batch_storage_bytes", self.batches.iter().map(|b|
            b.geometry.vertices.capacity()*std::mem::size_of::<Vertex>() + b.geometry.indices.capacity()*2
                + b.members.capacity()*std::mem::size_of::<Source>()).sum::<usize>() as u64);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn puff(tag: usize, blend: Blend) -> Puff {
        Puff { vertices: PuffVertices::Legacy(std::array::from_fn(|i| Vertex::new(tag as f32, i as f32, 0., i as f32, 0., WHITE))),
            texture: None, blend, before_group: 0 }
    }
    struct Restore(super::super::super::Frame);
    impl Drop for Restore {
        fn drop(&mut self) { FRAME.with(|f| *f.borrow_mut() = std::mem::take(&mut self.0)); }
    }
    fn frame() -> Restore {
        Restore(FRAME.with(|f| std::mem::replace(&mut *f.borrow_mut(), super::super::super::Frame {
            active: true, direct: true, ..Default::default() })))
    }
    fn pass_for(p: &Puff, effect: bool, equal: bool) -> super::super::super::Pass {
        let mut pass = super::super::super::pass(Mesh { vertices: p.vertices.array().to_vec(),
            indices: vec![0,1,2,0,2,3], texture: None },
            &Stage { blend: normalized(p.blend), depth_equal: equal, ..Default::default() },
            true, false, Default::default()).unwrap();
        pass.effect = effect; pass
    }
    fn groups(large: bool) -> Vec<super::super::super::Group> {
        let mut first = pass_for(&puff(100, Blend::Alpha), true, false);
        if large {
            first.mesh.vertices.resize(59996, first.mesh.vertices[0]);
            first.mesh.indices = vec![0,1,2];
        }
        vec![
            super::super::super::Group { passes: vec![first], transparent: true },
            super::super::super::Group { passes: vec![pass_for(&puff(101, Blend::Alpha), false, false)], transparent: true },
            super::super::super::Group { passes: vec![pass_for(&puff(102, Blend::Add), true, false)], transparent: true },
            super::super::super::Group { passes: vec![pass_for(&puff(103, Blend::Alpha), true, false),
                pass_for(&puff(103, Blend::Alpha), true, true)], transparent: true },
        ]
    }
    #[derive(Debug, PartialEq)]
    enum Draw { World(usize), Pass([u32;20], Vec<[u32;10]>, Vec<u16>) }
    fn snapshot(groups: &[super::super::super::Group], list: &[(f32,Source)]) -> Vec<Draw> {
        let mut result = Vec::new();
        for &(_, source) in list {
            match source {
                Source::World(id) => result.push(Draw::World(id)),
                Source::Actor(id) => for p in &groups[id].passes {
                    let mesh = p.geometry();
                    result.push(Draw::Pass(packet_key(p).state, mesh.vertices.iter().map(|v| {
                        let pos=v.position.to_array().map(f32::to_bits);let uv=v.uv.to_array().map(f32::to_bits);
                        let n=v.normal.to_array().map(f32::to_bits);
                        [pos[0],pos[1],pos[2],uv[0],uv[1],n[0],n[1],n[2],n[3],u32::from_ne_bytes(v.color)]
                    }).collect(), mesh.indices.clone()));
                },
                Source::Puff(_) => panic!("unpublished puff"),
            }
        }
        result
    }
    #[test]
    fn direct_batches_match_legacy_material_order_barriers_equal_layers_and_u16_boundary() {
        let _frame = frame();
        for material in [false,true] { for large in [false,true] {
            let puffs=(0..7).map(|i|puff(i,if i%2==0{Blend::Alpha}else{Blend::Add})).collect::<Vec<_>>();
            let list=order(vec![(5.,Source::World(7)),(5.,Source::Actor(0)),(5.,Source::Puff(0)),
                (4.9,Source::Puff(1)),(4.8,Source::Puff(2)),(4.5,Source::Actor(1)),
                (4.,Source::Puff(3)),(3.8,Source::World(8)),(3.5,Source::Actor(2)),
                (3.4,Source::Puff(4)),(3.,Source::Actor(3)),(-0.,Source::Puff(5)),
                (0.,Source::Puff(6))],true);
            let mut old_groups=groups(large);
            for p in &puffs {old_groups.push(super::super::super::Group {passes:vec![pass_for(p,true,false)],transparent:true});}
            let mut old_list=list.iter().map(|&(d,s)|(d,match s{Source::Puff(id)=>Source::Actor(4+id),_=>s})).collect();
            merge_cached_mode(&mut MergeCache::default(),&mut old_groups,&mut old_list,material,false,&mut BTreeMap::new(),false);
            let mut new_groups=groups(large);let mut new_list=list;let mut bank=Bank::default();
            bank.build(&mut new_groups,&puffs,&mut new_list,material);
            merge_cached_mode(&mut MergeCache::default(),&mut new_groups,&mut new_list,material,false,&mut BTreeMap::new(),false);
            assert_eq!(snapshot(&old_groups,&old_list),snapshot(&new_groups,&new_list));
            for batch in &bank.batches[..bank.live] {
                assert!(batch.geometry.vertices.len()<=60000);
                assert!(batch.geometry.indices.iter().all(|&i|(i as usize)<batch.geometry.vertices.len()));
            }
        }}
    }
    #[test]
    fn fused_final_storage_matches_legacy_for_changing_membership_ties_barriers_and_held_views() {
        let _frame=frame();
        for material in [false,true] {for large in [false,true] {
            let mut old_bank=Bank::default();let mut new_bank=Bank::default();
            let mut held:Option<(Rc<super::super::super::prepared::Geometry>,Vec<Vertex>)>=None;
            for step in 0..8 {
                let n=if step%3==0 {7}else{5};
                let numeric=(0..n).map(|i|crate::particles::quad::NumericQuad {
                    positions:std::array::from_fn(|v|vec3(i as f32+step as f32*0.13,v as f32,if v==0 {-0.}else{0.})),
                    color:[step as u8,255,127,if i%2==0 {128}else{255}]}).collect::<Vec<_>>();
                let legacy=numeric.iter().enumerate().map(|(i,n)|Puff {vertices:PuffVertices::Legacy(std::array::from_fn(|v|n.vertex(v))),
                    texture:None,blend:if (i+step)%2==0 {Blend::Alpha}else{Blend::Add},before_group:0}).collect::<Vec<_>>();
                let fused=numeric.iter().enumerate().map(|(i,&n)|Puff {vertices:PuffVertices::Fused(n),
                    texture:None,blend:if (i+step)%2==0 {Blend::Alpha}else{Blend::Add},before_group:0}).collect::<Vec<_>>();
                let mut old_groups=groups(large);let mut new_groups=groups(large);
                if large && step%2==1 {
                    // One more quad exceeds the index bound before the vertex bound.
                    old_groups[0].passes[0].mesh.indices=[0,1,2].repeat(59999);
                    new_groups[0].passes[0].mesh.indices=[0,1,2].repeat(59999);
                }
                let mut list=vec![(5.,Source::World(7)),(5.,Source::Actor(0)),(4.,Source::Actor(1)),
                    (3.,Source::Actor(2)),(2.,Source::Actor(3)),(1.,Source::World(8))];
                list.extend((0..n).map(|i|(if i%2==0 {5.}else{-0.},Source::Puff(i))));
                let mut old_list=order(list.clone(),true);let mut new_list=order(list,true);
                old_bank.build(&mut old_groups,&legacy,&mut old_list,material);
                new_bank.build(&mut new_groups,&fused,&mut new_list,material);
                assert_eq!(snapshot(&old_groups,&old_list),snapshot(&new_groups,&new_list));
                if let Some((view,bytes))=&held {assert!(vertices_equal(&view.vertices,bytes));}
                if step==0 {let view=new_bank.batches[0].geometry.clone();held=Some((view.clone(),view.vertices.clone()));}
                recycle(&mut old_groups);recycle(&mut new_groups);
            }
        }}
    }
    #[test]
    fn fused_final_pack_is_allocation_free_after_warmup_and_keeps_unchanged_revisions() {
        let _frame=frame();let mut bank=Bank::default();let mut groups=Vec::new();
        let mut puffs=(0..128).map(|i|Puff {vertices:PuffVertices::Fused(crate::particles::quad::NumericQuad {
            positions:[Vec3::splat(i as f32);4],color:[255;4]}),texture:None,blend:Blend::Alpha,before_group:0}).collect::<Vec<_>>();
        let original=(0..128).map(|i|(i as f32,Source::Puff(i))).collect::<Vec<_>>();let mut list=original.clone();
        bank.build(&mut groups,&puffs,&mut list,true);recycle(&mut groups);
        let version=bank.batches[0].geometry.version;
        for step in 0..10 {
            list.clone_from(&original);let scope=crate::kernel_alloc::Scope::begin();
            bank.build(&mut groups,&puffs,&mut list,true);recycle(&mut groups);assert_eq!(scope.finish(),0);
            assert_eq!(bank.batches[0].geometry.version,version);
            if step==9 {if let PuffVertices::Fused(n)=&mut puffs[0].vertices {n.color[0]=0;}}
        }
        list.clone_from(&original);let scope=crate::kernel_alloc::Scope::begin();
        bank.build(&mut groups,&puffs,&mut list,true);recycle(&mut groups);assert_eq!(scope.finish(),0);
        assert_ne!(bank.batches[0].geometry.version.vertices,version.vertices);
    }
    #[test]
    fn direct_puff_key_matches_existing_pass_metadata() {
        let _frame=frame();
        for blend in [Blend::Alpha,Blend::Add] {
            let p=puff(0,blend);assert_eq!(puff_key(&p).state,packet_key(&pass_for(&p,true,false)).state);
        }
    }
    fn recycle(groups: &mut Vec<super::super::super::Group>) {
        FRAME.with(|f| {let mut f=f.borrow_mut();for g in groups.iter_mut(){
            g.passes.clear();f.group_pool.push(std::mem::take(&mut g.passes));
        }});
        groups.clear();
    }
    #[test]
    fn warm_batch_storage_is_allocation_free_and_held_publications_are_immutable() {
        let _frame=frame();let mut bank=Bank::default();let mut groups=Vec::new();
        let mut puffs=(0..128).map(|i|puff(i,if i%2==0{Blend::Alpha}else{Blend::Add})).collect::<Vec<_>>();
        let original=(0..128).map(|i|(i as f32,Source::Puff(i))).collect::<Vec<_>>();
        let mut list=original.clone();bank.build(&mut groups,&puffs,&mut list,true);recycle(&mut groups);
        let pointers=(bank.batches[0].geometry.vertices.as_ptr(),bank.batches[0].geometry.indices.as_ptr());
        for step in 0..10 {
            puffs[0].vertices.legacy_mut()[0].color[0]=step;list.clone_from(&original);
            let scope=crate::kernel_alloc::Scope::begin();
            bank.build(&mut groups,&puffs,&mut list,true);recycle(&mut groups);
            assert_eq!(scope.finish(),0);
            assert_eq!(pointers,(bank.batches[0].geometry.vertices.as_ptr(),bank.batches[0].geometry.indices.as_ptr()));
        }
        let held=bank.batches[0].geometry.clone();let before=held.vertices.clone();
        puffs[0].vertices.legacy_mut()[0].position.x+=1.;list.clone_from(&original);
        bank.build(&mut groups,&puffs,&mut list,true);
        assert!(vertices_equal(&held.vertices,&before));
        assert!(!Rc::ptr_eq(&held,&bank.batches[0].geometry));
        recycle(&mut groups);
    }
}
