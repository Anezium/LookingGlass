use super::*;
use crate::{
    collision::{Collider, Liquid as LiquidVolume},
    interaction::Interactions,
    level::{LevelController, ReflectionActor, Slot},
    water::Liquid,
};
use std::any::Any;

struct Controller(usize);

impl LevelController for Controller {
    fn id(&self) -> &'static str {
        "presentation-fixture"
    }

    fn sky_origin(&self) -> Option<Vec3> {
        Some(Vec3::splat(self.0 as f32))
    }

    fn reflection(&self) -> (Vec<(usize, Vec3, Quat)>, Vec<usize>) {
        (
            vec![(self.0, Vec3::X * self.0 as f32, Quat::IDENTITY)],
            vec![self.0 + 10],
        )
    }

    fn reflection_actor(&self) -> Option<ReflectionActor> {
        Some(ReflectionActor {
            pose: Transform {
                translation: Vec3::Y * self.0 as f32,
                rotation: Quat::from_rotation_z(self.0 as f32),
            },
            time: self.0 as f32,
            alpha: 0.5,
        })
    }

    fn scene_fog(&self) -> Option<Vec4> {
        Some(vec4(0.25, 0.5, 0.75, 300. * self.0 as f32))
    }

    fn fog_distance(&self) -> Option<f32> {
        Some(500. * self.0 as f32)
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

fn scene_fixture() -> Scene {
    let mut scene = super::super::visibility_tests::scene_fixture();
    scene.atmosphere.volumes.push(crate::environment::Volume {
        min: Vec3::ZERO,
        max: Vec3::ONE,
        color: Vec4::ONE,
    });
    for mirror in [true, false] {
        scene.camera_portals.push(Portal {
            surface: 0,
            model: 0,
            center: Vec3::ZERO,
            normal: Vec3::Z,
            destination: Vec3::ZERO,
            forward: Vec3::X,
            mesh: Mesh {
                vertices: Vec::new(),
                indices: Vec::new(),
                texture: None,
                upload_identity: None,
                surface: None,
            },
            local: Vec::new(),
            target: None,
            material: None,
            ready: false,
            pupil: None,
            mirror,
            cat: None,
            actor: None,
            extra: vec![(0, Vec3::ZERO, Quat::IDENTITY)],
            hide: vec![0],
        });
    }
    scene
}

fn apply_direct(scene: &mut Scene, world: &World, camera: Vec3, interactions: &Interactions) {
    scene.atmosphere.liquid = Liquid::from_contents(world.liquid_at(camera)).tint();
    interactions.presentation.apply(scene);
    let levels = &interactions.levels;
    if let Some(origin) = levels.iter().find_map(|s| s.ctl.sky_origin()) {
        scene.set_sky_origin(origin);
    }
    let (mut mirror_extra, mut mirror_hide) = (Vec::new(), Vec::new());
    for slot in levels {
        let (extra, hide) = slot.ctl.reflection();
        mirror_extra.extend(extra);
        mirror_hide.extend(hide);
    }
    scene.set_reflection(mirror_extra, mirror_hide);
    scene.set_reflection_actor(levels.iter().find_map(|s| s.ctl.reflection_actor()));
    if let Some(fog) = levels.iter().find_map(|s| s.ctl.scene_fog()) {
        scene.atmosphere.distance = fog;
    }
    if let Some(distance) = levels.iter().find_map(|s| s.ctl.fog_distance()) {
        scene.atmosphere.distance.w = distance;
    }
}

fn assert_same_writes(actual: &Scene, expected: &Scene) {
    assert_eq!(actual.atmosphere.liquid, expected.atmosphere.liquid);
    assert_eq!(actual.atmosphere.distance, expected.atmosphere.distance);
    let volumes = |scene: &Scene| {
        scene
            .atmosphere
            .volumes
            .iter()
            .map(|v| (v.min, v.max, v.color))
            .collect::<Vec<_>>()
    };
    assert_eq!(volumes(actual), volumes(expected));
    assert_eq!(actual.sky_origin, expected.sky_origin);
    for (actual, expected) in actual.camera_portals.iter().zip(&expected.camera_portals) {
        assert_eq!(actual.extra, expected.extra);
        assert_eq!(actual.hide, expected.hide);
        let actor_state = |actor: ReflectionActor| {
            (
                actor.pose.translation,
                actor.pose.rotation,
                actor.time,
                actor.alpha,
            )
        };
        assert_eq!(
            actual.actor.map(actor_state),
            expected.actor.map(actor_state)
        );
    }
}

#[test]
fn captured_presentation_matches_direct_writes() {
    let mut world = World::fixture(&[]);
    world.set_dynamic_liquids(vec![LiquidVolume {
        contents: 32,
        volume: Collider::fixture(Vec3::splat(-1.), Vec3::ONE),
    }]);
    let mut interactions = Interactions::empty();
    let script_fog = vec4(0.75, 0.5, 0.25, 100.);
    let script_origin = Vec3::splat(10.);
    interactions.presentation =
        crate::sky_sequence::Controller::presentation_fixture(script_fog, script_origin);
    for id in [1, 2] {
        interactions.levels.push(Slot {
            reg: &crate::levels::synthetic::REGISTRATION,
            ctl: Box::new(Controller(id)),
        });
    }
    let mut actual = scene_fixture();
    let mut expected = scene_fixture();

    for camera in [Vec3::ZERO, Vec3::splat(2.)] {
        let input = PresentationInput::capture(&world, camera, &interactions);
        actual.apply_presentation(&input);
        apply_direct(&mut expected, &world, camera, &interactions);
        assert_same_writes(&actual, &expected);
        if camera == Vec3::ZERO {
            assert_eq!(actual.atmosphere.liquid, Liquid::Water.tint());
            assert_eq!(actual.atmosphere.distance, vec4(0.25, 0.5, 0.75, 500.));
            assert_eq!(actual.sky_origin, Some(Vec3::ONE));
            assert_eq!(actual.camera_portals[0].hide, [11, 12]);
            assert_eq!(actual.camera_portals[1].hide, [0]);
            assert!(
                actual
                    .camera_portals
                    .iter()
                    .all(|p| p.actor.unwrap().time == 1.)
            );
            interactions.levels.clear();
        } else {
            assert_eq!(actual.atmosphere.liquid, Liquid::Dry.tint());
            assert_eq!(actual.atmosphere.distance, script_fog);
            assert_eq!(actual.sky_origin, Some(script_origin));
            assert!(actual.camera_portals[0].hide.is_empty());
            assert!(actual.camera_portals.iter().all(|p| p.actor.is_none()));
        }
    }
}
