//! Per-actor emitter history; no shared-model clocks or render-driven damage.
use super::*;
use crate::{
    clockwork::Rig,
    wildlife::{Creature, Phase},
};
// Owned visual input: no Creature, target lookup, or World survives publication.
pub(super) struct Input {
    phase: Phase, time: f32, frozen: bool, visual_scale: f32,
    transform: Transform, scale: f32, clip: &'static str, looping: bool,
    web: Option<(Vec3, Vec3)>, beam: Option<(Vec3, Vec3, bool)>,
}
impl Input {
    pub fn capture(g: &Creature, data: &ModelData) -> Self {
        let transform = Transform { translation:g.feet, rotation:Quat::from_rotation_z(g.yaw) };
        Self { phase:g.phase,time:g.time,frozen:g.frozen,visual_scale:g.visual_scale(data),
            transform,scale:g.scale,clip:g.clip(),looping:g.loops(),
            web:g.web.map(|end|(g.target(0).center,end)),
            beam:g.beam.map(|end| {
                let from=if g.kind.jabber() { g.feet+transform.rotation*data.tag(g.clip(),g.time,"tag_beam").translation*g.scale }
                    else { g.target(0).center };
                (from,end,g.kind.jabber())
            }),
        }
    }
}
pub(super) struct Art {
    effect: Option<crate::particles::Attached>,
    templates: BTreeMap<String, crate::particles::Attached>,
    attachments: BTreeMap<String, crate::particles::Attached>,
    phase: Option<Phase>,
    time: f32,
}
impl Art {
    pub fn new(model: &ModelArt) -> Self {
        Self {
            effect: model.own_effect.as_ref().map(|t| t.fork()),
            templates: model
                .effects
                .iter()
                .map(|(n, e)| (n.clone(), e.fork()))
                .collect(),
            attachments: BTreeMap::new(),
            phase: None,
            time: 0.,
        }
    }
    pub fn draw(&mut self, g: &Creature, model: &ModelArt, camera: Vec3, atmosphere: &Atmosphere, beams: &attack_fx::Art) {
        self.draw_input(&Input::capture(g,&model.data),model,camera,atmosphere,beams);
    }
    pub fn draw_input(&mut self, g: &Input, model: &ModelArt, camera: Vec3, atmosphere: &Atmosphere, beams: &attack_fx::Art) {
        let data=model.data.as_ref();
        if let Some((start,end))=g.web {
            gl_use_default_material();
            crate::render_fx::legacy_line(start,end,Color::new(0.8,0.8,0.8,0.6));
        }
        if let Some((from,end,jabber))=g.beam {
            gl_use_default_material();
            if jabber { beams.lightning(from,end,camera,(g.time-data.clips[g.clip].frame_time).max(0.),1000.,false,atmosphere); }
            else { crate::render_fx::legacy_line(from,end,Color::new(0.45,0.75,1.,0.7)); }
        }
        if self.phase != Some(g.phase) || g.time < self.time {
            if let Some(effect) = &mut self.effect {
                *effect = effect.fork();
            }
            self.attachments.clear();
        }
        self.phase = Some(g.phase);
        self.time = g.time;
        if g.frozen || g.visual_scale <= 0. {
            return;
        }
        let transform = g.transform;
        let clip = &data.clips[g.clip];
        let visual = data.events.visual(
            g.clip,
            g.time,
            clip.duration(),
            clip.frame_time,
            g.looping,
        );
        self.attachments
            .retain(|tag, _| visual.attachments.contains_key(tag));
        for (tag, a) in visual.attachments.iter().take(8) {
            let Some(template) = self.templates.get(&a.model) else {
                continue;
            };
            let effect = self
                .attachments
                .entry(tag.clone())
                .or_insert_with(|| template.fork());
            let born = g.time - a.age;
            effect.draw(
                a.age,
                g.scale * a.scale,
                |at, child_tag| {
                    let local = data.tag(g.clip, born + at, tag);
                    let mut pose = Transform {
                        translation: transform.point(local.translation * g.scale),
                        rotation: transform.rotation * local.rotation,
                    };
                    if let (Some(prop), Some(child_tag)) = (model.props.get(&a.model), child_tag) {
                        pose.translation = prop.point(pose, child_tag, g.scale * a.scale);
                    }
                    pose
                },
                |_, _, default| default,
                camera,
                atmosphere,
            );
        }
        let Some(effect) = &mut self.effect else {
            return;
        };
        effect.draw(
            g.time,
            g.scale,
            |at, tag| {
                if let Some(tag) = tag {
                    let local = data.tag(
                        g.clip,
                        if g.looping {
                            at.rem_euclid(clip.duration())
                        } else {
                            at
                        },
                        tag,
                    );
                    Transform {
                        translation: transform.point(local.translation * g.scale),
                        rotation: transform.rotation * local.rotation,
                    }
                } else {
                    transform
                }
            },
            |name, at, default| {
                data.events
                    .visual(g.clip, at, clip.duration(), clip.frame_time, g.looping)
                    .emitters
                    .get(name)
                    .copied()
                    .unwrap_or(default)
            },
            camera,
            atmosphere,
        );
    }
}
