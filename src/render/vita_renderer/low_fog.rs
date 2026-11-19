//! Conservative fog-free certification for a low opaque draw.
use super::{shader::Uniforms, Geometry, Group, Recipe};
use macroquad::prelude::*;

const MAX_SCAN_INDICES: usize = 384;
const INDICES_PER_BOX: usize = 12;
/// Keep all coordinate differences and the ray length within finite half
/// range too. The shader can still evaluate length for a ray that misses.
const COORD_LIMIT: f32 = 16_384.;
const HALF_NORMAL_MIN: f32 = 1. / 16_384.;
const HALF_FINITE_MAX: f32 = 65_504.;
pub(super) const FOG_FREE_PROGRAM: usize = 2;

#[derive(Clone, Copy)]
pub(super) struct Bounds {
    min: Vec3,
    max: Vec3,
}

impl Bounds {
    /// Built once from the original indexed triangles, before shared-buffer packing.
    fn chunks(geometry: &Geometry) -> Box<[Self]> {
        if geometry.indices.len() > MAX_SCAN_INDICES {
            return Box::default();
        }
        let mut boxes = Vec::new();
        for chunk in geometry.indices.chunks(INDICES_PER_BOX) {
            let mut min = Vec3::splat(f32::INFINITY);
            let mut max = Vec3::splat(f32::NEG_INFINITY);
            for &index in chunk {
                let p = Vec3::from_array(geometry.vertices[index as usize].position);
                min = min.min(p);
                max = max.max(p);
            }
            boxes.push(Self { min, max });
        }
        boxes.into_boxed_slice()
    }

    /// The interpolated position and every point between it and the eye are
    /// in the eye/box hull. One separating axis is sufficient. Leave the
    /// same outward allowance as the retained effect packet test.
    fn misses(self, eye: Vec3, volume: Self) -> bool {
        let scale = eye
            .abs()
            .max(self.min.abs())
            .max(self.max.abs())
            .max(volume.min.abs())
            .max(volume.max.abs());
        let margin = Vec3::splat(4.) + scale * 1e-3;
        let below = volume.min - margin;
        let above = volume.max + margin;
        ((self.max.cmplt(below) & eye.cmplt(below)) | (self.min.cmpgt(above) & eye.cmpgt(above)))
            .any()
    }
}

pub(super) fn bounds(recipe: &Recipe, geometry: &Geometry) -> Option<Box<[Bounds]>> {
    if recipe.stage.alpha_test != 0
        || !recipe.opaque
        || recipe.model.is_some()
        || recipe.motion.is_some()
        || recipe.deform.is_some()
        || geometry
            .vertices
            .iter()
            .any(|v| !supported(Vec3::from_array(v.position)))
    {
        return None;
    }
    Some(Bounds::chunks(geometry))
}

fn supported(p: Vec3) -> bool {
    p.abs().cmple(Vec3::splat(COORD_LIMIT)).all()
}

pub(super) fn free(group: &Group, chunks: Option<&[Bounds]>, u: &Uniforms) -> bool {
    if u.distance_fog[3] > 0. || u.liquid_fog[3] > 0. {
        return false;
    }
    let volumes = [
        (&u.fog_a, u.fog_a_min, u.fog_a_max),
        (&u.fog_b, u.fog_b_min, u.fog_b_max),
    ];
    if !volumes.iter().any(|(color, ..)| color[3] > 0.) {
        return true;
    }
    // Moving/deformed positions are not enclosed by the original boxes.
    let Some(chunks) = chunks else {
        return false;
    };
    let eye = Vec3::from_array(u.fog_eye);
    let bounds = Bounds {
        min: group.min,
        max: group.max,
    };
    if !supported(eye) {
        return false;
    }
    for (color, min, max) in volumes {
        if !(color[3] > 0.) {
            continue;
        }
        let volume = Bounds {
            min: Vec3::from_array(min),
            max: Vec3::from_array(max),
        };
        if !supported(volume.min)
            || !supported(volume.max)
            || !volume.min.cmple(volume.max).all()
            || !(HALF_NORMAL_MIN..=HALF_FINITE_MAX).contains(&color[3])
            || !Vec3::from_slice(&color[..3])
                .abs()
                .cmple(Vec3::splat(HALF_FINITE_MAX))
                .all()
        {
            return false;
        }
        if !bounds.misses(eye, volume)
            && (chunks.is_empty() || !chunks.iter().all(|b| b.misses(eye, volume)))
        {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_the_eye_to_geometry_hull_and_keep_a_boundary_margin() {
        let volume = Bounds {
            min: Vec3::splat(-10.),
            max: Vec3::splat(10.),
        };
        let wall = Bounds {
            min: vec3(20., -5., -5.),
            max: vec3(21., 5., 5.),
        };
        assert!(wall.misses(vec3(30., 0., 0.), volume));
        assert!(!wall.misses(vec3(-30., 0., 0.), volume));
        assert!(!wall.misses(Vec3::ZERO, volume));
        assert!(!wall.misses(vec3(11., 0., 0.), volume));

        let beside = Bounds {
            min: vec3(-25., 20., -5.),
            max: vec3(-20., 25., 5.),
        };
        let broad = Bounds {
            min: wall.min.min(beside.min),
            max: wall.max.max(beside.max),
        };
        let eye = vec3(30., 30., 0.);
        assert!(!broad.misses(eye, volume));
        assert!(wall.misses(eye, volume) && beside.misses(eye, volume));
    }
}
