//! Checked FAKK leaf/PVS data. Unknown or solid viewpoints deliberately fail open.
use crate::bsp::Plane;
use anyhow::{ensure, Context, Result};
use macroquad::prelude::*;

#[derive(Debug)]
struct Node {
    plane: usize,
    children: [i32; 2],
}
#[derive(Debug)]
struct Leaf {
    cluster: i32,
    min: Vec3,
    max: Vec3,
    faces: Vec<usize>,
}
#[derive(Debug, Default)]
pub struct Visibility {
    nodes: Vec<Node>,
    leaves: Vec<Leaf>,
    /// Union of every compiled leaf bound, solid leaves included. A box outside it has
    /// unclassified volume and must not be declared hidden.
    world_min: Vec3,
    world_max: Vec3,
    rows: Vec<u8>,
    stride: usize,
    clusters: usize,
    face_count: usize,
}
impl Visibility {
    /// The retained renderer prepares every PVS draw list before entering its frame loop.
    #[cfg(feature = "vita_renderer")]
    pub(crate) fn cluster_count(&self) -> usize {
        self.clusters
    }
    pub fn parse(
        nodes: &[u8],
        leaves: &[u8],
        faces: &[u8],
        vis: &[u8],
        plane_count: usize,
        face_count: usize,
    ) -> Result<Self> {
        let int =
            |b: &[u8], i: usize| -> i32 { i32::from_le_bytes(b[i..i + 4].try_into().unwrap()) };
        ensure!(
            nodes.len() % 36 == 0 && leaves.len() % 48 == 0 && faces.len() % 4 == 0,
            "Invalid visibility record size"
        );
        if nodes.is_empty() || leaves.is_empty() || vis.is_empty() {
            return Ok(Self::default());
        }
        ensure!(vis.len() >= 8, "Truncated visibility header");
        let clusters = usize::try_from(int(vis, 0))?;
        let stride = usize::try_from(int(vis, 4))?;
        ensure!(
            clusters <= 1_000_000
                && stride >= clusters.div_ceil(8)
                && stride.checked_mul(clusters).context("PVS overflow")? == vis.len() - 8,
            "Invalid PVS dimensions"
        );
        let nodes = nodes
            .chunks_exact(36)
            .map(|b| {
                let plane = usize::try_from(int(b, 0))?;
                ensure!(plane < plane_count, "Invalid BSP node plane");
                Ok(Node {
                    plane,
                    children: [int(b, 4), int(b, 8)],
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let leaves = leaves
            .chunks_exact(48)
            .map(|b| {
                let cluster = int(b, 0);
                ensure!(
                    cluster >= -1 && cluster < clusters as i32,
                    "Invalid leaf cluster"
                );
                let min = vec3(int(b, 8) as f32, int(b, 12) as f32, int(b, 16) as f32);
                let max = vec3(int(b, 20) as f32, int(b, 24) as f32, int(b, 28) as f32);
                ensure!(min.cmple(max).all(), "Inverted leaf bounds");
                let first = usize::try_from(int(b, 32))?;
                let count = usize::try_from(int(b, 36))?;
                ensure!(
                    first
                        .checked_add(count)
                        .is_some_and(|v| v <= faces.len() / 4),
                    "Invalid leaf face span"
                );
                let faces = (first..first + count)
                    .map(|i| {
                        let f = usize::try_from(int(faces, i * 4))?;
                        ensure!(f < face_count, "Invalid leaf face");
                        Ok(f)
                    })
                    .collect::<Result<Vec<_>>>()?;
                Ok(Leaf {
                    cluster,
                    min,
                    max,
                    faces,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        for n in &nodes {
            for &child in &n.children {
                ensure!(
                    if child >= 0 {
                        (child as usize) < nodes.len()
                    } else {
                        (-1_i64 - child as i64) < leaves.len() as i64
                    },
                    "Invalid BSP child"
                );
            }
        }
        let mut world_min = Vec3::splat(f32::INFINITY);
        let mut world_max = Vec3::splat(f32::NEG_INFINITY);
        for leaf in &leaves {
            world_min = world_min.min(leaf.min);
            world_max = world_max.max(leaf.max);
        }
        Ok(Self {
            nodes,
            leaves,
            world_min,
            world_max,
            rows: vis[8..].to_vec(),
            stride,
            clusters,
            face_count,
        })
    }
    pub fn cluster(&self, eye: Vec3, planes: &[Plane]) -> Option<usize> {
        if self.nodes.is_empty() || !eye.is_finite() {
            return None;
        }
        let mut index = 0_i32;
        for _ in 0..self.nodes.len() + 1 {
            if index < 0 {
                let leaf = &self.leaves[(-1_i64 - index as i64) as usize];
                // Also catches noclip outside the compiled world bounds.
                return (eye.cmpge(leaf.min - Vec3::ONE).all()
                    && eye.cmple(leaf.max + Vec3::ONE).all()
                    && leaf.cluster >= 0)
                    .then_some(leaf.cluster as usize);
            }
            let n = &self.nodes[index as usize];
            let p = planes[n.plane];
            index = n.children[usize::from(eye.dot(p.normal) < p.distance)];
        }
        None
    }
    pub fn faces(&self, cluster: Option<usize>) -> Option<Vec<bool>> {
        let c = cluster.filter(|&c| c < self.clusters)?;
        let mut visible = vec![false; self.face_count];
        for leaf in &self.leaves {
            if leaf.cluster >= 0 && self.sees(c, leaf.cluster as usize) {
                for &f in &leaf.faces {
                    visible[f] = true;
                }
            }
        }
        Some(visible)
    }
    fn sees(&self, from: usize, to: usize) -> bool {
        from == to || self.rows[from * self.stride + to / 8] & (1 << (to % 8)) != 0
    }
    /// Inverted face provenance for retained loading: build once, query PVS bits later.
    #[cfg(feature = "vita_renderer")]
    pub(crate) fn face_clusters(&self) -> Vec<Vec<usize>> {
        let mut result=vec![Vec::new();self.face_count];
        for leaf in &self.leaves {
            if leaf.cluster<0 {continue;}
            for &face in &leaf.faces {result[face].push(leaf.cluster as usize);}
        }
        for clusters in &mut result {clusters.sort_unstable();clusters.dedup();}
        result
    }
    /// The old bounds test's overlap set is independent of the viewer cluster.
    /// None preserves its fail-open policy when geometry overlaps no compiled leaf.
    #[cfg(feature = "vita_renderer")]
    pub(crate) fn bounds_clusters(&self,min:Vec3,max:Vec3) -> Option<Vec<usize>> {
        let mut result=self.leaves.iter().filter(|l|l.cluster>=0 && min.cmple(l.max).all() && max.cmpge(l.min).all()).map(|l|l.cluster as usize).collect::<Vec<_>>();
        result.sort_unstable();result.dedup();
        (!result.is_empty()).then_some(result)
    }
    #[cfg(feature = "vita_renderer")]
    pub(crate) fn sees_clusters(&self,from:usize,clusters:&[usize])->bool {
        clusters.iter().any(|&to|self.sees(from,to))
    }
    #[cfg(feature = "vita_renderer")]
    pub(crate) fn retained_cache_key(&self,hash:&mut impl sha2::Digest) {
        hash.update((self.clusters as u64).to_le_bytes());hash.update((self.stride as u64).to_le_bytes());hash.update((self.face_count as u64).to_le_bytes());
        hash.update((self.rows.len() as u64).to_le_bytes());hash.update(&self.rows);
        hash.update((self.leaves.len() as u64).to_le_bytes());
        for leaf in &self.leaves {
            hash.update(leaf.cluster.to_le_bytes());
            for n in leaf.min.to_array().into_iter().chain(leaf.max.to_array()){hash.update(n.to_bits().to_le_bytes());}
            hash.update((leaf.faces.len() as u64).to_le_bytes());for &face in &leaf.faces{hash.update((face as u64).to_le_bytes());}
        }
    }
    pub fn bounds(&self, cluster: Option<usize>, min: Vec3, max: Vec3) -> bool {
        let Some(c) = cluster.filter(|&c| c < self.clusters) else {
            return true;
        };
        let mut found = false;
        for leaf in &self.leaves {
            if leaf.cluster < 0 || !min.cmple(leaf.max).all() || !max.cmpge(leaf.min).all() {
                continue;
            }
            found = true;
            if self.sees(c, leaf.cluster as usize) {
                return true;
            }
        }
        !found
    }
    /// Whether a box can be seen from `cluster`, through the same leaf/PVS data `bounds` reads,
    /// without scanning every leaf: a standard BSP descent visits only the nodes the box can
    /// reach. `Some(false)` means the box is wholly inside the compiled leaf bounds, every leaf
    /// the descent reached overlaps it, and every non-solid leaf it overlaps has a cluster the
    /// PVS row of `cluster` leaves unset, so no point of the box is visible. `None` is unknown
    /// (`cluster` unknown, no data, a box not wholly inside the compiled world, a box over solid
    /// space only, a reached leaf whose recorded bounds miss the box, or a traversal that exceeds
    /// its visit budget) and every caller must treat it as visible.
    #[cfg(feature = "vita_renderer")]
    pub(crate) fn bounds_visible(&self, cluster: Option<usize>, planes: &[Plane], min: Vec3, max: Vec3) -> Option<bool> {
        let c = cluster.filter(|&c| c < self.clusters)?;
        if self.nodes.is_empty() || self.leaves.is_empty() || planes.is_empty()
            || !min.is_finite() || !max.is_finite() || min.cmpgt(max).any() {
            return None;
        }
        // Part of the box outside every compiled leaf is unclassified space, not hidden space.
        if min.cmplt(self.world_min).any() || max.cmpgt(self.world_max).any() {
            return None;
        }
        // Depth-first over both children at a straddling plane stays within one entry a level.
        // The budget bounds work, not stack capacity: a malformed cyclic tree would otherwise
        // revisit the same node forever, and it answers unknown when the budget runs out.
        let mut stack = [0_i32; 96];
        let mut length = 1;
        let mut budget = self.nodes.len().saturating_mul(2).saturating_add(2);
        let mut overlapped = false;
        let mut unclassified = false;
        while length > 0 {
            budget = budget.checked_sub(1)?;
            length -= 1;
            let index = stack[length];
            if index < 0 {
                let leaf = self.leaves.get(usize::try_from(-1_i64 - index as i64).ok()?)?;
                if min.cmple(leaf.max).all() && max.cmpge(leaf.min).all() {
                    if leaf.cluster >= 0 {
                        overlapped = true;
                        if self.sees(c, leaf.cluster as usize) {
                            return Some(true);
                        }
                    }
                } else {
                    // The descent reached this leaf but its recorded bounds do not cover the
                    // box volume that reached it, so the leaf data cannot classify the box.
                    unclassified = true;
                }
                continue;
            }
            let node = self.nodes.get(index as usize)?;
            let plane = planes.get(node.plane)?;
            let normal = plane.normal;
            // Support corners of the box along and against the plane normal.
            let high = normal.dot(vec3(
                if normal.x >= 0. { max.x } else { min.x },
                if normal.y >= 0. { max.y } else { min.y },
                if normal.z >= 0. { max.z } else { min.z }));
            let low = normal.dot(vec3(
                if normal.x >= 0. { min.x } else { max.x },
                if normal.y >= 0. { min.y } else { max.y },
                if normal.z >= 0. { min.z } else { max.z }));
            // Ordinary f32 dot products carry a few ulps of error at map coordinates. Widen
            // both sides by 32 ulps of the largest term so rounding can only add a child to
            // the descent, never drop one: missing a visible leaf would hide geometry.
            let scale = high.abs().max(low.abs()).max(plane.distance.abs()).max(1.);
            let tolerance = 32. * f32::EPSILON * scale;
            // Child 0 is the `dot >= distance` side, child 1 the other, as `cluster` walks it.
            let front = high + tolerance >= plane.distance;
            let back = low - tolerance <= plane.distance;
            let push = |value: i32, stack: &mut [i32; 96], length: &mut usize| -> Option<()> {
                *stack.get_mut(*length)? = value;
                *length += 1;
                Some(())
            };
            if front { push(node.children[0], &mut stack, &mut length)?; }
            if back { push(node.children[1], &mut stack, &mut length)?; }
        }
        (overlapped && !unclassified).then_some(false)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    pub(crate) fn two_rooms() -> Visibility {
        Visibility {
            nodes: vec![Node {
                plane: 0,
                children: [-1, -2],
            }],
            leaves: vec![
                Leaf {
                    cluster: 0,
                    min: vec3(0., -10., -10.),
                    max: Vec3::splat(10.),
                    faces: vec![0],
                },
                Leaf {
                    cluster: 1,
                    min: Vec3::splat(-10.),
                    max: vec3(0., 10., 10.),
                    faces: vec![1],
                },
            ],
            world_min: Vec3::splat(-10.),
            world_max: Vec3::splat(10.),
            rows: vec![1, 2],
            stride: 1,
            clusters: 2,
            face_count: 2,
        }
    }
    /// The same geometry with the clusters exchanged: a second map whose identical cache key
    /// must answer differently.
    pub(crate) fn two_rooms_swapped() -> Visibility {
        let mut v = two_rooms();
        v.leaves[0].cluster = 1;
        v.leaves[1].cluster = 0;
        v.rows = vec![2, 1];
        v
    }
    #[cfg(feature = "vita_renderer")]
    #[test]
    fn retained_sources_preserve_faces_overlaps_and_fail_open_bounds() {
        let v=two_rooms();let faces=v.face_clusters();
        for c in 0..v.cluster_count() {
            let mask=v.faces(Some(c)).unwrap();
            for (face,sources) in faces.iter().enumerate(){assert_eq!(v.sees_clusters(c,sources),mask[face]);}
            for (min,max) in [(Vec3::splat(-1.),Vec3::splat(1.)),(vec3(2.,-1.,-1.),vec3(3.,1.,1.)),(Vec3::splat(100.),Vec3::splat(101.))] {
                let source=v.bounds_clusters(min,max);
                assert_eq!(source.as_ref().is_none_or(|s|v.sees_clusters(c,s)),v.bounds(Some(c),min,max));
            }
        }
    }
    #[test]
    fn rooms_solid_or_unknown_views_and_moving_bounds() {
        let v = two_rooms();
        let planes = [Plane {
            normal: Vec3::X,
            distance: 0.,
        }];
        let c = v.cluster(Vec3::X, &planes);
        assert_eq!(c, Some(0));
        assert_eq!(v.faces(c), Some(vec![true, false]));
        assert!(!v.bounds(c, vec3(-9., -1., -1.), vec3(-2., 1., 1.)));
        assert!(v.bounds(c, vec3(-2., -1., -1.), vec3(2., 1., 1.)));
        assert_eq!(v.cluster(Vec3::X * 50., &planes), None);
        assert!(v.bounds(None, Vec3::splat(-9.), Vec3::splat(-2.)));
        let mut cycle = two_rooms();
        cycle.nodes[0].children = [0, 0];
        assert_eq!(cycle.cluster(Vec3::X, &planes), None);
        let mut solid = two_rooms();
        solid.leaves[0].cluster = -1;
        assert_eq!(solid.cluster(Vec3::X, &planes), None);
    }
    #[test]
    fn pvs_rejects_bad_ranges_and_fails_open_without_data() {
        assert!(Visibility::parse(&[0; 35], &[], &[], &[], 0, 0).is_err());
        let v = Visibility::default();
        assert!(v.faces(None).is_none());
        assert!(v.bounds(None, Vec3::ZERO, Vec3::ONE));
    }
    #[cfg(feature = "vita_renderer")]
    #[test]
    fn box_visibility_matches_the_leaf_bounds_pvs_test() {
        let v = two_rooms();
        let planes = [Plane { normal: Vec3::X, distance: 0. }];
        // Each room's own box is visible only from its own cluster.
        assert_eq!(v.bounds_visible(Some(0), &planes, vec3(2., -1., -1.), vec3(3., 1., 1.)), Some(true));
        assert_eq!(v.bounds_visible(Some(1), &planes, vec3(2., -1., -1.), vec3(3., 1., 1.)), Some(false));
        // A box crossing the split overlaps both rooms.
        assert_eq!(v.bounds_visible(Some(1), &planes, vec3(-2., -1., -1.), vec3(2., 1., 1.)), Some(true));
        // Unknown cluster, no data and a box outside the world fail open.
        assert_eq!(v.bounds_visible(None, &planes, Vec3::ZERO, Vec3::ONE), None);
        assert_eq!(Visibility::default().bounds_visible(Some(0), &planes, Vec3::ZERO, Vec3::ONE), None);
        assert_eq!(v.bounds_visible(Some(0), &planes, vec3(50., 50., 50.), vec3(51., 51., 51.)), None);
        let inverted = vec3(1., 1., 1.);
        assert_eq!(v.bounds_visible(Some(0), &planes, inverted, Vec3::ZERO), None);
        // Same answers as the leaf scan wherever the scan has data.
        for cluster in 0..2 {
            for (min, max) in [(vec3(2., -1., -1.), vec3(3., 1., 1.)),
                               (vec3(-9., -1., -1.), vec3(-2., 1., 1.)),
                               (vec3(-2., -1., -1.), vec3(2., 1., 1.)),
                               (vec3(-2., -1., -1.), vec3(-2., 1., 1.)),
                               (vec3(50., 50., 50.), vec3(51., 51., 51.))] {
                assert_eq!(v.bounds_visible(Some(cluster), &planes, min, max).unwrap_or(true),
                           v.bounds(Some(cluster), min, max), "{cluster} {min} {max}");
            }
        }
    }
    #[cfg(feature = "vita_renderer")]
    #[test]
    fn boxes_partly_outside_the_compiled_world_fail_open() {
        let v = two_rooms();
        let planes = [Plane { normal: Vec3::X, distance: 0. }];
        // The hidden leaf ends at x = 10; x > 10 is not compiled, so it cannot be hidden.
        assert_eq!(v.bounds_visible(Some(1), &planes, vec3(2., -1., -1.), vec3(50., 1., 1.)), None);
        // The same extension on the other side and on an unrelated axis also fails open.
        assert_eq!(v.bounds_visible(Some(1), &planes, vec3(-50., -1., -1.), vec3(-2., 1., 1.)), None);
        assert_eq!(v.bounds_visible(Some(1), &planes, vec3(2., -1., -1.), vec3(3., 50., 50.)), None);
        // A box that ends exactly at the compiled bound is still classified and cut.
        assert_eq!(v.bounds_visible(Some(1), &planes, vec3(2., -1., -1.), vec3(10., 1., 1.)), Some(false));
    }
    #[cfg(feature = "vita_renderer")]
    #[test]
    fn bounds_visible_bounds_visits_and_fails_open_on_cycles() {
        let planes = [Plane { normal: Vec3::X, distance: 0. }];
        let (min, max) = (vec3(2., -1., -1.), vec3(3., 1., 1.));
        // The front child revisits node 0. Stack capacity never grows, so the old loop ran
        // forever; the visit budget now answers unknown, which callers treat as visible.
        let mut cycle = two_rooms();
        cycle.nodes[0].children = [0, -1];
        assert_eq!(cycle.bounds_visible(Some(1), &planes, min, max), None);
        // A one-sided chain deeper than the 96-slot sibling stack still reaches its leaf.
        let depth = 400;
        let mut deep = two_rooms();
        deep.nodes = (0..depth)
            .map(|i| Node { plane: 0, children: [if i + 1 < depth { (i + 1) as i32 } else { -1 }, -2] })
            .collect();
        assert_eq!(deep.bounds_visible(Some(0), &planes, min, max), Some(true));
        assert_eq!(deep.bounds_visible(Some(1), &planes, min, max), Some(false));
    }
    #[cfg(feature = "vita_renderer")]
    #[test]
    fn near_plane_tolerance_at_corpus_extremes_never_answers_hidden() {
        // Largest |leaf coordinate| over gvillage, hatter1, skool2, wchess2, utemple,
        // potears2, wchess1 and keep, rounded up.
        let extreme = 8192_f32;
        let normal = vec3(3., 4., 0.).normalize();
        let model = Mat4::from_rotation_translation(
            Quat::from_rotation_z(0.7),
            vec3(extreme * 0.8, extreme * 0.8, 0.));
        let (min, max) = (vec3(-0.02, -0.02, -0.02), vec3(0.02, 0.02, 0.02));
        let mut low = Vec3::splat(f32::INFINITY);
        let mut high = Vec3::splat(f32::NEG_INFINITY);
        for corner in 0..8 {
            let p = vec3(if corner & 1 == 0 { min.x } else { max.x },
                         if corner & 2 == 0 { min.y } else { max.y },
                         if corner & 4 == 0 { min.z } else { max.z });
            let q = model.transform_point3(p);
            low = low.min(q);
            high = high.max(q);
        }
        let support = vec3(if normal.x >= 0. { high.x } else { low.x },
                           if normal.y >= 0. { high.y } else { low.y },
                           if normal.z >= 0. { high.z } else { low.z });
        let high_dot = normal.dot(support);
        assert!(high_dot > 0.);
        let extent = 2. * extreme;
        let world = Visibility {
            nodes: vec![Node { plane: 0, children: [-1, -2] }],
            leaves: vec![
                Leaf { cluster: 0, min: Vec3::splat(-extent), max: Vec3::splat(extent), faces: vec![0] },
                Leaf { cluster: 1, min: Vec3::splat(-extent), max: Vec3::splat(extent), faces: vec![1] },
            ],
            world_min: Vec3::splat(-extent),
            world_max: Vec3::splat(extent),
            rows: vec![1, 0],
            stride: 1,
            clusters: 2,
            face_count: 2,
        };
        let one_ulp_up = f32::from_bits(high_dot.to_bits() + 1);
        let at = |distance| [Plane { normal, distance }];
        // One f32 ulp beyond the support corner: without the outward allowance the front
        // child is dropped and the hidden back leaf would answer Some(false).
        assert!(high_dot < one_ulp_up);
        assert_eq!(world.bounds_visible(Some(0), &at(one_ulp_up), low, high), Some(true));
        // A whole unit behind the plane is still classified and cut.
        assert_eq!(world.bounds_visible(Some(0), &at(high_dot + 1.), low, high), Some(false));
    }
}
