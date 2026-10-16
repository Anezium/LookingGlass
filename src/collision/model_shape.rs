//! Cached local geometry for opt-in moving brush and patch collision.
use super::*;
#[derive(Debug)]
struct Convex {
    source: Vec<Plane>,
    vertices: Vec<Vec3>,
    edges: Vec<Vec3>,
}
impl Convex {
    fn orient(&self, rotation: Quat, edges: &mut Vec<Vec3>, normals: &mut Vec<Vec3>) {
        edges.clear();
        for delta in &self.edges {
            let edge = (rotation * *delta).normalize();
            if !edges
                .iter()
                .any(|e| e.distance_squared(edge).min(e.distance_squared(-edge)) < 1e-10)
            {
                edges.push(edge);
            }
        }
        normals.clear();
        normals.extend(self.source.iter().map(|p| rotation * p.normal));
        let mut support = |normal: Vec3| {
            if !normals.iter().any(|n| n.distance_squared(normal) < 1e-10) {
                normals.push(normal);
            }
        };
        // Same first representatives, thresholds and emission order as at().
        for axis in [Vec3::X, Vec3::Y, Vec3::Z] {
            support(axis);
            support(-axis);
        }
        for edge in edges.iter().copied() {
            for axis in [Vec3::X, Vec3::Y, Vec3::Z] {
                let n = edge.cross(axis);
                if n.length_squared() > 0.000001 {
                    support(n.normalize());
                    support(-n.normalize());
                }
            }
        }
    }
    fn new(source: &[Plane]) -> Result<Self> {
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
                    // Keep the unnormalized edge and its original order. The
                    // legacy path normalizes/deduplicates after rotation; doing
                    // that locally changes rounding at nearly parallel bevels.
                    edges.push(*b - *a);
                }
            }
        }

        Ok(Self {
            source: source.to_vec(),
            vertices,
            edges,
        })
    }
    fn at(&self, origin: Vec3, rotation: Quat) -> Hull {
        let mut edges = Vec::<Vec3>::new();
        for delta in &self.edges {
            let edge = (rotation * *delta).normalize();
            if !edges
                .iter()
                .any(|e| e.distance_squared(edge).min(e.distance_squared(-edge)) < 1e-10)
            {
                edges.push(edge);
            }
        }
        let points = self
            .vertices
            .iter()
            .map(|v| origin + rotation * *v)
            .collect::<Vec<_>>();
        let mut planes = self
            .source
            .iter()
            .map(|p| {
                let normal = rotation * p.normal;
                Plane {
                    normal,
                    distance: p.distance + normal.dot(origin),
                }
            })
            .collect::<Vec<_>>();
        for axis in [Vec3::X, Vec3::Y, Vec3::Z] {
            add_support(&mut planes, &points, axis);
            add_support(&mut planes, &points, -axis);
        }
        for edge in edges {
            for axis in [Vec3::X, Vec3::Y, Vec3::Z] {
                let n = edge.cross(axis);
                if n.length_squared() > 0.000001 {
                    add_support(&mut planes, &points, n.normalize());
                    add_support(&mut planes, &points, -n.normalize());
                }
            }
        }
        Hull {
            planes,
            bounds: Bounds {
                min: points.iter().copied().reduce(Vec3::min).unwrap(),
                max: points.iter().copied().reduce(Vec3::max).unwrap(),
            },
        }
    }
}
#[derive(Debug)]
struct Mesh {
    vertices: Vec<Vec3>,
    triangles: Vec<[usize; 3]>,
}
#[derive(Debug)]
struct Geometry {
    mesh: Option<Mesh>,
    brushes: Vec<Convex>,
    patches: Vec<[Vec3; 3]>,
    bounds: Option<Bounds>,
}
#[derive(Debug, Default)]
struct EagerCache {
    rotation: Option<[u32; 4]>,
    last: Option<([u32; 3], Collider)>,
    normals: Vec<Vec<Vec3>>,
    edges: Vec<Vec3>,
    points: Vec<Vec3>,
}
#[derive(Debug)]
pub struct Template {
    geometry: std::sync::Arc<Geometry>,
    eager: std::cell::RefCell<EagerCache>,
    last_lazy: std::cell::RefCell<Option<([u32; 7], Collider)>>,
}
#[derive(Debug)]
pub(super) struct Placed {
    geometry: std::sync::Arc<Geometry>,
    origin: Vec3,
    rotation: Quat,
    hulls: std::sync::OnceLock<Vec<Hull>>,
}
impl Placed {
    #[cfg(test)]
    pub(super) fn pending(&self) -> bool { self.hulls.get().is_none() }
    pub(super) fn hulls(&self) -> &[Hull] {
        self.hulls
            .get_or_init(|| {
                #[cfg(target_os = "vita")]
                let _profile = crate::frame_profile::span("collider_lazy_build");
                #[cfg(target_os = "vita")]
                add_counter(Counter::LazyHullBuilds, 1);
                crate::sim_maps::count("lazy_hull_builds", 1);
                self.geometry.at(self.origin, self.rotation)
            })
    }
}
impl Geometry {
    fn triangles_at(&self, origin: Vec3, rotation: Quat) -> Vec<Hull> {
        if let Some(mesh) = &self.mesh {
            let points = mesh.vertices.iter().map(|v| origin + rotation * *v).collect::<Vec<_>>();
            mesh.triangles.iter().filter_map(|t| triangle_hull(points[t[0]], points[t[1]], points[t[2]])).collect()
        } else {
            self.patches.iter().filter_map(|p| triangle_hull(origin + rotation * p[0], origin + rotation * p[1], origin + rotation * p[2])).collect()
        }
    }
    fn at_cached(&self, origin: Vec3, rotation: Quat, cache: &mut EagerCache) -> Vec<Hull> {
        // No epsilon or quaternion sign equivalence: only identical input bits
        // may reuse rotated normals. Translation never affects their selection.
        let key = Some(rotation.to_array().map(f32::to_bits));
        let reorient = cache.rotation != key;
        cache.normals.resize_with(self.brushes.len(), Vec::new);
        let mut hulls = Vec::with_capacity(self.brushes.len() + self.patches.len());
        for (brush, normals) in self.brushes.iter().zip(&mut cache.normals) {
            if reorient {
                brush.orient(rotation, &mut cache.edges, normals);
            }
            cache.points.clear();
            cache
                .points
                .extend(brush.vertices.iter().map(|v| origin + rotation * *v));
            let planes = normals
                .iter()
                .enumerate()
                .map(|(i, &normal)| {
                    let distance = if i < brush.source.len() {
                        brush.source[i].distance + normal.dot(origin)
                    } else {
                        // Preserve world-space f32 support products and maximum order.
                        // Translating a cached distance would round differently.
                        cache
                            .points
                            .iter()
                            .map(|p| normal.dot(*p))
                            .fold(f32::NEG_INFINITY, f32::max)
                    };
                    Plane { normal, distance }
                })
                .collect();
            hulls.push(Hull {
                planes,
                bounds: Bounds {
                    min: cache.points.iter().copied().reduce(Vec3::min).unwrap(),
                    max: cache.points.iter().copied().reduce(Vec3::max).unwrap(),
                },
            });
        }
        cache.rotation = key;
        hulls.extend(self.triangles_at(origin, rotation));
        hulls
    }
    fn at(&self, origin: Vec3, rotation: Quat) -> Vec<Hull> {
        let mut hulls = self
            .brushes
            .iter()
            .map(|s| s.at(origin, rotation))
            .collect::<Vec<_>>();
        hulls.extend(self.triangles_at(origin, rotation));
        hulls
    }
}
impl Template {
    /// Preserve triangle order and bit-identical vertices, including degenerate faces.
    pub fn triangles(triangles: &[[Vec3; 3]]) -> Self {
        let mut ids = std::collections::BTreeMap::new();
        let mut vertices = Vec::new();
        let triangles_indexed = triangles.iter().map(|t| t.map(|v| {
            *ids.entry(v.to_array().map(f32::to_bits)).or_insert_with(|| {
                let id = vertices.len(); vertices.push(v); id
            })
        })).collect();
        let mut template = Self::from_parts(Vec::new(), triangles.to_vec());
        std::sync::Arc::get_mut(&mut template.geometry).unwrap().mesh = Some(Mesh { vertices, triangles: triangles_indexed });
        template
    }
    /// Reuse only identical transform bits; published/lazy hulls remain immutable.
    pub fn at_reused(&self, origin: Vec3, rotation: Quat) -> Collider {
        let mut key = [0; 7];
        key[..3].copy_from_slice(&origin.to_array().map(f32::to_bits));
        key[3..].copy_from_slice(&rotation.to_array().map(f32::to_bits));
        let mut last = self.last_lazy.borrow_mut();
        if let Some((old, collider)) = &*last {
            if *old == key { return collider.clone(); }
        }
        let collider = self.at(origin, rotation);
        *last = Some((key, collider.clone()));
        collider
    }
    pub fn model(map: &Bsp, index: usize) -> Result<Self> {
        Self::load(map, index, true)
    }
    /// Cache the same solid brushes used by Collider::model, without adding patches.
    pub fn brush_model(map: &Bsp, index: usize) -> Result<Self> {
        Self::load(map, index, false)
    }
    fn load(map: &Bsp, index: usize, include_patches: bool) -> Result<Self> {
        let model = map
            .models
            .get(index)
            .ok_or_else(|| anyhow::anyhow!("Invalid moving model"))?;
        #[cfg(target_os = "vita")]
        crate::frame_profile::counts(&[("collider_local_builds", 1)]);
        let mut brushes = Vec::new();
        let mut patches = Vec::new();
        for i in model.brushes.clone() {
            let b = &map.brushes[i];
            if map.shaders[b.shader].contents & PLAYER_MASK == 0 {
                continue;
            }
            brushes.push(Convex::new(
                &map.side_planes[b.sides.clone()]
                    .iter()
                    .map(|p| map.planes[*p])
                    .collect::<Vec<_>>(),
            )?);
        }
        for i in model.surfaces.clone().filter(|_| include_patches) {
            let s = &map.surfaces[i];
            let shader = &map.shaders[s.shader];
            if s.kind != 2 || shader.contents & PLAYER_MASK == 0 || shader.flags & 0x4000 != 0 {
                continue;
            }
            let (v, indices) = map.triangulate(s);
            patches.extend(indices.chunks_exact(3).map(|i| {
                [
                    v[i[0] as usize].position,
                    v[i[1] as usize].position,
                    v[i[2] as usize].position,
                ]
            }));
        }
        Ok(Self::from_parts(brushes, patches))
    }
    fn from_parts(brushes: Vec<Convex>, patches: Vec<[Vec3; 3]>) -> Self {
        let points = brushes
            .iter()
            .flat_map(|b| b.vertices.iter().copied())
            .chain(patches.iter().flat_map(|p| p.iter().copied()))
            .collect::<Vec<_>>();
        let bounds = points
            .iter()
            .copied()
            .reduce(Vec3::min)
            .zip(points.iter().copied().reduce(Vec3::max))
            .map(|(min, max)| Bounds {
                min: min - Vec3::splat(0.125),
                max: max + Vec3::splat(0.125),
            });
        Self {
            eager: std::cell::RefCell::default(),
            last_lazy: std::cell::RefCell::default(),
            geometry: std::sync::Arc::new(Geometry {
                mesh: None,
                brushes,
                patches,
                bounds,
            }),
        }
    }
    /// Place cached geometry immediately, with the exact union of hull bounds.
    /// Unlike `at`, this keeps Collider::model's broad phase and profiles the
    /// entire placement in the caller's construction span. Only world supports
    /// and bevel distances are regenerated; identical rotations also reuse their
    /// exact normal sequence. Scratch vectors retain capacity between placements.
    pub fn at_eager(&self, origin: Vec3, rotation: Quat) -> Collider {
        #[cfg(target_os = "vita")]
        let _transform_profile = crate::frame_profile::span("collider_transform");
        let mut cache = self.eager.borrow_mut();
        let origin_key = origin.to_array().map(f32::to_bits);
        if cache.rotation == Some(rotation.to_array().map(f32::to_bits)) {
            if let Some((key, collider)) = &cache.last {
                if *key == origin_key {
                    // A rejected mover can request the same candidate on the
                    // next substep. Published Arc geometry stays immutable.
                    return collider.clone();
                }
            }
        }
        let collider = Collider::from_hulls(self.geometry.at_cached(origin, rotation, &mut cache));
        cache.last = Some((origin_key, collider.clone()));
        collider
    }
    pub fn at(&self, origin: Vec3, rotation: Quat) -> Collider {
        // Conservative transformed bounds reject distant contacts without building
        // thousands of patch hulls. The exact hulls are cached on the first query.
        let bounds = self.geometry.bounds.map(|b| {
            let mut min = Vec3::splat(f32::INFINITY);
            let mut max = -min;
            for x in [b.min.x, b.max.x] {
                for y in [b.min.y, b.max.y] {
                    for z in [b.min.z, b.max.z] {
                        let p = origin + rotation * Vec3::new(x, y, z);
                        min = min.min(p);
                        max = max.max(p);
                    }
                }
            }
            Bounds { min, max }
        });
        Collider {
            hulls: std::sync::Arc::new(Vec::new()),
            bounds,
            lazy: Some(std::sync::Arc::new(Placed {
                geometry: self.geometry.clone(),
                origin,
                rotation,
                hulls: std::sync::OnceLock::new(),
            })),
        }
    }
    pub fn counts(&self) -> (usize, usize) {
        (self.geometry.brushes.len(), self.geometry.patches.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn brush_map() -> Bsp {
        use crate::bsp::{MapBrush, Model, Shader};
        let box_planes = |min, max| World::fixture(&[(min, max)]).hulls.remove(0).planes;
        let shelf = box_planes(vec3(-80., -12., -5.), vec3(80., 12., 5.));
        let mut chamfered = box_planes(Vec3::splat(-24.), Vec3::splat(24.));
        for normal in [
            vec3(1., 1., 0.),
            vec3(-1., 1., 0.),
            vec3(-1., -1., 0.),
            vec3(1., -1., 0.),
        ] {
            chamfered.push(Plane {
                normal: normal.normalize(),
                distance: 32.,
            });
        }
        let prism = vec![
            Plane {
                normal: -Vec3::Y,
                distance: 20.,
            },
            Plane {
                normal: vec3(1., 1., 0.).normalize(),
                distance: 30.,
            },
            Plane {
                normal: vec3(-1., 1., 0.).normalize(),
                distance: 30.,
            },
            Plane {
                normal: Vec3::Z,
                distance: 8.,
            },
            Plane {
                normal: -Vec3::Z,
                distance: 8.,
            },
        ];
        let oblique = box_planes(vec3(-35., -17., -9.), vec3(35., 17., 9.))
            .into_iter()
            .map(|p| Plane {
                normal: Quat::from_rotation_x(0.37) * Quat::from_rotation_z(0.23) * p.normal,
                ..p
            })
            .collect();
        let mut map = crate::interaction::tests::fixture();
        map.models.clear();
        map.brushes.clear();
        map.planes.clear();
        map.side_planes.clear();
        map.entities.clear();
        map.shaders.extend([
            Shader {
                name: "nonsolid".into(),
                flags: 0,
                contents: 0,
            },
            Shader {
                name: "playerclip".into(),
                flags: 0,
                contents: 0x10000,
            },
        ]);
        for (i, source) in [shelf, chamfered, prism, oblique].into_iter().enumerate() {
            let start = map.planes.len();
            map.planes.extend(source);
            let end = map.planes.len();
            map.side_planes.extend(start..end);
            map.brushes.push(MapBrush {
                sides: start..end,
                shader: if i == 2 { 2 } else { 0 },
            });
            map.models.push(Model {
                min: Vec3::splat(-100.),
                max: Vec3::splat(100.),
                surfaces: 0..0,
                brushes: i..i + 1,
            });
        }
        // An invalid non-solid brush must be skipped by both solid-only paths.
        map.brushes.push(MapBrush {
            sides: 0..0,
            shader: 1,
        });
        map.models.push(Model {
            min: Vec3::splat(-100.),
            max: Vec3::splat(100.),
            surfaces: 0..0,
            brushes: 0..5,
        });
        map.models.push(Model {
            min: Vec3::ZERO,
            max: Vec3::ZERO,
            surfaces: 0..0,
            brushes: 4..5,
        });
        map
    }
    fn poses() -> Vec<(Vec3, Quat)> {
        let mut poses = [0., 0.0001, 0.31, 1.2, 2.5, std::f32::consts::FRAC_PI_2]
            .into_iter()
            .map(|a| (vec3(400., -200., 50.), Quat::from_rotation_z(a)))
            .collect::<Vec<_>>();
        poses.extend([
            (Vec3::ZERO, Quat::IDENTITY),
            (
                vec3(-4188., 6004., 32.03125),
                Quat::from_euler(glam::EulerRot::ZYX, 0.73, -0.41, 1.13),
            ),
            (
                vec3(-120., 210., -32.),
                Quat::from_euler(glam::EulerRot::ZYX, -1.71, 0.85, -0.37),
            ),
        ]);
        poses
    }
    // Independently recover the reference hull vertices from its world planes.
    fn vertices(planes: &[Plane]) -> Vec<Vec3> {
        let mut points = Vec::<Vec3>::new();
        for (i, a) in planes.iter().enumerate() {
            for (j, b) in planes.iter().enumerate().skip(i + 1) {
                for c in &planes[j + 1..] {
                    let det = a.normal.dot(b.normal.cross(c.normal));
                    if det.abs() < 0.00001 {
                        continue;
                    }
                    let p = (a.distance * b.normal.cross(c.normal)
                        + b.distance * c.normal.cross(a.normal)
                        + c.distance * a.normal.cross(b.normal))
                        / det;
                    if p.is_finite()
                        && planes.iter().all(|a| a.normal.dot(p) <= a.distance + 0.01)
                        && !points.iter().any(|a| a.distance_squared(p) < 0.0001)
                    {
                        points.push(p);
                    }
                }
            }
        }
        points
    }
    fn near(a: Vec3, b: Vec3) {
        assert!((a - b).abs().max_element() <= 1e-3, "{a:?} != {b:?}");
    }
    fn same_trace(a: Trace, b: Trace) {
        assert_eq!(a.start_solid, b.start_solid);
        assert_eq!(a.all_solid, b.all_solid);
        assert!((a.fraction - b.fraction).abs() <= 1e-3, "{a:?} != {b:?}");
        near(a.normal, b.normal);
    }
    fn trace_bits(t: Trace) -> [u32; 6] {
        [
            t.fraction.to_bits(),
            t.normal.x.to_bits(),
            t.normal.y.to_bits(),
            t.normal.z.to_bits(),
            u32::from(t.start_solid),
            u32::from(t.all_solid),
        ]
    }
    #[test]
    fn eager_orientation_cache_is_bitwise_exact_across_pose_sequences() {
        let map = brush_map();
        let mut comparisons = 0;
        for model in 0..map.models.len() {
            let shape = Template::brush_model(&map, model).unwrap();
            // Cold rotation, unchanged rotation with translation, and return to
            // an earlier translation. Each next pose also invalidates rotation.
            for (base, rotation) in poses() {
                for origin in [
                    base,
                    base + vec3(0.003, -0.125, 1.),
                    base + Vec3::splat(100.),
                    base,
                ] {
                    // Geometry::at is the unchanged 8de8940 eager algorithm.
                    let reference = Collider::from_hulls(shape.geometry.at(origin, rotation));
                    let cached = shape.at_eager(origin, rotation);
                    assert_eq!(reference.geometry_bits(), cached.geometry_bits());
                    let repeated = shape.at_eager(origin, rotation);
                    assert!(std::sync::Arc::ptr_eq(&cached.hulls, &repeated.hulls));
                    assert_eq!(reference.geometry_bits(), repeated.geometry_bits());
                    for half in [Vec3::ZERO, Vec3::ONE, PLAYER_HALF] {
                        for x in -4..=4 {
                            for y in -4..=4 {
                                let offset = vec3(x as f32 * 25., y as f32 * 25., 0.);
                                for delta in [
                                    Vec3::Z * 150.,
                                    Vec3::X * 150.,
                                    vec3(120., -90., 100.),
                                    Vec3::ZERO,
                                ] {
                                    let start = origin + offset + delta;
                                    let end = origin + offset - delta;
                                    assert_eq!(
                                        trace_bits(reference.trace(start, end, half)),
                                        trace_bits(cached.trace(start, end, half))
                                    );
                                    comparisons += 1;
                                }
                            }
                        }
                    }
                }
            }
            // Very small rotations, adjacent f32 quarters, signed zero and
            // equivalent quaternion signs must not use an approximate key.
            let quarter = std::f32::consts::FRAC_PI_2;
            for angle in [
                0.,
                -0.,
                1e-6,
                1e-5,
                0.0001,
                f32::from_bits(quarter.to_bits() - 1),
                quarter,
                f32::from_bits(quarter.to_bits() + 1),
            ] {
                let q = Quat::from_rotation_y(angle);
                for rotation in [q, -q, q] {
                    for origin in [vec3(-4188., 6004., 32.03125), Vec3::ZERO] {
                        let reference = Collider::from_hulls(shape.geometry.at(origin, rotation));
                        let cached = shape.at_eager(origin, rotation);
                        assert_eq!(reference.geometry_bits(), cached.geometry_bits());
                    }
                }
            }
        }
        assert_eq!(comparisons, 209_952);
    }
    #[test]
    fn eager_cache_preserves_skin_sensitive_redundant_tetrahedron_bevel() {
        let source = [
            Plane {
                normal: -Vec3::X,
                distance: 0.,
            },
            Plane {
                normal: -Vec3::Y,
                distance: 0.,
            },
            Plane {
                normal: -Vec3::Z,
                distance: 0.,
            },
            Plane {
                normal: Vec3::ONE.normalize(),
                distance: 1. / 3_f32.sqrt(),
            },
        ];
        let shape = Template::from_parts(vec![Convex::new(&source).unwrap()], Vec::new());
        let normal = -vec3(1., 1., 0.).normalize();
        for origin in [Vec3::ZERO, vec3(-4188., 6004., 32.03125), Vec3::ZERO] {
            let old = Collider::from_hulls(shape.geometry.at(origin, Quat::IDENTITY));
            let new = shape.at_eager(origin, Quat::IDENTITY);
            assert_eq!(old.geometry_bits(), new.geometry_bits());
            let start = origin + vec3(-0.25, -0.25, 0.25);
            let end = origin + vec3(0.25, 0.25, 0.25);
            let t = new.trace(start, end, Vec3::ZERO);
            assert_eq!(trace_bits(old.trace(start, end, Vec3::ZERO)), trace_bits(t));
            assert!(t.normal.distance_squared(normal) < 1e-10);
            if origin == Vec3::ZERO {
                assert!((t.fraction - (0.5 - SKIN / (0.5 * 2_f32.sqrt()))).abs() < 1e-6);
                let mut reduced = old.hulls().to_vec();
                reduced[0]
                    .planes
                    .retain(|p| p.normal.distance_squared(normal) >= 1e-10);
                let reduced = Collider::from_hulls(reduced).trace(start, end, Vec3::ZERO);
                assert_ne!(trace_bits(t), trace_bits(reduced));
            }
        }
    }
    #[test]
    fn eager_brush_model_matches_legacy_planes_bounds_and_vertices() {
        let map = brush_map();
        for model in 0..map.models.len() {
            let shape = Template::brush_model(&map, model).unwrap();
            for (origin, rotation) in poses() {
                let old = Collider::model(&map, model, origin, rotation, true).unwrap();
                let new = shape.at_eager(origin, rotation);
                assert!(new.lazy.is_none());
                assert_eq!(old.bounds.is_some(), new.bounds.is_some());
                if let (Some(a), Some(b)) = (old.bounds, new.bounds) {
                    near(a.min, b.min);
                    near(a.max, b.max);
                }
                assert_eq!(old.hulls().len(), new.hulls().len());
                for (a, b) in old.hulls().iter().zip(new.hulls()) {
                    near(a.bounds.min, b.bounds.min);
                    near(a.bounds.max, b.bounds.max);
                    assert_eq!(a.planes.len(), b.planes.len());
                    for (p, q) in a.planes.iter().zip(&b.planes) {
                        near(p.normal, q.normal);
                        assert!((p.distance - q.distance).abs() <= 1e-3);
                    }
                    let a = vertices(&a.planes);
                    let b = vertices(&b.planes);
                    assert_eq!(a.len(), b.len());
                    for (a, b) in a.into_iter().zip(b) {
                        near(a, b);
                    }
                }
                for brush in &shape.geometry.brushes {
                    let reference = vertices(&brush.source);
                    assert_eq!(reference.len(), brush.vertices.len());
                    for (a, b) in reference.into_iter().zip(&brush.vertices) {
                        near(origin + rotation * a, origin + rotation * *b);
                    }
                }
            }
        }
    }
    #[test]
    fn eager_brush_model_matches_legacy_sweeps_and_rider_contacts() {
        let map = brush_map();
        let mut hits = 0;
        let mut solids = 0;
        let mut riders = 0;
        for model in 0..map.models.len() {
            let shape = Template::brush_model(&map, model).unwrap();
            for (origin, rotation) in poses() {
                let old = Collider::model(&map, model, origin, rotation, true).unwrap();
                let new = shape.at_eager(origin, rotation);
                for half in [Vec3::ZERO, Vec3::ONE, PLAYER_HALF] {
                    for x in -4..=4 {
                        for y in -4..=4 {
                            let offset = vec3(x as f32 * 25., y as f32 * 25., 0.);
                            for delta in [
                                Vec3::Z * 150.,
                                Vec3::X * 150.,
                                vec3(120., -90., 100.),
                                Vec3::ZERO,
                            ] {
                                let start = origin + offset + delta;
                                let end = origin + offset - delta;
                                let a = old.trace(start, end, half);
                                let b = new.trace(start, end, half);
                                hits += usize::from(a.fraction < 1.);
                                solids += usize::from(a.start_solid);
                                same_trace(a, b);
                                assert_eq!(
                                    old.touches(start, end, half),
                                    new.touches(start, end, half)
                                );
                            }
                            let feet = origin + offset + Vec3::Z * 5.;
                            let a = old.rider_feet(feet);
                            let b = new.rider_feet(feet);
                            assert_eq!(a.is_some(), b.is_some());
                            if let (Some((a, n)), Some((b, m))) = (a, b) {
                                riders += 1;
                                near(a, b);
                                near(n, m);
                            }
                        }
                    }
                }
                let mut a = World::fixture(&[]);
                let mut b = World::fixture(&[]);
                a.set_dynamic(vec![old]);
                b.set_dynamic(vec![new]);
                for x in -4..=4 {
                    let feet = origin + vec3(x as f32 * 25., 0., 80.);
                    same_trace(
                        a.body_trace(feet, feet - Vec3::Z * 160.),
                        b.body_trace(feet, feet - Vec3::Z * 160.),
                    );
                    assert_eq!(a.body_clear(feet), b.body_clear(feet));
                }
            }
        }
        assert!(hits > 0 && solids > 0 && riders > 0);
    }
    #[test]
    fn cached_convex_keeps_separating_planes_after_rotation() {
        let source = World::fixture(&[(vec3(-80., -12., -5.), vec3(80., 12., 5.))])
            .hulls
            .remove(0)
            .planes;
        let cached = Convex::new(&source).unwrap();
        for angle in [0., 0.31, 1.2, 2.5] {
            let rotation = Quat::from_rotation_z(angle) * Quat::from_rotation_y(0.2);
            let origin = vec3(400., -200., 50.);
            let a = Collider::from_hulls(vec![cached.at(origin, rotation)]);
            let b =
                Collider::from_hulls(vec![Hull::transformed(&source, origin, rotation).unwrap()]);
            for x in -12..=12 {
                for y in -12..=12 {
                    let start = origin + vec3(x as f32 * 10., y as f32 * 10., 100.);
                    let end = start - Vec3::Z * 200.;
                    let p = a.trace(start, end, PLAYER_HALF);
                    let q = b.trace(start, end, PLAYER_HALF);
                    assert_eq!(p.start_solid, q.start_solid);
                    assert!((p.fraction - q.fraction).abs() < 1e-5);
                }
            }
        }
    }
    #[test]
    fn patch_only_template_has_rotated_support_and_empty_outside() {
        let t = Template::from_parts(
            vec![],
            vec![[vec3(-60., -60., 0.), vec3(60., -60., 0.), vec3(0., 60., 0.)]],
        );
        let c = t.at(vec3(20., 30., 40.), Quat::from_rotation_y(0.3));
        assert!(c.lazy.as_ref().unwrap().hulls.get().is_none());
        assert!(!c.touches(vec3(300., 30., 40.), vec3(300., 30., 40.), PLAYER_HALF));
        assert!(c.lazy.as_ref().unwrap().hulls.get().is_none());
        let hit = c.trace(vec3(20., 30., 140.), vec3(20., 30., -60.), Vec3::ONE);
        assert!(hit.fraction < 1. && hit.normal.z > 0.9);
        assert!(!c.touches(vec3(300., 30., 40.), vec3(300., 30., 40.), PLAYER_HALF));
    }
    #[test]
    fn lazy_shapes_preserve_exact_sweeps_membership_and_clone_cache() {
        let source = World::fixture(&[(vec3(-70., -15., -8.), vec3(70., 15., 8.))])
            .hulls
            .remove(0)
            .planes;
        let t = Template::from_parts(
            vec![Convex::new(&source).unwrap()],
            vec![[
                vec3(-60., -80., 20.),
                vec3(60., -80., 20.),
                vec3(0., -20., 20.),
            ]],
        );
        for angle in [0., 0.3, 1.2, 2.5] {
            let rotation = Quat::from_rotation_z(angle) * Quat::from_rotation_y(0.2);
            let origin = vec3(400., -200., 50.);
            let lazy = t.at(origin, rotation);
            let clone = lazy.clone();
            let eager = Collider::from_hulls(t.geometry.at(origin, rotation));
            assert!(clone.lazy.as_ref().unwrap().hulls.get().is_none());
            for x in -12..=12 {
                for y in -12..=12 {
                    let start = origin + vec3(x as f32 * 10., y as f32 * 10., 100.);
                    let end = start - Vec3::Z * 200.;
                    let a = lazy.trace(start, end, PLAYER_HALF);
                    let b = eager.trace(start, end, PLAYER_HALF);
                    assert_eq!(a.start_solid, b.start_solid);
                    assert_eq!(a.fraction, b.fraction);
                    assert_eq!(
                        lazy.contains(start - Vec3::Z * 100.),
                        eager.contains(start - Vec3::Z * 100.)
                    );
                }
            }
            assert!(clone.lazy.as_ref().unwrap().hulls.get().is_some());
            assert_eq!(lazy.interior_point(), eager.interior_point());
        }
    }
}

#[cfg(test)]
mod mesh_tests {
    use super::*;
    fn bits(c: &Collider) -> Vec<u32> {
        c.hulls().iter().flat_map(|h| {
            h.bounds.min.to_array().into_iter().chain(h.bounds.max.to_array())
                .chain(h.planes.iter().flat_map(|p| p.normal.to_array().into_iter().chain([p.distance])))
                .map(f32::to_bits).collect::<Vec<_>>()
        }).collect()
    }
    #[test]
    fn indexed_mesh_placements_preserve_world_hull_bits_and_contacts() {
        let t = vec![
            [Vec3::ZERO, Vec3::X * 40., Vec3::Y * 30.],
            [Vec3::ZERO, Vec3::Y * 30., Vec3::Z * 20.],
            [Vec3::ONE; 3],
            [vec3(-2., 0.01, 1.), vec3(1., 0., -3.), vec3(4., 2., 0.)],
        ];
        let template = Template::triangles(&t);
        for i in 0..41 {
            let origin = vec3(i as f32 * 90.1 - 2000., -17.21, 100.31);
            let rotation = Quat::from_euler(macroquad::prelude::EulerRot::XYZ, i as f32 * 0.23, 0.17, -0.39);
            let legacy = Collider::triangles(&t, origin, rotation);
            let cached = template.at_reused(origin, rotation);
            assert_eq!(bits(&legacy), bits(&cached), "pose {i}");
            let again = template.at_reused(origin, rotation);
            assert!(std::sync::Arc::ptr_eq(cached.lazy.as_ref().unwrap(), again.lazy.as_ref().unwrap()));
            for j in 0..25 {
                let start = origin + vec3(j as f32 * 5. - 60., -55., 70.);
                let end = origin + vec3(15., 40., -30.);
                let a = legacy.trace(start, end, PLAYER_HALF);
                let b = cached.trace(start, end, PLAYER_HALF);
                assert_eq!(a.fraction.to_bits(), b.fraction.to_bits());
                assert_eq!(a.normal, b.normal);
                assert_eq!((a.start_solid, a.all_solid), (b.start_solid, b.all_solid));
            }
        }
    }
}
