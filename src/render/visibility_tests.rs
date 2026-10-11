//! Compare production selection with the uncached selector from 6f036e1.
//! No GPU, PK3, timer, or screen-size dependency in these fixtures.
use super::*;

#[derive(Clone, Copy, Default)]
struct View {
    matrix: Option<Mat4>,
}
impl View {
    fn outside(self, min: Vec3, max: Vec3) -> bool {
        if let Some(matrix) = self.matrix {
            crate::render_fx::set_projection(matrix);
            outside_bounds(min, max)
        } else {
            false
        }
    }
    fn error(self, center: Vec3, radius: f32, error: f32) -> f32 {
        self.matrix.map_or(f32::INFINITY, |m| {
            crate::render_fx::projected_error_for(m, 544., center, radius, error)
        })
    }
}
fn batch(faces: &[usize]) -> Batch {
    let parts: Vec<_> = faces
        .iter()
        .enumerate()
        .map(|(i, &face)| {
            let center = vec3(if face == 0 { 3. } else { -3. }, 0., 0.);
            Part {
                face,
                indices: i * 3..i * 3 + 3,
                min: center - Vec3::splat(0.25),
                max: center + Vec3::splat(0.25),
            }
        })
        .collect();
    let full_indices = vec![4, 1, 3, 7, 2, 5, 9, 6, 8];
    let full_indices = if parts.is_empty() {
        full_indices
    } else {
        full_indices[..parts.len() * 3].to_vec()
    };
    let mut b = Batch {
        #[cfg(target_os = "vita")]
        static_offset: None,
        face_side: crate::materials::FaceSide::Both,
        motion: None,
        mesh: Mesh {
            vertices: Vec::new(),
            indices: full_indices.clone(),
            texture: None,
        },
        full_indices,
        parts,
        visibility_work: VisibilityWork::default(),
        bounds: (vec3(-4., -1., -1.), vec3(4., 1., 1.)),
        detail: Vec::new(),
        animation: None,
        base: Vec::new(),
        normals: Vec::new(),
        deforms: Vec::new(),
        sky: false,
        portal: false,
        center: Vec3::ZERO,
        model: None,
        // Transparency policy only needs the stage; no texture is created.
        layers: vec![Layer {
            spec: Stage::default(),
            textures: Vec::new(),
            material: 0,
            lit: true,
        }],
    };
    b.prepare_visibility();
    b
}

// Intentionally retain the original intertwined PVS/frustum/copy algorithm.
fn legacy(
    b: &Batch,
    transform: Option<&(usize, Vec3, Quat)>,
    offset: Vec3,
    backdrop: bool,
    c: &mut DrawContext<'_>,
    view: View,
) -> Option<Vec<u16>> {
    let (min, max) = transform_bounds(b.bounds.0, b.bounds.1, transform, offset);
    let can_cull = (c.enabled || c.required_pvs)
        && !b
            .deforms
            .iter()
            .any(crate::materials::Deform::moves_vertices)
        && b.motion.is_none();
    if can_cull && c.enabled && view.outside(min, max) {
        return None;
    }
    let mut indices = Vec::new();
    if can_cull && !b.parts.is_empty() {
        for p in &b.parts {
            if c.faces.as_ref().is_some_and(|v| !v[p.face]) {
                continue;
            }
            let (lo, hi) = transform_bounds(p.min, p.max, transform, offset);
            if !c.enabled || !view.outside(lo, hi) {
                indices.extend_from_slice(&b.full_indices[p.indices.clone()]);
            }
        }
    } else {
        if can_cull
            && b.model.is_none()
            && b.animation.is_none()
            && b.parts.is_empty()
            && !backdrop
            && !c.visibility.bounds(c.cluster, min, max)
        {
            return None;
        }
        indices.extend_from_slice(&b.full_indices);
    }
    if indices.is_empty() {
        return None;
    }
    if can_cull && c.enabled && !backdrop && b.parts.is_empty() {
        let center = (min + max) * 0.5;
        let radius = (max - min).length() * 0.5;
        if let Some(level) = b
            .detail
            .iter()
            .rev()
            .find(|l| view.error(center, radius, l.error) <= 0.35)
        {
            c.stats.detail_saved += (indices.len() - level.indices.len()) / 3;
            indices.clone_from(&level.indices);
        }
    }
    Some(indices)
}

#[allow(clippy::too_many_arguments)]
fn compare(
    b: &mut Batch,
    cache: &mut FaceMaskCache,
    visibility: &crate::visibility::Visibility,
    cluster: Option<usize>,
    enabled: bool,
    required_pvs: bool,
    view: View,
    transform: Option<&(usize, Vec3, Quat)>,
    offset: Vec3,
    backdrop: bool,
) -> (Option<Vec<u16>>, Option<IndexSelection>) {
    let faces = cache.faces(visibility, cluster);
    let mut old = DrawContext {
        enabled,
        required_pvs,
        faces: faces.clone(),
        cluster,
        visibility,
        stats: DrawStats::default(),
    };
    let mut new = DrawContext {
        enabled,
        required_pvs,
        faces,
        cluster,
        visibility,
        stats: DrawStats::default(),
    };
    old.stats.batches += 1;
    new.stats.batches += 1;
    let expected = legacy(b, transform, offset, backdrop, &mut old, view);
    let source = b.full_indices.clone();
    let source_ptr = b.full_indices.as_ptr();
    let selection = select_batch_indices_for(
        b,
        transform,
        offset,
        backdrop,
        &mut new,
        |lo, hi| view.outside(lo, hi),
        |center, radius, error| view.error(center, radius, error),
    );
    let actual = selection.map(|selection| {
        let read_only = b.indices_read_only(false);
        let lease = IndexLease::new(b, selection, read_only);
        lease.mesh.indices.clone()
    });
    assert_eq!(
        actual, expected,
        "cluster {cluster:?}, optimize={enabled}, required={required_pvs}"
    );
    for (c, result) in [(&mut old, &expected), (&mut new, &actual)] {
        if let Some(indices) = result {
            c.stats.triangles += indices.len() / 3 * b.layers.len();
        } else {
            c.stats.culled += 1;
        }
    }
    assert_eq!(
        (
            new.stats.batches,
            new.stats.culled,
            new.stats.triangles,
            new.stats.detail_saved
        ),
        (
            old.stats.batches,
            old.stats.culled,
            old.stats.triangles,
            old.stats.detail_saved
        )
    );
    assert_eq!(b.full_indices, source);
    assert_eq!(b.full_indices.as_ptr(), source_ptr);
    (actual, selection)
}
fn run(
    b: &mut Batch,
    cache: &mut FaceMaskCache,
    v: &crate::visibility::Visibility,
    cluster: Option<usize>,
    view: View,
) -> (Option<Vec<u16>>, Option<IndexSelection>) {
    compare(
        b,
        cache,
        v,
        cluster,
        true,
        false,
        view,
        None,
        Vec3::ZERO,
        false,
    )
}

#[test]
fn cold_and_warm_all_empty_and_some_parts_preserve_exact_source_order() {
    let v = crate::visibility::tests::two_rooms();
    for (faces, cluster, expected_kind) in [
        (vec![0, 0, 0], Some(0), Some(IndexSelection::Full)),
        (vec![1, 1, 1], Some(0), None),
        (vec![0, 1, 0], Some(0), Some(IndexSelection::Filtered)),
    ] {
        let mut b = batch(&faces);
        let mut cache = FaceMaskCache::default();
        for _ in 0..3 {
            let (_, selection) = run(&mut b, &mut cache, &v, cluster, View::default());
            assert_eq!(selection, expected_kind);
        }
    }
}

#[test]
fn portal_clusters_a_b_a_and_none_rebuild_only_pvs_candidates() {
    let v = crate::visibility::tests::two_rooms();
    let mut b = batch(&[0, 1, 0]);
    let mut cache = FaceMaskCache::default();
    for cluster in [
        Some(0),
        Some(0),
        Some(1),
        Some(1),
        Some(0),
        None,
        None,
        Some(0),
    ] {
        run(&mut b, &mut cache, &v, cluster, View::default());
        assert_eq!(b.visibility_work.pvs.as_ref().unwrap().cluster, cluster);
    }
    let mut unknown = batch(&[0, 1, 0]);
    assert!(unknown.visibility_work.pvs.is_none());
    let (_, selection) = run(&mut unknown, &mut cache, &v, None, View::default());
    assert_eq!(selection, Some(IndexSelection::Full));
    assert_eq!(unknown.visibility_work.pvs.as_ref().unwrap().cluster, None);
}

#[test]
fn camera_rotation_in_one_cluster_retests_global_and_part_frustums() {
    let v = crate::visibility::tests::two_rooms();
    let mut b = batch(&[0, 0, 0]);
    b.parts[1].min.x = -3.25;
    b.parts[1].max.x = -2.75;
    b.prepare_visibility();
    let mut cache = FaceMaskCache::default();
    let p = Mat4::perspective_rh_gl(1., 1., 0.1, 100.);
    let mut outputs = Vec::new();
    for direction in [Vec3::X, -Vec3::X, Vec3::X, Vec3::Y] {
        let view = View {
            matrix: Some(p * Mat4::look_at_rh(Vec3::ZERO, direction, Vec3::Z)),
        };
        outputs.push(run(&mut b, &mut cache, &v, Some(0), view).0);
    }
    assert_ne!(outputs[0], outputs[1]);
    assert_eq!(outputs[0], outputs[2]);
    assert_eq!(outputs[3], None);
    let offscreen = View {
        matrix: Some(p * Mat4::look_at_rh(Vec3::Z * 50., Vec3::Z * 51., Vec3::Y)),
    };
    assert_eq!(run(&mut b, &mut cache, &v, Some(1), offscreen).0, None);
    // A rejected global AABB must not consult or replace the PVS entry.
    assert_eq!(b.visibility_work.pvs.as_ref().unwrap().cluster, Some(0));
}

#[test]
fn required_pvs_with_optimize_disabled_and_toggle_does_not_poison_cache() {
    let v = crate::visibility::tests::two_rooms();
    let mut b = batch(&[0, 1, 0]);
    let mut cache = FaceMaskCache::default();
    for (enabled, required) in [(false, true), (false, false), (true, false), (false, true)] {
        let (indices, _) = compare(
            &mut b,
            &mut cache,
            &v,
            Some(0),
            enabled,
            required,
            View::default(),
            None,
            Vec3::ZERO,
            false,
        );
        assert_eq!(
            indices.unwrap().len(),
            if enabled || required { 6 } else { 9 }
        );
    }
    let mut fixed = batch(&[]);
    fixed.bounds = (vec3(-9., -1., -1.), vec3(-2., 1., 1.));
    assert_eq!(
        compare(
            &mut fixed,
            &mut cache,
            &v,
            Some(0),
            false,
            true,
            View::default(),
            None,
            Vec3::ZERO,
            false
        )
        .0,
        None
    );
}

#[test]
fn fixed_bounds_cache_handles_cluster_changes_unknown_and_changed_bounds() {
    let v = crate::visibility::tests::two_rooms();
    let mut b = batch(&[]);
    b.bounds = (vec3(-9., -1., -1.), vec3(-2., 1., 1.));
    let mut cache = FaceMaskCache::default();
    for cluster in [Some(0), Some(0), Some(1), Some(0), None, None] {
        run(&mut b, &mut cache, &v, cluster, View::default());
    }
    b.bounds = (vec3(2., -1., -1.), vec3(9., 1., 1.));
    assert!(run(&mut b, &mut cache, &v, Some(0), View::default())
        .0
        .is_some());
    // Deliberately change bounds without explicit invalidation: the snapshot
    // still prevents a stale fixed-prop result from being reused.
    b.bounds = (vec3(-9., -1., -1.), vec3(-2., 1., 1.));
    assert_eq!(
        run(&mut b, &mut cache, &v, Some(0), View::default()).0,
        None
    );
    b.bounds = (Vec3::splat(50.), Vec3::splat(51.));
    assert!(run(&mut b, &mut cache, &v, Some(0), View::default())
        .0
        .is_some());
}

#[test]
fn transformed_fixed_pose_and_sky_parts_use_current_view_without_stale_bounds() {
    let v = crate::visibility::tests::two_rooms();
    let mut cache = FaceMaskCache::default();
    let mut b = batch(&[]);
    b.bounds = (vec3(2., -1., -1.), vec3(9., 1., 1.));
    run(&mut b, &mut cache, &v, Some(0), View::default());
    let transform = (0, Vec3::ZERO, Quat::from_rotation_z(std::f32::consts::PI));
    assert_eq!(
        compare(
            &mut b,
            &mut cache,
            &v,
            Some(0),
            true,
            false,
            View::default(),
            Some(&transform),
            Vec3::ZERO,
            false
        )
        .0,
        None
    );
    assert_eq!(
        compare(
            &mut b,
            &mut cache,
            &v,
            Some(0),
            true,
            false,
            View::default(),
            None,
            Vec3::X * -12.,
            false
        )
        .0,
        None
    );
    assert!(run(&mut b, &mut cache, &v, Some(0), View::default())
        .0
        .is_some());
    let mut sky = batch(&[0, 1, 0]);
    for cluster in [Some(0), Some(1), Some(0)] {
        compare(
            &mut sky,
            &mut cache,
            &v,
            cluster,
            true,
            false,
            View::default(),
            Some(&transform),
            Vec3::X * 4.,
            true,
        );
    }
}

#[test]
fn lod_near_far_and_return_preserve_indices_and_detail_saved() {
    let v = crate::visibility::tests::two_rooms();
    let mut b = batch(&[]);
    b.bounds = (vec3(2., -0.25, -0.25), vec3(2.5, 0.25, 0.25));
    b.detail = vec![crate::model_detail::Level {
        indices: vec![9, 6, 8],
        error: 0.1,
    }];
    let mut cache = FaceMaskCache::default();
    let p = Mat4::perspective_rh_gl(1., 1., 0.1, 1000.);
    for (eye, expected) in [
        (Vec3::ZERO, IndexSelection::Full),
        (Vec3::X * -200., IndexSelection::Detail(0)),
        (Vec3::ZERO, IndexSelection::Full),
    ] {
        let view = View {
            matrix: Some(p * Mat4::look_at_rh(eye, eye + Vec3::X, Vec3::Z)),
        };
        let (_, selection) = run(&mut b, &mut cache, &v, Some(0), view);
        assert_eq!(selection, Some(expected));
    }
}

#[test]
fn motions_deformations_animations_inline_models_and_water_keep_original_guards() {
    let v = crate::visibility::tests::two_rooms();
    let mut cache = FaceMaskCache::default();
    for mode in 0..5 {
        let mut b = batch(&[]);
        b.bounds = (vec3(-9., -1., -1.), vec3(-2., 1., 1.));
        match mode {
            0 => b.motion = Some(crate::sky_sequence::Motion::default()),
            1 => b.deforms.push(crate::materials::Deform::Bulge([1.; 3])),
            2 => b.model = Some(1),
            3 => {
                b.animation = Some(crate::ambient_animation::Frames {
                    positions: Rc::new(Vec::new()),
                    frame_time: 0.1,
                    scale: 1.,
                    transform: crate::skeletal::Transform {
                        rotation: Quat::IDENTITY,
                        translation: Vec3::ZERO,
                    },
                })
            }
            _ => b.deforms.push(crate::materials::Deform::WaveNormal(
                1.,
                Vec3::X,
                crate::materials::Wave {
                    kind: "sin".into(),
                    values: [0., 1., 0., 1.],
                },
            )),
        }
        for _ in 0..2 {
            let (indices, _) = run(&mut b, &mut cache, &v, Some(0), View::default());
            assert_eq!(indices.is_some(), mode != 4);
        }
        if mode != 4 {
            assert!(b.visibility_work.pvs.is_none());
        }
    }
    // Displacing deforms and motion must also bypass the global frustum.
    for motion in [false, true] {
        let mut b = batch(&[1, 1]);
        if motion {
            b.motion = Some(crate::sky_sequence::Motion::default());
        } else {
            b.deforms.push(crate::materials::Deform::Sprite(false));
        }
        let view = View {
            matrix: Some(Mat4::IDENTITY),
        };
        assert!(run(&mut b, &mut cache, &v, Some(0), view).0.is_some());
    }
}

#[test]
fn full_vec_exchange_restores_sources_after_return_and_unwind() {
    let mut b = batch(&[]);
    let source = b.full_indices.clone();
    let source_ptr = b.full_indices.as_ptr();
    let scratch_ptr = b.mesh.indices.as_ptr();
    let draw = |b: &mut Batch| {
        let lease = IndexLease::new(b, IndexSelection::Full, true);
        assert_eq!(lease.mesh.indices.as_ptr(), source_ptr);
        assert_eq!(lease.full_indices.as_ptr(), scratch_ptr);
        // Scope exit models an early return from submission.
    };
    draw(&mut b);
    assert_eq!(b.full_indices.as_ptr(), source_ptr);
    assert_eq!(b.mesh.indices.as_ptr(), scratch_ptr);
    let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _lease = IndexLease::new(&mut b, IndexSelection::Full, true);
        panic!("synthetic submission failure");
    }));
    assert!(panic.is_err());
    assert_eq!(b.full_indices, source);
    assert_eq!(b.full_indices.as_ptr(), source_ptr);
    assert_eq!(b.mesh.indices.as_ptr(), scratch_ptr);
}

#[test]
fn sided_and_legacy_transparent_batches_mutate_only_reusable_scratch() {
    let mut b = batch(&[]);
    for (side, blend, active, read_only) in [
        (crate::materials::FaceSide::Both, Blend::Opaque, false, true),
        (
            crate::materials::FaceSide::Front,
            Blend::Opaque,
            false,
            false,
        ),
        (crate::materials::FaceSide::Back, Blend::Alpha, true, false),
        (crate::materials::FaceSide::Both, Blend::Alpha, false, false),
        (crate::materials::FaceSide::Both, Blend::Alpha, true, true),
    ] {
        b.face_side = side;
        b.layers[0].spec.blend = blend;
        assert_eq!(b.indices_read_only(active), read_only);
        let source = b.full_indices.clone();
        {
            let mut lease = IndexLease::new(&mut b, IndexSelection::Full, read_only);
            if !read_only {
                lease.mesh.indices.reverse();
                lease.mesh.indices.truncate(3);
            }
        }
        assert_eq!(b.full_indices, source);
    }
}

#[test]
fn cluster_replacements_reuse_part_and_index_capacities() {
    let v = crate::visibility::tests::two_rooms();
    let mut b = batch(&[0, 1, 0]);
    let mut cache = FaceMaskCache::default();
    let parts_ptr = b.visibility_work.pvs_parts.as_ptr();
    let visible_ptr = b.visibility_work.visible_parts.as_ptr();
    let indices_ptr = b.mesh.indices.as_ptr();
    for _ in 0..8 {
        for cluster in [Some(0), Some(1), None, Some(0)] {
            run(&mut b, &mut cache, &v, cluster, View::default());
            let pvs_ptr = match &b.visibility_work.pvs.as_ref().unwrap().result {
                PvsResult::Parts(PvsParts::SomeParts(parts)) => parts.as_ptr(),
                _ => b.visibility_work.pvs_parts.as_ptr(),
            };
            assert_eq!(pvs_ptr, parts_ptr);
            assert_eq!(b.visibility_work.visible_parts.as_ptr(), visible_ptr);
            assert_eq!(b.mesh.indices.as_ptr(), indices_ptr);
        }
    }
}

#[test]
fn face_mask_is_shared_by_contexts_and_new_scenes_start_empty() {
    let v = crate::visibility::tests::two_rooms();
    let mut cache = FaceMaskCache::default();
    let a = cache.faces(&v, Some(0)).unwrap();
    let warm = cache.faces(&v, Some(0)).unwrap();
    assert!(Rc::ptr_eq(&a, &warm));
    let b = cache.faces(&v, Some(1)).unwrap();
    assert_eq!(&*b, &[false, true]);
    assert_eq!(&*a, &[true, false]);
    let again = cache.faces(&v, Some(0)).unwrap();
    assert_eq!(&*a, &*again);
    assert!(!Rc::ptr_eq(&a, &again));
    assert!(cache.faces(&v, None).is_none());
    assert!(cache.faces(&v, None).is_none());
    let mut new_scene = FaceMaskCache::default();
    assert!(new_scene.entry.is_none());
    assert!(new_scene
        .faces(&crate::visibility::Visibility::default(), Some(0))
        .is_none());
}

#[test]
fn rebuilding_parts_or_source_indices_invalidates_membership_and_full_detection() {
    let v = crate::visibility::tests::two_rooms();
    let mut b = batch(&[0, 1, 0]);
    let mut cache = FaceMaskCache::default();
    run(&mut b, &mut cache, &v, Some(0), View::default());
    b.parts[1].face = 0;
    b.full_indices[3..6].copy_from_slice(&[8, 6, 9]);
    b.prepare_visibility();
    assert!(b.visibility_work.pvs.is_none());
    assert_eq!(
        run(&mut b, &mut cache, &v, Some(0), View::default()).1,
        Some(IndexSelection::Full)
    );
    // Non-contiguous or reordered ranges must never take the Full fast path.
    b.parts.swap(0, 2);
    b.prepare_visibility();
    assert_eq!(
        run(&mut b, &mut cache, &v, Some(0), View::default()).1,
        Some(IndexSelection::Filtered)
    );
}
