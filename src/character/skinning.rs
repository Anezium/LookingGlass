//! Visual skinning only; animation, tags, sounds and gameplay retain Transform.
use crate::skeletal::{SkinVertex, Transform};
use macroquad::prelude::*;

// Adapted from NextOs-Ports/Looking-Glass-NextOS 15779e4 (felc18-blip).
pub(super) fn rotation(q: Quat) -> Mat3 {
    // Basis expansion preserves Quat * Vec3 even for packed non-unit rotations.
    Mat3::from_cols(q * Vec3::X, q * Vec3::Y, q * Vec3::Z)
}
pub(super) fn palette(pose: &[Transform], scales: &[f32]) -> Vec<(Mat3, Vec3)> {
    pose.iter().enumerate().map(|(i, bone)| {
        let scale = scales.get(i).copied().unwrap_or(1.);
        (Mat3::from_cols(bone.rotation * (Vec3::X * scale),
            bone.rotation * (Vec3::Y * scale), bone.rotation * (Vec3::Z * scale)),
            bone.translation)
    }).collect()
}
pub(super) fn position(src: &SkinVertex, palette: &[(Mat3, Vec3)]) -> Vec3 {
    src.weights.iter().map(|w| {
        let (rotation, translation) = palette[w.bone];
        (rotation * w.offset + translation) * w.amount
    }).sum()
}

thread_local! { static PRESERVE_SKIN: std::cell::Cell<bool> = const { std::cell::Cell::new(false) }; }
/// Expanding overlays need current vertices even when the base skin is offscreen.
pub(crate) struct CullGuard(bool);
impl CullGuard {
    pub(crate) fn preserve(required: bool) -> Self {
        Self(PRESERVE_SKIN.with(|c| c.replace(c.get() || required)))
    }
}
impl Drop for CullGuard {
    fn drop(&mut self) { PRESERVE_SKIN.with(|c| c.set(self.0)); }
}
pub(super) fn culling_allowed() -> bool { PRESERVE_SKIN.with(|c| !c.get()) }

// Adapted from NextOs-Ports/Looking-Glass-NextOS 51b6224 (felc18-blip).
pub(super) fn world_bounds(bounds: &[(Vec3, Vec3)], palette: &[(Mat3, Vec3)],
    world: Mat3, origin: Vec3, scale: f32) -> Option<(Vec3, Vec3)> {
    if bounds.len() != palette.len() || !world.is_finite() || !origin.is_finite() || !scale.is_finite() { return None; }
    let mut lo = Vec3::splat(f32::INFINITY);
    let mut hi = Vec3::splat(f32::NEG_INFINITY);
    for (&(min, max), &(rotation, translation)) in bounds.iter().zip(palette) {
        if min == Vec3::splat(f32::INFINITY) && max == Vec3::splat(f32::NEG_INFINITY) { continue; }
        if !min.is_finite() || !max.is_finite() || !rotation.is_finite() || !translation.is_finite() { return None; }
        let center = world * ((rotation * (min * 0.5 + max * 0.5) + translation) * scale) + origin;
        let matrix = world * rotation * scale;
        let half = (max * 0.5 - min * 0.5).abs();
        let extent = matrix.x_axis.abs() * half.x + matrix.y_axis.abs() * half.y + matrix.z_axis.abs() * half.z;
        // Cover quaternion/matrix rounding and up to 200 normalized weights.
        // Uncancelled magnitudes keep slack under large translation cancellation.
        let magnitude = world.x_axis.abs() + world.y_axis.abs() + world.z_axis.abs();
        let rotation_max = rotation.to_cols_array().into_iter().map(f32::abs).fold(0_f32, f32::max);
        let pad = Vec3::splat(0.1) + (center.abs() + extent + origin.abs()
            + magnitude * (translation.abs().max_element() + 3. * rotation_max
                * min.abs().max(max.abs()).max_element()) * scale.abs()) * (1024. * f32::EPSILON);
        if !(center.is_finite() && extent.is_finite() && pad.is_finite()) { return None; }
        lo = lo.min(center - extent - pad);
        hi = hi.max(center + extent + pad);
    }
    (lo.is_finite() && hi.is_finite()).then_some((lo, hi))
}

/// Headless real-data microbench, including palette construction once per draw.
pub(crate) fn bench(assets: &mut crate::assets::Assets) -> anyhow::Result<()> {
    use crate::skeletal::{Animation, Definition, Skeleton};
    use std::{hint::black_box, time::Instant};
    let def = Definition::alice(assets)?;
    let skeleton = Skeleton::parse(&assets.read(&format!("{}/{}", def.path, def.model))?)?;
    let clip = Animation::parse(&assets.read(&format!("{}/run_nowep.ska", def.path))?, skeleton.bones.len())?;
    let poses = clip.frames.iter().take(16).map(|frame| skeleton.global_pose(&frame.pose)).collect::<Vec<_>>();
    let scales = (0..skeleton.bones.len()).map(|b| [0.65, 1.4, 1.][b % 3]).collect::<Vec<_>>();
    let world = Transform { rotation: Quat::from_rotation_z(0.731), translation: vec3(100., -200., 30.) };
    let model_scale = -0.75;
    let vertices = skeleton.surfaces.iter().flat_map(|s| &s.vertices).collect::<Vec<_>>();
    let mut output = vec![Vec3::ZERO; vertices.len()];
    let mut max_error = 0_f32;
    for pose in &poses {
        let p = palette(pose, &scales);
        for src in &vertices {
            let old: Vec3 = src.weights.iter().map(|w| pose[w.bone].point(w.offset * scales[w.bone]) * w.amount).sum();
            let error = (world.point(old * model_scale) - (rotation(world.rotation) * (position(src, &p) * model_scale) + world.translation)).abs().max_element();
            max_error = max_error.max(error);
        }
    }
    anyhow::ensure!(max_error < 0.001, "palette error {max_error}");
    let draws = 5000;
    let mut old_times = Vec::new();
    let mut new_times = Vec::new();
    // Alternating order limits warm-up/order bias. black_box retains every vertex.
    for round in 0..8 {
        for mode in [round % 2, 1 - round % 2] {
            let start = Instant::now();
            for i in 0..draws {
                let pose = black_box(&poses[i % poses.len()]);
                let scales = black_box(scales.as_slice());
                if mode == 1 {
                    let p = palette(pose, scales);
                    let r = rotation(black_box(world.rotation));
                    for (src, dst) in vertices.iter().zip(&mut output) {
                        *dst = r * (position(src, &p) * model_scale) + world.translation;
                    }
                } else {
                    for (src, dst) in vertices.iter().zip(&mut output) {
                        let p: Vec3 = src.weights.iter().map(|w| pose[w.bone].point(w.offset * scales[w.bone]) * w.amount).sum();
                        *dst = world.point(p * model_scale);
                    }
                }
                black_box(&output);
            }
            let ns = start.elapsed().as_nanos() as f64 / (draws * vertices.len()) as f64;
            if mode == 1 { new_times.push(ns); } else { old_times.push(ns); }
        }
    }
    old_times.sort_by(f64::total_cmp); new_times.sort_by(f64::total_cmp);
    println!("[lg/skin-bench] {}", serde_json::json!({"model":format!("{}/{}",def.path,def.model),
        "bones":skeleton.bones.len(),"vertices":vertices.len(),"draws_per_round":draws,"rounds":8,
        "reference_ns_per_vertex":old_times[4],"palette_ns_per_vertex":new_times[4],
        "reference_samples":old_times,"palette_samples":new_times,"max_error_units":max_error,
        "palette_setup_included":true,"note":"x86 relative indication only; Cortex-A9 gain unmeasured"}));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skeletal::Weight;
    #[test]
    fn bone_bounds_enclose_weighted_reference_vertices() {
        let bounds = [(vec3(-30., -20., -10.), vec3(60., 50., 40.)),
            (vec3(-11., -32., -52.), vec3(12., 44., 63.))];
        for i in 0..1024 {
            let t = i as f32 * 0.017;
            let pose = [Transform { rotation: Quat::from_rotation_z(t) * 1.005, translation: vec3(t, 73., 15.) },
                Transform { rotation: Quat::from_rotation_y(-t), translation: vec3(-31., t, 28.) }];
            for scales in [[0.65, 1.4], [-1.7, 0.]] {
                let p = palette(&pose, &scales);
                let world = Transform { rotation: Quat::from_rotation_x(t) * 0.997, translation: vec3(-1200., 4300., -85.) };
                for scale in [1.5, -0.75, 0.] {
                    let (lo, hi) = world_bounds(&bounds, &p, rotation(world.rotation), world.translation, scale).unwrap();
                    for a in 0..8 {
                        for b in 0..8 {
                            let vertex: Vec3 = bounds.iter().enumerate().map(|(bone, &(min, max))| {
                                let corner = [a, b][bone];
                                let offset = vec3(if corner & 1 == 0 { min.x } else { max.x },
                                    if corner & 2 == 0 { min.y } else { max.y }, if corner & 4 == 0 { min.z } else { max.z });
                                pose[bone].point(offset * scales[bone]) * [0.37, 0.63][bone]
                            }).sum();
                            let vertex = world.point(vertex * scale);
                            assert!(vertex.cmpge(lo).all() && vertex.cmple(hi).all());
                        }
                    }
                }
            }
        }
    }
    #[test]
    fn invalid_bounds_fail_open_and_overlay_guards_restore_the_scope() {
        let p = [(Mat3::IDENTITY, Vec3::ZERO)];
        assert!(world_bounds(&[], &p, Mat3::IDENTITY, Vec3::ZERO, 1.).is_none());
        assert!(world_bounds(&[(Vec3::splat(f32::NAN), Vec3::ONE)], &p, Mat3::IDENTITY, Vec3::ZERO, 1.).is_none());
        assert!(world_bounds(&[(Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY))], &p, Mat3::IDENTITY, Vec3::ZERO, 1.).is_none());
        assert!(culling_allowed());
        { let _outer = CullGuard::preserve(true); assert!(!culling_allowed());
            { let _inner = CullGuard::preserve(false); assert!(!culling_allowed()); }
            assert!(!culling_allowed()); }
        assert!(culling_allowed());
    }
    #[test]
    fn palette_preserves_positions_with_nonunit_rotations_and_signed_scales() {
        for i in 0..1024 {
            let t = i as f32 * 0.017;
            let pose = [Transform { rotation: Quat::from_rotation_z(t) * 1.005, translation: vec3(t, -t * 2., 73.) },
                Transform { rotation: Quat::from_rotation_x(-t), translation: vec3(17., t, -31.) }];
            for scales in [&[][..], &[0.65, 1.4][..], &[-0.75, 0.][..]] {
                let p = palette(&pose, scales);
                let src = SkinVertex { uv: Vec2::ZERO, weights: vec![
                    Weight { bone: 0, amount: 0.37, offset: vec3(t * 3., 47., -t) },
                    Weight { bone: 1, amount: 0.63, offset: vec3(-t, 12., t * 2.) }] };
                let expected: Vec3 = src.weights.iter().map(|w| pose[w.bone].point(w.offset * scales.get(w.bone).copied().unwrap_or(1.)) * w.amount).sum();
                let actual = position(&src, &p);
                assert!((expected - actual).length() < 0.0001);
                let world = Transform { rotation: Quat::from_rotation_y(t) * 0.997, translation: vec3(-1200., 4300., -85.) };
                for scale in [1.5, -0.75, 0.] {
                    assert!((world.point(expected * scale) - (rotation(world.rotation) * (actual * scale) + world.translation)).abs().max_element() < 0.0005);
                }
            }
        }
    }
}
