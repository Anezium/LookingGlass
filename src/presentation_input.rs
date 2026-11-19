//! Simulation values captured for the viewer's presentation writes.
use crate::{collision::World, interaction::Interactions, level::ReflectionActor, water::Liquid};
use macroquad::prelude::{Quat, Vec3, Vec4};

pub(crate) struct PresentationInput {
    pub liquid: Vec4,
    pub script_fog: Option<Vec4>,
    pub script_sky_origin: Option<Vec3>,
    pub controller_sky_origin: Option<Vec3>,
    pub reflection: (Vec<(usize, Vec3, Quat)>, Vec<usize>),
    pub reflection_actor: Option<ReflectionActor>,
    pub controller_fog: Option<Vec4>,
    pub fog_distance: Option<f32>,
}

const _: fn() = || {
    fn assert_send<T: Send>() {}
    assert_send::<PresentationInput>();
};

impl PresentationInput {
    pub fn capture(world: &World, camera: Vec3, interactions: &Interactions) -> Self {
        let liquid = Liquid::from_contents(world.liquid_at(camera)).tint();
        let (script_fog, script_sky_origin) = interactions.presentation.inputs();
        let levels = &interactions.levels;
        let controller_sky_origin = levels.iter().find_map(|slot| slot.ctl.sky_origin());
        let (mut mirror_extra, mut mirror_hide) = (Vec::new(), Vec::new());
        for slot in levels {
            let (extra, hide) = slot.ctl.reflection();
            mirror_extra.extend(extra);
            mirror_hide.extend(hide);
        }
        let reflection_actor = levels.iter().find_map(|slot| slot.ctl.reflection_actor());
        let controller_fog = levels.iter().find_map(|slot| slot.ctl.scene_fog());
        let fog_distance = levels.iter().find_map(|slot| slot.ctl.fog_distance());
        Self {
            liquid,
            script_fog,
            script_sky_origin,
            controller_sky_origin,
            reflection: (mirror_extra, mirror_hide),
            reflection_actor,
            controller_fog,
            fog_distance,
        }
    }
}
