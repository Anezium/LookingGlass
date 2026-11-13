//! World draw cuts, each behind its own key and default off: `world_min_pixels` (screen size) and
//! `world_texture_sort` (opaque order). The actor, prop and FX packet cuts are in
//! `render_fx::retained::packet_cull`.
use super::*;

/// True when the box, seen through `m`, is shorter than `limit` pixels of the 960x544 screen on
/// its longer side. A box that crosses the eye plane is never small. `limit` 0 is off.
#[inline]
pub(super) fn below_pixels(m: Mat4, min: Vec3, max: Vec3, limit: f32) -> bool {
    limit > 0. && crate::render::projected_pixels(m, min, max).is_some_and(|size| size < limit)
}

/// Opaque world order for `world_texture_sort`. Rank first, so a second layer still follows the
/// first one over the same faces. Then draws without alpha test, grouped by pipeline and texture,
/// front to back inside one texture. Then alpha-tested draws front to back: they discard, so they
/// go after the geometry they can be rejected against. Kept while the input list repeats.
#[derive(Default)]
pub(super) struct TextureSort { input: Vec<(u32, usize)>, output: Vec<(f32, usize)>, valid: bool }
impl TextureSort {
    pub fn apply(&mut self, draws: &mut Vec<(f32, usize)>, groups: &[Group]) {
        let same = self.valid && self.input.len() == draws.len()
            && self.input.iter().zip(draws.iter()).all(|(a, b)| a.0 == b.0.to_bits() && a.1 == b.1);
        if same {
            draws.clear();
            draws.extend_from_slice(&self.output);
            return;
        }
        self.input.clear();
        self.input.extend(draws.iter().map(|d| (d.0.to_bits(), d.1)));
        draws.sort_unstable_by(|a, b| key(&groups[a.1]).cmp(&key(&groups[b.1]))
            .then_with(|| a.0.total_cmp(&b.0)).then_with(|| a.1.cmp(&b.1)));
        self.output.clear();
        self.output.extend_from_slice(draws);
        self.valid = true;
    }
}
fn key(g: &Group) -> (usize, bool, usize, usize) {
    if g.recipe.stage.alpha_test > 0 { (g.recipe.rank, true, 0, 0) }
    else { (g.recipe.rank, false, g.recipe.blend * 3 + g.recipe.cull, g.material_order) }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn below_pixels_is_off_at_zero_and_never_small_across_the_eye() {
        let projection = Mat4::perspective_rh_gl(1.2, 960. / 544., 4., 8192.);
        let view = Mat4::look_at_rh(Vec3::ZERO, vec3(1., 0., 0.), Vec3::Z);
        let m = projection * view;
        // A 2-unit box 1000 units ahead is about one pixel wide.
        let (lo, hi) = (vec3(1000., -1., -1.), vec3(1002., 1., 1.));
        assert!(below_pixels(m, lo, hi, 6.));
        assert!(!below_pixels(m, lo, hi, 0.), "0 is off");
        assert!(!below_pixels(m, lo, hi, 0.5));
        // A box spanning the eye plane has no screen rectangle.
        assert!(!below_pixels(m, vec3(-1., -1., -1.), vec3(1., 1., 1.), 64.));
        // A large near box is not small.
        assert!(!below_pixels(m, vec3(50., -40., -40.), vec3(60., 40., 40.), 64.));
    }
}
