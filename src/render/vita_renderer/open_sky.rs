//! BSP PVS assumes the authored sky seals the world. The gradient path omits those
//! depth surfaces, so models behind them can reach the framebuffer through the sky.
//! A PVS exclusion alone is not an occlusion proof in that rendering mode.
use super::*;

#[derive(Default)]
pub(super) struct OpenSky {
    triangles: Vec<[Vec3; 3]>,
    unknown: bool,
}
impl OpenSky {
    pub(super) fn new(scene: &Scene, enabled: bool) -> Self {
        let mut result = Self::default();
        if !enabled { return result; }
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
    pub(super) fn may_show(&self, eye: Vec3, model: Mat4, min: Vec3, max: Vec3) -> bool {
        if self.unknown { return true; }
        let (low, high) = placed_bounds(model, min, max);
        self.triangles.iter().any(|&tri| cone_overlaps(eye, tri, low, high))
    }
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
