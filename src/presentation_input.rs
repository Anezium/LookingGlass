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
    pub(crate) fn apply_atmosphere(&self, atmosphere: &mut crate::environment::Atmosphere) {
        atmosphere.liquid = self.liquid;
        if let Some(fog) = self.controller_fog.or(self.script_fog) {
            atmosphere.distance = fog;
        }
        if let Some(distance) = self.fog_distance {
            atmosphere.distance.w = distance;
        }
    }

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{environment::Atmosphere, render_snapshot::DynamicInput};
    use macroquad::prelude::Mat4;

    #[test]
    fn finishes_consecutive_frames_from_current_atmosphere() {
        let mut presentation = PresentationInput {
            liquid: Vec4::ZERO,
            script_fog: None,
            script_sky_origin: None,
            controller_sky_origin: None,
            reflection: Default::default(),
            reflection_actor: None,
            controller_fog: None,
            fog_distance: None,
        };
        let mut atmosphere = Atmosphere::default();
        let mut dynamic = DynamicInput::default();
        let storyboard = Vec4::new(0.1, 0.2, 0.3, 100.);
        let controller = Vec4::new(0.4, 0.5, 0.6, 200.);
        let partial = controller.truncate().extend(250.);
        let underwater = Liquid::Water.tint();
        let frames = [
            (Some(storyboard), None, None, Vec4::ZERO, storyboard),
            (
                Some(storyboard),
                Some(controller),
                None,
                underwater,
                controller,
            ),
            (None, None, Some(250.), underwater, partial),
            (None, None, None, Vec4::ZERO, partial),
        ];
        for (index, (script, actual, distance, liquid, expected)) in frames.into_iter().enumerate()
        {
            presentation.script_fog = script;
            presentation.controller_fog = actual;
            presentation.fog_distance = distance;
            presentation.liquid = liquid;
            presentation.apply_atmosphere(&mut atmosphere);
            let view = Mat4::from_translation(Vec3::X * index as f32);
            dynamic.finish_view(view, &atmosphere);
            assert_eq!(dynamic.fog, expected);
            assert_eq!(dynamic.liquid, liquid);
            assert_eq!(dynamic.view, view);
        }
    }
}
