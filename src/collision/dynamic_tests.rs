use super::*;

fn bits(t: Trace) -> [u32; 6] {
    [
        t.fraction.to_bits(),
        t.normal.x.to_bits(),
        t.normal.y.to_bits(),
        t.normal.z.to_bits(),
        u32::from(t.start_solid),
        u32::from(t.all_solid),
    ]
}

fn colliders(phase: f32) -> Vec<Collider> {
    let map = crate::interaction::tests::fixture();
    let shape = model_shape::Template::brush_model(&map, 1).unwrap();
    let planes = World::box_hulls(&[(vec3(-35., -17., -9.), vec3(35., 17., 9.))])
        .remove(0)
        .planes;
    (0..25)
        .map(|i| {
            let origin = vec3(
                (i % 5) as f32 * 200. - 400. + phase,
                (i / 5) as f32 * 200. - 400.,
                100.,
            );
            let rotation =
                Quat::from_rotation_y(i as f32 * 0.13) * Quat::from_rotation_z(i as f32 * 0.0001);
            match i % 5 {
                0 => shape.at(origin, rotation), // Lazy geometry and conservative bounds.
                1 => shape.at_eager(origin, rotation),
                2 => {
                    Collider::from_hulls(
                        vec![Hull::transformed(&planes, origin, rotation).unwrap()],
                    )
                }
                3 => Collider::from_hulls(
                    World::box_hulls(&[
                        (origin - Vec3::splat(10.), origin + Vec3::splat(10.)),
                        (origin + vec3(40., -10., -10.), origin + vec3(60., 10., 10.)),
                    ]),
                ),
                _ => Collider::from_hulls(Vec::new()),
            }
        })
        .collect()
}

#[test]
fn dynamic_broad_phase_is_bitwise_exact_after_publish_move_reorder_copy_and_clear() {
    let boxes = [
        (vec3(-2000., -2000., -30.), vec3(2000., 2000., 0.)),
        (vec3(-50., -1000., 0.), vec3(-40., 1000., 500.)),
    ];
    let mut world = World::fixture(&boxes);
    let mut comparisons = 0;
    for state in 0..6 {
        match state {
            0 | 5 => world.set_dynamic(Vec::new()),
            1 => world.set_dynamic(colliders(0.)),
            2 => world.set_dynamic(colliders(1000.)),
            3 => {
                let mut list = colliders(-1000.);
                list.reverse();
                world.set_dynamic(list);
            }
            4 => {
                let mut other = World::fixture(&[]);
                other.set_dynamic(colliders(20.));
                world.copy_dynamic_from(&other);
            }
            _ => unreachable!(),
        }
        assert_eq!(world.dynamic_bounds.is_some(), !world.dynamic.is_empty());
        for half in [Vec3::ZERO, Vec3::ONE, PLAYER_HALF] {
            for x in -8..=8 {
                for y in -8..=8 {
                    for z in [-100., 0., 100., 400., 2000.] {
                        let center = vec3(x as f32 * 150., y as f32 * 150., z);
                        for delta in [
                            Vec3::ZERO,
                            Vec3::Z * 400.,
                            Vec3::X * 400.,
                            vec3(2000., -1800., 800.),
                        ] {
                            let start = center + delta;
                            let end = center - delta;
                            assert_eq!(
                                bits(world.sweep_geometry_legacy(start, end, half)),
                                bits(world.sweep_geometry(start, end, half)),
                                "state={state}, start={start:?}, end={end:?}, half={half:?}"
                            );
                            comparisons += 1;
                        }
                    }
                }
            }
        }
        let mut reference = World::fixture(&boxes);
        reference.copy_dynamic_from(&world);
        reference.legacy_dynamic_sweeps = true;
        let camera = Collider::fixture(vec3(30., -20., 20.), vec3(35., 20., 120.));
        world.set_camera_obstacles(vec![camera.clone()]);
        reference.set_camera_obstacles(vec![camera]);
        world.weapon_obstacles = vec![(
            7,
            Collider::fixture(vec3(-20., -20., 60.), vec3(20., 20., 65.)),
        )];
        reference.weapon_obstacles = world.weapon_obstacles.clone();
        for (start, end) in [
            (vec3(0., 0., 200.), Vec3::ZERO),
            (vec3(-80., 0., 40.), vec3(80., 0., 40.)),
            (vec3(0., 0., 62.), vec3(0., 0., 62.)),
        ] {
            for half in [Vec3::ZERO, PLAYER_HALF] {
                for ignore in [None, Some(7), Some(8)] {
                    assert_eq!(
                        bits(world.sweep_except(start, end, half, ignore)),
                        bits(reference.sweep_except(start, end, half, ignore))
                    );
                }
                assert_eq!(
                    bits(world.camera_sweep(start, end, half)),
                    bits(reference.camera_sweep(start, end, half))
                );
            }
        }
    }
    assert_eq!(comparisons, 104_040);
}

#[test]
fn dynamic_aabb_rejection_preserves_skin_faces_corners_and_solid_flags() {
    for origin in [Vec3::ZERO, vec3(-4188., 6004., 32.03125)] {
        let mut world = World::fixture(&[]);
        let min = origin - Vec3::splat(10.);
        let max = origin + Vec3::splat(10.);
        world.set_dynamic(vec![
            Collider::fixture(min, max),
            Collider::fixture(min, max),
        ]);
        for half in [Vec3::ZERO, Vec3::ONE, PLAYER_HALF] {
            let padding = half + Vec3::splat(SKIN);
            for axis in 0..3 {
                for distance in [0., -SKIN, SKIN, 0.000001, -0.000001] {
                    let mut start = origin;
                    start[axis] = min[axis] - padding[axis] + distance;
                    for end in [start, origin, max + padding, min - padding] {
                        assert_eq!(
                            bits(world.sweep_geometry_legacy(start, end, half)),
                            bits(world.sweep_geometry(start, end, half))
                        );
                    }
                }
            }
            for (start, end) in [
                (min - padding, max + padding),
                (origin, origin),
                (origin, max + padding),
                (max + padding, max + padding),
            ] {
                assert_eq!(
                    bits(world.sweep_geometry_legacy(start, end, half)),
                    bits(world.sweep_geometry(start, end, half))
                );
            }
        }
        world.set_dynamic(vec![Collider::from_hulls(Vec::new())]);
        assert!(world.dynamic_bounds.is_none());
        assert_eq!(
            bits(world.sweep_geometry_legacy(origin, origin, PLAYER_HALF)),
            bits(world.sweep_geometry(origin, origin, PLAYER_HALF))
        );
    }
}
