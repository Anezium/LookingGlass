//! BSP PVS assumes the authored sky seals the world. The gradient path omits those
//! depth surfaces, so models behind them can reach the framebuffer through the sky.
//! A PVS exclusion alone is not an occlusion proof in that rendering mode.
use super::*;

#[derive(Default)]
pub(super) struct OpenSky {
    triangles: Vec<[Vec3; 3]>,
    unknown: bool,
    // Eye-dependent planes are shared by every group. Cluster alone is insufficient:
    // moving within a cluster changes the cone. Bit keys preserve signed zero and NaNs.
    eye: Option<[u32; 3]>,
    cones: Vec<[ConeEdge; 3]>,
    // Group geometry is immutable for this renderer/map; matrix bits key placed bounds.
    // Final answers additionally require the exact eye, stricter than the BSP PVS cache.
    groups: Vec<Option<SkyGroup>>,
    #[cfg(all(feature = "sim_maps_probe", not(target_os = "vita")))]
    probe: Probe,
}
impl OpenSky {
    pub(super) fn new(scene: &Scene, enabled: bool) -> Self {
        let mut result = Self::default();
        if !enabled { return result; }
        #[cfg(all(feature = "sim_maps_probe", not(target_os = "vita")))]
        { result.probe.enabled = std::env::var_os("LOOKING_GLASS_SKY_PROFILE").is_some();
          result.probe.verify = std::env::var_os("LOOKING_GLASS_SKY_VERIFY").is_some();
          result.probe.old = std::env::var_os("LOOKING_GLASS_SKY_OLD").is_some(); }
        // Retain a conservative aperture even with sky-depth keys: authored/capture modes
        // have different depth policies. Extra retained draws are safe in all of them.
        for b in scene.batches.iter().filter(|b| b.sky) {
            if b.model.is_some() || b.animation.is_some() || b.motion.is_some()
                || b.deforms.iter().any(Deform::moves_vertices) {
                result.unknown = true;
                continue;
            }
            for indices in b.full_indices.chunks_exact(3) {
                result.triangles.push([0, 1, 2].map(|i| b.base[indices[i] as usize].position));
            }
        }
        result
    }
    pub(super) fn begin_frame(&mut self) {
        #[cfg(all(feature = "sim_maps_probe", not(target_os = "vita")))]
        {
            if self.probe.enabled || self.probe.verify {
                eprintln!("SKY_GUARD frame={} ns={} queries={} triangles={} tested={} hits={} verified={}",
                    self.probe.frame, self.probe.ns, self.probe.queries, self.triangles.len(),
                    self.probe.tested, self.probe.hits, self.probe.verified);
                if self.probe.enabled { eprintln!("SKY_GUARD_TESTS frame={} histogram={:?}",
                    self.probe.frame, self.probe.histogram); }
                self.probe.histogram.clear();
                self.probe.frame += 1;
                self.probe.ns = 0; self.probe.queries = 0; self.probe.tested = 0;
                self.probe.hits = 0; self.probe.verified = 0;
            }
        }
    }
    pub(super) fn may_show(&mut self, id: usize, eye: Vec3, model: Mat4, min: Vec3, max: Vec3) -> bool {
        #[cfg(all(feature = "sim_maps_probe", not(target_os = "vita")))]
        let start = self.probe.enabled.then(std::time::Instant::now);
        let mut tested = 0;
        let mut hit = false;
        let key = eye.to_array().map(f32::to_bits);
        let bits = model.to_cols_array().map(f32::to_bits);
        if self.groups.len() <= id { self.groups.resize(id + 1, None); }
        let prior = self.groups[id];
        let bounds = prior.filter(|p| p.model == bits).map(|p| p.bounds)
            .unwrap_or_else(|| placed_bounds(model, min, max));
        let (low, high) = bounds;
        let answer;
        #[cfg(all(feature = "sim_maps_probe", not(target_os = "vita")))]
        let old = self.probe.old;
        #[cfg(not(all(feature = "sim_maps_probe", not(target_os = "vita"))))]
        let old = false;
        if self.unknown { answer = true; }
        else if old {
            answer = self.triangles.iter().any(|&tri| { tested += 1; cone_overlaps(eye, tri, low, high) });
        } else if let Some(p) = prior.filter(|p| p.model == bits && p.eye == key) {
            hit = true;
            answer = p.answer;
        } else {
            if self.eye != Some(key) {
                self.eye = Some(key);
                self.cones.clear();
                self.cones.extend(self.triangles.iter().map(|&tri| prepare_cone(eye, tri)));
            }
            answer = self.cones.iter().any(|cone| { tested += 1; prepared_overlaps(eye, cone, low, high) });
        }
        self.groups[id] = Some(SkyGroup { model: bits, eye: key, bounds, answer });
        #[cfg(all(feature = "sim_maps_probe", not(target_os = "vita")))]
        {
            if let Some(start) = start {
                self.probe.ns += start.elapsed().as_nanos();
                self.probe.queries += 1;
                self.probe.tested += tested;
                self.probe.hits += u64::from(hit);
                *self.probe.histogram.entry(tested).or_insert(0) += 1;
            }
            if self.probe.verify {
                let (original_low, original_high) = placed_bounds(model, min, max);
                assert_eq!(answer, self.unknown || self.triangles.iter().any(|&tri| cone_overlaps(eye, tri, original_low, original_high)),
                    "cached sky cone differs from the original guard");
                self.probe.verified += 1;
            }
        }
        let _ = (tested, hit);
        answer
    }
}

#[cfg(all(feature = "sim_maps_probe", not(target_os = "vita")))]
impl Drop for OpenSky {
    fn drop(&mut self) { self.begin_frame(); }
}

#[derive(Clone, Copy)]
struct SkyGroup {
    model: [u32; 16],
    eye: [u32; 3],
    bounds: (Vec3, Vec3),
    answer: bool,
}
#[cfg(all(feature = "sim_maps_probe", not(target_os = "vita")))]
#[derive(Default)]
struct Probe {
    enabled: bool, verify: bool, old: bool,
    frame: u64, ns: u128, queries: u64, tested: u64, hits: u64, verified: u64,
    histogram: std::collections::BTreeMap<u64, u64>,
}

// Keep the edge order: a later degenerate edge must not override an earlier separator.
#[derive(Clone, Copy, Default)]
struct ConeEdge {
    normal: glam::DVec3,
    magnitude: f64,
    unknown: bool,
}
fn prepare_cone(eye: Vec3, triangle: [Vec3; 3]) -> [ConeEdge; 3] {
    if !eye.is_finite() || triangle.iter().any(|p| !p.is_finite()) {
        return [ConeEdge { unknown: true, ..ConeEdge::default() }; 3];
    }
    let rays = triangle.map(|p| p.as_dvec3() - eye.as_dvec3());
    [0, 1, 2].map(|edge| {
        let a = rays[edge];
        let b = rays[(edge + 1) % 3];
        let mut n = a.cross(b);
        let orientation = n.dot(rays[(edge + 2) % 3]);
        let error = a.abs().max(b.abs()).max(rays[(edge + 2) % 3].abs()).max_element().max(1.).powi(3)
            * 64. * f64::EPSILON;
        if orientation < 0. { n = -n; }
        ConeEdge { normal: n, magnitude: a.abs().max(b.abs()).max_element(),
            unknown: orientation.abs() <= error }
    })
}
fn prepared_overlaps(eye: Vec3, cone: &[ConeEdge; 3], low: Vec3, high: Vec3) -> bool {
    if !eye.is_finite() || !low.is_finite() || !high.is_finite() { return true; }
    let lo = low.as_dvec3() - eye.as_dvec3();
    let hi = high.as_dvec3() - eye.as_dvec3();
    let box_magnitude = lo.abs().max(hi.abs()).max_element().max(1.);
    for edge in cone {
        if edge.unknown { return true; }
        let n = edge.normal;
        let support = dvec3(if n.x >= 0. { hi.x } else { lo.x },
            if n.y >= 0. { hi.y } else { lo.y }, if n.z >= 0. { hi.z } else { lo.z });
        let magnitude = edge.magnitude.max(box_magnitude);
        if n.dot(support) < -magnitude.powi(3) * 128. * f64::EPSILON { return false; }
    }
    true
}

/// Three eye/edge planes enclose the infinite cone through the triangle. A separating
/// plane proves the whole box misses the aperture; otherwise retain it. This intentionally
/// also keeps boxes in front of the sky, and false positives between the plane tests.
/// f64 arithmetic plus a term-magnitude allowance makes cancellation fail outward.
fn cone_overlaps(eye: Vec3, triangle: [Vec3; 3], low: Vec3, high: Vec3) -> bool {
    if !eye.is_finite() || !low.is_finite() || !high.is_finite()
        || triangle.iter().any(|p| !p.is_finite()) { return true; }
    let rays = triangle.map(|p| p.as_dvec3() - eye.as_dvec3());
    for edge in 0..3 {
        let a = rays[edge];
        let b = rays[(edge + 1) % 3];
        let mut n = a.cross(b);
        let orientation = n.dot(rays[(edge + 2) % 3]);
        let error = a.abs().max(b.abs()).max(rays[(edge + 2) % 3].abs()).max_element().max(1.).powi(3)
            * 64. * f64::EPSILON;
        // Eye on/near the triangle plane or a degenerate triangle: no trustworthy cone.
        if orientation.abs() <= error { return true; }
        if orientation < 0. { n = -n; }
        let lo = low.as_dvec3() - eye.as_dvec3();
        let hi = high.as_dvec3() - eye.as_dvec3();
        let support = dvec3(if n.x >= 0. { hi.x } else { lo.x },
            if n.y >= 0. { hi.y } else { lo.y }, if n.z >= 0. { hi.z } else { lo.z });
        let magnitude = a.abs().max(b.abs()).max(lo.abs()).max(hi.abs()).max_element().max(1.);
        if n.dot(support) < -magnitude.powi(3) * 128. * f64::EPSILON { return false; }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn prepared_cones_match_original_queries_and_cache_invalidation() {
        let mut seed = 17u32;
        let mut sample = || { seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            (seed as i32 as f32) / (i32::MAX as f32) * 8192. };
        for _ in 0..20000 {
            let eye = vec3(sample(), sample(), sample());
            let tri = [0, 1, 2].map(|_| vec3(sample(), sample(), sample()));
            let a = vec3(sample(), sample(), sample());
            let b = vec3(sample(), sample(), sample());
            let (lo, hi) = (a.min(b), a.max(b));
            assert_eq!(cone_overlaps(eye, tri, lo, hi), prepared_overlaps(eye, &prepare_cone(eye, tri), lo, hi));
        }
        let tri = [Vec3::X, Vec3::Y, Vec3::Z];
        let mut sky = OpenSky::default();
        sky.triangles.push(tri);
        for eye in [Vec3::ZERO, Vec3::ONE, Vec3::ZERO, tri[0], Vec3::splat(f32::NAN)] {
            for model in [Mat4::IDENTITY, Mat4::from_translation(Vec3::splat(100.)), Mat4::IDENTITY] {
                let (lo, hi) = placed_bounds(model, -Vec3::ONE, Vec3::ONE);
                let expected = cone_overlaps(eye, tri, lo, hi);
                for _ in 0..2 { assert_eq!(sky.may_show(7, eye, model, -Vec3::ONE, Vec3::ONE), expected); }
            }
        }
    }
    #[test]
    fn hatter2_pane_is_visible_through_the_omitted_sky_seal() {
        // Actual map surface 487, camera leaf 855/cluster 531, group 616 of *53.
        let eye = vec3(5664.3003, -1141.2856, 32.961304);
        let tri = [vec3(4728., -1184., -192.), vec3(4728., -1184., 896.), vec3(4728., 1280., -192.)];
        let (lo, hi) = placed_bounds(Mat4::from_translation(vec3(1096., 352., 456.)),
            vec3(72., 268., 16.), vec3(128., 276., 168.));
        assert!(cone_overlaps(eye, tri, lo, hi));
        assert!(cone_overlaps(eye, [tri[2], tri[1], tri[0]], lo, hi));
        assert!(!cone_overlaps(eye, tri, vec3(1190., 20000., 540.), vec3(1200., 20010., 550.)));
        // A boundary touch is retained, as is an eye in the sky plane.
        assert!(cone_overlaps(eye, tri, tri[0], tri[0]));
        assert!(cone_overlaps(tri[0], tri, lo, hi));
    }
    #[test]
    #[ignore = "requires LOOKING_GLASS_TEST_DATA"]
    fn hatter2_map_pvs_excludes_a_pane_behind_a_sky_surface() -> anyhow::Result<()> {
        let mut assets = crate::assets::Assets::open(std::path::Path::new(&std::env::var("LOOKING_GLASS_TEST_DATA")?))?;
        let map = crate::bsp::Bsp::parse(&assets.read("maps/hatter2.bsp")?)?;
        let eye = vec3(5664.3003, -1141.2856, 32.961304);
        let cluster = map.visibility.cluster(eye, &map.planes);
        assert_eq!(cluster, Some(531));
        let (lo, hi) = placed_bounds(Mat4::from_translation(vec3(1096., 352., 456.)),
            vec3(72., 268., 16.), vec3(128., 276., 168.));
        assert_eq!(map.visibility.bounds_visible(cluster, &map.planes, lo, hi), Some(false));
        let surface = &map.surfaces[487];
        assert_eq!(map.shaders[surface.shader].name, "textures/sky/km_testsky");
        let indices = &map.indices[surface.first_index..surface.first_index + 3];
        let tri = [0,1,2].map(|i| map.vertices[surface.first_vertex + indices[i]].position);
        assert!(cone_overlaps(eye, tri, lo, hi));
        Ok(())
    }
}
