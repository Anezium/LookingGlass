//! Additive fill cuts, each behind its own key and default off.
//!
//! `fx_add_zero_alpha_cut`: the port draws additive effects with source alpha (one source factor
//! of src_alpha, see `render_fx::effect`), so an additive effect whose texture has alpha 0 in
//! every texel adds src * 0 to each pixel it covers. Such puffs and effect groups are not drawn;
//! no pixel changes. Particle loading registers those textures.
//!
//! `fx_add_max_area`: an additive world puff (an emitter quad such as waterfall spray) whose
//! projected area is more than this fraction of the screen shrinks about its centre until it
//! covers that fraction. The centre, and so the joint sort depth, is kept; the texture is kept
//! whole, so the quad looks the same, only smaller. Changes pixels.
use super::*;
use crate::materials::Blend;
use std::cell::RefCell;
use std::collections::HashSet;

thread_local! {
    /// Textures whose alpha is 0 in every texel. The handles keep their ids from being reused.
    static ZERO_ALPHA: RefCell<(Vec<Texture2D>, HashSet<TextureId>)> = RefCell::default();
}
/// Register `texture` when every texel of `rgba` has alpha 0.
pub(super) fn note_texture(texture: &Texture2D, rgba: &[u8]) {
    if rgba.is_empty() || rgba.chunks_exact(4).any(|p| p[3] != 0) { return; }
    ZERO_ALPHA.with(|z| {
        let mut z = z.borrow_mut();
        if z.1.insert(texture.raw_miniquad_id()) { z.0.push(texture.clone()); }
    });
}
fn zero_alpha(texture: Option<&Texture2D>) -> bool {
    texture.is_some_and(|t| ZERO_ALPHA.with(|z| z.borrow().1.contains(&t.raw_miniquad_id())))
}
/// Drop additive puffs and all-additive effect groups whose texture has no alpha. Returns
/// (groups, puffs) cut.
pub(super) fn cut_zero_alpha(groups: &mut [super::super::Group], puffs: &mut Vec<world_puffs::Puff>, recycle: bool) -> (u64, u64) {
    let add = crate::render::blend_index(Blend::AlphaAdd);
    let mut cut_groups = 0;
    for g in groups.iter_mut() {
        if !g.passes.is_empty() && g.passes.iter().all(|p| p.effect && p.blend_index == add && zero_alpha(p.mesh.texture.as_ref())) {
            if recycle { super::super::actor_cache::recycle(&mut g.passes); }
            g.passes.clear();
            cut_groups += 1;
        }
    }
    let before = puffs.len();
    puffs.retain(|p| !(matches!(p.blend, Blend::Add | Blend::AlphaAdd) && zero_alpha(p.texture.as_ref())));
    (cut_groups, (before - puffs.len()) as u64)
}

/// Area of the quad's projection as a fraction of the view, or infinity when a corner is at or
/// behind the eye plane (its projection is unbounded).
fn screen_fraction(m: Mat4, corners: [Vec3; 4]) -> f32 {
    let mut ndc = [Vec2::ZERO; 4];
    for (out, p) in ndc.iter_mut().zip(corners) {
        let c = m * p.extend(1.);
        if !(c.w > 1e-3) { return f32::INFINITY; }
        *out = c.truncate().truncate() / c.w;
    }
    // Shoelace; the NDC view is 2 x 2.
    let twice: f32 = (0..4).map(|i| { let (a, b) = (ndc[i], ndc[(i + 1) % 4]); a.x * b.y - b.x * a.y }).sum();
    twice.abs() * 0.5 / 4.
}

/// The bounds centre, the point the joint sort reads.
fn centre(corners: [Vec3; 4]) -> Vec3 {
    let (lo, hi) = corners.iter().fold((Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY)),
        |(lo, hi), &q| (lo.min(q), hi.max(q)));
    (lo + hi) * 0.5
}
/// The largest scale in (0, 1] about the bounds centre that keeps the quad within `cap`.
pub(super) fn scale(m: Mat4, corners: [Vec3; 4], cap: f32) -> f32 {
    let centre = centre(corners);
    let at = |s: f32| screen_fraction(m, corners.map(|p| centre + (p - centre) * s));
    if at(1.) <= cap { return 1.; }
    let (mut lo, mut hi) = (0_f32, 1_f32);
    for _ in 0..12 {
        let mid = (lo + hi) * 0.5;
        if at(mid) <= cap { lo = mid; } else { hi = mid; }
    }
    lo
}

fn additive(blend: Blend) -> bool { matches!(blend, Blend::Add | Blend::AlphaAdd | Blend::ColorAdd) }

/// Shrink every additive puff over `cap`. Returns how many changed.
pub(super) fn cap(puffs: &mut [world_puffs::Puff], m: Mat4, cap: f32) -> u64 {
    let mut changed = 0;
    for p in puffs.iter_mut().filter(|p| additive(p.blend)) {
        let corners = p.vertices.map(|v| v.position);
        let s = scale(m, corners, cap);
        if s < 1. {
            // About the bounds centre, so the sort depth is unchanged.
            let centre = centre(corners);
            for v in &mut p.vertices { v.position = centre + (v.position - centre) * s; }
            changed += 1;
        }
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_quad_over_the_cap_shrinks_to_it_and_one_under_it_stays() {
        let m = Mat4::perspective_rh_gl(75_f32.to_radians(), 960. / 544., 2., 30000.);
        // A camera-facing square at z = -100 (camera looks down -Z).
        let quad = |half: f32| [vec3(-half, -half, -100.), vec3(half, -half, -100.), vec3(half, half, -100.), vec3(-half, half, -100.)];
        let full = screen_fraction(m, quad(40.));
        assert!(full > 0.1 && full < 1., "{full}");
        assert_eq!(scale(m, quad(10.), 0.25), 1.);
        let s = scale(m, quad(40.), 0.05);
        assert!(s < 1. && (screen_fraction(m, quad(40. * s)) - 0.05).abs() < 0.001, "{s}");
        // A corner behind the eye: unbounded, shrunk until it is in front and small.
        let straddle = [vec3(-50., -50., 20.), vec3(50., -50., 20.), vec3(50., 50., -100.), vec3(-50., 50., -100.)];
        assert_eq!(screen_fraction(m, straddle), f32::INFINITY);
        let s = scale(m, straddle, 0.1);
        assert!(s > 0. && s < 1.);
    }
}
