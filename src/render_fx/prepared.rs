//! Immutable geometry publications. Identity never derives from a reusable address.
//! Mutable producers are observed once; downstream merge/upload use generations.
use super::*;
use super::retained::state_cache::vertices_equal;
use std::cell::Cell;
thread_local! { static NEXT: Cell<u64> = const { Cell::new(1) }; }
fn generation() -> u64 { NEXT.with(|n| { let v=n.get(); n.set(v.checked_add(1).expect("FX generation overflow")); v }) }
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Version { pub identity: u64, pub vertices: u64, pub topology: u64 }
pub(crate) struct Geometry { pub mesh: Mesh, pub version: Version, pub valid: bool }
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
    pub(super) fn advance_owned(&mut self, topology:bool) {
        self.version.vertices=generation();
        if topology {self.version.topology=generation();self.valid=valid(&self.mesh);}
    }
    pub(crate) fn update_quad(&mut self, vertices: &[Vertex; 4]) {
        if vertices_equal(&self.mesh.vertices, vertices) { return; }
        self.mesh.vertices.clear(); self.mesh.vertices.extend_from_slice(vertices);
        self.version.vertices = generation();
        self.valid = true;
    }
    pub(crate) fn new(mesh: Mesh) -> Self {
        Self { valid: valid(&mesh), mesh, version: Version {
            identity: generation(), vertices: generation(), topology: generation() } }
    }
}
/// Storage and validation survive frames. Rc prevents writes while a packet still owns it.
#[derive(Default)]
pub(super) struct Cache { current: Option<Rc<Geometry>>, producer: Option<(u64, u64, bool)> }
impl Cache {
    pub(super) fn version(&self) -> Option<Version> { self.current.as_ref().map(|g|g.version) }
    /// Only an immediate, validated skin handoff may use a producer revision.
    /// Topology comes from the owning immutable CPU surface. Live UVs are checked
    /// exactly; a later overlay/untracked caller continues to use observe().
    pub(super) fn publish(&mut self, mesh: &Mesh, alpha: bool,
        input: &actor_vertices::Publication<'_>) -> Rc<Geometry> {
        assert_eq!(mesh.vertices.len(), input.vertices.len());
        let variant = usize::from(alpha);
        let stamp = (input.revision.identity, input.revision.vertices[variant], alpha);
        let old = self.current.as_ref();
        let same_producer = self.producer.is_some_and(|p| p.0 == stamp.0 && p.2 == alpha);
        let same_topology = same_producer && old.is_some_and(|g| g.vertices.len() == mesh.vertices.len());
        let same_uv = old.is_some_and(|g| g.vertices.len() == mesh.vertices.len()
            && g.vertices.iter().zip(&mesh.vertices).all(|(a,b)|
                a.uv.to_array().map(f32::to_bits) == b.uv.to_array().map(f32::to_bits)));
        let same_vertices = self.producer == Some(stamp) && same_uv;
        if same_vertices && same_topology { return old.unwrap().clone(); }
        let mut version = old.map(|g| g.version).unwrap_or(Version {
            identity: generation(), vertices: generation(), topology: generation() });
        if !same_vertices { version.vertices = generation(); }
        if !same_topology { version.topology = generation(); }
        let cached_valid = old.filter(|_| same_topology).map(|g| g.valid);
        let slot = self.current.get_or_insert_with(|| Rc::new(Geometry::new(Mesh {
            vertices: Vec::new(), indices: Vec::new(), texture: None })));
        // Queued packets own old immutable arrays until finish; never mutate them.
        if Rc::get_mut(slot).is_none() {
            *slot = Rc::new(Geometry::new(Mesh { vertices: Vec::new(), indices: Vec::new(), texture: None }));
        }
        let g = Rc::get_mut(slot).unwrap();
        g.mesh.vertices.clear();
        g.mesh.vertices.extend(input.vertices.iter().zip(&mesh.vertices).map(|(prepared, live)| {
            let mut v = prepared[variant]; v.uv = live.uv; v
        }));
        if !same_topology || g.indices.len() != input.indices.len() {
            g.mesh.indices.clear(); g.mesh.indices.extend_from_slice(input.indices);
        }
        g.version = version;
        g.valid = cached_valid.unwrap_or_else(|| valid(&g.mesh));
        self.producer = Some(stamp);
        slot.clone()
    }
    pub(super) fn observe(&mut self, mesh: &Mesh, alpha_variant: Option<bool>) -> Rc<Geometry> {
        self.producer = None;
        let color = |v: &Vertex| {
            let mut c = v.color;
            if let Some(alpha) = alpha_variant { c[3] = if alpha { (255. * (c[3] as f32 / 255.).clamp(0., 1.)) as u8 } else { 255 }; }
            c
        };
        let probe = crate::actor_prep_profile::enabled();
        let mut compared = 0usize;
        let old = self.current.as_ref();
        let same_vertices = old.is_some_and(|g| {
            if alpha_variant.is_none() {
                if !probe {vertices_equal(&g.vertices,&mesh.vertices)} else {
                    g.vertices.len()==mesh.vertices.len() && g.vertices.iter().zip(&mesh.vertices).all(|(a,b)|{
                        compared+=std::mem::size_of::<Vertex>();
                        vertices_equal(std::slice::from_ref(a),std::slice::from_ref(b))
                    })
                }
            }
            else { g.vertices.len() == mesh.vertices.len() && g.vertices.iter().zip(&mesh.vertices).all(|(a,b)| {
                if probe {compared += std::mem::size_of::<Vertex>();}
                let mut b = *b; b.color = color(&b); vertices_equal(std::slice::from_ref(a), std::slice::from_ref(&b))
            }) }
        });
        let same_topology = old.is_some_and(|g| g.vertices.len() == mesh.vertices.len() && if !probe {g.indices == mesh.indices} else {
            if g.indices.len()!=mesh.indices.len() {return false;}
            g.indices.iter().zip(&mesh.indices).all(|(a,b)|{compared+=2;a==b})
        });
        crate::actor_prep_profile::count(crate::actor_prep_profile::Count::ComparedBytes, compared);
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
            crate::actor_prep_profile::count(crate::actor_prep_profile::Count::CopiedBytes, mesh.vertices.len()*std::mem::size_of::<Vertex>());
            g.mesh.vertices.clear(); g.mesh.vertices.extend(mesh.vertices.iter().map(|v| { let mut v=*v; v.color=color(&v); v }));
        }
        // A replaced Rc needs both arrays even when only one generation changed.
        if !same_topology || g.indices != mesh.indices {
            crate::actor_prep_profile::count(crate::actor_prep_profile::Count::CopiedBytes, mesh.indices.len()*2);
            g.mesh.indices.clone_from(&mesh.indices); }
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
    fn producer_publications_match_reference_and_keep_live_uv_and_packet_lifetimes() {
        for alpha in [false,true] {
            let mut cache=Cache::default();let mut reference=Cache::default();let mut input=mesh();
            let mut revision=actor_vertices::Revision::new();
            for step in 0..256 {
                input.vertices[0].color[3]=step as u8;
                input.vertices[1].position.z=step as f32;
                input.vertices[2].normal.x=f32::from_bits(0x7fc00001+step);
                revision.changed([true;2]);
                let vertices:Vec<_>=input.vertices.iter().map(|v|[
                    actor_vertices::normalize(*v,false),actor_vertices::normalize(*v,true)]).collect();
                let publication=actor_vertices::Publication {revision,vertices:&vertices,indices:&input.indices};
                let a=cache.publish(&input,alpha,&publication);let b=reference.observe(&input,Some(alpha));
                assert!(vertices_equal(&a.vertices,&b.vertices));assert_eq!(a.indices,b.indices);
                let again=cache.publish(&input,alpha,&publication);assert!(Rc::ptr_eq(&a,&again));
                // UVs may be edited after gather and are deliberately excluded from
                // the numeric worker revision. A frozen producer must still notice.
                input.vertices[0].uv.x=-0.;
                let c=cache.publish(&input,alpha,&publication);
                assert_eq!(c.vertices[0].uv.x.to_bits(),(-0_f32).to_bits());
                assert_eq!(a.vertices[0].uv.x.to_bits(),0_f32.to_bits());
                assert_ne!(a.version.vertices,c.version.vertices);
                assert_eq!(a.version.topology,c.version.topology);
                input.vertices[0].uv.x=0.;
            }
            // Untracked overlays invalidate the certificate; producer restart,
            // hidden/replaced topology and new lifetime also require a publication.
            let old=cache.observe(&input,Some(alpha));
            input.indices=vec![0,2,1];revision=actor_vertices::Revision::new();
            let vertices:Vec<_>=input.vertices.iter().map(|v|[
                actor_vertices::normalize(*v,false),actor_vertices::normalize(*v,true)]).collect();
            let a=cache.publish(&input,alpha,&actor_vertices::Publication {revision,vertices:&vertices,indices:&input.indices});
            assert_ne!(old.version.topology,a.version.topology);assert_eq!(a.indices,input.indices);
            assert!(vertices_equal(&a.vertices,&Cache::default().observe(&input,Some(alpha)).vertices));
        }
    }
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
