//! Item particles use authored tags and clip events, independently of item grants.
use crate::{assets::Assets, environment::Atmosphere, skeletal::Transform, texture};
use anyhow::Result;
use macroquad::prelude::*;
use std::{cell::RefCell, collections::BTreeMap};

pub(crate) fn cached() -> bool {
    #[cfg(feature = "vita_renderer")]
    {
        crate::render::vita_renderer::config().pickup_fx_cache
    }
    #[cfg(not(feature = "vita_renderer"))]
    {
        false
    }
}

struct Instance {
    origin: [u32; 3],
    spin: u32,
    used: bool,
    particles: Vec<crate::particles::Attached>,
}

pub(crate) struct Effect {
    model: crate::tan::Model,
    scale: f32,
    events: crate::animation_events::Model,
    attached: Option<crate::particles::Attached>,
    bursts: Option<crate::particles::Attached>,
    clip: &'static str,
    duration: f32,
    timeline: crate::animation_events::EmitterTimeline,
    instances: RefCell<BTreeMap<String, Instance>>,
}
impl Effect {
    pub fn load(
        a: &mut Assets,
        name: &str,
        clip: &'static str,
        specs: &BTreeMap<String, texture::MaterialSpec>,
    ) -> Result<Self> {
        let (def, model) = crate::weapons::read_model_clip(a, name, Some(clip))?;
        let frames = model
            .surfaces
            .first()
            .map(|s| s.frames.len())
            .or_else(|| model.tags.values().next().map(Vec::len))
            .unwrap_or(1);
        let duration = frames as f32 * model.frame_time;
        let events = crate::animation_events::Model::load(a, &format!("models/{name}.tik"))?;
        let timeline = events.emitter_timeline(clip, duration, model.frame_time, clip == "idle");
        let mut effect = Self {
            attached: crate::particles::Attached::load(a, name, specs)?,
            bursts: crate::particles::Attached::load_clip_bursts(
                a,
                name,
                Some(clip),
                model.frame_time,
                specs,
            )?,
            events,
            timeline,
            instances: RefCell::default(),
            model,
            scale: def.scale,
            clip,
            duration,
        };
        for p in [&mut effect.attached, &mut effect.bursts]
            .into_iter()
            .flatten()
        {
            p.use_sprite_dimensions();
        }
        Ok(effect)
    }
    pub(crate) fn begin_frame(&self) {
        for instance in self.instances.borrow_mut().values_mut() {
            instance.used = false;
        }
    }
    pub(crate) fn end_frame(&self) {
        self.instances.borrow_mut().retain(|_, i| i.used);
    }
    pub(crate) fn clear(&self) {
        self.instances.borrow_mut().clear();
    }
    fn fresh(&self, origin: Vec3, spin: f32) -> Instance {
        Instance {
            origin: origin.to_array().map(f32::to_bits),
            spin: spin.to_bits(),
            used: true,
            particles: [&self.attached, &self.bursts]
                .into_iter()
                .flatten()
                .map(|p| p.fork_cached())
                .collect(),
        }
    }
    pub fn draw(
        &self,
        identity: Option<&str>,
        age: f32,
        origin: Vec3,
        spin: f32,
        camera: Vec3,
        atmosphere: &Atmosphere,
    ) {
        self.sample_with(identity, age, origin, spin, cached(), |p| {
            p.draw_sampled(camera, atmosphere)
        });
    }
    fn sample_with(
        &self,
        identity: Option<&str>,
        age: f32,
        origin: Vec3,
        spin: f32,
        use_cache: bool,
        mut consume: impl FnMut(&mut crate::particles::Attached),
    ) {
        let looping = self.clip == "idle";
        let pose = |t: f32, tag: Option<&str>| {
            let rotation = Quat::from_rotation_z(t * spin);
            let Some(points) = tag.and_then(|tag| self.model.tags.get(tag)) else {
                return Transform {
                    translation: origin,
                    rotation,
                };
            };
            let frame = t.max(0.) / self.model.frame_time;
            let frame = if looping {
                frame % points.len() as f32
            } else {
                frame.min((points.len() - 1) as f32)
            };
            let a = frame as usize;
            let b = if looping {
                (a + 1) % points.len()
            } else {
                (a + 1).min(points.len() - 1)
            };
            let axes = &self.model.tag_rotations[tag.unwrap()];
            Transform {
                translation: origin
                    + rotation * (points[a].lerp(points[b], frame.fract()) * self.scale),
                rotation: rotation * axes[a].slerp(axes[b], frame.fract()),
            }
        };
        let enabled = |name: &str, t: f32, initial: bool| {
            let _lookup =
                crate::frame_profile::object_timer(crate::frame_profile::ObjectWork::EventLookup);
            if use_cache {
                self.timeline.enabled(name, t, initial)
            } else {
                self.events
                    .visual(self.clip, t, self.duration, self.model.frame_time, looping)
                    .emitters
                    .get(name)
                    .copied()
                    .unwrap_or(initial)
            }
        };
        if let Some(identity) = identity.filter(|_| use_cache) {
            let mut instances = self.instances.borrow_mut();
            if !instances.contains_key(identity) {
                instances.insert(identity.into(), self.fresh(origin, spin));
            }
            let instance = instances.get_mut(identity).unwrap();
            // An ID can be reused by a restored visit or a drop at a new origin.
            // Rebuild all birth poses together; Attached::sample handles rewind.
            if instance.origin != origin.to_array().map(f32::to_bits)
                || instance.spin != spin.to_bits()
            {
                *instance = self.fresh(origin, spin);
            }
            instance.used = true;
            for p in &mut instance.particles {
                let sampling =
                    crate::frame_profile::object_timer(crate::frame_profile::ObjectWork::Sampling);
                p.sample(age, 1., pose, enabled);
                drop(sampling);
                consume(p);
            }
        } else {
            self.clear();
            for template in [&self.attached, &self.bursts].into_iter().flatten() {
                let mut p = template.fork();
                let sampling =
                    crate::frame_profile::object_timer(crate::frame_profile::ObjectWork::Sampling);
                p.sample(age, 1., pose, enabled);
                drop(sampling);
                consume(&mut p);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture(looping: bool) -> Effect {
        let clip = if looping { "idle" } else { "pickup" };
        let events = crate::animation_events::Model::parse(&format!("animations {{ {clip} a.tan {{ client {{ entry emitteron idle 2 emitteron go_up 4 emitteroff idle 5 emitteron burst 7 emitteroff go_up last emitteroff burst exit emitteroff idle }} }} }}")).unwrap();
        let model = crate::tan::Model {
            surfaces: vec![],
            tags: BTreeMap::from([("tag_tip".into(), vec![Vec3::X, Vec3::Y, Vec3::Z])]),
            tag_rotations: BTreeMap::from([(
                "tag_tip".into(),
                vec![
                    Quat::IDENTITY,
                    Quat::from_rotation_z(0.4),
                    Quat::from_rotation_x(0.2),
                ],
            )]),
            frame_time: 0.1,
        };
        Effect {
            timeline: events.emitter_timeline(clip, 1., model.frame_time, looping),
            events,
            model,
            scale: 2.,
            clip,
            duration: 1.,
            attached: Some(crate::particles::tests::pickup_fixture(false)),
            bursts: Some(crate::particles::tests::pickup_fixture(true)),
            instances: RefCell::default(),
        }
    }
    #[test]
    fn cache_on_off_particle_geometry_colours_births_order_and_lifecycle_match() {
        let camera = vec3(80., -60., 40.);
        for looping in [true, false] {
            let reference = fixture(looping);
            let optimized = fixture(looping);
            let mut seen = 0;
            for (frame, age) in [
                0., 0.02, 0.12, 0.19, 0.2, 0.25, 0.4, 0.5, 0.5, 0.7, 0.9, 1., 1.02, 1.5, 2.01, 8.,
                0.3, 0.31,
            ]
            .into_iter()
            .enumerate()
            {
                reference.begin_frame();
                optimized.begin_frame();
                for (id, origin) in [
                    ("placed:1", Vec3::ZERO),
                    ("drop:2", Vec3::X * 20.),
                    ("collection:3", Vec3::Y * 30.),
                ] {
                    if (frame == 9 || frame == 10) && id == "drop:2" {
                        continue;
                    } // collection/removal then respawn
                    let origin = origin
                        + if frame >= 12 && id == "drop:2" {
                            Vec3::Z * 40.
                        } else {
                            Vec3::ZERO
                        };
                    let sample = |effect: &Effect, cache| {
                        let mut out = Vec::new();
                        effect.sample_with(Some(id), age, origin, 0.4, cache, |p| {
                            out.push(p.output_bits(camera))
                        });
                        out
                    };
                    let old = sample(&reference, false);
                    let new = sample(&optimized, true);
                    seen += old
                        .iter()
                        .map(|v| v.as_array().unwrap().len())
                        .sum::<usize>();
                    assert_eq!(old, new, "loop={looping} frame={frame} id={id}");
                }
                reference.end_frame();
                optimized.end_frame();
                assert_eq!(
                    optimized.instances.borrow().len(),
                    if frame == 9 || frame == 10 { 2 } else { 3 }
                );
                if frame == 13 {
                    reference.clear();
                    optimized.clear();
                } // reload mid-effect
            }
            assert!(seen > 100);
            optimized.begin_frame();
            optimized.end_frame();
            assert!(optimized.instances.borrow().is_empty());
        }
    }
}
