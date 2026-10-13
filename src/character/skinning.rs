//! Visual skinning only; animation, tags, sounds and gameplay retain Transform.
use crate::skeletal::{SkinVertex, Transform};
use macroquad::prelude::*;

// Adapted from NextOs-Ports/Looking-Glass-NextOS 15779e4 (felc18-blip).
pub(super) fn rotation(q: Quat) -> Mat3 {
    // Basis expansion preserves Quat * Vec3 even for packed non-unit rotations.
    Mat3::from_cols(q * Vec3::X, q * Vec3::Y, q * Vec3::Z)
}
fn palette_bone(bone: &Transform, scale: f32) -> (Mat3, Vec3) {
    (Mat3::from_cols(bone.rotation * (Vec3::X * scale),
        bone.rotation * (Vec3::Y * scale), bone.rotation * (Vec3::Z * scale)),
        bone.translation)
}
fn fill_palette(output: &mut Vec<(Mat3, Vec3)>, pose: &[Transform], scales: &[f32]) {
    output.clear();
    output.extend(pose.iter().enumerate().map(|(i, bone)| {
        palette_bone(bone, scales.get(i).copied().unwrap_or(1.))
    }));
}
thread_local! {
    static PALETTE: std::cell::RefCell<Vec<(Mat3, Vec3)>> = const { std::cell::RefCell::new(Vec::new()) };
}
/// Borrow only for an accepted palette draw. Submission does not re-enter skinning.
/// Capacity survives between actors and frames; reference/rejected draws never fill it.
pub(super) fn with_palette<R>(enabled: bool, pose: &[Transform], scales: &[f32],
    draw: impl FnOnce(&[(Mat3, Vec3)]) -> R) -> R {
    if !enabled { return draw(&[]); }
    PALETTE.with(|p| {
        let mut p = p.borrow_mut();
        fill_palette(&mut p, pose, scales);
        draw(&p)
    })
}
fn palette(pose: &[Transform], scales: &[f32]) -> Vec<(Mat3, Vec3)> {
    let mut p = Vec::new();
    fill_palette(&mut p, pose, scales);
    p
}
pub(super) fn position(src: &SkinVertex, palette: &[(Mat3, Vec3)]) -> Vec3 {
    src.weights.iter().map(|w| {
        let (rotation, translation) = palette[w.bone];
        (rotation * w.offset + translation) * w.amount
    }).sum()
}

thread_local! { static PRESERVE_SKIN: std::cell::Cell<bool> = const { std::cell::Cell::new(false) }; }
/// Expanding/normal-driven overlays need current vertices and rebuilt normals.
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

pub(super) fn visual_options(cull: bool, flat: bool, meshes: &[Option<Mesh>]) -> (bool, bool) {
    if !cull && !flat { return (false, false); }
    let allowed = culling_allowed()
        && !meshes.iter().flatten().any(crate::render_fx::skin_requires_preservation);
    (cull && allowed, flat && allowed)
}

pub(super) fn outside_skin_bounds(cull: bool, bounds: &[(Vec3, Vec3)],
    pose: &[Transform], scales: &[f32], world: Mat3, origin: Vec3, scale: f32) -> bool {
    cull && world_bounds_from_pose(bounds, pose, scales, world, origin, scale)
        .is_some_and(|(lo, hi)| crate::render::outside_bounds(lo, hi))
}

/// Rebuild normals before any later shell, environment UV or dot-alpha layer.
fn shade_arrays(vertices: &mut [Vertex], indices: &[u16], flat: bool, fullbright: bool, alpha: f32) {
    if flat {
        let light = if fullbright { 255 } else { 217 };
        for v in &mut *vertices {
            v.normal = Vec4::ZERO;
            v.color = [light, light, light, (alpha.clamp(0., 1.) * 255.) as u8];
        }
        return;
    }
    normals(vertices, indices);
    colors(vertices, fullbright, alpha);
}
fn normals(vertices: &mut [Vertex], indices: &[u16]) {
    for tri in indices.chunks_exact(3) {
        let [a, b, c] = [tri[0] as usize, tri[1] as usize, tri[2] as usize];
        let n = (vertices[b].position - vertices[a].position)
            .cross(vertices[c].position - vertices[a].position);
        for i in [a, b, c] { vertices[i].normal += n.extend(0.); }
    }
}
fn colors(vertices: &mut [Vertex], fullbright: bool, alpha: f32) {
    for v in &mut *vertices {
        let n = v.normal.truncate().normalize_or_zero();
        let light = if fullbright { 1. } else {
            0.65 + 0.35 * n.dot(vec3(0.3, -0.5, 1.).normalize()).max(0.)
        };
        v.color = [(light * 255.) as u8, (light * 255.) as u8,
            (light * 255.) as u8, (alpha.clamp(0., 1.) * 255.) as u8];
    }
}

/// Only these disjoint CPU arrays cross the scope boundary; never a Mesh/Texture.
pub(super) struct Job<'a> {
    pub input: &'a [SkinVertex], pub vertices: &'a mut [Vertex], pub indices: &'a [u16],
}
#[derive(Clone, Copy)]
pub(super) struct Params<'a> {
    pub pose: &'a [Transform], pub scales: &'a [f32], pub palette: &'a [(Mat3, Vec3)],
    pub transform: Transform, pub scale: f32, pub flat: bool, pub fullbright: bool, pub alpha: f32,
}
pub(super) fn positions(job: &mut Job<'_>, p: Params<'_>) {
    for (src, dst) in job.input.iter().zip(&mut *job.vertices) {
        let position = if !p.palette.is_empty() { position(src, p.palette) }
            else if p.scales.is_empty() { src.position(p.pose) }
            else { src.weights.iter().map(|w| p.pose[w.bone].point(w.offset * p.scales[w.bone]) * w.amount).sum() };
        dst.position = if !p.palette.is_empty() { rotation(p.transform.rotation) * (position * p.scale) + p.transform.translation }
            else { p.transform.point(position * p.scale) };
        dst.normal = Vec4::ZERO;
    }
}
pub(super) fn kernel(job: &mut Job<'_>, p: Params<'_>) {
    positions(job, p);
    shade_arrays(job.vertices, job.indices, p.flat, p.fullbright, p.alpha);
}
pub(super) fn shade(mesh: &mut Mesh, flat: bool, fullbright: bool, alpha: f32) {
    if flat { shade_arrays(&mut mesh.vertices, &mesh.indices, flat, fullbright, alpha); return; }
    let timer = crate::frame_profile::characters_span("characters_skinning_normals");
    normals(&mut mesh.vertices, &mesh.indices); drop(timer);
    let timer = crate::frame_profile::characters_span("characters_skinning_shading");
    colors(&mut mesh.vertices, fullbright, alpha); drop(timer);
}
/// Split coarse surface batches within an actor. Triangle reduction order stays serial.
/// Joining the scope precedes every layer/GL consumer in the same frame.
pub(super) fn run(jobs: &mut [Job<'_>], p: Params<'_>, workers: usize) -> [u64; 3] {
    assert!(workers <= 2);
    if workers == 0 || jobs.is_empty() {
        for job in jobs { kernel(job, p); }
        return [0; 3];
    }
    let total = jobs.iter().map(|j| j.vertices.len()).sum::<usize>();
    let target = total.div_ceil(workers + 1);
    std::thread::scope(|scope| {
        let mut remaining = jobs;
        let mut handles = Vec::with_capacity(workers);
        for worker in 0..workers {
            if remaining.is_empty() { break; }
            let mut n = 0;
            let mut cost = 0;
            while n + 1 < remaining.len() && cost < target {
                cost += remaining[n].vertices.len(); n += 1;
            }
            let (batch, rest) = remaining.split_at_mut(n.max(1));
            remaining = rest;
            handles.push(std::thread::Builder::new().name(format!("skin-{worker}"))
                .stack_size(crate::skin_worker::STACK_BYTES).spawn_scoped(scope, move || {
                    crate::skin_worker::pin(worker, false);
                    let start = std::time::Instant::now();
                    for job in batch { kernel(job, p); }
                    start.elapsed().as_micros() as u64
                }).expect("start scoped skin worker"));
        }
        let start = std::time::Instant::now();
        for job in remaining { kernel(job, p); }
        let main_us = start.elapsed().as_micros() as u64;
        let wait = std::time::Instant::now();
        let service = handles.into_iter().map(|h| h.join().expect("skin worker panicked")).sum();
        [main_us, service, wait.elapsed().as_micros() as u64]
    })
}

// Adapted from NextOs-Ports/Looking-Glass-NextOS 51b6224 (felc18-blip).
#[cfg(test)]
fn world_bounds(bounds: &[(Vec3, Vec3)], palette: &[(Mat3, Vec3)],
    world: Mat3, origin: Vec3, scale: f32) -> Option<(Vec3, Vec3)> {
    bounds_with(bounds, palette.len(), |i| palette[i], world, origin, scale)
}
/// Stream the transforms needed for occupied bone boxes, without a vertex palette.
pub(crate) fn world_bounds_from_pose(bounds: &[(Vec3, Vec3)], pose: &[Transform],
    scales: &[f32], world: Mat3, origin: Vec3, scale: f32) -> Option<(Vec3, Vec3)> {
    bounds_with(bounds, pose.len(), |i| palette_bone(&pose[i], scales.get(i).copied().unwrap_or(1.)),
        world, origin, scale)
}
fn bounds_with(bounds: &[(Vec3, Vec3)], bones: usize,
    mut bone: impl FnMut(usize) -> (Mat3, Vec3), world: Mat3, origin: Vec3,
    scale: f32) -> Option<(Vec3, Vec3)> {
    if bounds.len() != bones || !world.is_finite() || !origin.is_finite() || !scale.is_finite() { return None; }
    let mut lo = Vec3::splat(f32::INFINITY);
    let mut hi = Vec3::splat(f32::NEG_INFINITY);
    for (i, &(min, max)) in bounds.iter().enumerate() {
        if min == Vec3::splat(f32::INFINITY) && max == Vec3::splat(f32::NEG_INFINITY) { continue; }
        let (rotation, translation) = bone(i);
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

/// Fixed-pose parity using the user-supplied Alice SKB/SKA, without graphics.
pub(crate) fn workers_check(assets: &mut crate::assets::Assets) -> anyhow::Result<()> {
    use crate::skeletal::{Animation, Definition, Skeleton};
    let def=Definition::alice(assets)?;
    let skeleton=Skeleton::parse(&assets.read(&format!("{}/{}",def.path,def.model))?)?;
    let clip=Animation::parse(&assets.read(&format!("{}/run_nowep.ska",def.path))?,skeleton.bones.len())?;
    let mut comparisons=0;
    for frame in clip.frames.iter().take(16) {
        let pose=skeleton.global_pose(&frame.pose);
        for palette_on in [false,true] {for scaled in [false,true] {for flat in [false,true] {for fullbright in [false,true] {
            let scales=if scaled {(0..pose.len()).map(|i|[0.65,1.4,-0.75][i%3]).collect::<Vec<_>>()} else {vec![]};
            let pal=if palette_on {palette(&pose,&scales)} else {vec![]};
            let params=Params {pose:&pose,scales:&scales,palette:&pal,
                transform:Transform {rotation:Quat::from_rotation_z(0.731),translation:vec3(100.,-200.,30.)},
                scale:-0.75,flat,fullbright,alpha:0.731};
            let initial=skeleton.surfaces.iter().map(|s|s.vertices.iter().map(|v|Vertex {
                position:Vec3::ZERO,uv:v.uv,color:[0;4],normal:Vec4::ZERO }).collect::<Vec<_>>()).collect::<Vec<_>>();
            let mut reference=initial.clone();
            let mut jobs=skeleton.surfaces.iter().zip(&mut reference).map(|(s,v)|Job {
                input:&s.vertices,indices:&s.indices,vertices:v }).collect::<Vec<_>>();
            run(&mut jobs,params,0); drop(jobs);
            for workers in [1,2] {
                let mut outputs=initial.clone();
                let mut jobs=skeleton.surfaces.iter().zip(&mut outputs).map(|(s,v)|Job {
                    input:&s.vertices,indices:&s.indices,vertices:v }).collect::<Vec<_>>();
                run(&mut jobs,params,workers); drop(jobs);
                for (a,b) in reference.iter().zip(&outputs) {for (a,b) in a.iter().zip(b) {
                    anyhow::ensure!(a.position.to_array().map(f32::to_bits)==b.position.to_array().map(f32::to_bits)
                        && a.normal.to_array().map(f32::to_bits)==b.normal.to_array().map(f32::to_bits)
                        && a.uv.to_array().map(f32::to_bits)==b.uv.to_array().map(f32::to_bits)
                        && a.color==b.color,"skin worker {workers} mismatch");
                }}
                comparisons+=1;
            }
        }}}}
    }
    println!("PASS fixed-pose skin workers: {comparisons} actor comparisons, {} surfaces, {} bones, {} vertices, workers=1/2, exact position/normal/color/UV bits",
        skeleton.surfaces.len(),skeleton.bones.len(),skeleton.surfaces.iter().map(|s|s.vertices.len()).sum::<usize>());
    Ok(())
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
        with_palette(true, pose, &scales, |p| {
            for src in &vertices {
                let old: Vec3 = src.weights.iter().map(|w| pose[w.bone].point(w.offset * scales[w.bone]) * w.amount).sum();
                let error = (world.point(old * model_scale) - (rotation(world.rotation) * (position(src, p) * model_scale) + world.translation)).abs().max_element();
                max_error = max_error.max(error);
            }
        });
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
                    with_palette(true, pose, scales, |p| {
                        let r = rotation(black_box(world.rotation));
                        for (src, dst) in vertices.iter().zip(&mut output) {
                            *dst = r * (position(src, p) * model_scale) + world.translation;
                        }
                    });
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
    fn scoped_workers_match_fixed_pose_reference_bit_for_bit() {
        fn same(a: &[Vertex], b: &[Vertex]) {
            assert_eq!(a.len(),b.len());
            for (a,b) in a.iter().zip(b) {
                assert_eq!(a.position.to_array().map(f32::to_bits),b.position.to_array().map(f32::to_bits));
                assert_eq!(a.normal.to_array().map(f32::to_bits),b.normal.to_array().map(f32::to_bits));
                assert_eq!(a.uv.to_array().map(f32::to_bits),b.uv.to_array().map(f32::to_bits));
                assert_eq!(a.color,b.color);
            }
        }
        // Independent reference retains the pre-extraction operation/reduction order.
        fn reference(src: &[SkinVertex], dst: &mut [Vertex], indices: &[u16], p: Params<'_>) {
            let world = rotation(p.transform.rotation);
            for (src,dst) in src.iter().zip(&mut *dst) {
                let v = if !p.palette.is_empty() {
                    src.weights.iter().map(|w| { let (r,t)=p.palette[w.bone]; (r*w.offset+t)*w.amount }).sum()
                } else if p.scales.is_empty() { src.position(p.pose) }
                else { src.weights.iter().map(|w| p.pose[w.bone].point(w.offset*p.scales[w.bone])*w.amount).sum() };
                dst.position = if !p.palette.is_empty() { world*(v*p.scale)+p.transform.translation } else { p.transform.point(v*p.scale) };
                dst.normal = Vec4::ZERO;
            }
            if !p.flat {
                for tri in indices.chunks_exact(3) {
                    let (a,b,c)=(tri[0] as usize,tri[1] as usize,tri[2] as usize);
                    let n=(dst[b].position-dst[a].position).cross(dst[c].position-dst[a].position);
                    for i in [a,b,c] {dst[i].normal+=n.extend(0.);}
                }
            }
            for v in dst {
                let light = if p.fullbright {1.} else if p.flat {217. / 255.} else {
                    0.65+0.35*v.normal.truncate().normalize_or_zero().dot(vec3(0.3,-0.5,1.).normalize()).max(0.)
                };
                let color = if p.flat && !p.fullbright {217} else {(light*255.) as u8};
                v.color=[color,color,color,(p.alpha.clamp(0.,1.)*255.) as u8];
            }
        }
        let indices=[0,1,2,0,2,3];
        let input=(0..4).map(|v|SkinVertex { uv:vec2(v as f32/3.,-0.),
            weights:(0..7).map(|b|Weight {bone:b,amount:1. / 7.,offset:vec3(v as f32,(v%2) as f32,b as f32*0.2)}).collect() }).collect::<Vec<_>>();
        fn send<T: Send>() {} send::<Job<'_>>(); send::<Params<'_>>();
        for frame in 0..16 {
            let pose=(0..7).map(|b|Transform {rotation:Quat::from_rotation_z(frame as f32*0.071+b as f32*0.05)*1.005,
                translation:vec3(b as f32,-2.,frame as f32)}).collect::<Vec<_>>();
            for palette_on in [false,true] { for scaled in [false,true] { for flat in [false,true] { for fullbright in [false,true] {
                let scales=if scaled {vec![-0.75,1.4,1.,0.,2.,0.5,1.1]} else {vec![]};
                let pal=if palette_on {palette(&pose,&scales)} else {vec![]};
                let params=Params {pose:&pose,scales:&scales,palette:&pal,
                    transform:Transform {rotation:Quat::from_rotation_y(0.73)*0.99,translation:vec3(1e3,-200.,30.)},
                    scale:if frame%2==0 {-0.75} else {1.5},flat,fullbright,alpha:frame as f32/12.};
                let initial=input.iter().map(|v|Vertex {position:Vec3::ZERO,uv:v.uv,normal:Vec4::ONE,color:[11;4]}).collect::<Vec<_>>();
                let mut expected=initial.clone(); reference(&input,&mut expected,&indices,params);
                for count in [1,2,5] { for workers in [0,1,2] {
                    let mut outputs=vec![initial.clone();count];
                    let mut jobs=outputs.iter_mut().map(|v|Job {input:&input,vertices:v,indices:&indices}).collect::<Vec<_>>();
                    run(&mut jobs,params,workers); drop(jobs);
                    for out in outputs {same(&expected,&out);}
                }}
            }}}}
        }
    }
    #[test]
    fn palette_scratch_reuses_capacity_and_disabled_draws_do_not_borrow_it() {
        let pose = [Transform { rotation: Quat::IDENTITY, translation: Vec3::ONE }; 131];
        let pointer = with_palette(true, &pose, &[], |p| p.as_ptr());
        for count in [131, 2, 100, 0, 131] {
            let scales = vec![-0.75; count];
            with_palette(true, &pose[..count], &scales, |p| {
                assert_eq!(p.len(), count);
                assert_eq!(p.as_ptr(), pointer);
                for &(r, t) in p { assert_eq!(r, Mat3::from_diagonal(Vec3::splat(-0.75))); assert_eq!(t, Vec3::ONE); }
            });
        }
        PALETTE.with(|p| {
            let scratch = p.borrow_mut();
            // A reference draw must not even borrow or mutate the scratch palette.
            with_palette(false, &pose, &[], |p| assert!(p.is_empty()));
            assert_eq!(scratch.len(), pose.len());
        });
    }
    #[test]
    fn streamed_bounds_match_palette_and_rejected_draws_leave_scratch_untouched() {
        let bounds = [(vec3(-3., -2., -1.), vec3(6., 5., 4.)),
            (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY))];
        let pose = [Transform { rotation: Quat::from_rotation_y(0.7) * 1.005,
            translation: vec3(10., 20., 30.) },
            Transform { rotation: Quat::from_xyzw(f32::NAN, 0., 0., 1.), translation: Vec3::ZERO }];
        for scales in [&[][..], &[-1.7, 0.][..]] {
            let p = palette(&pose, scales);
            for scale in [-0.75, 1.5] {
                let expected = world_bounds(&bounds, &p, Mat3::IDENTITY, Vec3::ZERO, scale);
                assert!(expected.is_some());
                // Bounds/culling can run while scratch is unavailable: no palette fill.
                PALETTE.with(|scratch| {
                    let mut scratch = scratch.borrow_mut();
                    scratch.clear();
                    assert_eq!(world_bounds_from_pose(&bounds, &pose, scales,
                        Mat3::IDENTITY, Vec3::ZERO, scale), expected);
                    assert!(scratch.is_empty());
                });
            }
        }
        assert!(world_bounds_from_pose(&bounds[..1], &pose, &[], Mat3::IDENTITY, Vec3::ZERO, 1.).is_none());
    }
    #[test]
    fn late_overlay_survives_side_and_perspective_near_plane_rejection() {
        let pose = [Transform { rotation: Quat::IDENTITY, translation: Vec3::ZERO }];
        let projection = Mat4::perspective_rh_gl(std::f32::consts::FRAC_PI_2, 1., 1., 100.);
        crate::render_fx::set_projection(projection);
        // Shock can expand eight units along its outward normal plus 0.2 offset.
        for (center, normal) in [(vec3(6., 0., -5.), -Vec3::X),
            (vec3(0., 0., -0.4), -Vec3::Z)] {
            let bounds = [(center - Vec3::splat(0.05), center + Vec3::splat(0.05))];
            let shell = center + normal * 8.2;
            assert!(outside_skin_bounds(true, &bounds, &pose, &[], Mat3::IDENTITY, Vec3::ZERO, 1.));
            assert!(!crate::render::outside_matrix(projection, shell, shell));
            let _overlay = CullGuard::preserve(true);
            let (cull, flat) = visual_options(true, true, &[]);
            assert!(!flat);
            assert!(!outside_skin_bounds(cull, &bounds, &pose, &[], Mat3::IDENTITY, Vec3::ZERO, 1.));
        }
    }
    #[test]
    fn lean_overlay_keeps_reference_normals_and_shell_expansion() {
        let mesh = || Mesh {
            vertices: [Vec3::ZERO, Vec3::X, Vec3::Y].map(|position| Vertex {
                position, uv: Vec2::ZERO, normal: Vec4::ZERO, color: [255; 4],
            }).to_vec(),
            indices: vec![0, 1, 2], texture: None,
        };
        let mut reference = mesh();
        shade(&mut reference, false, false, 0.7);
        let mut lean = mesh();
        shade(&mut lean, true, false, 0.7);
        assert!(lean.vertices.iter().all(|v| v.normal == Vec4::ZERO));
        let _overlay = CullGuard::preserve(true);
        let mut protected = mesh();
        let (_, flat) = visual_options(true, true, &[]);
        shade(&mut protected, flat, false, 0.7);
        for (expected, actual) in reference.vertices.iter().zip(&protected.vertices) {
            assert_eq!(actual.normal, expected.normal);
            assert_eq!(actual.color, expected.color);
            let shell = actual.position - actual.normal.truncate().normalize_or_zero() * 8.2;
            assert!((shell - actual.position).length() > 8.);
        }
    }
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
                    assert_eq!(world_bounds_from_pose(&bounds, &pose, &scales,
                        rotation(world.rotation), world.translation, scale), Some((lo, hi)));
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
