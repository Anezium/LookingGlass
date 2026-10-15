//! Numeric per-view snapshot; resources and publications stay on the game thread.
use super::{Emitter, Puff};
use macroquad::prelude::*;
use crate::render_fx::prepared::Geometry;
use std::rc::Rc;
#[derive(Clone, Copy)]
struct Recipe {
    position: Vec3, size: f32, growth: f32, age: f32, life: f32, roll: f32,
    alpha: f32, fade: bool, fade_in: f32, color: Vec3, camera: Vec3,
    right: Vec3, up: Vec3,
}
fn basis(direction: Vec3) -> (Vec3, Vec3) {
    let right = direction.cross(Vec3::Z).try_normalize().unwrap_or(Vec3::X);
    (right, right.cross(direction).normalize_or_zero())
}
fn recipe(p: &Puff, e: &Emitter, camera: Vec3, axes: (Vec3, Vec3)) -> Recipe {
    Recipe { position: p.position, size: p.size, growth: e.spec.growth, age: p.age,
        life: p.life, roll: p.roll, alpha: e.spec.alpha, fade: e.spec.fade,
        fade_in: e.spec.fade_in, color: e.spec.color, camera, right: axes.0, up: axes.1 }
}
/// Plain numeric worker output; no UV/normal Vertex arrays or render handles.
#[derive(Clone, Copy, Default)]
pub(crate) struct NumericQuad {pub(crate) positions:[Vec3;4],pub(crate) color:[u8;4]}
impl NumericQuad {
    pub(crate) fn vertex(&self,i:usize)->Vertex {
        const UV:[[f32;2];4]=[[0.,1.],[1.,1.],[1.,0.],[0.,0.]];
        Vertex {position:self.positions[i],uv:Vec2::from_array(UV[i]),color:self.color,normal:Vec4::ZERO}
    }
    pub(crate) fn center(&self)->Vec3 {
        let (lo,hi)=self.positions.iter().fold((Vec3::splat(f32::INFINITY),Vec3::splat(f32::NEG_INFINITY)),
            |(lo,hi),&p|(lo.min(p),hi.max(p)));
        (lo+hi)*0.5
    }
    pub(crate) fn same(&self,old:&[Vertex])->bool {
        old.len()==4 && old.iter().enumerate().all(|(i,v)| {
            let new=self.vertex(i);
            v.position.to_array().map(f32::to_bits)==new.position.to_array().map(f32::to_bits)
                && v.uv.to_array().map(f32::to_bits)==new.uv.to_array().map(f32::to_bits)
                && v.normal.to_array().map(f32::to_bits)==new.normal.to_array().map(f32::to_bits) && v.color==new.color
        })
    }
    pub(crate) fn append(&self,target:&mut Vec<Vertex>) {target.extend((0..4).map(|i|self.vertex(i)));}
}
fn numeric(p: &Recipe) -> NumericQuad {
    let radius = p.size * (1. + p.growth * p.age).max(0.);
    let x = (p.right * p.roll.cos() + p.up * p.roll.sin()) * radius;
    let y = (-p.right * p.roll.sin() + p.up * p.roll.cos()) * radius;
    let opacity = p.alpha * if p.fade { 1. - p.age / p.life } else { 1. }
        * (p.age / p.fade_in.max(0.02)).min(1.)
        * (p.camera.distance(p.position) / 28.).min(1.);
    let color: [u8;4] = Color::new(p.color.x, p.color.y, p.color.z, opacity).into();
    let positions = [p.position-x-y,p.position+x-y,p.position+x+y,p.position-x+y];
    NumericQuad {positions,color}
}
fn quad(p:&Recipe)->[Vertex;4] {let n=numeric(p);std::array::from_fn(|i|n.vertex(i))}
fn kernel(recipes: &[Recipe], output: &mut [[Vertex; 4]]) {
    for (r, out) in recipes.iter().zip(output) { *out = quad(r); }
}
#[derive(Default)]
pub(super) struct Buffer {
    recipes: Vec<Recipe>, output: Vec<[Vertex;4]>, publications: Vec<Rc<Geometry>>, live: usize, raw_live: usize,
    numeric:Vec<NumericQuad>,numeric_live:usize,
}
impl Buffer {
    pub(super) fn prepare(&mut self, puffs: &[Puff], emitters: &[Emitter], camera: Vec3, direction: Option<Vec3>) {
        self.prepare_mode(puffs, emitters, camera, direction, false,false);
    }
    pub(super) fn prepare_world(&mut self, puffs: &[Puff], emitters: &[Emitter], camera: Vec3, direction: Vec3) {
        self.prepare_mode(puffs, emitters, camera, Some(direction), true,false);
    }
    pub(super) fn prepare_world_fused(&mut self,puffs:&[Puff],emitters:&[Emitter],camera:Vec3,direction:Vec3) {
        self.prepare_mode(puffs,emitters,camera,Some(direction),true,true);
    }
    fn prepare_mode(&mut self, puffs: &[Puff], emitters: &[Emitter], camera: Vec3, direction: Option<Vec3>, raw: bool,fused:bool) {
        let _span = crate::frame_profile::span("particle_producer");
        self.live = 0;
        self.raw_live = 0;
        self.numeric_live=0;
        self.recipes.clear();
        let axes = direction.map(basis); // Shared world camera basis, exactly the old arithmetic.
        self.recipes.extend(puffs.iter().map(|p| recipe(p, &emitters[p.emitter], camera,
            axes.unwrap_or_else(|| basis((p.position-camera).normalize_or_zero())))));
        if raw && fused {
            self.numeric.resize(puffs.len(),NumericQuad::default());
            let kernel=|recipes:&[Recipe],output:&mut [NumericQuad]| {for (r,out) in recipes.iter().zip(output){*out=numeric(r);}};
            if puffs.len()>=32 && crate::render::vita_renderer::config().fx_worker_prep {
                let mid=puffs.len()/2;let (a,b)=self.numeric.split_at_mut(mid);let (ra,rb)=self.recipes.split_at(mid);
                crate::character::skinning::overlap::cpu_pair(||kernel(ra,a),||kernel(rb,b));
            } else {kernel(&self.recipes,&mut self.numeric);}
            self.numeric_live=puffs.len();
            crate::render_fx::retained::count("particle_direct_quads",self.numeric_live as u64);
            crate::render_fx::retained::count("puff_numeric_output_bytes",(self.numeric_live*std::mem::size_of::<NumericQuad>()) as u64);
            crate::render_fx::retained::count("puff_numeric_output_capacity_bytes",(self.numeric.capacity()*std::mem::size_of::<NumericQuad>()) as u64);
            return;
        }
        self.output.resize(puffs.len(), [Vertex::new(0.,0.,0.,0.,0.,WHITE);4]);
        if puffs.len() >= 32 && crate::render::vita_renderer::config().fx_worker_prep {
            let mid = puffs.len()/2;
            let (a,b) = self.output.split_at_mut(mid);
            let (ra,rb) = self.recipes.split_at(mid);
            crate::character::skinning::overlap::cpu_pair(||kernel(ra,a),||kernel(rb,b));
        } else { kernel(&self.recipes,&mut self.output); }
        if raw {
            self.raw_live = puffs.len();
            crate::render_fx::retained::count("puff_intermediate_vertex_bytes",(self.raw_live*4*std::mem::size_of::<Vertex>()) as u64);
            crate::render_fx::retained::count("puff_vertex_output_capacity_bytes",(self.output.capacity()*4*std::mem::size_of::<Vertex>()) as u64);
            crate::render_fx::retained::count("particle_direct_quads", self.raw_live as u64);
            return;
        }
        for vertices in &self.output {
            let i = self.live;
            if self.publications.len() <= i {
                self.publications.push(Rc::new(Geometry::new(Mesh {vertices:Vec::with_capacity(4),
                    indices:vec![0,1,2,0,2,3], texture:None})));
            }
            // Reflections / queued prior views retain immutable publications.
            if Rc::get_mut(&mut self.publications[i]).is_none() {
                self.publications[i] = Rc::new(Geometry::new(Mesh {vertices:Vec::with_capacity(4),
                    indices:vec![0,1,2,0,2,3],texture:None}));
            }
            Rc::get_mut(&mut self.publications[i]).unwrap().update_quad(vertices);
            self.live += 1;
        }
        crate::render_fx::retained::count("particle_reused_quads", self.live as u64);
    }
    pub(super) fn geometry(&self, index: usize) -> Option<Rc<Geometry>> {
        (index < self.live).then(|| self.publications[index].clone())
    }
    pub(super) fn raw_quad(&self, index: usize) -> Option<[Vertex; 4]> {
        (index < self.raw_live).then(|| self.output[index])
    }
    pub(super) fn numeric_quad(&self,index:usize)->Option<NumericQuad> {(index<self.numeric_live).then(||self.numeric[index])}
}
#[cfg(test)]
mod tests {
    use super::*;
    fn same(a: &[Vertex], b: &[Vertex]) -> bool {
        a.len()==b.len() && a.iter().zip(b).all(|(a,b)| a.position.to_array().map(f32::to_bits)==b.position.to_array().map(f32::to_bits)
            && a.uv.to_array().map(f32::to_bits)==b.uv.to_array().map(f32::to_bits)
            && a.normal.to_array().map(f32::to_bits)==b.normal.to_array().map(f32::to_bits) && a.color==b.color)
    }
    #[test]
    fn producer_reuses_arrays_without_allocations_and_worker_output_matches_legacy() {
        crate::character::skinning::overlap::startup(2,false);
        let mut steam=super::super::tests::attached_fixture(4).steam;
        steam.update(0.2,Vec3::ZERO,&crate::collision::World::fixture(&[]));
        assert!(steam.puffs.len()>=32);
        let camera=vec3(25.,-10.,5.);
        let mut buffer=Buffer::default();buffer.prepare(&steam.puffs,&steam.emitters,camera,Some(Vec3::X));
        let pointers=(buffer.recipes.as_ptr(),buffer.output.as_ptr(),buffer.publications[0].vertices.as_ptr());
        for _ in 0..8 {
            let scope=crate::kernel_alloc::Scope::begin();
            buffer.prepare(&steam.puffs,&steam.emitters,camera,Some(Vec3::X));
            assert_eq!(scope.finish(),0);
            assert_eq!(pointers,(buffer.recipes.as_ptr(),buffer.output.as_ptr(),buffer.publications[0].vertices.as_ptr()));
            for i in 0..steam.puffs.len() {
                assert!(same(&steam.puff_mesh(i,camera,Vec3::X).vertices,&buffer.geometry(i).unwrap().vertices));
            }
        }
        let scope=crate::kernel_alloc::Scope::begin();
        for i in 0..steam.puffs.len() {std::hint::black_box(steam.puff_mesh(i,camera,Vec3::X));}
        assert!(scope.finish()>=steam.puffs.len() as u64*2);
        let output=&mut buffer.output;let recipes=&buffer.recipes;
        crate::character::skinning::overlap::cpu_pair(|| {
            let scope=crate::kernel_alloc::Scope::begin();kernel(recipes,output);assert_eq!(scope.finish(),0);
        },||{});
        buffer.prepare(&[],&steam.emitters,camera,Some(Vec3::Z));assert!(buffer.geometry(0).is_none());
    }
    #[test]
    fn world_and_attached_quads_match_legacy_bits_and_keep_snapshots() {
        let mut steam = super::super::tests::attached_fixture(2).steam;
        steam.update(0.2, Vec3::ZERO, &crate::collision::World::fixture(&[]));
        assert!(!steam.puffs.is_empty());
        for direction in [Vec3::X,Vec3::Z,Vec3::ZERO,vec3(0.2,-0.8,0.3)] {
            for frame in 0..20 {
                for (i,p) in steam.puffs.iter_mut().enumerate() {
                    p.age=frame as f32*0.03; p.roll=i as f32*0.7; p.size=frame as f32;
                }
                for attached in [false,true] {
                    let camera=vec3(17.,-23.,8.);
                    let mut buffer=Buffer::default();
                    buffer.prepare(&steam.puffs,&steam.emitters,camera,(!attached).then_some(direction));
                    for (i,p) in steam.puffs.iter().enumerate() {
                        let d=if attached {(p.position-camera).normalize_or_zero()} else {direction};
                        let reference=steam.puff_mesh(i,camera,d);
                        let g=buffer.geometry(i).unwrap();
                        assert!(same(&reference.vertices,&g.vertices));
                        assert_eq!(reference.indices,g.indices);
                    }
                    let old=buffer.geometry(0).unwrap(); let bytes=old.vertices.clone();
                    buffer.prepare(&steam.puffs,&steam.emitters,Vec3::ZERO,Some(Vec3::Z));
                    assert!(same(&old.vertices,&bytes));
                }
            }
        }
    }
    #[test]
    fn fused_numeric_producer_matches_legacy_bits_without_intermediate_vertices_or_warm_allocations() {
        crate::character::skinning::overlap::startup(2,false);
        let mut steam=super::super::tests::attached_fixture(2).steam;
        steam.update(0.2,Vec3::ZERO,&crate::collision::World::fixture(&[]));
        let camera=vec3(17.,-23.,8.);let mut buffer=Buffer::default();
        for direction in [Vec3::X,Vec3::Z,Vec3::ZERO,vec3(0.2,-0.8,0.3)] {
            buffer.prepare_world_fused(&steam.puffs,&steam.emitters,camera,direction);
            for frame in 0..12 {
                for p in &mut steam.puffs {p.age=frame as f32*0.03;p.roll=frame as f32*0.7;}
                let scope=crate::kernel_alloc::Scope::begin();
                buffer.prepare_world_fused(&steam.puffs,&steam.emitters,camera,direction);
                assert_eq!(scope.finish(),0);
                assert!(buffer.output.is_empty() && buffer.publications.is_empty());
                for i in 0..steam.puffs.len() {
                    let n=buffer.numeric_quad(i).unwrap();let mut vertices=Vec::with_capacity(4);n.append(&mut vertices);
                    let reference=steam.puff_mesh(i,camera,direction);
                    assert!(same(&vertices,&reference.vertices));assert!(n.same(&reference.vertices));
                    let (lo,hi)=reference.vertices.iter().fold((Vec3::splat(f32::INFINITY),Vec3::splat(f32::NEG_INFINITY)),
                        |(lo,hi),v|(lo.min(v.position),hi.max(v.position)));
                    assert_eq!(n.center().to_array().map(f32::to_bits),((lo+hi)*0.5).to_array().map(f32::to_bits));
                }
            }
        }
        buffer.prepare_world_fused(&[],&steam.emitters,camera,Vec3::X);assert!(buffer.numeric_quad(0).is_none());
        buffer.prepare(&steam.puffs,&steam.emitters,camera,None);assert!(buffer.numeric_quad(0).is_none());
    }
    #[test]
    fn raw_world_producer_matches_legacy_without_per_quad_publications_or_warm_allocations() {
        let mut steam=super::super::tests::attached_fixture(2).steam;
        steam.update(0.2,Vec3::ZERO,&crate::collision::World::fixture(&[]));
        let camera=vec3(17.,-23.,8.);let mut buffer=Buffer::default();
        buffer.prepare_world(&steam.puffs,&steam.emitters,camera,Vec3::X);
        for frame in 0..12 {
            for p in &mut steam.puffs {p.age=frame as f32*0.03;p.roll=frame as f32*0.7;}
            let scope=crate::kernel_alloc::Scope::begin();
            buffer.prepare_world(&steam.puffs,&steam.emitters,camera,Vec3::X);
            assert_eq!(scope.finish(),0);assert!(buffer.publications.is_empty());
            for i in 0..steam.puffs.len(){assert!(same(&buffer.raw_quad(i).unwrap(),&steam.puff_mesh(i,camera,Vec3::X).vertices));}
        }
        buffer.prepare(&steam.puffs,&steam.emitters,camera,None);
        assert!(buffer.raw_quad(0).is_none());assert!(buffer.geometry(0).is_some());
    }
}
