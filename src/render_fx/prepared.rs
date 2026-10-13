//! Immutable geometry publications. Identity never derives from a reusable address.
//! Mutable producers are observed once; downstream merge/upload use generations.
use super::*;
use super::retained::state_cache::vertices_equal;
use std::cell::Cell;
thread_local! { static NEXT: Cell<u64> = const { Cell::new(1) }; }
fn generation() -> u64 { NEXT.with(|n| { let v=n.get(); n.set(v.checked_add(1).expect("FX generation overflow")); v }) }
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Version { pub identity: u64, pub vertices: u64, pub topology: u64 }
pub(super) struct Geometry { pub mesh: Mesh, pub version: Version, pub valid: bool }
impl std::ops::Deref for Geometry { type Target = Mesh; fn deref(&self) -> &Mesh { &self.mesh } }
pub(super) fn valid(mesh: &Mesh) -> bool {
    !mesh.vertices.is_empty() && !mesh.indices.is_empty() && mesh.vertices.len() <= 65536
        && mesh.indices.len() % 3 == 0 && mesh.indices.len() <= i32::MAX as usize
        && mesh.indices.iter().all(|&i| (i as usize) < mesh.vertices.len())
}
impl Geometry {
    pub(super) fn merged(mesh: Mesh, previous: Option<Version>, same_topology: bool) -> Self {
        // The merge plan already certified every source index and the u16 limit.
        // Rebuilding animated vertices must not rescan unchanged merged topology.
        debug_assert!(valid(&mesh));
        let mut g=Self {mesh, valid:true, version:Version {
            identity:generation(), vertices:generation(), topology:generation() }};
        if let Some(old)=previous {
            g.version.identity=old.identity;
            if same_topology {g.version.topology=old.topology;}
        }
        g
    }
    pub(super) fn new(mesh: Mesh) -> Self {
        Self { valid: valid(&mesh), mesh, version: Version {
            identity: generation(), vertices: generation(), topology: generation() } }
    }
}
/// Storage and validation survive frames. Rc prevents writes while a packet still owns it.
#[derive(Default)]
pub(super) struct Cache { current: Option<Rc<Geometry>> }
impl Cache {
    pub(super) fn version(&self) -> Option<Version> { self.current.as_ref().map(|g|g.version) }
    pub(super) fn observe(&mut self, mesh: &Mesh, alpha_variant: Option<bool>) -> Rc<Geometry> {
        let color = |v: &Vertex| {
            let mut c = v.color;
            if let Some(alpha) = alpha_variant { c[3] = if alpha { (255. * (c[3] as f32 / 255.).clamp(0., 1.)) as u8 } else { 255 }; }
            c
        };
        let old = self.current.as_ref();
        let same_vertices = old.is_some_and(|g| {
            if alpha_variant.is_none() { vertices_equal(&g.vertices, &mesh.vertices) }
            else { g.vertices.len() == mesh.vertices.len() && g.vertices.iter().zip(&mesh.vertices).all(|(a,b)| {
                let mut b = *b; b.color = color(&b); vertices_equal(std::slice::from_ref(a), std::slice::from_ref(&b))
            }) }
        });
        let same_topology = old.is_some_and(|g| g.vertices.len() == mesh.vertices.len() && g.indices == mesh.indices);
        if same_vertices && same_topology { return old.unwrap().clone(); }
        let mut version = old.map(|g|g.version).unwrap_or(Version {
            identity: generation(), vertices: generation(), topology: generation() });
        if !same_vertices { version.vertices = generation(); }
        if !same_topology { version.topology = generation(); }
        let cached_valid = old.filter(|_| same_topology).map(|g|g.valid);
        let slot = self.current.get_or_insert_with(|| Rc::new(Geometry::new(Mesh {
            vertices: Vec::new(), indices: Vec::new(), texture: None })));
        if Rc::get_mut(slot).is_none() {
            *slot = Rc::new(Geometry::new(Mesh { vertices: Vec::new(), indices: Vec::new(), texture: None }));
        }
        let g = Rc::get_mut(slot).unwrap();
        if !same_vertices || g.vertices.len() != mesh.vertices.len() {
            g.mesh.vertices.clear(); g.mesh.vertices.extend(mesh.vertices.iter().map(|v| { let mut v=*v; v.color=color(&v); v }));
        }
        // A replaced Rc needs both arrays even when only one generation changed.
        if !same_topology || g.indices != mesh.indices { g.mesh.indices.clone_from(&mesh.indices); }
        g.version = version;
        g.valid = cached_valid.unwrap_or_else(|| valid(&g.mesh));
        slot.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn mesh() -> Mesh { Mesh {vertices:vec![Vertex::new(0.,0.,0.,0.,0.,WHITE);4],indices:vec![0,1,2,0,2,3],texture:None} }
    #[test]
    fn publications_separate_topology_vertices_and_lifetime_with_exact_bits() {
        let mut cache=Cache::default(); let mut input=mesh();
        let a=cache.observe(&input,None); assert!(a.valid);
        let b=cache.observe(&input,None); assert!(Rc::ptr_eq(&a,&b));
        input.vertices[0].position.x=-0.;
        let c=cache.observe(&input,None);
        assert_eq!(a.version.identity,c.version.identity); assert_eq!(a.version.topology,c.version.topology);
        assert_ne!(a.version.vertices,c.version.vertices); assert_eq!(c.vertices[0].position.x.to_bits(),(-0_f32).to_bits());
        // Old queued packets remain immutable even when the producer publishes again.
        assert_eq!(a.vertices[0].position.x.to_bits(),0_f32.to_bits());
        input.indices[1]=3; let d=cache.observe(&input,None);
        assert_ne!(d.version.topology,c.version.topology); assert_eq!(d.version.vertices,c.version.vertices);
        input.indices[0]=99; let invalid=cache.observe(&input,None); assert!(!invalid.valid);
        input.indices[0]=0; assert!(cache.observe(&input,None).valid);
        let other=Cache::default().observe(&input,None); assert_ne!(other.version.identity,a.version.identity);
        input.vertices[1].normal.x=f32::from_bits(0x7fc00001); let nan=cache.observe(&input,None);
        assert!(Rc::ptr_eq(&nan,&cache.observe(&input,None)));
        input.vertices[1].normal.x=f32::from_bits(0x7fc00002);
        assert_ne!(nan.version.vertices,cache.observe(&input,None).version.vertices);
    }
    #[test]
    fn alpha_variants_keep_output_exact_and_storage_reuses_capacity() {
        let mut cache=Cache::default(); let mut input=mesh(); input.vertices[0].color[3]=31;
        let a=cache.observe(&input,Some(false)); let pointer=a.vertices.as_ptr(); assert_eq!(a.vertices[0].color[3],255);
        drop(a); input.vertices[0].position.z=1.;
        let b=cache.observe(&input,Some(false)); assert_eq!(b.vertices.as_ptr(),pointer);
        drop(b); input.vertices[0].color[3]=17;
        let c=cache.observe(&input,Some(false)); let version=c.version; drop(c);
        input.vertices[0].color[3]=22; assert_eq!(cache.observe(&input,Some(false)).version,version);
        assert_eq!(Cache::default().observe(&input,Some(true)).vertices[0].color[3],
            (255.*(22_f32/255.).clamp(0.,1.)) as u8);
    }
}
