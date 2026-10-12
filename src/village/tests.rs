use super::*;
use crate::collision::Trace;

fn fixture() -> (Bsp, Village) {
    let mut map = crate::interaction::tests::fixture();
    for axis in 0..3 {
        let half = [48., 48., 4.][axis];
        map.planes[6 + axis * 2].distance = half;
        map.planes[7 + axis * 2].distance = half;
    }
    let objects = [
        ("cam_piston", vec3(0., 0., 100.), 0.1),
        ("spinrod1", vec3(1000., 0., 100.), 0.1),
        ("spinrod1_arm", vec3(1000., 200., 100.), 1.4),
        ("openthis", vec3(0., 300., 100.), 0.),
        ("cam_wheel", vec3(1000., 500., 100.), 0.1),
    ]
    .into_iter()
    .map(|(name, base, time)| {
        let pose = sample(name, base, time, 0.);
        let shape = Template::brush_model(&map, 1).unwrap();
        let collider = shape.at_eager(pose.origin, pose.rotation());
        Object {
            name: name.into(),
            model: 1,
            base,
            pose,
            time,
            solid: !name.starts_with("spinrod"),
            shape,
            collider,
        }
    })
    .collect();
    let village = Village {
        placement_tick: 0,
        cinema: cinema::Cinema::fixture(),
        objects,
        emitters: Vec::new(),
        hatch_open: false,
        hatch_time: 0.,
    };
    (map, village)
}

fn trace_bits(t: Trace) -> [u32; 6] {
    [
        t.fraction.to_bits(),
        t.normal.x.to_bits(),
        t.normal.y.to_bits(),
        t.normal.z.to_bits(),
        u32::from(t.start_solid),
        u32::from(t.all_solid),
    ]
}

#[test]
fn nonsolid_poses_preserve_publications_contacts_transport_and_restore() {
    let mut transports = 0;
    let mut rejected = 0;
    let mut omitted = 0;
    for scenario in 0..3 {
        let (map, mut old) = fixture();
        let (_, mut new) = fixture();
        let initial = old.snapshot();
        let feet = if scenario == 2 {
            vec3(-200., 0., 80.03125)
        } else {
            vec3(0., 0., 80.03125)
        };
        let mut a = Player::new(feet);
        let mut b = a.clone();
        let fixed = if scenario == 1 {
            vec![Collider::fixture(
                vec3(-100., -100., 60.),
                vec3(100., 100., 80.),
            )]
        } else {
            Vec::new()
        };
        let mut wa = World::fixture(&[]);
        let mut wb = World::fixture(&[]);
        for tick in 0..180 {
            if tick == 10 || tick == 100 {
                assert!(old.event("openthis").is_some());
                assert!(new.event("openthis").is_some());
            }
            if tick == 80 {
                // A stale open-hatch collider must be rebuilt before becoming solid.
                old.restore(&initial, &map).unwrap();
                new.restore(&initial, &map).unwrap();
                a = Player::new(feet);
                b = a.clone();
            }
            wa.dynamic_publications = Some(Vec::new());
            wb.dynamic_publications = Some(Vec::new());
            let dt = [1. / 120., 0.05, 0.002, 0.][tick % 4];
            let before_feet = a.feet;
            let before_time = old.objects[0].time;
            old.advance_legacy(dt, &map, &mut wa, &mut a, &fixed)
                .unwrap();
            new.advance(dt, &map, &mut wb, &mut b, &fixed).unwrap();
            transports += usize::from(a.feet != before_feet);
            rejected += usize::from(dt > 0. && old.objects[0].time == before_time);
            assert_eq!(
                serde_json::to_value(&a).unwrap(),
                serde_json::to_value(&b).unwrap()
            );
            assert_eq!(
                serde_json::to_value(old.snapshot()).unwrap(),
                serde_json::to_value(new.snapshot()).unwrap()
            );
            let legacy = wa.dynamic_publications.take().unwrap();
            let optimized = wb.dynamic_publications.take().unwrap();
            let full_len = fixed.len() + old.colliders().count();
            // Non-solid visits publish all solids but do no world queries. Only
            // those unused calls disappear; every solid-visit and final list is exact.
            let meaningful = legacy
                .iter()
                .enumerate()
                .filter(|(i, list)| *i + 1 == legacy.len() || list.len() < full_len)
                .map(|(_, list)| list)
                .collect::<Vec<_>>();
            omitted += legacy.len() - meaningful.len();
            assert_eq!(meaningful.len(), optimized.len());
            for (left, right) in meaningful.into_iter().zip(&optimized) {
                assert_eq!(
                    left.iter().map(Collider::geometry_bits).collect::<Vec<_>>(),
                    right
                        .iter()
                        .map(Collider::geometry_bits)
                        .collect::<Vec<_>>()
                );
            }
            for (left, right) in old.colliders().zip(new.colliders()) {
                assert_eq!(left.geometry_bits(), right.geometry_bits());
            }
            for center in [
                a.feet + PLAYER_CENTER,
                vec3(0., 300., 100.),
                vec3(1000., 0., 100.),
            ] {
                for half in [Vec3::ZERO, PLAYER_HALF] {
                    for delta in [Vec3::ZERO, Vec3::Z * 150., Vec3::X * 150.] {
                        assert_eq!(
                            trace_bits(wa.sweep(center + delta, center - delta, half)),
                            trace_bits(wb.sweep(center + delta, center - delta, half))
                        );
                    }
                }
            }
        }
    }
    assert!(transports > 0 && rejected > 0 && omitted > 0);
}

/// Same rider, obstruction, hatch and restore sequences as the sim8 oracle.
/// Serialize the actual saved player/village components after every step.
pub(crate) fn assert_config_contacts_and_save(config: crate::vita_sim::Config) {
    let mut transported = 0;
    for scenario in 0..3 {
        let (map, mut old) = fixture();
        let (_, mut new) = fixture();
        let initial = old.snapshot();
        let feet = if scenario == 2 { vec3(-200., 0., 80.03125) }
            else { vec3(0., 0., 80.03125) };
        let mut a = Player::new(feet);
        let mut b = a.clone();
        let fixed = if scenario == 1 {
            vec![Collider::fixture(vec3(-100., -100., 60.),vec3(100., 100., 80.))]
        } else { Vec::new() };
        let mut wa = World::fixture(&[]);
        let mut wb = World::fixture(&[]);
        let mut steam = crate::particles::tests::world_test_steam();
        for tick in 0..180 {
            if tick == 10 || tick == 100 {
                old.event("openthis"); new.event("openthis");
            }
            if tick == 80 {
                old.restore(&initial, &map).unwrap();
                new.restore(&initial, &map).unwrap();
                a = Player::new(feet); b = a.clone();
            }
            let dt = [1. / 120., 0.05, 0.002, 0.][tick % 4];
            let before = a.feet;
            old.advance_legacy(dt, &map, &mut wa, &mut a, &fixed).unwrap();
            new.advance_config(dt, &map, &mut wb, &mut b, &fixed, &config).unwrap();
            steam.update_clocks_config(0.05, dt, b.feet, &wb, &config);
            transported += usize::from(before != a.feet);
            assert_eq!(crate::save::simulation_save_bytes(&a,old.snapshot()),
                crate::save::simulation_save_bytes(&b,new.snapshot()), "scenario {scenario}, tick {tick}");
            for center in [a.feet + PLAYER_CENTER,vec3(0.,300.,100.),vec3(1000.,0.,100.)] {
                for half in [Vec3::ZERO,PLAYER_HALF] {
                    for delta in [Vec3::ZERO,Vec3::Z * 150.,Vec3::X * 150.] {
                        assert_eq!(trace_bits(wa.sweep(center+delta,center-delta,half)),
                            trace_bits(wb.sweep(center+delta,center-delta,half)));
                    }
                }
            }
        }
    }
    assert!(transported > 0);
}
#[test]
fn puff_cap_preserves_player_contacts_transport_and_save_bytes() {
    assert_config_contacts_and_save(crate::vita_sim::Config { puff_cap: 8, ..Default::default() });
}
#[test]
fn puff_collision_modes_preserve_player_contacts_transport_and_save_bytes() {
    for mode in [crate::vita_sim::PuffCollide::Moving,crate::vita_sim::PuffCollide::None] {
        assert_config_contacts_and_save(crate::vita_sim::Config { puff_collide: mode, ..Default::default() });
    }
}
#[test]
fn puff_cadence_preserves_player_contacts_transport_and_save_bytes() {
    assert_config_contacts_and_save(crate::vita_sim::Config { puff_sweep_every: 4, ..Default::default() });
}

#[test]
fn mover_cadences_preserve_player_contacts_transport_and_save_bytes() {
    for every in [2,4,16,120] {
        assert_config_contacts_and_save(crate::vita_sim::Config { mover_far_every: every, ..Default::default() });
    }
}
#[test]
fn deferred_colliders_keep_current_pose_and_resume_exactly_on_range_entry() {
    let (map,mut old) = fixture();
    let (_,mut new) = fixture();
    let mut a = Player::new(Vec3::splat(5000.));
    let mut b = a.clone();
    let mut wa = World::fixture(&[]);
    let mut wb = World::fixture(&[]);
    let config = crate::vita_sim::Config { mover_far_every: 120,..Default::default() };
    old.advance_legacy(1./120.,&map,&mut wa,&mut a,&[]).unwrap();
    new.advance_config(1./120.,&map,&mut wb,&mut b,&[],&config).unwrap();
    assert!(new.objects[0].collider.placement_pending());
    assert_eq!(serde_json::to_vec(&old.snapshot()).unwrap(),serde_json::to_vec(&new.snapshot()).unwrap());
    // A projectile/body query at the far object's CURRENT position builds the
    // exact hull on demand. No earlier pose or delayed movement is published.
    for (left,right) in old.colliders().zip(new.colliders()) {
        for x in [-100.,0.,100.,1000.] {
            assert_eq!(trace_bits(left.trace(vec3(x,0.,200.),vec3(x,0.,0.),PLAYER_HALF)),
                trace_bits(right.trace(vec3(x,0.,200.),vec3(x,0.,0.),PLAYER_HALF)));
        }
    }
    assert!(!new.objects[0].collider.placement_pending());
    a = Player::new(vec3(0.,0.,80.03125)); b = a.clone();
    for _ in 0..120 {
        old.advance_legacy(1./120.,&map,&mut wa,&mut a,&[]).unwrap();
        new.advance_config(1./120.,&map,&mut wb,&mut b,&[],&config).unwrap();
        assert!(!new.objects[0].collider.placement_pending());
        assert_eq!(serde_json::to_vec(&(&a,old.snapshot())).unwrap(),serde_json::to_vec(&(&b,new.snapshot())).unwrap());
    }
}
#[test]
fn all_compromises_preserve_player_contacts_transport_and_save_bytes() {
    assert_config_contacts_and_save(crate::vita_sim::Config {
        puff_cap:128,puff_collide:crate::vita_sim::PuffCollide::None,puff_sweep_every:4,
        anim_cull:true,anim_far_distance:Some(800.),anim_far_every:4,mover_far_every:4,
    });
}
