//! `packet_frustum_cull` and `packet_min_pixels`, both default off. Actor, prop and FX packets
//! that cannot reach the screen, or that would cover only a few pixels of it, leave the frame
//! before the joint sort, the merge and the upload, so they cost neither a copy nor a draw.
//!
//! The actor program draws `Projection * Model * position` with an identity model, so a pass's
//! vertices are world positions in the clip space of the frame's projection. A pass whose vertices
//! all lie outside one clip plane draws no fragment: cutting it changes no pixel. The size test
//! does change pixels.
//!
//! Cost: a visible pass usually shows a vertex inside the view, or vertices on both sides of every
//! plane, within its first few vertices, and the test stops there. Only a pass that may be outside
//! gets the box of all its vertices, kept per geometry while its vertex generation repeats. The
//! size test needs that box for every pass.
use super::*;
use std::collections::HashMap;

#[derive(Clone, Copy)]
pub(super) struct Cuts { frustum: bool, min_pixels: f32 }
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Cut { Outside, Small }
impl Cuts {
    pub fn new(c: &crate::render::vita_renderer::Config) -> Self {
        Self { frustum: c.packet_frustum_cull, min_pixels: c.packet_min_pixels }
    }
    pub fn active(self) -> bool { self.frustum || self.min_pixels > 0. }
    #[cfg(test)]
    fn cut(self, m: Mat4, min: Vec3, max: Vec3) -> Option<Cut> {
        if self.frustum && crate::render::outside_matrix(m, min, max) { return Some(Cut::Outside); }
        small(m, min, max, self.min_pixels).then_some(Cut::Small)
    }
}
fn small(m: Mat4, min: Vec3, max: Vec3, limit: f32) -> bool {
    limit > 0. && min.cmple(max).all() && crate::render::projected_pixels(m, min, max).is_some_and(|s| s < limit)
}

/// The six clip planes a point lies strictly outside of, one bit each. A relative margin keeps a
/// point that the GPU might round onto the plane counted as inside.
#[inline]
fn outcode(m: Mat4, p: Vec3) -> u8 {
    let c = m * p.extend(1.);
    let out = |a: f32| { let margin = 1e-5 * (a.abs() + c.w.abs()); (u8::from(a < -c.w - margin), u8::from(a > c.w + margin)) };
    let (x0, x1) = out(c.x);
    let (y0, y1) = out(c.y);
    let (z0, z1) = out(c.z);
    x0 | x1 << 1 | y0 << 2 | y1 << 3 | z0 << 4 | z1 << 5
}
const PLANES: u8 = 0b11_1111;
const PROBE_VERTICES: usize = 16;

/// Vertex bounds of a published geometry, kept while its vertex generation repeats. A geometry
/// keeps its identity across updates and takes a new vertex generation whenever any vertex bit
/// changes, so an entry is exact for as long as both match.
#[derive(Default)]
pub(super) struct Bounds { known: HashMap<u64, (u64, Vec3, Vec3)> }
const BOUNDS_LIMIT: usize = 4096;
impl Bounds {
    fn of(&mut self, p: &Pass, scanned: &mut u64) -> (Vec3, Vec3) {
        if let Some(g) = &p.geometry {
            let version = g.version;
            if let Some(&(vertices, lo, hi)) = self.known.get(&version.identity) {
                if vertices == version.vertices { return (lo, hi); }
            }
            *scanned += g.vertices.len() as u64;
            let (lo, hi) = extent(&g.vertices);
            if self.known.len() >= BOUNDS_LIMIT { self.known.clear(); }
            self.known.insert(version.identity, (version.vertices, lo, hi));
            return (lo, hi);
        }
        *scanned += p.mesh.vertices.len() as u64;
        extent(&p.mesh.vertices)
    }
}
fn extent(vertices: &[Vertex]) -> (Vec3, Vec3) {
    vertices.iter().fold((Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY)),
        |(lo, hi), v| (lo.min(v.position), hi.max(v.position)))
}
/// True only when no vertex of the pass can be drawn. An empty pass is left alone.
fn pass_outside(m: Mat4, p: &Pass, bounds: &mut Bounds, probed: &mut u64, scanned: &mut u64) -> bool {
    let vertices = &p.geometry().vertices;
    if vertices.is_empty() { return false; }
    let mut common = PLANES;
    for v in vertices.iter().take(PROBE_VERTICES) {
        *probed += 1;
        common &= outcode(m, v.position);
        if common == 0 { return false; }
    }
    if vertices.len() <= PROBE_VERTICES { return true; }
    let (lo, hi) = bounds.of(p, scanned);
    crate::render::outside_matrix(m, lo, hi)
}

/// Empties every group whose passes all fall to a cut and drops such world puffs. Group indices
/// stay, so puffs keep their `before_group` places; an empty group joins neither list.
pub(super) fn cull(bounds: &mut Bounds, groups: &mut [super::super::Group], puffs: &mut Vec<world_puffs::Puff>,
                   m: Mat4, cuts: Cuts, recycle: bool) {
    let (mut outside, mut small_groups, mut probed, mut scanned) = (0u64, 0u64, 0u64, 0u64);
    for g in groups.iter_mut() {
        if g.passes.is_empty() { continue; }
        if cuts.frustum && g.passes.iter().all(|p| pass_outside(m, p, bounds, &mut probed, &mut scanned)) {
            outside += 1;
        } else if cuts.min_pixels > 0. {
            let (mut lo, mut hi) = (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY));
            for p in &g.passes {
                let (a, b) = bounds.of(p, &mut scanned);
                lo = lo.min(a);
                hi = hi.max(b);
            }
            if !small(m, lo, hi, cuts.min_pixels) { continue; }
            small_groups += 1;
        } else {
            continue;
        }
        // The actor layer cache takes its pass storage back at the end of the frame; give it now.
        if recycle { super::super::actor_cache::recycle(&mut g.passes); }
        g.passes.clear();
    }
    let (mut puffs_outside, mut puffs_small) = (0u64, 0u64);
    puffs.retain(|p| {
        if cuts.frustum && p.vertices.iter().fold(PLANES, |common, v| common & outcode(m, v.position)) != 0 {
            puffs_outside += 1;
            return false;
        }
        let (lo, hi) = extent(&p.vertices);
        if small(m, lo, hi, cuts.min_pixels) { puffs_small += 1; return false; }
        true
    });
    count("vita_fx_packets_cut_outside", outside);
    count("vita_fx_packets_cut_small", small_groups);
    count("vita_fx_puffs_cut_outside", puffs_outside);
    count("vita_fx_puffs_cut_small", puffs_small);
    count("vita_fx_packet_cut_probed_vertices", probed);
    count("vita_fx_packet_cut_scanned_vertices", scanned);
}

#[cfg(test)]
mod tests {
    use super::*;
    fn view() -> Mat4 {
        Mat4::perspective_rh_gl(1.2, 960. / 544., 4., 8192.) * Mat4::look_at_rh(Vec3::ZERO, vec3(1., 0., 0.), Vec3::Z)
    }
    #[test]
    fn cuts_name_outside_before_small_and_stay_off_by_default() {
        let m = view();
        let off = Cuts { frustum: false, min_pixels: 0. };
        assert!(!off.active());
        let behind = (vec3(-100., -1., -1.), vec3(-90., 1., 1.));
        let far_small = (vec3(1000., -1., -1.), vec3(1002., 1., 1.));
        let near = (vec3(50., -40., -40.), vec3(60., 40., 40.));
        let both = Cuts { frustum: true, min_pixels: 6. };
        assert_eq!(both.cut(m, behind.0, behind.1), Some(Cut::Outside));
        assert_eq!(both.cut(m, far_small.0, far_small.1), Some(Cut::Small));
        assert_eq!(both.cut(m, near.0, near.1), None);
        let frustum = Cuts { frustum: true, min_pixels: 0. };
        assert_eq!(frustum.cut(m, far_small.0, far_small.1), None, "frustum only never calls a visible box small");
        assert_eq!(off.cut(m, behind.0, behind.1), None);
    }
    fn pass_with(points: &[Vec3]) -> Pass {
        Pass {_game_thread:crate::game_thread::GameThread::default(),geometry:None,effect:false,blend_index:0,depth_equal:false,
            #[cfg(target_os="vita")] world_static:None,
            #[cfg(target_os="vita")] world_geometry_counted:false,
            mesh:Mesh {vertices:points.iter().map(|p| Vertex::new(p.x,p.y,p.z,0.,0.,WHITE)).collect(),indices:Vec::new(),texture:None},
            material:None,lit:false,clamp:false,cutout:0.,cutout_reference:0.,gain:Vec4::ONE,fog:0.,model:true,
            appearance:super::super::super::Appearance::default(),filter_fade:0.}
    }
    #[test]
    fn outcode_names_planes_and_keeps_points_on_a_plane_inside() {
        let m = view();
        assert_eq!(outcode(m, vec3(100., 0., 0.)), 0, "ahead, centred");
        assert_ne!(outcode(m, vec3(-100., 0., 0.)), 0, "behind the eye");
        assert_ne!(outcode(m, vec3(100., 0., 0.)) | outcode(m, vec3(100., 1000., 0.)), 0);
        // Clip space is linear, so a plane half-space holds every blend of points inside it.
        assert_eq!(outcode(m, vec3(100., 1000., 0.)) & outcode(m, vec3(100., -1000., 0.)), 0, "two sides share no plane");
        assert_eq!(outcode(Mat4::IDENTITY, vec3(1., 0., 0.)), 0, "on the plane x = w counts as inside");
        assert_eq!(outcode(Mat4::IDENTITY, vec3(1. + 1e-7, 0., 0.)), 0, "within the margin counts as inside");
        assert_ne!(outcode(Mat4::IDENTITY, vec3(1.01, 0., 0.)), 0);
        assert_eq!(outcode(Mat4::IDENTITY, vec3(f32::NAN, 0., 0.)) & 0b11, 0, "NaN is never outside");
    }
    #[test]
    fn a_pass_is_outside_only_when_its_vertices_share_a_plane() {
        let m = view();
        let (mut bounds, mut probed, mut scanned) = (Bounds::default(), 0, 0);
        // Left and right of the view: each vertex outside, but not of one plane, and the
        // triangle crosses the view, so kept.
        let split = pass_with(&[vec3(100., 1000., 0.), vec3(100., -1000., 0.), vec3(100., 1000., 5.)]);
        assert!(!pass_outside(m, &split, &mut bounds, &mut probed, &mut scanned));
        let behind = pass_with(&[vec3(-50., 0., 0.), vec3(-60., 10., 0.), vec3(-70., 0., 9.)]);
        assert!(pass_outside(m, &behind, &mut bounds, &mut probed, &mut scanned));
        assert!(!pass_outside(m, &pass_with(&[]), &mut bounds, &mut probed, &mut scanned), "nothing to bound: kept");
        // Past the probe, the box decides: one vertex in view keeps a long pass.
        let mut long: Vec<Vec3> = (0..40).map(|i| vec3(-50. - i as f32, 0., 0.)).collect();
        assert!(pass_outside(m, &pass_with(&long), &mut bounds, &mut probed, &mut scanned));
        assert_eq!(scanned, 40, "the box scan ran once");
        long.push(vec3(100., 0., 0.));
        assert!(!pass_outside(m, &pass_with(&long), &mut bounds, &mut probed, &mut scanned));
        // A visible pass stops at its first vertex in view.
        let before = probed;
        let visible = pass_with(&(0..40).map(|i| vec3(100. + i as f32, 0., 0.)).collect::<Vec<_>>());
        assert!(!pass_outside(m, &visible, &mut bounds, &mut probed, &mut scanned));
        assert_eq!(probed - before, 1);
    }
    #[test]
    fn bounds_follow_the_vertex_generation() {
        let quad = |x: f32| Mesh { vertices: vec![Vertex::new(x, 0., 0., 0., 0., WHITE), Vertex::new(x + 1., 2., 3., 0., 0., WHITE),
            Vertex::new(x, 2., 0., 0., 0., WHITE)], indices: vec![0, 1, 2], texture: None };
        let mut g = super::super::super::prepared::Geometry::new(quad(5.));
        let mut bounds = Bounds::default();
        let mut scanned = 0;
        let mut pass = Pass {_game_thread:crate::game_thread::GameThread::default(),geometry:None,effect:false,blend_index:0,depth_equal:false,
            #[cfg(target_os="vita")] world_static:None,
            #[cfg(target_os="vita")] world_geometry_counted:false,
            mesh:Mesh {vertices:Vec::new(),indices:Vec::new(),texture:None},material:None,lit:false,clamp:false,cutout:0.,
            cutout_reference:0.,gain:Vec4::ONE,fog:0.,model:true,appearance:super::super::super::Appearance::default(),filter_fade:0.};
        pass.geometry = Some(std::rc::Rc::new(super::super::super::prepared::Geometry { mesh: quad(5.), version: g.version, valid: true }));
        assert_eq!(bounds.of(&pass, &mut scanned), (vec3(5., 0., 0.), vec3(6., 2., 3.)));
        assert_eq!(scanned, 3);
        assert_eq!(bounds.of(&pass, &mut scanned).0, vec3(5., 0., 0.));
        assert_eq!(scanned, 3, "same generation: no scan");
        g.mesh = quad(-7.);
        g.advance_owned(false);
        pass.geometry = Some(std::rc::Rc::new(super::super::super::prepared::Geometry { mesh: quad(-7.), version: g.version, valid: true }));
        assert_eq!(bounds.of(&pass, &mut scanned).0, vec3(-7., 0., 0.));
        assert_eq!(scanned, 6);
    }
}
