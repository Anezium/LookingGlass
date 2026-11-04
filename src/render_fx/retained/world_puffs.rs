//! Direct quad publications after the exact joint-alpha order is known.
//! Non-particle packets, invalid meshes and authored multi-pass layers are barriers.
use super::*;
use crate::materials::{Blend, Stage};
use std::rc::Rc;

pub(crate) struct Puff {
    pub vertices: [Vertex; 4],
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
fn vertices<'a>(source: Source, groups: &'a [super::super::Group], puffs: &'a [Puff]) -> &'a [Vertex] {
    match source {
        Source::Actor(id) => &groups[id].passes[0].geometry().vertices,
        Source::Puff(id) => &puffs[id].vertices,
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
    /// Quads last packed into `geometry` when its indices are exactly that many quads in order.
    quad_members: usize,
}
impl Default for Batch {
    fn default() -> Self {
        Self { members: Vec::new(), output: 0, quad_members: 0,
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
        // puff_key depends only on the texture and the blend, and a run shares both.
        let reuse_key = crate::sim_maps::fx_attached_persist();
        let mut last_puff: Option<(Option<macroquad::miniquad::TextureId>, Blend, PacketKey)> = None;
        for &(depth, source) in list {
            let puff_meta = match source {
                Source::Puff(id) if reuse_key => {
                    let p = &puffs[id];
                    let texture = p.texture.as_ref().map(Texture2D::raw_miniquad_id);
                    let hit = last_puff.as_ref().filter(|l| l.0 == texture && l.1 == p.blend).map(|l| l.2.clone());
                    let key = hit.unwrap_or_else(|| {
                        let key = puff_key(p);
                        last_puff = Some((texture, p.blend, key.clone()));
                        key
                    });
                    Some((key, 4, 6))
                }
                _ => meta(source, groups, puffs),
            };
            let Some((key, vc, ic)) = puff_meta else {
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
        if Rc::get_mut(&mut batch.geometry).is_none() {
            batch.geometry = Batch::default().geometry;
            batch.quad_members = 0;
        }
        let g = Rc::get_mut(&mut batch.geometry).unwrap();
        let mut vb = 0; let mut ib = 0;
        let mut same_vertices = true; let mut same_indices = true;
        let persist = crate::sim_maps::fx_attached_persist();
        let all_quads = batch.members.iter().all(|s| matches!(s, Source::Puff(_)));
        // The same number of raw quads as last time: the index arrays are equal by construction.
        let known_indices = persist && all_quads && batch.quad_members == batch.members.len();
        for &source in &batch.members {
            let v = vertices(source, groups, puffs); let i = indices(source, groups);
            if !persist || same_vertices {
                same_vertices &= g.vertices.get(vb..vb+v.len()).is_some_and(|old| vertices_equal(old, v));
            }
            if !known_indices && (!persist || same_indices) {
                same_indices &= g.indices.get(ib..ib+i.len()).is_some_and(|old|
                    old.iter().zip(i).all(|(&a,&b)| a as usize == b as usize + vb));
            }
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
            if !same_vertices { g.mesh.vertices.extend_from_slice(v); }
            if !same_indices { g.mesh.indices.extend(indices(source, groups).iter().map(|&i| i + base as u16)); }
            base += v.len();
        }
        if !same_vertices || !same_indices {
            let old = g.version;
            let mesh = std::mem::replace(&mut g.mesh, Mesh { vertices: Vec::new(), indices: Vec::new(), texture: None });
            *g = super::super::prepared::Geometry::merged(mesh, Some(old), same_indices);
            if same_vertices { g.version.vertices = old.vertices; }
        }
        batch.quad_members = if all_quads { batch.members.len() } else { 0 };
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
        Puff { vertices: std::array::from_fn(|i| Vertex::new(tag as f32, i as f32, 0., i as f32, 0., WHITE)),
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
        let mut pass = super::super::super::pass(Mesh { vertices: p.vertices.to_vec(),
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
    #[derive(Debug, PartialEq)]
    struct Packed { draws: Vec<Draw>, vertices_new: Vec<bool>, topology_new: Vec<bool> }
    /// The bank on a script of frames: moving quads, repeats, count changes, mixed material
    /// runs, and a held publication. Everything the lever may not change is returned.
    fn bank_script(persist: bool) -> Vec<Packed> {
        crate::sim_maps::with_fx_attached_persist(persist, || {
            let mut bank = Bank::default();
            let mut groups = Vec::new();
            let mut previous: Vec<(u64, u64)> = Vec::new();
            let mut held = None;
            let mut out = Vec::new();
            for frame in 0..24usize {
                let count = [40, 40, 40, 41, 41, 12, 12, 40][frame % 8];
                let moving = frame % 5 != 1 && frame % 5 != 2;
                let mut puffs = (0..count).map(|i| {
                    let run = i / 7 % 3;
                    let mut p = puff(i + if moving { frame * 1000 } else { 0 }, if run == 1 { Blend::Add } else { Blend::Alpha });
                    p.before_group = 0;
                    p
                }).collect::<Vec<_>>();
                if frame % 11 == 3 { puffs[3].vertices[2].color[1] = 9; }
                let mut list = (0..count).map(|i| ((count - i) as f32, Source::Puff(i))).collect::<Vec<_>>();
                bank.build(&mut groups, &puffs, &mut list, true);
                let draws = snapshot(&groups, &list);
                let versions = bank.batches[..bank.live].iter().map(|b| (b.geometry.version.vertices, b.geometry.version.topology)).collect::<Vec<_>>();
                let vertices_new = versions.iter().enumerate().map(|(i, v)| previous.get(i).is_none_or(|p| p.0 != v.0)).collect();
                let topology_new = versions.iter().enumerate().map(|(i, v)| previous.get(i).is_none_or(|p| p.1 != v.1)).collect();
                previous = versions;
                if frame == 9 { held = Some(bank.batches[0].geometry.clone()); }
                if frame == 11 { held = None; }
                for b in &bank.batches[..bank.live] { assert!(b.geometry.indices.chunks(6).enumerate().all(|(q, c)|
                    c == [0, 1, 2, 0, 2, 3].map(|i| i + q as u16 * 4))); }
                out.push(Packed { draws, vertices_new, topology_new });
                recycle(&mut groups);
            }
            drop(held);
            out
        })
    }
    #[test]
    fn persisted_bank_packs_the_same_bytes_with_the_same_generations() {
        let _frame = frame();
        let (off, on) = (bank_script(false), bank_script(true));
        assert_eq!(off, on);
        // The script reached the cases the lever skips work in, and the others.
        assert!(on.iter().flat_map(|p| &p.topology_new).any(|new| *new));
        assert!(on.iter().flat_map(|p| &p.topology_new).any(|new| !*new));
        assert!(on.iter().flat_map(|p| &p.vertices_new).any(|new| !*new));
    }
    #[test]
    fn a_whole_set_registers_like_one_quad_at_a_time_up_to_the_vertex_limit() {
        let quads = (0..6).map(|i| puff(i, if i % 2 == 0 { Blend::Alpha } else { Blend::Add })).collect::<Vec<_>>();
        let run = |whole: bool| {
            let _frame = frame();
            super::super::super::test_support::set_frame_vertices(super::super::super::test_support::MAX - 12);
            if whole {
                super::super::super::world_puff_quads(quads.iter().map(|q| (&q.vertices, None, q.blend)));
            } else {
                for q in &quads { super::super::super::world_puff_quad(q.vertices, None, q.blend); }
            }
            FRAME.with(|f| { let f = f.borrow();
                (f.vertices, f.dropped, f.world_puffs.iter().map(|p| (p.before_group, p.blend,
                    p.vertices.iter().map(|v| v.position.to_array().map(f32::to_bits)).collect::<Vec<_>>())).collect::<Vec<_>>()) })
        };
        let (one, whole) = (run(false), run(true));
        assert_eq!(one, whole);
        assert_eq!((whole.1, whole.2.len()), (3, 3));
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
            puffs[0].vertices[0].color[0]=step;list.clone_from(&original);
            let scope=crate::kernel_alloc::Scope::begin();
            bank.build(&mut groups,&puffs,&mut list,true);recycle(&mut groups);
            assert_eq!(scope.finish(),0);
            assert_eq!(pointers,(bank.batches[0].geometry.vertices.as_ptr(),bank.batches[0].geometry.indices.as_ptr()));
        }
        let held=bank.batches[0].geometry.clone();let before=held.vertices.clone();
        puffs[0].vertices[0].position.x+=1.;list.clone_from(&original);
        bank.build(&mut groups,&puffs,&mut list,true);
        assert!(vertices_equal(&held.vertices,&before));
        assert!(!Rc::ptr_eq(&held,&bank.batches[0].geometry));
        recycle(&mut groups);
    }
}
