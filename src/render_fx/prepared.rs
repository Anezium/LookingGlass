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
    /// The publication `observe(mesh, None)` would return, without copying a changed array:
    /// the caller's mesh is its pass's private copy and `Pass::geometry()` reads the
    /// publication once it exists, so the pass gives up the array. Comparisons, generation
    /// order and validation match `observe`. Nothing is taken on a hit. The second index
    /// comparison goes too: `same_topology` already compared these same two arrays unless
    /// the publication was just replaced.
    pub(super) fn observe_owned(&mut self, mesh: &mut Mesh) -> Rc<Geometry> {
        self.producer = None;
        let old = self.current.as_ref();
        let same_vertices = old.is_some_and(|g| vertices_equal(&g.vertices, &mesh.vertices));
        let same_topology = old.is_some_and(|g| g.vertices.len() == mesh.vertices.len() && g.indices == mesh.indices);
        if same_vertices && same_topology { return old.unwrap().clone(); }
        let mut version = old.map(|g|g.version).unwrap_or(Version {
            identity: generation(), vertices: generation(), topology: generation() });
        if !same_vertices { version.vertices = generation(); }
        if !same_topology { version.topology = generation(); }
        let cached_valid = old.filter(|_| same_topology).map(|g|g.valid);
        let slot = self.current.get_or_insert_with(|| Rc::new(Geometry::new(Mesh {
            vertices: Vec::new(), indices: Vec::new(), texture: None })));
        let replaced = Rc::get_mut(slot).is_none();
        if replaced {
            *slot = Rc::new(Geometry::new(Mesh { vertices: Vec::new(), indices: Vec::new(), texture: None }));
        }
        let g = Rc::get_mut(slot).unwrap();
        if !same_vertices || g.vertices.len() != mesh.vertices.len() {
            crate::sim_maps::count("fx_observe_moved_vertices", mesh.vertices.len() as u64);
            std::mem::swap(&mut g.mesh.vertices, &mut mesh.vertices);
            mesh.vertices.clear();
        }
        // A replaced Rc needs both arrays even when only one generation changed.
        if !same_topology || replaced {
            std::mem::swap(&mut g.mesh.indices, &mut mesh.indices);
            mesh.indices.clear();
        }
        g.version = version;
        g.valid = cached_valid.unwrap_or_else(|| valid(&g.mesh));
        slot.clone()
    }
    pub(super) fn observe(&mut self, mesh: &Mesh, alpha_variant: Option<bool>) -> Rc<Geometry> {
        self.producer = None;
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
    fn vertex_bits(v: &[Vertex]) -> Vec<[u32;10]> {
        v.iter().map(|v| {
            let (p,t,n)=(v.position.to_array().map(f32::to_bits),v.uv.to_array().map(f32::to_bits),v.normal.to_array().map(f32::to_bits));
            [p[0],p[1],p[2],t[0],t[1],u32::from_le_bytes(v.color),n[0],n[1],n[2],n[3]]
        }).collect()
    }
    /// One observable publication, with versions reduced to what changed since the previous
    /// step: generation values come from one counter shared by both caches under test.
    #[derive(Debug, PartialEq)]
    struct Observed { vertices: Vec<[u32;10]>, indices: Vec<u16>, valid: bool, same_rc: bool, changed: [bool;3] }
    fn observed(g: &Rc<Geometry>, previous: &Option<(Rc<Geometry>, Version)>) -> Observed {
        let old=previous.as_ref().map(|p|p.1);
        Observed { vertices:vertex_bits(&g.vertices), indices:g.indices.clone(), valid:g.valid,
            same_rc:previous.as_ref().is_some_and(|p|Rc::ptr_eq(&p.0,g)),
            changed:[old.is_none_or(|o|o.identity!=g.version.identity),
                old.is_none_or(|o|o.vertices!=g.version.vertices),
                old.is_none_or(|o|o.topology!=g.version.topology)] }
    }
    /// Deterministic edit stream over bit-exact vertices, topology, invalid indices, resizes,
    /// empty meshes and publications a packet still holds, which force a replaced Rc.
    fn observe_script(step: u32, input: &mut Mesh, rng: &mut u32) -> bool {
        *rng ^= *rng<<13; *rng ^= *rng>>17; *rng ^= *rng<<5;
        let pick=*rng; let at=(pick>>8) as usize;
        let len=input.vertices.len();
        match pick%12 {
            0|1|2 if len>0 => input.vertices[at%len].position.x=step as f32,
            3 if len>0 => input.vertices[at%len].normal.y=f32::from_bits(0x7fc00000+(step&0xff)),
            4 if len>0 => input.vertices[at%len].uv.x=if step&1==0 {0.} else {-0.},
            5 if len>0 => input.vertices[at%len].color[3]=step as u8,
            6 if input.indices.len()>1 => {let i=at%(input.indices.len()-1);input.indices.swap(i,i+1);}
            7 if len>0 => input.indices.push(if pick>>30==3 {60000} else {(at%len) as u16}),
            8 => input.vertices.push(Vertex::new(step as f32,0.,0.,0.,0.,WHITE)),
            9 if len>1 => {input.vertices.pop();input.indices.retain(|&i|(i as usize)<len-1);}
            10 if step%97==0 => {input.vertices.clear();input.indices.clear();}
            _ => {}
        }
        pick>>16&7==0
    }
    #[test]
    fn owned_observation_equals_the_copying_observer_at_every_step() {
        for seed in [0x9e3779b9u32,0x1234567,0xdeadbeef,0x31415926] {
            let (mut copying,mut owning)=(Cache::default(),Cache::default());
            let (mut last_copy,mut last_own)=(None,None);
            let mut held=Vec::new();let mut input=mesh();let mut rng=seed;
            for step in 0..4000 {
                let hold=observe_script(step,&mut input,&mut rng);
                // Each producer hands over a fresh private mesh, as a pass does.
                let a=copying.observe(&copy_mesh(&input),None);
                let mut private=copy_mesh(&input);
                let b=owning.observe_owned(&mut private);
                assert_eq!(observed(&a,&last_copy),observed(&b,&last_own),"seed {seed} step {step}");
                assert_eq!(a.indices,input.indices);assert_eq!(vertex_bits(&a.vertices),vertex_bits(&input.vertices));
                if hold {held.push((a.clone(),b.clone()));if held.len()>3 {held.remove(0);}} else if step%5==0 {held.clear();}
                last_copy=Some((a.clone(),a.version));last_own=Some((b.clone(),b.version));
                // Rc::get_mut decides replacement; both caches see one reference released.
                if step%3==0 {last_copy=None;last_own=None;}
            }
            // Packets keep the bytes they were given even after later publications.
            for (a,b) in &held {assert_eq!(vertex_bits(&a.vertices),vertex_bits(&b.vertices));assert_eq!(a.indices,b.indices);}
        }
    }
    /// Negative control: the comparison above must see each kind of wrong publication.
    #[test]
    fn owned_observation_comparison_rejects_wrong_publications() {
        let mut cache=Cache::default();let input=mesh();
        let first=cache.observe_owned(&mut copy_mesh(&input));
        let previous=Some((first.clone(),first.version));
        let reference=observed(&first,&None);
        let mut wrong=observed(&first,&None);wrong.vertices[0][2]^=1;assert_ne!(reference,wrong);
        let mut wrong=observed(&first,&None);wrong.indices.swap(0,1);assert_ne!(reference,wrong);
        let mut wrong=observed(&first,&None);wrong.valid=false;assert_ne!(reference,wrong);
        assert_ne!(reference,observed(&first,&previous));
        // Bit identity, not float equality: -0 == 0 must still count as a difference.
        let mut other=input;other.vertices[0].position.x=-0.;
        let second=cache.observe_owned(&mut copy_mesh(&other));
        assert_ne!(vertex_bits(&first.vertices),vertex_bits(&second.vertices));
        assert!(observed(&second,&previous).changed[1] && !observed(&second,&previous).changed[2]);
    }
    /// Witness: the lever changes who owns the bytes. Without this the equality above would
    /// also pass on a copying implementation.
    #[test]
    fn owned_observation_moves_changed_arrays_and_leaves_hits_alone() {
        let mut cache=Cache::default();let mut input=mesh();
        let mut first=copy_mesh(&input);let (vertices,indices)=(first.vertices.as_ptr(),first.indices.as_ptr());
        let a=cache.observe_owned(&mut first);
        assert_eq!((a.vertices.as_ptr(),a.indices.as_ptr()),(vertices,indices));
        assert!(first.vertices.is_empty()&&first.indices.is_empty());drop(a);
        // Unchanged: the same publication, and the pass keeps its arrays.
        let mut hit=copy_mesh(&input);let b=cache.observe_owned(&mut hit);
        assert_eq!(hit.vertices.len(),4);assert_eq!(hit.indices.len(),6);
        assert_eq!(b.vertices.as_ptr(),vertices);drop(b);
        // Vertices changed, topology did not: only the vertices move.
        input.vertices[0].position.z=1.;
        let mut moved=copy_mesh(&input);let pointer=moved.vertices.as_ptr();
        let c=cache.observe_owned(&mut moved);
        assert_eq!(c.vertices.as_ptr(),pointer);assert_eq!(c.indices.as_ptr(),indices);
        assert!(moved.vertices.is_empty());assert_eq!(moved.indices.len(),6);
        // A packet still holds the publication: it stays intact, the replacement owns both arrays.
        input.vertices[0].position.z=2.;
        let mut replaced=copy_mesh(&input);let (v,i)=(replaced.vertices.as_ptr(),replaced.indices.as_ptr());
        let d=cache.observe_owned(&mut replaced);
        assert_eq!((d.vertices.as_ptr(),d.indices.as_ptr()),(v,i));
        assert_eq!(c.vertices[0].position.z,1.);assert_eq!(c.indices.len(),6);
        assert_eq!(c.version.topology,d.version.topology);assert_ne!(c.version.vertices,d.version.vertices);
        // The copying observer never empties its input.
        let mut copying=Cache::default();let kept=copy_mesh(&input);
        let e=copying.observe(&kept,None);assert_eq!(kept.vertices.len(),4);assert_ne!(e.vertices.as_ptr(),kept.vertices.as_ptr());
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
