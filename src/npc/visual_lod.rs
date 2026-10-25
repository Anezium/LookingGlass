//! Render-only cache for attachment-free ambient actors. No Actor state lives here.
use super::*;

#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub(super) enum Decision { #[default] Exact, Hidden, Refresh, Reuse }
#[derive(Default)]
pub(super) struct Cache {
    ticks: u32,
    key: Option<([u32; 8], bool)>,
    pub meshes: Vec<Mesh>,
    pub prepared: Decision,
}
impl Cache {
    pub fn decide(&mut self, config: &crate::vita_sim::Config, safe: bool,
        outside: bool, distance_squared: f32, transform: Transform, scale: f32,
        fullbright: bool) -> Decision {
        let far = config.anim_far_distance.is_some_and(|d| distance_squared > d * d);
        if !safe || (!config.anim_cull && (!far || config.anim_far_every == 1)) {
            self.invalidate();
            return Decision::Exact;
        }
        if config.anim_cull && outside {
            self.invalidate();
            return Decision::Hidden;
        }
        if !far || config.anim_far_every == 1 {
            self.invalidate();
            return Decision::Exact;
        }
        let p = transform.translation.to_array();
        let r = transform.rotation.to_array();
        let key = ([p[0].to_bits(),p[1].to_bits(),p[2].to_bits(),r[0].to_bits(),
            r[1].to_bits(),r[2].to_bits(),r[3].to_bits(),scale.to_bits()], fullbright);
        self.ticks += 1;
        if self.key != Some(key) || self.ticks >= config.anim_far_every {
            self.key = Some(key);
            self.ticks = 0;
            Decision::Refresh
        } else { Decision::Reuse }
    }
    fn invalidate(&mut self) {
        self.key = None;
        self.ticks = 0;
        self.meshes.clear();
    }
    pub fn capture(&mut self, model: &mut Model) {
        model.require_live_skin();
        if !model.visible.iter().any(|v| *v) {
            self.invalidate();
            return;
        }
        self.meshes.clear();
        self.meshes.extend(model.meshes.iter().zip(&model.visible)
            .filter_map(|(mesh, visible)| visible.then_some(mesh.as_ref()).flatten())
            .map(|mesh| Mesh { vertices: mesh.vertices.clone(), indices: mesh.indices.clone(),
                texture: mesh.texture.clone() }));
    }
}

pub(super) fn safe(actor: &Actor, model: &Model) -> bool {
    actor.visual_lod_safe()
        && model.data.combat.is_none()
        && model.data.meta.attachments.is_empty()
        && model.attachments.is_empty()
        && model.props.is_empty()
        && model.effects.is_empty()
        && model.own_effect.is_none()
        && model.bone_scales.is_empty()
}
impl Actor {
    fn visual_lod_safe(&self) -> bool {
        !self.spawn.model.to_ascii_lowercase().contains("alice")
            && !self.spawn.name.to_ascii_lowercase().contains("alice")
            && self.guard.is_none() && self.ant.is_none() && self.chess.is_none()
            && self.imp.is_none() && self.clock.is_none() && self.resident.is_none()
            && self.guide.is_none() && self.electric <= 0.
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn transform() -> Transform { Transform { translation: Vec3::ZERO, rotation: Quat::IDENTITY } }
    #[test]
    fn defaults_always_take_exact_path_and_protected_actors_never_skip() {
        let mut cache = Cache::default();
        let all = crate::vita_sim::Config { anim_cull: true, anim_far_distance: Some(1.),
            anim_far_every: 120, ..Default::default() };
        for tick in 0..180 {
            assert_eq!(cache.decide(&Default::default(),true,true,1e10,transform(),1.,false),Decision::Exact);
            assert_eq!(cache.decide(&all,false,tick % 2 == 0,1e10,transform(),1.,false),Decision::Exact);
        }
        let mut actor = super::super::tests::actor();
        assert!(actor.visual_lod_safe());
        actor.spawn.model = "alice".into();
        assert!(!actor.visual_lod_safe());
        actor.spawn.model = "c_torchgnome".into();
        actor.clock = Some(crate::clockwork::Automaton::new(Vec3::ZERO,0.,1.,0,true));
        assert!(!actor.visual_lod_safe());
    }
    #[test]
    fn frustum_reentry_distance_crossing_and_root_changes_refresh_immediately() {
        let config = crate::vita_sim::Config { anim_cull: true,anim_far_distance: Some(100.),
            anim_far_every: 4,..Default::default() };
        let mut cache = Cache::default();
        assert_eq!(cache.decide(&config,true,true,40000.,transform(),1.,false),Decision::Hidden);
        assert_eq!(cache.decide(&config,true,false,40000.,transform(),1.,false),Decision::Refresh);
        for _ in 0..3 { assert_eq!(cache.decide(&config,true,false,40000.,transform(),1.,false),Decision::Reuse); }
        assert_eq!(cache.decide(&config,true,false,40000.,transform(),1.,false),Decision::Refresh);
        assert_eq!(cache.decide(&config,true,false,10000.,transform(),1.,false),Decision::Exact);
        assert_eq!(cache.decide(&config,true,false,40000.,transform(),1.,false),Decision::Refresh);
        let moved = Transform { translation: Vec3::X, ..transform() };
        assert_eq!(cache.decide(&config,true,false,40000.,moved,1.,false),Decision::Refresh);
        assert_eq!(cache.decide(&config,true,false,40000.,moved,1.,true),Decision::Refresh);
    }
    #[test]
    fn visual_decisions_leave_saved_actor_and_contacts_unchanged() {
        let mut actor = super::super::tests::actor();
        let mut control = actor.clone();
        let world = World::fixture(&[]);
        let configs = [
            crate::vita_sim::Config {anim_cull:true,..Default::default()},
            crate::vita_sim::Config {anim_far_distance:Some(100.),anim_far_every:4,..Default::default()},
            crate::vita_sim::Config {anim_far_distance:Some(100.),anim_far_every:8,..Default::default()},
        ];
        for config in configs {
            let mut cache = Cache::default();
            for tick in 0..180 {
                let dt = [1./120.,0.05,0.002,0.][tick % 4];
                actor.tick(dt,&world,Vec3::ZERO);
                control.tick(dt,&world,Vec3::ZERO);
                let before = serde_json::to_vec(&actor).unwrap();
                cache.decide(&config,actor.visual_lod_safe(),tick % 3 == 0,40000.,transform(),1.,false);
                assert_eq!(before,serde_json::to_vec(&actor).unwrap());
                assert_eq!(serde_json::to_vec(&control).unwrap(),before);
                assert_eq!(actor.position(),control.position());
                assert_eq!(actor.target(),control.target());
            }
            crate::village::tests::assert_config_contacts_and_save(config);
        }
    }
}
