//! Independent continuous box sweeps against static convex brush and patch hulls.
//! No original executable or gameplay code is called by this module.
pub mod model_shape;
mod sah;
#[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
pub(crate) mod sah_probe;
use crate::bsp::{Bsp, Plane};
#[cfg(target_os = "vita")]
use crate::frame_profile::{add_counter, Counter};
use anyhow::{ensure, Result};
#[cfg(test)]
use macroquad::prelude::vec3;
use macroquad::prelude::{Quat, Vec3};

pub const SKIN: f32 = 0.03125;
pub const PLAYER_HALF: Vec3 = Vec3::new(15., 15., 28.);
pub const PLAYER_CENTER: Vec3 = Vec3::new(0., 0., 28.);
// Solid plus player-clip. Water, triggers, and monster-only clips are not solid.
const PLAYER_MASK: i32 = 1 | 0x10000;
pub const LIQUID_MASK: i32 = 0x08 | 0x10 | 0x20;

#[derive(Clone, Copy, Debug)]
struct Bounds {
    min: Vec3,
    max: Vec3,
}
impl Bounds {
    fn overlaps(self, other: Self) -> bool {
        self.min.cmple(other.max).all() && self.max.cmpge(other.min).all()
    }
    fn union(self, other: Self) -> Self {
        Self {
            min: self.min.min(other.min),
            max: self.max.max(other.max),
        }
    }
}
/// Conservative segment/expanded-box broad phase. A long diagonal sight line
/// must not test every brush in the entire rectangle between its endpoints.
struct SweepBounds {
    bounds: Bounds,
    start: Vec3,
    inverse: Vec3,
    padding: Vec3,
}
impl SweepBounds {
    fn new(start: Vec3, end: Vec3, half: Vec3) -> Self {
        let delta = end - start;
        let mut inverse = Vec3::ZERO;
        for axis in 0..3 {
            if delta[axis] != 0. { inverse[axis] = 1. / delta[axis]; }
        }
        let padding = half + Vec3::splat(SKIN);
        Self {
            bounds: Bounds {
                min: start.min(end) - padding,
                max: start.max(end) + padding,
            },
            start,
            inverse,
            padding,
        }
    }
    fn overlaps(&self, bounds: Bounds) -> bool {
        if !bounds.overlaps(self.bounds) { return false; }
        let mut enter = 0_f32;
        let mut leave = 1_f32;
        for axis in 0..3 {
            // The enclosing box above already checks parallel/zero-length axes.
            if self.inverse[axis] == 0. { continue; }
            let a = (bounds.min[axis] - self.padding[axis] - self.start[axis]) * self.inverse[axis];
            let b = (bounds.max[axis] + self.padding[axis] - self.start[axis]) * self.inverse[axis];
            enter = enter.max(a.min(b));
            leave = leave.min(a.max(b));
            if enter > leave { return false; }
        }
        true
    }
}
#[derive(Clone, Debug)]
struct Hull {
    planes: Vec<Plane>,
    bounds: Bounds,
}
impl Hull {
    /// Exact convex-volume membership, the same test `World::liquid_at` applies to static liquids.
    fn contains(&self, point: Vec3) -> bool {
        point.cmpge(self.bounds.min).all()
            && point.cmple(self.bounds.max).all()
            && self
                .planes
                .iter()
                .all(|p| p.normal.dot(point) <= p.distance)
    }
}
impl Hull {
    /// Box sweeps also need world-axis and edge/axis separating planes. Rotating
    /// the BSP's precomputed bevels alone leaves phantom solid corners outside
    /// the model bounds, so a downward trace can report start-solid at clear feet.
    fn transformed(source: &[Plane], origin: Vec3, rotation: Quat) -> Result<Self> {
        crate::sim_maps::count("brush_hulls_built", 1);
        #[cfg(target_os = "vita")]
        let _vertices_profile = crate::frame_profile::span("collider_vertices");
        let mut vertices = Vec::<Vec3>::new();
        for (i, a) in source.iter().enumerate() {
            for (j, b) in source.iter().enumerate().skip(i + 1) {
                for c in &source[j + 1..] {
                    let determinant = a.normal.dot(b.normal.cross(c.normal));
                    if determinant.abs() < 0.00001 {
                        continue;
                    }
                    let v = (a.distance * b.normal.cross(c.normal)
                        + b.distance * c.normal.cross(a.normal)
                        + c.distance * a.normal.cross(b.normal))
                        / determinant;
                    if v.is_finite()
                        && source.iter().all(|p| p.normal.dot(v) - p.distance <= 0.01)
                        && !vertices.iter().any(|p| p.distance_squared(v) < 0.0001)
                    {
                        vertices.push(v);
                    }
                }
            }
        }
        ensure!(
            vertices.len() >= 4,
            "Inline collision brush has no closed volume"
        );
        #[cfg(target_os = "vita")]
        drop(_vertices_profile);
        #[cfg(target_os = "vita")]
        crate::frame_profile::counts(&[
            ("collider_source_planes", source.len() as u64),
            (
                "collider_plane_triplets",
                (source.len() as u64).saturating_sub(2)
                    * (source.len() as u64).saturating_sub(1)
                    * source.len() as u64
                    / 6,
            ),
            ("collider_local_vertices", vertices.len() as u64),
            (
                "collider_vertex_pairs",
                vertices.len() as u64 * (vertices.len() as u64).saturating_sub(1) / 2,
            ),
        ]);
        #[cfg(target_os = "vita")]
        let _edges_profile = crate::frame_profile::span("collider_edges");
        let mut edges = Vec::<Vec3>::new();
        for (i, a) in vertices.iter().enumerate() {
            for b in &vertices[i + 1..] {
                // Two shared supporting planes identify a hull edge. Do not add
                // diagonals joining vertices across a face or the brush interior.
                if source
                    .iter()
                    .filter(|p| {
                        (p.normal.dot(*a) - p.distance).abs() <= 0.01
                            && (p.normal.dot(*b) - p.distance).abs() <= 0.01
                    })
                    .count()
                    >= 2
                {
                    let edge = (rotation * (*b - *a)).normalize();
                    if !edges
                        .iter()
                        .any(|e| e.distance_squared(edge).min(e.distance_squared(-edge)) < 1e-10)
                    {
                        edges.push(edge);
                    }
                }
            }
        }
        #[cfg(target_os = "vita")]
        drop(_edges_profile);
        #[cfg(target_os = "vita")]
        let _transform_profile = crate::frame_profile::span("collider_transform");
        let points = vertices
            .iter()
            .map(|v| origin + rotation * *v)
            .collect::<Vec<_>>();
        let mut planes = source
            .iter()
            .map(|p| {
                let normal = rotation * p.normal;
                Plane {
                    normal,
                    distance: p.distance + normal.dot(origin),
                }
            })
            .collect::<Vec<_>>();
        let axes = [Vec3::X, Vec3::Y, Vec3::Z];
        for axis in axes {
            add_support(&mut planes, &points, axis);
            add_support(&mut planes, &points, -axis);
        }
        for edge in edges {
            for axis in axes {
                let bevel = edge.cross(axis);
                if bevel.length_squared() > 0.000001 {
                    add_support(&mut planes, &points, bevel.normalize());
                    add_support(&mut planes, &points, -bevel.normalize());
                }
            }
        }
        Ok(Self {
            planes,
            bounds: Bounds {
                min: points.iter().copied().reduce(Vec3::min).unwrap(),
                max: points.iter().copied().reduce(Vec3::max).unwrap(),
            },
        })
    }
}
fn add_support(planes: &mut Vec<Plane>, points: &[Vec3], normal: Vec3) {
    // Near-parallel is insufficient here: even the first fraction of a degree
    // of shelf rotation needs its own world-axis bevel to agree with the bounds.
    if planes
        .iter()
        .any(|p| p.normal.distance_squared(normal) < 1e-10)
    {
        return;
    }
    let distance = points
        .iter()
        .map(|p| normal.dot(*p))
        .fold(f32::NEG_INFINITY, f32::max);
    planes.push(Plane { normal, distance });
}
/// A checked inline BSP model, transformed independently of the static BVH.
#[derive(Clone, Debug)]
pub struct Collider {
    hulls: std::sync::Arc<Vec<Hull>>,
    bounds: Option<Bounds>,
    lazy: Option<std::sync::Arc<model_shape::Placed>>,
}
/// A brush-entity or moving liquid volume, published by a level controller each frame.
/// `contents` carries the liquid bits (`LIQUID_MASK`); the volume may move between frames.
#[derive(Clone, Debug)]
pub struct Liquid {
    pub contents: i32,
    pub volume: Collider,
}
impl Collider {
    /// Distance between a collider AABB and Alice's upright body AABB. Missing
    /// bounds or non-finite inputs cannot certify a safe deferred placement.
    pub(crate) fn far_from_player(&self, feet: Vec3, margin: f32) -> bool {
        self.bounds.is_some_and(|b| {
            let center = feet + PLAYER_CENTER;
            let half = PLAYER_HALF + Vec3::splat(SKIN);
            let gap = (b.min - (center + half)).max((center - half) - b.max).max(Vec3::ZERO);
            gap.is_finite() && gap.length_squared() > margin * margin
        })
    }
    #[cfg(test)]
    pub(crate) fn placement_pending(&self) -> bool {
        self.lazy.as_ref().is_some_and(|p| p.pending())
    }
    #[cfg(test)]
    pub(crate) fn geometry_bits(&self) -> Vec<u32> {
        let mut bits = Vec::new();
        bits.push(u32::from(self.bounds.is_some()));
        if let Some(b) = self.bounds {
            bits.extend(b.min.to_array().map(f32::to_bits));
            bits.extend(b.max.to_array().map(f32::to_bits));
        }
        bits.push(self.hulls().len() as u32);
        for h in self.hulls() {
            bits.extend(h.bounds.min.to_array().map(f32::to_bits));
            bits.extend(h.bounds.max.to_array().map(f32::to_bits));
            bits.push(h.planes.len() as u32);
            for p in &h.planes {
                bits.extend(p.normal.to_array().map(f32::to_bits));
                bits.push(p.distance.to_bits());
            }
        }
        bits
    }
    fn hulls(&self) -> &[Hull] {
        self.lazy.as_ref().map_or(self.hulls.as_slice(), |p|p.hulls())
    }
    /// A representative point in one convex brush, even when the union's
    /// bounding-box centre lies in a gap between disconnected brushes.
    pub(crate) fn interior_point(&self) -> Option<Vec3> {
        for hull in self.hulls().iter() {
            let center = (hull.bounds.min + hull.bounds.max) * 0.5;
            if hull.contains(center) {
                return Some(center);
            }
            let mut sum = Vec3::ZERO;
            let mut count = 0;
            for (i, a) in hull.planes.iter().enumerate() {
                for (j, b) in hull.planes.iter().enumerate().skip(i + 1) {
                    for c in &hull.planes[j + 1..] {
                        let det = a.normal.dot(b.normal.cross(c.normal));
                        if det.abs() < 0.00001 {
                            continue;
                        }
                        let v = (a.distance * b.normal.cross(c.normal)
                            + b.distance * c.normal.cross(a.normal)
                            + c.distance * a.normal.cross(b.normal))
                            / det;
                        if v.is_finite()
                            && hull
                                .planes
                                .iter()
                                .all(|p| p.normal.dot(v) <= p.distance + 0.01)
                        {
                            sum += v;
                            count += 1;
                        }
                    }
                }
            }
            if count > 0 && hull.contains(sum / count as f32) {
                return Some(sum / count as f32);
            }
        }
        None
    }
    /// Whether the point lies inside any of this collider's convex hulls.
    pub fn contains(&self, point: Vec3) -> bool {
        self.hulls().iter().any(|h| h.contains(point))
    }
    #[cfg(test)]
    pub fn fixture(min: Vec3, max: Vec3) -> Self {
        Self::box_bounds(min, max)
    }
    /// An axis-aligned entity hull (model rotation does not rotate its bounds).
    pub fn box_bounds(min: Vec3, max: Vec3) -> Self {
        Self::from_hulls(World::box_hulls(&[(min, max)]))
    }
    fn from_hulls(hulls: Vec<Hull>) -> Self {
        let bounds = hulls.iter().map(|h| h.bounds).reduce(Bounds::union);
        Self {
            hulls: std::sync::Arc::new(hulls),
            bounds,
            lazy: None,
        }
    }
    pub fn triangles(triangles: &[[Vec3; 3]], origin: Vec3, rotation: Quat) -> Self {
        Self::from_hulls(
            triangles
                .iter()
                .filter_map(|t| {
                    triangle_hull(
                        origin + rotation * t[0],
                        origin + rotation * t[1],
                        origin + rotation * t[2],
                    )
                })
                .collect(),
        )
    }
    pub fn model(
        map: &Bsp,
        index: usize,
        origin: Vec3,
        rotation: Quat,
        solids_only: bool,
    ) -> Result<Self> {
        crate::sim_maps::count("collider_model_calls", 1);
        let model = map
            .models
            .get(index)
            .ok_or_else(|| anyhow::anyhow!("Invalid inline model {index}"))?;
        #[cfg(target_os = "vita")]
        crate::frame_profile::counts(&[("collider_model_calls", 1)]);
        #[cfg(target_os = "vita")]
        crate::frame_profile::record_model(index);
        #[cfg(target_os = "vita")]
        let mut brushes_rebuilt = 0u64;
        let mut hulls = Vec::new();
        for i in model.brushes.clone() {
            let brush = &map.brushes[i];
            if solids_only && map.shaders[brush.shader].contents & PLAYER_MASK == 0 {
                continue;
            }
            #[cfg(target_os = "vita")]
            let _extract_profile = crate::frame_profile::span("collider_extract");
            let planes = map.side_planes[brush.sides.clone()]
                .iter()
                .map(|&p| map.planes[p])
                .collect::<Vec<_>>();
            #[cfg(target_os = "vita")]
            {
                brushes_rebuilt += 1;
            }
            #[cfg(target_os = "vita")]
            drop(_extract_profile);
            hulls.push(Hull::transformed(&planes, origin, rotation)?);
        }
        #[cfg(target_os = "vita")]
        crate::frame_profile::counts(&[("collider_brushes_rebuilt", brushes_rebuilt)]);
        Ok(Self::from_hulls(hulls))
    }
    pub fn trace(&self, start: Vec3, end: Vec3, half: Vec3) -> Trace {
        self.trace_query(start, end, half, &SweepBounds::new(start, end, half))
    }
    fn visit_query(&self, query: &SweepBounds, mut visit: impl FnMut(&Hull)) {
        if crate::sim_maps::triangle_candidates() {
            if let Some(p) = &self.lazy { p.visit_query(query, visit); return; }
        }
        for h in self.hulls() { visit(h); }
    }
    fn trace_query(&self, start: Vec3, end: Vec3, half: Vec3, query: &SweepBounds) -> Trace {
        #[cfg(target_os = "vita")]
        let (mut tested, mut clipped) = (0u32, 0u32);
        let mut out = Trace::default();
        if self.bounds.is_some_and(|b| query.overlaps(b)) {
            self.visit_query(query, |h| {
                #[cfg(target_os = "vita")]
                {
                    tested += 1;
                }
                if query.overlaps(h.bounds) {
                    #[cfg(target_os = "vita")]
                    {
                        clipped += 1;
                    }
                    clip_hull(h, start, end, half, &mut out);
                }
            });
        }
        #[cfg(target_os = "vita")]
        {
            add_counter(Counter::ColliderTraceCalls, 1);
            add_counter(Counter::ColliderHullsTested, tested);
            add_counter(Counter::ColliderHullsClipped, clipped);
        }
        out
    }
    pub fn touches(&self, start: Vec3, end: Vec3, half: Vec3) -> bool {
        let t = self.trace(start, end, half);
        t.start_solid || t.fraction < 1.
    }
    /// Small contact correction for opt-in rotating pushers. Their passenger's
    /// axis-aligned box does not rotate with the model, so carrying its centre
    /// alone may leave a corner inside a neighbouring face. World obstruction
    /// must still be checked by the caller before committing this displacement.
    pub fn separate_body(&self, feet: Vec3, limit: f32) -> Option<Vec3> {
        let start=feet+PLAYER_CENTER;
        let mut point=start;
        for _ in 0..32 {
            let correction=self.hulls().iter().find_map(|h| {
                if point.cmplt(h.bounds.min-PLAYER_HALF).any() || point.cmpgt(h.bounds.max+PLAYER_HALF).any() {return None;}
                let mut nearest=(f32::INFINITY,Vec3::ZERO);
                for p in &h.planes {
                    let depth=p.distance+p.normal.abs().dot(PLAYER_HALF)-p.normal.dot(point);
                    if depth<0. {return None;}
                    if depth<nearest.0 {nearest=(depth,p.normal);}
                }
                nearest.0.is_finite().then_some(nearest.1*(nearest.0+SKIN))
            });
            let Some(delta)=correction else {
                return (!self.touches(point,point,PLAYER_HALF)).then_some(point-PLAYER_CENTER);
            };
            point+=delta;
            if point.distance(start)>limit {return None;}
        }
        None
    }
    /// A rider stays upright when a platform tilts. Rotating the feet alone can
    /// embed the bottom corner of the body in the newly tilted support.
    pub fn rider_feet(&self, carried: Vec3) -> Option<(Vec3, Vec3)> {
        let above = carried + Vec3::Z * 2.;
        let below = carried - Vec3::Z * 2.;
        let t = self.trace(above + PLAYER_CENTER, below + PLAYER_CENTER, PLAYER_HALF);
        let feet = above.lerp(below, t.fraction);
        (!t.start_solid
            && t.fraction < 1.
            && t.normal.z >= 0.65
            && !self.touches(feet + PLAYER_CENTER, feet + PLAYER_CENTER, PLAYER_HALF))
        .then_some((feet, t.normal))
    }
}
#[derive(Clone, Debug)]
enum Node {
    Leaf {
        bounds: Bounds,
        hulls: Vec<usize>,
    },
    Branch {
        bounds: Bounds,
        left: Box<Node>,
        right: Box<Node>,
    },
}
impl Node {
    fn visit_counted(&self,query:&SweepBounds,c:&mut sah::Counts,callback:&mut impl FnMut(usize,&mut sah::Counts)) {
        c.nodes+=1;
        match self {
            Self::Leaf{bounds,hulls}=>if query.overlaps(*bounds) {for &h in hulls {callback(h,c);}},
            Self::Branch{bounds,left,right}=>if query.overlaps(*bounds) {left.visit_counted(query,c,callback);right.visit_counted(query,c,callback);}
        }
    }
    fn build(hulls: &[Hull], indices: &mut [usize]) -> Self {
        let bounds = indices
            .iter()
            .map(|&i| hulls[i].bounds)
            .reduce(Bounds::union)
            .unwrap();
        if indices.len() <= 8 {
            return Self::Leaf {
                bounds,
                hulls: indices.to_vec(),
            };
        }
        let extent = bounds.max - bounds.min;
        let axis = if extent.x >= extent.y && extent.x >= extent.z {
            0
        } else if extent.y >= extent.z {
            1
        } else {
            2
        };
        indices.sort_unstable_by(|&a, &b| {
            (hulls[a].bounds.min[axis] + hulls[a].bounds.max[axis])
                .total_cmp(&(hulls[b].bounds.min[axis] + hulls[b].bounds.max[axis]))
        });
        let middle = indices.len() / 2;
        let (a, b) = indices.split_at_mut(middle);
        Self::Branch {
            bounds,
            left: Box::new(Self::build(hulls, a)),
            right: Box::new(Self::build(hulls, b)),
        }
    }
    fn visit(&self, query: &SweepBounds, callback: &mut impl FnMut(usize)) {
        match self {
            Self::Leaf { bounds, hulls } => {
                if query.overlaps(*bounds) {
                    for &h in hulls {
                        callback(h);
                    }
                }
            }
            Self::Branch {
                bounds,
                left,
                right,
            } => {
                if query.overlaps(*bounds) {
                    left.visit(query, callback);
                    right.visit(query, callback);
                }
            }
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Trace {
    pub fraction: f32,
    pub normal: Vec3,
    pub start_solid: bool,
    pub all_solid: bool,
}
impl Default for Trace {
    fn default() -> Self {
        Self {
            fraction: 1.,
            normal: Vec3::ZERO,
            start_solid: false,
            all_solid: false,
        }
    }
}
/// Spatially sorted groups of four AABBs. Query results are sorted back into
/// publication order before narrow phase, including equal-fraction contacts.
#[derive(Debug)]
struct DynamicBucket { bounds: Bounds, indices: Vec<usize> }
impl DynamicBucket {
    fn build(colliders: &[Collider]) -> Vec<Self> {
        let mut indices: Vec<_> = (0..colliders.len()).filter(|&i| colliders[i].bounds.is_some()).collect();
        indices.sort_by(|&a, &b| {
            let a = colliders[a].bounds.unwrap();
            let b = colliders[b].bounds.unwrap();
            (a.min.x + a.max.x).total_cmp(&(b.min.x + b.max.x))
        });
        indices.chunks(4).map(|chunk| Self {
            bounds: chunk.iter().map(|&i| colliders[i].bounds.unwrap()).reduce(Bounds::union).unwrap(),
            indices: chunk.to_vec(),
        }).collect()
    }
}
#[derive(Debug, Default)]
struct LightCertificates { entries: Vec<([u32; 9], bool)>, next: usize }

#[derive(Debug)]
pub struct World {
    // All mutable solid geometry is private and invalidates this bounded cache
    // on publication. Static hulls/tree are immutable for this World's lifetime.
    npc_sight_cache: std::cell::RefCell<std::collections::HashMap<[u32; 9], Trace>>,
    query_cache: std::cell::RefCell<std::collections::HashMap<[u32; 9], Trace>>,
    query_cache_active: std::cell::Cell<bool>,
    pub traversal: crate::traversal::Traversal,
    dynamic: Vec<Collider>,
    dynamic_bounds: Option<Bounds>,
    dynamic_buckets: std::sync::Arc<Vec<DynamicBucket>>,
    light_certificates: std::cell::RefCell<LightCertificates>,
    pub(crate) npc_profile: crate::npc::profile::CollisionCounts,
    #[cfg(test)]
    pub(crate) legacy_dynamic_sweeps: bool,
    #[cfg(test)]
    pub(crate) dynamic_publications: Option<Vec<Vec<Collider>>>,
    settled_supports: Vec<Collider>,
    weapon_obstacles: Vec<(usize, Collider)>,
    camera_obstacles: Vec<Collider>,
    liquids: Vec<(i32, Hull)>,
    dynamic_liquids: Vec<Liquid>,
    static_geometry: std::sync::Arc<StaticGeometry>,
    pub brush_count: usize,
    pub patch_count: usize,
    pub min: Vec3,
    pub max: Vec3,
}

/// Immutable backing shared by main-thread collision and acoustic packets.
/// No caches, lazy colliders or traversal state cross the producer boundary.
#[derive(Debug)]
pub(crate) struct StaticGeometry { hulls: Vec<Hull>, tree: Option<Node>, sah: Option<sah::Hierarchy> }
#[cfg(test)]
thread_local! {
    static SAH_OVERRIDE: std::cell::Cell<Option<bool>> = const { std::cell::Cell::new(None) };
}
#[cfg(test)]
pub(crate) fn set_sah_enabled_for_test(enabled: Option<bool>) {
    SAH_OVERRIDE.with(|cell| cell.set(enabled));
}
fn sah_switch() -> bool {
    #[cfg(test)]
    if let Some(enabled) = SAH_OVERRIDE.with(|cell| cell.get()) { return enabled; }
    #[cfg(feature="vita_renderer")]
    { crate::render::vita_renderer::config().collision_sah }
    #[cfg(not(feature="vita_renderer"))]
    { false }
}
static R13_GEOMETRIES:std::sync::atomic::AtomicUsize=std::sync::atomic::AtomicUsize::new(0);
static R13_GEOMETRY_BYTES:[std::sync::atomic::AtomicUsize;3]=[const{std::sync::atomic::AtomicUsize::new(0)};3];
impl StaticGeometry {
    fn census_capacities(&self)->[usize;3] {
        fn tree(n:&Node)->usize {match n {
            Node::Leaf{hulls,..}=>hulls.capacity()*std::mem::size_of::<usize>(),
            Node::Branch{left,right,..}=>2*std::mem::size_of::<Node>()+tree(left)+tree(right),
        }}
        [self.hulls.capacity()*std::mem::size_of::<Hull>()
            +self.hulls.iter().map(|h|h.planes.capacity()*std::mem::size_of::<crate::bsp::Plane>()).sum::<usize>(),
            self.tree.as_ref().map_or(0,tree),self.sah.as_ref().map_or(0,|h|h.storage_bytes())]
    }
    fn new(hulls: Vec<Hull>, tree: Option<Node>) -> Self {
        Self::with_acceleration(hulls, tree, true)
    }
    fn with_acceleration(hulls: Vec<Hull>, tree: Option<Node>, accelerate: bool) -> Self {
        let sah = if accelerate && sah_switch() { tree.as_ref().map(|t| sah::Hierarchy::build(&hulls,t)) } else {None};
        let out=Self {hulls,tree,sah};
        if crate::allocation_census::enabled(){
            use std::sync::atomic::Ordering::Relaxed;
            for (counter,n) in R13_GEOMETRY_BYTES.iter().zip(out.census_capacities()){counter.fetch_add(n,Relaxed);}
            R13_GEOMETRIES.fetch_add(1,Relaxed);
        }
        out
    }
    fn trace(&self, start: Vec3, end: Vec3, half: Vec3, query: &SweepBounds) -> (Trace,u32,u32) {
        let mut counts = sah::Counts::default();
        let timer = crate::phase_census::static_traversal();
        let result = self.trace_inner(start,end,half,query,&mut counts);
        timer.finish(counts.nodes,counts.tested,counts.clipped,counts.planes,counts.certificates,counts.fallbacks);
        #[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
        sah_probe::record(self,start,end,half,result,counts);
        (result,counts.tested as u32,counts.clipped as u32)
    }
    fn trace_inner(&self, start: Vec3, end: Vec3, half: Vec3, query: &SweepBounds, counts: &mut sah::Counts) -> Trace {
        if let Some(sah) = &self.sah {
            if let Some(result) = sah.trace(self,start,end,half,query,counts) {return result;}
            counts.fallbacks += 1;
        }
        self.legacy_trace(start,end,half,query,counts)
    }
    fn legacy_trace(&self,start:Vec3,end:Vec3,half:Vec3,query:&SweepBounds,counts:&mut sah::Counts)->Trace {
        let mut result=Trace::default();
        if let Some(tree)=&self.tree {
            tree.visit_counted(query,counts,&mut |i,counts| {
                counts.tested+=1;
                let hull=&self.hulls[i];
                if query.overlaps(hull.bounds) {counts.clipped+=1;clip_hull_counted(hull,start,end,half,&mut result,counts);}
            });
        }
        result
    }
    pub(crate) fn sweep(&self, start: Vec3, end: Vec3) -> Trace {
        let query = SweepBounds::new(start, end, Vec3::ZERO);
        self.trace(start,end,Vec3::ZERO,&query).0
    }
}
impl Drop for StaticGeometry {fn drop(&mut self){
    if crate::allocation_census::enabled(){
        use std::sync::atomic::Ordering::Relaxed;
        for (counter,n) in R13_GEOMETRY_BYTES.iter().zip(self.census_capacities()){counter.fetch_sub(n,Relaxed);}
        R13_GEOMETRIES.fetch_sub(1,Relaxed);
    }
}}
impl World {
    pub(crate) fn census_payload(&self)->serde_json::Value {
        fn tree(n:&Node)->usize {match n {
            Node::Leaf{hulls,..}=>hulls.capacity()*std::mem::size_of::<usize>(),
            Node::Branch{left,right,..}=>2*std::mem::size_of::<Node>()+tree(left)+tree(right),
        }}
        let g=&self.static_geometry;
        serde_json::json!({"geometry_identity":std::sync::Arc::as_ptr(g) as usize,
            "geometry_arc_owners":std::sync::Arc::strong_count(g),
            "all_live_unique_static_geometries":R13_GEOMETRIES.load(std::sync::atomic::Ordering::Relaxed),
            "all_live_hulls_tree_sah_capacity_bytes":R13_GEOMETRY_BYTES.each_ref().map(|v|v.load(std::sync::atomic::Ordering::Relaxed)),
            "hulls_planes_capacity_bytes":g.hulls.capacity()*std::mem::size_of::<Hull>()
                +g.hulls.iter().map(|h|h.planes.capacity()*std::mem::size_of::<crate::bsp::Plane>()).sum::<usize>(),
            "legacy_tree_capacity_bytes":g.tree.as_ref().map_or(0,tree),
            "sah_capacity_bytes":g.sah.as_ref().map_or(0,|h|h.storage_bytes()),
            "scope":"one live static geometry; Arc aliases share it; excludes dynamic colliders and allocator metadata"})
    }

    fn invalidate_npc_sight(&mut self) {
        self.npc_sight_cache.get_mut().clear();
    }
    /// Full trace reuse, including dynamic/settled/weapon geometry. Keys use
    /// float bits (no tolerance), and never escape the owning collision world.
    pub(crate) fn npc_sight(&self, start: Vec3, end: Vec3) -> Trace {
        if !crate::sim_maps::npc_sight_cache() {
            return self.sweep(start, end, Vec3::splat(0.5));
        }
        let key = Self::query_key(start, end, Vec3::splat(0.5));
        if let Some(trace) = self.npc_sight_cache.borrow().get(&key).copied() {
            crate::sim_maps::count("npc_sight_cache_hits", 1);
            return trace;
        }
        crate::sim_maps::count("npc_sight_cache_misses", 1);
        let trace = self.sweep(start, end, Vec3::splat(0.5));
        let mut cache = self.npc_sight_cache.borrow_mut();
        if cache.len() < 256 { cache.insert(key, trace); }
        trace
    }
    pub(crate) fn acoustic_scene_key(&self) -> usize { std::sync::Arc::as_ptr(&self.static_geometry) as usize }
    pub(crate) fn audio_geometry(&self) -> std::sync::Arc<StaticGeometry> {
        self.static_geometry.clone()
    }
    /// One actor_footing query, then this world is dropped. Share the main
    /// world's immutable static geometry when the caller already built it.
    /// Callers without a world get the legacy tree only, so a second SAH is
    /// never constructed for a shell that does not survive the query.
    pub(crate) fn footing_shared(shared: std::sync::Arc<StaticGeometry>, min: Vec3, max: Vec3) -> Self {
        eprintln!("[lg/collision] footing source=shared_static");
        Self::empty_shell(shared, 0, min, max)
    }
    pub(crate) fn footing(map: &Bsp, shared: Option<std::sync::Arc<StaticGeometry>>) -> Result<Self> {
        if let Some(static_geometry) = shared {
            return Ok(Self::footing_shared(static_geometry, map.world_min, map.world_max));
        }
        eprintln!("[lg/collision] footing source=legacy_tree");
        Self::from_bsp_mask(map, PLAYER_MASK, false)
    }
    fn empty_shell(static_geometry: std::sync::Arc<StaticGeometry>, brush_count: usize, min: Vec3, max: Vec3) -> Self {
        Self {
            npc_sight_cache: std::cell::RefCell::default(),
            query_cache: std::cell::RefCell::default(),
            query_cache_active: std::cell::Cell::new(false),
            dynamic: Vec::new(),
            dynamic_bounds: None,
            dynamic_buckets: std::sync::Arc::default(),
            light_certificates: std::cell::RefCell::default(),
            npc_profile: Default::default(),
            #[cfg(test)]
            legacy_dynamic_sweeps: false,
            #[cfg(test)]
            dynamic_publications: None,
            settled_supports: Vec::new(),
            weapon_obstacles: Vec::new(),
            camera_obstacles: Vec::new(),
            liquids: Vec::new(),
            dynamic_liquids: Vec::new(),
            traversal: Default::default(),
            brush_count,
            patch_count: 0,
            static_geometry,
            min,
            max,
        }
    }
    /// Complete the immutable static result against this frame's live geometry.
    /// Audio uses only fraction/start-solid; preserve the original clipping order.
    pub(crate) fn audio_complete(&self, mut result: Trace, start: Vec3, end: Vec3) -> Trace {
        let query = SweepBounds::new(start, end, Vec3::ZERO);
        for c in &self.settled_supports {
            if crate::sim_maps::npc_queries() && c.bounds.is_none_or(|b| !query.overlaps(b)) { continue; }
            for h in c.hulls().iter() {
                if crate::sim_maps::npc_queries() && !query.overlaps(h.bounds) { continue; }
                clip_hull(h, start, end, Vec3::ZERO, &mut result);
            }
        }
        let mut result = self.sweep_dynamic(result, start, end, Vec3::ZERO);
        for (_, c) in &self.weapon_obstacles {
            for h in c.hulls().iter() { clip_hull(h, start, end, Vec3::ZERO, &mut result); }
        }
        result
    }
    /// Scope exact static/settled query reuse to one immutable NPC update. Dynamic
    /// colliders are still swept on every request. Entries never survive a publication; scratch capacity is reused.
    pub fn with_npc_queries<T>(&self, run: impl FnOnce() -> T) -> T {
        if !crate::sim_maps::npc_queries() { return run(); }
        struct Reset<'a>(&'a World);
        impl Drop for Reset<'_> {
            fn drop(&mut self) {
                let _prepare = crate::npc::profile::controller(
                    crate::npc::profile::Kind::Prepare, &self.0.npc_profile, false);
                self.0.query_cache.borrow_mut().clear();
                self.0.query_cache_active.set(false);
            }
        }
        // Nested users share the outer scope rather than discarding its entries.
        let prepare = crate::npc::profile::controller(
            crate::npc::profile::Kind::Prepare, &self.npc_profile, false);
        if self.query_cache_active.replace(true) { drop(prepare); return run(); }
        debug_assert!(self.query_cache.borrow().is_empty());
        drop(prepare);
        let _reset = Reset(self);
        run()
    }
    fn query_key(start: Vec3, end: Vec3, half: Vec3) -> [u32; 9] {
        let mut key = [0; 9];
        key[..3].copy_from_slice(&start.to_array().map(f32::to_bits));
        key[3..6].copy_from_slice(&end.to_array().map(f32::to_bits));
        key[6..].copy_from_slice(&half.to_array().map(f32::to_bits));
        key
    }
    pub fn from_bsp(map: &Bsp) -> Result<Self> {
        Self::from_bsp_mask(map, PLAYER_MASK, true)
    }
    /// Scripted actors use solid and monster-clip, not Alice's player-only clips.
    pub fn actor_world(map: &Bsp) -> Result<Self> {
        Self::from_bsp_mask(map, 1 | 0x20000, true)
    }
    fn from_bsp_mask(map: &Bsp, mask: i32, accelerate: bool) -> Result<Self> {
        let _r13_memory = crate::allocation_census::Scope::new(3);
        let mut hulls = Vec::new();
        let mut liquids = Vec::new();
        for index in map.world_brushes.clone() {
            let brush = &map.brushes[index];
            let contents = map.shaders[brush.shader].contents;
            if contents & (mask | LIQUID_MASK) == 0 {
                continue;
            }
            let planes = map.side_planes[brush.sides.clone()]
                .iter()
                .map(|&p| map.planes[p])
                .collect::<Vec<_>>();
            // The shipped brushes include six axial bounding planes. Require those rather than
            // guessing finite bounds and accidentally culling a collidable wall.
            let mut min = Vec3::splat(f32::NEG_INFINITY);
            let mut max = Vec3::splat(f32::INFINITY);
            for p in &planes {
                for axis in 0..3 {
                    let other = (axis + 1) % 3;
                    let last = (axis + 2) % 3;
                    if p.normal[other].abs() < 1e-6 && p.normal[last].abs() < 1e-6 {
                        if p.normal[axis] > 0.99999 {
                            max[axis] = max[axis].min(p.distance / p.normal[axis]);
                        }
                        if p.normal[axis] < -0.99999 {
                            min[axis] = min[axis].max(p.distance / p.normal[axis]);
                        }
                    }
                }
            }
            ensure!(
                min.is_finite() && max.is_finite() && min.cmple(max + Vec3::splat(0.01)).all(),
                "Brush {index} lacks valid axial bounds"
            );
            let hull = Hull {
                planes,
                bounds: Bounds { min, max },
            };
            if contents & LIQUID_MASK != 0 {
                liquids.push((contents & LIQUID_MASK, hull.clone()));
            }
            if contents & mask != 0 {
                hulls.push(hull);
            }
        }
        let brush_count = hulls.len();
        for index in map.world_surfaces.clone() {
            let surface = &map.surfaces[index];
            let shader = &map.shaders[surface.shader];
            if surface.kind != 2 || shader.contents & mask == 0 || shader.flags & 0x4000 != 0 {
                continue;
            }
            let (v, i) = map.triangulate(surface);
            for t in i.chunks_exact(3) {
                if let Some(hull) = triangle_hull(
                    v[t[0] as usize].position,
                    v[t[1] as usize].position,
                    v[t[2] as usize].position,
                ) {
                    hulls.push(hull);
                }
            }
        }
        let patch_count = hulls.len() - brush_count;
        let tree = if hulls.is_empty() {
            None
        } else {
            Some(Node::build(
                &hulls,
                &mut (0..hulls.len()).collect::<Vec<_>>(),
            ))
        };
        Ok(Self {
            traversal: crate::traversal::Traversal::load(map)?,
            npc_sight_cache: std::cell::RefCell::default(),
            query_cache: std::cell::RefCell::default(),
            query_cache_active: std::cell::Cell::new(false),
            dynamic: Vec::new(),
            dynamic_bounds: None,
            dynamic_buckets: std::sync::Arc::default(),
            light_certificates: std::cell::RefCell::default(),
            npc_profile: Default::default(),
            #[cfg(test)]
            legacy_dynamic_sweeps: false,
            #[cfg(test)]
            dynamic_publications: None,
            settled_supports: Vec::new(),
            weapon_obstacles: Vec::new(),
            camera_obstacles: Vec::new(),
            liquids,
            dynamic_liquids: Vec::new(),
            static_geometry: std::sync::Arc::new(StaticGeometry::with_acceleration(hulls, tree, accelerate)),
            brush_count,
            patch_count,
            min: map.world_min,
            max: map.world_max,
        })
    }

    /// A cached negative certificate is valid only at bit-identical endpoints
    /// in this World. Static hulls/tree never change after construction. Settled,
    /// dynamic and weapon geometry can only lower the final sweep fraction.
    /// Return (visible, reused_negative_certificate).
    pub(crate) fn light_visible_cached(&self, eye: Vec3, light: Vec3) -> (bool, bool) {
        let key = Self::query_key(eye, light, Vec3::ZERO);
        let cached = self.light_certificates.borrow().entries.iter().find(|(k, _)| *k == key).map(|(_, b)| *b);
        if cached == Some(true) { return (false, true); }
        if cached.is_none() {
            let query = SweepBounds::new(eye, light, Vec3::ZERO);
            let trace = self.static_geometry.trace(eye,light,Vec3::ZERO,&query).0;
            let blocked = trace.fraction < 0.995;
            crate::sim_maps::count("lights_certificate_builds", 1);
            let mut cache = self.light_certificates.borrow_mut();
            if cache.entries.len() < 64 { cache.entries.push((key, blocked)); }
            else { let next = cache.next; cache.entries[next] = (key, blocked); cache.next = (next + 1) % 64; }
            if blocked { return (false, false); }
        }
        (self.sweep(eye, light, Vec3::ZERO).fraction >= 0.995, false)
    }

    /// Trace a box centre from start to end. Zero extents give a point trace.
    pub fn sweep(&self, start: Vec3, end: Vec3, half: Vec3) -> Trace {
        let _census = crate::phase_census::sweep();
        #[cfg(target_os = "vita")]
        {
            add_counter(Counter::SweepCalls, 1);
        }
        self.sweep_except(start, end, half, None)
    }
    /// Damage traces may contact a destructible obstacle itself, but must still
    /// respect every other obstacle. Movement never supplies an exception.
    pub fn sweep_except(&self, start: Vec3, end: Vec3, half: Vec3, ignore: Option<usize>) -> Trace {
        let _census = crate::phase_census::sweep();
        #[cfg(target_os = "vita")]
        {
            add_counter(Counter::SweepExceptCalls, 1);
        }
        let mut result = self.sweep_geometry(start, end, half);
        for (id, collider) in &self.weapon_obstacles {
            if Some(*id) == ignore {
                continue;
            }
            for h in collider.hulls().iter() {
                clip_hull(h, start, end, half, &mut result);
            }
        }
        result
    }
    /// Visible scenery can block the view without changing player or weapon collision.
    pub fn set_camera_obstacles(&mut self, obstacles: Vec<Collider>) {
        self.camera_obstacles = obstacles;
    }
    pub fn camera_sweep(&self, start: Vec3, end: Vec3, half: Vec3) -> Trace {
        let _owner = crate::phase_census::owner(crate::phase_census::Owner::Camera);
        let _census = crate::phase_census::sweep();
        #[cfg(target_os = "vita")]
        {
            add_counter(Counter::CameraSweepCalls, 1);
        }
        let mut result = self.sweep(start, end, half);
        for collider in &self.camera_obstacles {
            let trace = collider.trace(start, end, half);
            result.start_solid |= trace.start_solid;
            result.all_solid |= trace.all_solid;
            if trace.fraction < result.fraction {
                result.fraction = trace.fraction;
                result.normal = trace.normal;
            }
        }
        result
    }
    pub fn set_weapon_obstacles(&mut self, obstacles: Vec<crate::combat::Target>) {
        self.invalidate_npc_sight();
        self.weapon_obstacles = obstacles
            .into_iter()
            .map(|t| {
                (
                    t.id,
                    Collider::from_hulls(
                        Self::box_hulls(&[(t.center - t.half, t.center + t.half)]),
                    ),
                )
            })
            .collect();
    }
    pub fn sweep_geometry(&self, start: Vec3, end: Vec3, half: Vec3) -> Trace {
        let _census = crate::phase_census::sweep();
        self.npc_profile.sweep();
        crate::sim_maps::count("world_sweeps", 1);
        #[cfg(test)]
        if self.legacy_dynamic_sweeps {
            return self.sweep_geometry_legacy(start, end, half);
        }
        #[cfg(target_os = "vita")]
        {
            add_counter(Counter::SweepGeometryCalls, 1);
        }
        self.sweep_dynamic(self.ledge_trace(start, end, half), start, end, half)
    }
    fn sweep_dynamic(&self, mut result: Trace, start: Vec3, end: Vec3, half: Vec3) -> Trace {
        if self.dynamic.is_empty() {
            return result;
        }
        let query = SweepBounds::new(start, end, half);
        // This is the exact first predicate of Collider::trace, applied to a
        // conservative union. Include SKIN through the unchanged SweepBounds.
        if self.dynamic_bounds.is_none_or(|b| !b.overlaps(query.bounds)) {
            #[cfg(target_os = "vita")]
            add_counter(Counter::DynamicBoundsRejected, 1);
            return result;
        }
        let candidates = crate::sim_maps::dynamic_buckets().then(|| {
            let mut indices = Vec::new();
            for bucket in self.dynamic_buckets.iter() {
                crate::sim_maps::count("dynamic_buckets_tested", 1);
                // Use precisely the old AABB predicate, rather than a tighter
                // slab predicate whose rounding could prune an old candidate.
                if bucket.bounds.overlaps(query.bounds) { indices.extend_from_slice(&bucket.indices); }
            }
            indices.sort_unstable();
            indices
        });
        let count = candidates.as_ref().map_or(self.dynamic.len(), Vec::len);
        crate::sim_maps::count("dynamic_colliders_tested", count as u64);
        #[cfg(target_os = "vita")]
        add_counter(Counter::DynamicCollidersTested, count as u32);
        let mut rejected = 0;
        for n in 0..count {
            let collider = &self.dynamic[candidates.as_ref().map_or(n, |v| v[n])];
            if collider.bounds.is_none_or(|b| !b.overlaps(query.bounds)) {
                rejected += 1;
                continue;
            }
            // Reuse reciprocal directions/padding rather than rebuilding them
            // for every dynamic collider. Slab rejection and clip remain exact.
            let t = collider.trace_query(start, end, half, &query);
            result.start_solid |= t.start_solid;
            result.all_solid |= t.all_solid;
            if t.fraction < result.fraction {
                result.fraction = t.fraction;
                result.normal = t.normal;
            }
        }
        #[cfg(target_os = "vita")]
        add_counter(Counter::DynamicCollidersRejected, rejected);
        crate::sim_maps::count("dynamic_colliders_rejected", rejected as u64);
        #[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
        if std::env::var_os("LOOKING_GLASS_HEDGE3_VERIFY").is_some() {
            let mut reference = self.ledge_trace(start, end, half);
            for collider in &self.dynamic {
                let t = collider.trace(start, end, half);
                reference.start_solid |= t.start_solid;
                reference.all_solid |= t.all_solid;
                if t.fraction < reference.fraction { reference.fraction = t.fraction; reference.normal = t.normal; }
            }
            let bits = |t: Trace| (t.fraction.to_bits(), t.normal.to_array().map(f32::to_bits), t.start_solid, t.all_solid);
            assert_eq!(bits(result), bits(reference), "dynamic per-query equality");
            crate::sim_maps::count("dynamic_query_equal", 1);
        }
        result
    }
    #[cfg(test)]
    pub(crate) fn sweep_geometry_legacy(&self, start: Vec3, end: Vec3, half: Vec3) -> Trace {
        // Independent 8de8940 dynamic loop, without the new broad phase/cache.
        let mut result = self.ledge_trace(start, end, half);
        for collider in &self.dynamic {
            let t = collider.trace(start, end, half);
            result.start_solid |= t.start_solid;
            result.all_solid |= t.all_solid;
            if t.fraction < result.fraction {
                result.fraction = t.fraction;
                result.normal = t.normal;
            }
        }
        result
    }
    /// Static support probes for hanging; moving/rotating entities are excluded.
    pub fn ledge_trace(&self, start: Vec3, end: Vec3, half: Vec3) -> Trace {
        let key = Self::query_key(start, end, half);
        if let Some(hit) = self.query_cache.borrow().get(&key).copied() {
            crate::sim_maps::count("static_query_hits", 1);
            return hit;
        }
        crate::sim_maps::count("static_queries_executed", 1);
        #[cfg(target_os = "vita")]
        {
            add_counter(Counter::LedgeTraceCalls, 1);
        }
        #[cfg(target_os = "vita")]
        let mut support_hulls = 0u32;
        let query = SweepBounds::new(start, end, half);
        let (mut result,tested,_clipped) = self.static_geometry.trace(start,end,half,&query);
        for c in &self.settled_supports {
            // Match dynamic collision's conservative broad phase, including SKIN.
            // Test the placement before asking a lazy template for its hulls.
            if crate::sim_maps::npc_queries() && c.bounds.is_none_or(|b| !query.overlaps(b)) { continue; }
            for hull in c.hulls().iter() {
                if crate::sim_maps::npc_queries() && !query.overlaps(hull.bounds) { continue; }
                crate::sim_maps::count("settled_hull_tests", 1);
                #[cfg(target_os = "vita")]
                {
                    support_hulls += 1;
                }
                clip_hull(hull, start, end, half, &mut result);
            }
        }
        #[cfg(target_os = "vita")]
        {
            add_counter(Counter::StaticHullsTested, tested);
            add_counter(Counter::StaticHullsClipped, _clipped);
            add_counter(Counter::SettledHullsClipped, support_hulls);
        }
        self.npc_profile.hulls(tested as u64);
        if self.query_cache_active.get() {
            let mut cache = self.query_cache.borrow_mut();
            // Bound scratch memory even when a large cast issues unique queries.
            if cache.len() < 4096 { cache.insert(key, result); }
        }
        result
    }
    /// Keep movers, doors and live weapon obstacles when using an actor collision mask.
    pub fn copy_dynamic_from(&mut self, other: &Self) {
        self.invalidate_npc_sight();
        self.dynamic.clone_from(&other.dynamic);
        self.dynamic_bounds = other.dynamic_bounds;
        self.dynamic_buckets = other.dynamic_buckets.clone();
        self.traversal.actor_pushes.clone_from(&other.traversal.actor_pushes);
        self.settled_supports.clone_from(&other.settled_supports);
        self.weapon_obstacles.clone_from(&other.weapon_obstacles);
        self.dynamic_liquids.clone_from(&other.dynamic_liquids);
    }
    pub fn set_dynamic(&mut self, colliders: Vec<Collider>) {
        self.invalidate_npc_sight();
        self.dynamic_buckets = if crate::sim_maps::dynamic_buckets() {
            crate::sim_maps::count("dynamic_bucket_builds", 1);
            std::sync::Arc::new(DynamicBucket::build(&colliders))
        } else { std::sync::Arc::default() };
        self.dynamic_bounds = colliders.iter().filter_map(|c| c.bounds).reduce(Bounds::union);
        #[cfg(test)]
        if let Some(publications) = &mut self.dynamic_publications {
            publications.push(colliders.clone());
        }
        self.dynamic = colliders;
    }
    /// Geometry a controller has permanently settled may support hanging and pull-ups.
    /// Active movers are deliberately excluded.
    pub fn set_settled_supports(&mut self, colliders: Vec<Collider>) {
        self.invalidate_npc_sight();
        self.settled_supports = colliders;
    }
    /// Replace the brush-entity and moving liquid volumes. They add to the map's own liquids.
    pub fn set_dynamic_liquids(&mut self, liquids: Vec<Liquid>) {
        self.dynamic_liquids = liquids;
    }
    /// Exact convex-volume membership, including sloped liquid faces. Liquids are not solids.
    pub fn liquid_at(&self, point: Vec3) -> i32 {
        let map_liquid = self.liquids.iter().fold(0, |mask, (contents, hull)| {
            if point.cmpge(hull.bounds.min).all()
                && point.cmple(hull.bounds.max).all()
                && hull
                    .planes
                    .iter()
                    .all(|p| p.normal.dot(point) <= p.distance)
            {
                mask | contents
            } else {
                mask
            }
        });
        self.dynamic_liquids
            .iter()
            .fold(map_liquid, |mask, liquid| {
                if liquid.volume.contains(point) {
                    mask | (liquid.contents & LIQUID_MASK)
                } else {
                    mask
                }
            })
    }
    pub fn liquid_count(&self) -> usize {
        self.liquids.len()
    }
    pub fn body_trace(&self, feet: Vec3, end: Vec3) -> Trace {
        self.sweep(feet + PLAYER_CENTER, end + PLAYER_CENTER, PLAYER_HALF)
    }
    pub fn body_clear(&self, feet: Vec3) -> bool {
        !self.body_trace(feet, feet).start_solid
    }
    /// Resolve an editor actor origin onto the first walkable support below it.
    /// Check the whole declared body, including dynamic platforms. A small lift
    /// permits editor placements that overlap the floor; walls and ceilings are
    /// never crossed to search for another room's floor.
    pub fn actor_footing(&self, origin: Vec3, center: Vec3, half: Vec3, drop: f32) -> Option<Vec3> {
        if !origin.is_finite()
            || !center.is_finite()
            || !half.is_finite()
            || half.min_element() <= 0.
            || !drop.is_finite()
            || drop < 0.
        {
            return None;
        }
        for lift in [0., 1., 2., 4., 8.] {
            let start = origin + Vec3::Z * lift;
            if self.sweep(start + center, start + center, half).start_solid {
                continue;
            }
            let end = origin - Vec3::Z * drop;
            let trace = self.sweep(start + center, end + center, half);
            if !trace.start_solid && trace.fraction < 1. && trace.normal.z >= 0.65 {
                let landed = start.lerp(end, trace.fraction);
                if !self
                    .sweep(landed + center, landed + center, half)
                    .start_solid
                {
                    return Some(landed);
                }
            }
            // The first clear body determines the accessible space. Raising it
            // farther could jump over a thin obstruction or change floors.
            return None;
        }
        None
    }
    /// Correct only tiny existing overlaps. The entire escape must avoid any
    /// newly encountered solid, and its endpoint must fit the full player box.
    /// Deep obstructions are left for explicit recovery, never a large teleport.
    pub fn correct_footing(&self, feet: Vec3) -> Option<Vec3> {
        if self.body_clear(feet) {
            return None;
        }
        for distance in [0.125, 0.25, 0.5, 1.] {
            // Prefer lifting off a floor, then check equally small side/diagonal escapes.
            for z in [1., 0., -1.] {
                for x in [0., -1., 1.] {
                    for y in [0., -1., 1.] {
                        let direction = Vec3::new(x, y, z).normalize_or_zero();
                        if direction == Vec3::ZERO {
                            continue;
                        }
                        let next = feet + direction * distance;
                        let trace = self.body_trace(feet, next);
                        if !trace.all_solid && trace.fraction >= 1. && self.body_clear(next) {
                            return Some(next);
                        }
                    }
                }
            }
        }
        None
    }

    fn box_hulls(boxes: &[(Vec3, Vec3)]) -> Vec<Hull> {
        boxes
            .iter()
            .map(|&(min, max)| {
                let mut planes = Vec::new();
                for axis in 0..3 {
                    let mut normal = Vec3::ZERO;
                    normal[axis] = 1.;
                    planes.push(Plane {
                        normal,
                        distance: max[axis],
                    });
                    planes.push(Plane {
                        normal: -normal,
                        distance: -min[axis],
                    });
                }
                Hull {
                    planes,
                    bounds: Bounds { min, max },
                }
            })
            .collect()
    }

    pub fn fixture(boxes: &[(Vec3, Vec3)]) -> Self {
        let hulls = Self::box_hulls(boxes);
        let tree = if hulls.is_empty() {
            None
        } else {
            Some(Node::build(
                &hulls,
                &mut (0..hulls.len()).collect::<Vec<_>>(),
            ))
        };
        Self {
            npc_sight_cache: std::cell::RefCell::default(),
            query_cache: std::cell::RefCell::default(),
            query_cache_active: std::cell::Cell::new(false),
            dynamic: Vec::new(),
            dynamic_bounds: None,
            dynamic_buckets: std::sync::Arc::default(),
            light_certificates: std::cell::RefCell::default(),
            npc_profile: Default::default(),
            #[cfg(test)]
            legacy_dynamic_sweeps: false,
            #[cfg(test)]
            dynamic_publications: None,
            settled_supports: Vec::new(),
            weapon_obstacles: Vec::new(),
            camera_obstacles: Vec::new(),
            liquids: Vec::new(),
            dynamic_liquids: Vec::new(),
            traversal: Default::default(),
            brush_count: hulls.len(),
            patch_count: 0,
            static_geometry: std::sync::Arc::new(StaticGeometry::new(hulls, tree)),
            min: Vec3::splat(-10000.),
            max: Vec3::splat(10000.),
        }
    }
    #[cfg(test)]
    pub fn fixture_unaccelerated(boxes: &[(Vec3, Vec3)]) -> Self {
        let hulls = Self::box_hulls(boxes);
        let tree = if hulls.is_empty() { None } else {
            Some(Node::build(&hulls, &mut (0..hulls.len()).collect::<Vec<_>>()))
        };
        let brush_count = hulls.len();
        let mut world = Self::fixture(&[]);
        world.static_geometry = std::sync::Arc::new(StaticGeometry::with_acceleration(hulls, tree, false));
        world.brush_count = brush_count;
        world
    }
    #[cfg(test)]
    pub fn add_liquid(&mut self, min: Vec3, max: Vec3, contents: i32, slope: Option<Plane>) {
        let mut hull = Self::box_hulls(&[(min, max)]).remove(0);
        if let Some(p) = slope {
            hull.planes.push(p);
        }
        self.liquids.push((contents, hull));
    }
    #[cfg(test)]
    pub fn ramp_fixture() -> Self {
        let mut world = Self::fixture(&[(vec3(-200., -300., -1000.), vec3(500., 300., 1000.))]);
        std::sync::Arc::get_mut(&mut world.static_geometry).unwrap().hulls[0].planes.push(Plane {
            normal: vec3(-0.5, 0., 1.).normalize(),
            distance: 0.,
        });
        world
    }
}

fn clip_hull(hull: &Hull, start: Vec3, end: Vec3, half: Vec3, result: &mut Trace) {
    clip_hull_impl::<false>(hull,start,end,half,result,&mut sah::Counts::default());
}
fn clip_hull_counted(hull:&Hull,start:Vec3,end:Vec3,half:Vec3,result:&mut Trace,counts:&mut sah::Counts) {
    if crate::phase_census::is_active() || cfg!(all(feature="sim_maps_probe",not(target_os="vita"))) {
        clip_hull_impl::<true>(hull,start,end,half,result,counts);
    } else {clip_hull_impl::<false>(hull,start,end,half,result,counts);}
}
fn clip_hull_impl<const COUNT:bool>(hull: &Hull, start: Vec3, end: Vec3, half: Vec3, result: &mut Trace, counts:&mut sah::Counts) {
    let mut enter = f32::NEG_INFINITY;
    let mut leave = 1.0f32;
    let mut normal = Vec3::ZERO;
    let mut starts_out = false;
    let mut ends_out = false;
    for p in &hull.planes {
        if COUNT {counts.planes+=1;}
        let expanded = p.distance + p.normal.abs().dot(half);
        let a = p.normal.dot(start) - expanded;
        let b = p.normal.dot(end) - expanded;
        starts_out |= a >= 0.;
        ends_out |= b >= 0.;
        // A start exactly on a face is allowed to move along it or out of it.
        if a >= 0. && b >= a {
            return;
        }
        if a < 0. && b < 0. {
            continue;
        }
        if a > b {
            let fraction = (a - SKIN) / (a - b);
            if fraction > enter {
                enter = fraction;
                normal = p.normal;
            }
        } else if b > a {
            leave = leave.min((a + SKIN) / (a - b));
        }
        if enter > leave {
            return;
        }
    }
    if !starts_out {
        result.start_solid = true;
        if !ends_out {
            result.all_solid = true;
            result.fraction = 0.;
        }
        return;
    }
    if enter.is_finite() && enter < leave && enter < result.fraction {
        result.fraction = enter.max(0.);
        result.normal = normal;
    }
}

// A thin, two-sided triangular prism. Face and edge/axis bevels describe the
// Minkowski expansion for a box. Tessellation is an approximation of curved patches.
fn triangle_hull(a: Vec3, b: Vec3, c: Vec3) -> Option<Hull> {
    crate::sim_maps::count("triangle_hulls_built", 1);
    let cross = (b - a).cross(c - a);
    if cross.length_squared() < 1e-6 {
        return None;
    }
    let n = cross.normalize();
    let thickness = 0.125;
    let points = [
        a + n * thickness,
        b + n * thickness,
        c + n * thickness,
        a - n * thickness,
        b - n * thickness,
        c - n * thickness,
    ];
    let mut normals = vec![n, -n];
    let axes = [Vec3::X, Vec3::Y, Vec3::Z];
    for axis in axes {
        normals.push(axis);
        normals.push(-axis);
    }
    for edge in [b - a, c - b, a - c, n] {
        let edge = edge.normalize();
        let side = edge.cross(n);
        if side.length_squared() > 1e-6 {
            normals.push(side.normalize());
        }
        for axis in axes {
            let bevel = edge.cross(axis);
            if bevel.length_squared() > 1e-6 {
                normals.push(bevel.normalize());
                normals.push(-bevel.normalize());
            }
        }
    }
    let mut planes: Vec<Plane> = Vec::new();
    for normal in normals {
        if planes.iter().any(|p| p.normal.dot(normal) > 0.99999) {
            continue;
        }
        let distance = points
            .iter()
            .map(|p| normal.dot(*p))
            .fold(f32::NEG_INFINITY, f32::max);
        planes.push(Plane { normal, distance });
    }
    let min = points.iter().copied().reduce(Vec3::min)?;
    let max = points.iter().copied().reduce(Vec3::max)?;
    Some(Hull {
        planes,
        bounds: Bounds { min, max },
    })
}

#[cfg(test)]
mod dynamic_tests;
#[cfg(test)]
mod tests {
    #[test]
    fn trigger_probe_is_inside_a_brush_in_a_disconnected_volume() {
        let world = super::World::fixture(&[
            (
                super::Vec3::new(-100., -10., -10.),
                super::Vec3::new(-60., 10., 10.),
            ),
            (
                super::Vec3::new(60., -10., -10.),
                super::Vec3::new(100., 10., 10.),
            ),
        ]);
        let collider = super::Collider::from_hulls(std::sync::Arc::try_unwrap(world.static_geometry).unwrap().hulls.clone());
        assert!(!collider.contains(super::Vec3::ZERO));
        assert!(collider.contains(collider.interior_point().unwrap()));
    }
    use super::*;
    #[test]
    fn only_settled_controller_geometry_can_support_a_ledge() {
        let mut w=World::fixture(&[]);
        let ledge=Collider::box_bounds(vec3(-64.,-64.,0.),vec3(64.,64.,128.));
        let high=vec3(0.,0.,160.);let low=vec3(0.,0.,80.);
        w.set_dynamic(vec![ledge.clone()]);
        assert!(w.sweep(high,low,Vec3::ZERO).fraction<1.);
        assert_eq!(w.ledge_trace(high,low,Vec3::ZERO).fraction,1.);
        w.set_settled_supports(vec![ledge]);
        assert!(w.ledge_trace(high,low,Vec3::ZERO).fraction<1.);
        let mut actor=World::fixture(&[]);actor.copy_dynamic_from(&w);
        assert!(actor.ledge_trace(high,low,Vec3::ZERO).fraction<1.);
        w.set_settled_supports(vec![]);
        assert_eq!(w.ledge_trace(high,low,Vec3::ZERO).fraction,1.);
    }
    #[test]
    fn controller_liquids_add_to_the_map_liquids_and_can_move_or_vanish() {
        let mut world = World::fixture(&[(vec3(-100., -100., -10.), vec3(100., 100., 0.))]);
        world.add_liquid(vec3(-100., -100., 0.), vec3(0., 100., 50.), 0x08, None);
        let in_map = vec3(-50., 0., 10.);
        let in_pool = vec3(50., 0., 10.);
        assert_eq!(world.liquid_at(in_map), 0x08);
        assert_eq!(world.liquid_at(in_pool), 0);
        let pool = |x: f32| Liquid {
            contents: 0x10 | 0x1000,
            volume: Collider::fixture(vec3(x, -100., 0.), vec3(x + 100., 100., 50.)),
        };
        world.set_dynamic_liquids(vec![pool(0.)]);
        // The controller's volume adds its liquid bits (only LIQUID_MASK bits survive).
        assert_eq!(world.liquid_at(in_pool), 0x10);
        assert_eq!(world.liquid_at(in_map), 0x08);
        // A moving liquid is replaced wholesale each frame.
        world.set_dynamic_liquids(vec![pool(100.)]);
        assert_eq!(world.liquid_at(in_pool), 0);
        world.set_dynamic_liquids(Vec::new());
        assert_eq!(world.liquid_at(in_map), 0x08);
    }
    #[test]
    fn tangent_landing_at_large_coordinates_does_not_pin_player_or_break_saving() {
        // Synthetic floor and oblique wall, reproducing the rounded tangent
        // contact encountered beside a school entrance pillar. No game assets.
        let mut world = World::fixture(&[(vec3(-2200., 1400., -600.), vec3(-1300., 2400., -512.))]);
        let mut wall = World::box_hulls(&[(vec3(-1800., 1800., -600.), vec3(-1400., 2200., 0.))])
            .remove(0);
        wall.planes.push(Plane {
            normal: vec3(-0.65079135, 0.7592566, 0.),
            distance: 2543.2927,
        });
        world.set_dynamic(vec![Collider::from_hulls(vec![wall])]);
        let mut player = crate::movement::Player::new(vec3(-1761.25, 1867.965, -508.66));
        player.velocity = vec3(97.96471, 83.96974, -200.);
        let trace = world.body_trace(player.feet, player.feet + player.velocity / 120.);
        assert_eq!(trace.fraction, 0.);
        assert!(!trace.start_solid && !trace.all_solid);
        for _ in 0..1200 {
            player.tick(&world, crate::movement::Controls::default());
            assert!(world.body_clear(player.feet));
            player.validate_save().unwrap();
        }
        assert!(player.grounded);
        assert!(player.velocity.length() < 0.1);
        assert_eq!(player.landings, 1);
        assert!((player.feet.z + 512.).abs() < 0.1);
    }
    #[test]
    fn rotated_corner_is_clear_and_has_walkable_top_contact() {
        let source = World::fixture(&[(vec3(-4., -40., -4.), vec3(52., 40., 480.))]);
        for degrees in [0.1875_f32, 22.5, 45., -45.] {
            let hull = Hull::transformed(
                &source.static_geometry.hulls[0].planes,
                vec3(100., 3000., 64.),
                Quat::from_rotation_y(degrees.to_radians()),
            )
            .unwrap();
            // Both the broad phase and the convex sweep must agree just above
            // the highest corner, including the first tiny rotation increment.
            let top = vec3(if degrees > 0. { -4. } else { 52. }, 0., 480.);
            let corner = vec3(100., 3000., 64.) + Quat::from_rotation_y(degrees.to_radians()) * top;
            let feet = corner + Vec3::Z * 0.1;
            let mut world = World::fixture(&[]);
            world.set_dynamic(vec![Collider::from_hulls(vec![hull])]);
            assert!(world.body_clear(feet), "{degrees}");
            let t = world.body_trace(feet, feet - Vec3::Z * 2.);
            assert!(
                !t.start_solid && !t.all_solid && t.fraction < 1.,
                "{degrees}: {t:?}"
            );
            assert_eq!(t.normal, Vec3::Z, "{degrees}");
            let mut p = crate::movement::Player::new(feet + Vec3::Z * 40.);
            for _ in 0..240 {
                p.tick(&world, Default::default());
            }
            assert!(p.grounded && world.body_clear(p.feet), "{degrees}: {p:?}");
            let before = p.feet;
            p.tick(
                &world,
                crate::movement::Controls {
                    jump: true,
                    ..Default::default()
                },
            );
            assert!(p.jumps == 1 && p.feet.z > before.z, "{degrees}");
        }
    }
    #[test]
    fn tiny_overlap_escape_is_bounded_and_does_not_cross_another_solid() {
        let floor = (vec3(-100., -100., -10.), vec3(100., 100., 0.));
        let w = World::fixture(&[floor]);
        let feet = vec3(0., 0., -0.001);
        let corrected = w.correct_footing(feet).unwrap();
        assert!(corrected.z > 0. && corrected.distance(feet) <= 1. && w.body_clear(corrected));
        assert!(w.correct_footing(vec3(0., 0., -2.)).is_none());
        assert!(w.correct_footing(Vec3::Z).is_none());
        let roof = (vec3(-100., -100., 56.02), vec3(100., 100., 56.04));
        let tight = World::fixture(&[floor, roof]);
        assert!(
            tight.correct_footing(feet).is_none(),
            "Must not pass through the thin roof to a clear endpoint"
        );
        let mut p = crate::movement::Player::new(feet);
        p.tick(
            &w,
            crate::movement::Controls {
                jump: true,
                ..Default::default()
            },
        );
        assert!(p.jumps == 1 && p.feet.z > 0. && w.body_clear(p.feet));
    }
    #[test]
    fn sweep_prevents_tunnelling_and_allows_parallel_motion() {
        let w = World::fixture(&[(vec3(0., -100., -100.), vec3(1., 100., 100.))]);
        let t = w.sweep(vec3(-100., 0., 0.), vec3(1000., 0., 0.), Vec3::splat(2.));
        assert!(!t.start_solid);
        assert!(t.fraction < 0.1);
        assert_eq!(t.normal, -Vec3::X);
        let stop = -100. + 1100. * t.fraction;
        assert!((stop + 2. + SKIN).abs() < 0.001);
        assert_eq!(
            w.sweep(vec3(-2., 0., 0.), vec3(-2., 10., 0.), Vec3::splat(2.))
                .fraction,
            1.
        );
        assert_eq!(
            w.sweep(vec3(-2., 0., 0.), vec3(-1.99999, 0., 0.), Vec3::splat(2.))
                .fraction,
            0.
        );
        assert_eq!(
            w.sweep(vec3(-2., 0., 0.), vec3(-10., 0., 0.), Vec3::splat(2.))
                .fraction,
            1.
        );
    }
    #[test]
    fn reports_embedded_body_and_can_exit() {
        let w = World::fixture(&[(Vec3::splat(-10.), Vec3::splat(10.))]);
        let t = w.sweep(Vec3::ZERO, Vec3::X, Vec3::ZERO);
        assert!(t.start_solid && t.all_solid);
        assert_eq!(t.fraction, 0.);
        let t = w.sweep(Vec3::ZERO, vec3(20., 0., 0.), Vec3::ZERO);
        assert!(t.start_solid && !t.all_solid);
        assert_eq!(t.fraction, 1.);
    }
    #[test]
    fn patch_triangle_has_both_sides_and_bounds() {
        let hull = triangle_hull(
            vec3(-100., -100., 0.),
            vec3(100., -100., 0.),
            vec3(0., 100., 0.),
        )
        .unwrap();
        for (start, end) in [
            (vec3(0., 0., 100.), vec3(0., 0., -100.)),
            (vec3(0., 0., -100.), vec3(0., 0., 100.)),
        ] {
            let mut t = Trace::default();
            clip_hull(&hull, start, end, Vec3::splat(1.), &mut t);
            assert!(t.fraction > 0.4 && t.fraction < 0.5);
        }
        let mut t = Trace::default();
        clip_hull(
            &hull,
            vec3(200., 0., 100.),
            vec3(200., 0., -100.),
            Vec3::splat(1.),
            &mut t,
        );
        assert_eq!(t.fraction, 1.);
    }
}

#[cfg(test)]
mod performance_tests;

#[cfg(test)]
mod sim_query_tests {
    use super::*;
    fn bits(t: Trace) -> [u32; 6] {
        [t.fraction.to_bits(), t.normal.x.to_bits(), t.normal.y.to_bits(), t.normal.z.to_bits(), t.start_solid as u32, t.all_solid as u32]
    }
    #[test]
    fn settled_pruning_and_scoped_reuse_keep_exact_traces_and_invalidate() {
        let mut world = World::fixture(&[(vec3(-200., -200., -20.), vec3(200., 200., 0.))]);
        world.set_settled_supports((-8..=8).map(|x| Collider::box_bounds(vec3(x as f32 * 100., -30., 20.), vec3(x as f32 * 100. + 40., 30., 60.))).collect());
        let mut probes = Vec::new();
        for x in -30..=30 {
            for delta in [Vec3::ZERO, Vec3::Z * 200., vec3(150., -80., 120.)] {
                for half in [Vec3::ZERO, Vec3::ONE, PLAYER_HALF] {
                    let at = vec3(x as f32 * 15. + SKIN, -0., 60. + SKIN);
                    probes.push((at + delta, at - delta, half));
                }
            }
        }
        let reference = crate::sim_maps::with_test_switches([false; 4], || probes.iter().map(|&(a,b,h)| bits(world.ledge_trace(a,b,h))).collect::<Vec<_>>());
        crate::sim_maps::with_test_switches([false,false,true,false], || {
            world.with_npc_queries(|| {
                for ((a,b,h), expected) in probes.iter().copied().zip(reference) {
                    assert_eq!(bits(world.ledge_trace(a,b,h)), expected);
                    assert_eq!(bits(world.ledge_trace(a,b,h)), expected);
                }
                assert_eq!(world.query_cache.borrow().len(), probes.len());
            });
            assert!(world.query_cache.borrow().is_empty());
            assert!(!world.query_cache_active.get());
            let high = vec3(20.,0.,100.); let low = vec3(20.,0.,10.);
            assert!(world.with_npc_queries(|| world.ledge_trace(high,low,Vec3::ZERO)).fraction < 1.);
            world.set_settled_supports(Vec::new());
            assert_eq!(world.with_npc_queries(|| world.ledge_trace(high,low,Vec3::ZERO)).fraction, 1.);
        });
    }
    #[test]
    fn scoped_query_scratch_retains_capacity_and_clears_on_unwind() {
        crate::sim_maps::with_test_switches([false, false, true, false], || {
            let world = World::fixture(&[]);
            world.with_npc_queries(|| {
                for i in 0..5000 {
                    world.ledge_trace(Vec3::X * i as f32, Vec3::ZERO, Vec3::ZERO);
                }
                assert_eq!(world.query_cache.borrow().len(), 4096);
                world.with_npc_queries(|| assert_eq!(world.query_cache.borrow().len(), 4096));
            });
            let capacity = world.query_cache.borrow().capacity();
            assert!(capacity >= 4096);
            for _ in 0..3 {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    world.with_npc_queries(|| {
                        assert!(world.query_cache.borrow().is_empty());
                        world.ledge_trace(Vec3::ZERO, Vec3::Z, Vec3::ZERO);
                        panic!("exercise scope cleanup");
                    });
                }));
                assert!(result.is_err());
                assert!(!world.query_cache_active.get());
                assert!(world.query_cache.borrow().is_empty());
                assert_eq!(world.query_cache.borrow().capacity(), capacity);
            }
            world.ledge_trace(Vec3::ZERO, Vec3::Z, Vec3::ZERO);
            assert!(world.query_cache.borrow().is_empty());
        });
    }
    #[test]
    fn distant_settled_template_stays_lazy() {
        let t = model_shape::Template::triangles(&[[Vec3::ZERO, Vec3::X * 30., Vec3::Y * 30.]]);
        let c = t.at(Vec3::splat(2000.), Quat::IDENTITY);
        let mut world = World::fixture(&[]);
        world.set_settled_supports(vec![c.clone()]);
        crate::sim_maps::with_test_switches([false,false,true,false], || {
            assert_eq!(world.ledge_trace(Vec3::ZERO,Vec3::Z,PLAYER_HALF).fraction, 1.);
            assert!(c.lazy.as_ref().unwrap().pending());
        });
    }
}

#[cfg(test)]
mod npc_sight_tests {
    use super::*;
    fn bits(t: Trace) -> (u32, [u32; 3], bool, bool) {
        (t.fraction.to_bits(), t.normal.to_array().map(f32::to_bits), t.start_solid, t.all_solid)
    }
    #[test]
    fn full_trace_cache_is_bounded_exact_and_invalidated_by_every_solid_publication() {
        crate::sim_maps::with_think_switches([false, true, false], || {
            let mut world = World::fixture(&[]);
            let start = Vec3::ZERO;
            let end = Vec3::X * 100.;
            let check = |w: &World| {
                let expected = bits(w.sweep(start, end, Vec3::splat(0.5)));
                assert_eq!(bits(w.npc_sight(start, end)), expected);
                assert_eq!(bits(w.npc_sight(start, end)), expected);
                assert_eq!(w.npc_sight_cache.borrow().len(), 1);
            };
            let wall = || Collider::fixture(vec3(40., -10., -10.), vec3(50., 10., 10.));
            check(&world);
            world.set_dynamic(vec![wall()]);
            assert!(world.npc_sight_cache.borrow().is_empty()); check(&world);
            world.set_dynamic(vec![]); check(&world);
            world.set_settled_supports(vec![wall()]);
            assert!(world.npc_sight_cache.borrow().is_empty()); check(&world);
            world.set_settled_supports(vec![]); check(&world);
            world.set_weapon_obstacles(vec![crate::combat::Target { id: 42, center: Vec3::X * 45., half: Vec3::splat(5.) }]);
            assert!(world.npc_sight_cache.borrow().is_empty()); check(&world);
            let other = World::fixture(&[]);
            world.copy_dynamic_from(&other);
            assert!(world.npc_sight_cache.borrow().is_empty()); check(&world);
            world.npc_sight(vec3(-0., 0., 0.), end);
            assert_eq!(world.npc_sight_cache.borrow().len(), 2);
            for n in 1..300 { world.npc_sight(Vec3::Z * n as f32, end); }
            assert_eq!(world.npc_sight_cache.borrow().len(), 256);
            let separate = World::fixture(&[(vec3(40., -10., -10.), vec3(50., 10., 10.))]);
            assert!(separate.npc_sight(start, end).fraction < 1.);
            assert_eq!(world.npc_sight(start, end).fraction, 1.);
        });
    }
}
