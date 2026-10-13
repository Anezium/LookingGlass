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
