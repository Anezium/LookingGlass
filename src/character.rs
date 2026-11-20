//! Alice's intact skin, locomotion animation and a collision-aware following camera.
use crate::mesh::{Mesh, draw_mesh};
pub use crate::weapons::WeaponInput;
use crate::{
    assets::Assets,
    collision::World,
    movement::Player,
    skeletal::{Animation, Definition, Skeleton, Transform},
    texture,
    weapons::{Action, Actions, VisualArt, Visuals},
};
use anyhow::{Context, Result};
use macroquad::{
    miniquad::{Comparison, PipelineParams, ShaderSource},
    prelude::*,
};
use std::{collections::BTreeMap, sync::Arc};
pub(crate) mod skinning;
pub(crate) fn skinning_bench_begin() {skinning::bench_begin();}
pub(crate) fn skinning_bench_end() {skinning::bench_end();}

pub(crate) const CLIPS: [&str; 32] = [
    "idle_base_01",
    "walk_nowep",
    "run_nowep",
    "jump_small_takeoff",
    "jump_air_03",
    "jump_small_land",
    "ready",
    "walk_smallwep",
    "run_smallwep",
    "walk_bigwep",
    "run_bigwep",
    "swim_frog",
    "swim_tred_wep",
    "ledge_climb_32",
    "ledge_climb_48",
    "ledge_climb_64",
    "death_faint",
    "death_falling_01",
    "death_drown",
    "rope_hang",
    "sit_minecart",
    "sit_airship",
    "rope_climb_up_right",
    "rope_climb_down_left",
    "ready_bw",
    "swim_tred",
    "ledge_hang",
    "ledge_shim_left",
    "ledge_shim_right",
    "ledge_pullup",
    "rope_climb_up_left",
    "rope_climb_down_right",
];

fn rope_pair(index: usize) -> Option<usize> {
    match index {
        22 => Some(30),
        23 => Some(31),
        _ => None,
    }
}
fn rope_length(player: &Player) -> Option<f32> {
    (player.script_motion == 1)
        .then(|| {
            player
                .rope
                .as_ref()
                .map(|g| g.length)
                .or(player.script_rope_length)
        })
        .flatten()
}
/// The controller loads frame files, but sound commands belong to TIKI aliases.
pub(crate) fn sound_alias(clip: &str) -> &str {
    match clip {
        "jump_air_03" => "fall",
        "swim_frog" => "swim_forward_frog",
        "ledge_climb_32" => "climb_32",
        "ledge_climb_48" => "climb_48",
        "ledge_climb_64" => "climb_64",
        "death_falling_01" => "death_falling1",
        "death_drown" => "death_drown1",
        "ready_bw" => "ready_bigwep",
        "ledge_hang" => "hang_idle",
        "ledge_shim_left" => "shimmy_left",
        "ledge_shim_right" => "shimmy_right",
        _ => clip,
    }
}
#[derive(Clone, Copy, PartialEq, Debug, serde::Serialize, serde::Deserialize)]
enum Motion {
    Idle,
    Walk,
    Run,
    Takeoff,
    Air,
    Land,
    Swim,
    Tread,
    Climb,
    Death,
    Rope,
    Cart,
    Airship,
    Hang,
    PullUp,
}
impl Motion {
    fn index(self) -> usize {
        self as usize
    }
    fn looping(self) -> bool {
        matches!(
            self,
            Self::Idle
                | Self::Walk
                | Self::Run
                | Self::Air
                | Self::Swim
                | Self::Tread
                | Self::Rope
                | Self::Cart
                | Self::Airship
                | Self::Hang
        )
    }
}

struct Animator {
    last_feet: Option<Vec3>,
    last_rope_length: Option<f32>,
    scale: f32,
    sound_clip: Option<usize>,
    resume_audio: bool,
    sound_span: Option<(usize, crate::audio::events::Span)>,
    clips: Arc<[Animation]>,
    motion: Motion,
    time: f32,
    transition: f32,
    previous: Vec<Transform>,
    pose: Vec<Transform>,
    jumps: u64,
    landings: u64,
    large: bool,
    unarmed: bool,
    offset: f32,
    previous_offset: f32,
}
impl Animator {
    fn snapshot(&self) -> AnimatorSave {
        AnimatorSave {
            last_feet: self.last_feet,
            last_rope_length: self.last_rope_length,
            motion: self.motion,
            time: self.time,
            transition: self.transition,
            previous: self.previous.clone(),
            pose: self.pose.clone(),
            jumps: self.jumps,
            landings: self.landings,
            large: self.large,
            offset: self.offset,
            previous_offset: self.previous_offset,
        }
    }
    fn restore(&mut self, s: &AnimatorSave) {
        self.last_feet = s.last_feet;
        self.last_rope_length = s.last_rope_length;
        self.sound_span = None;
        self.sound_clip = None;
        self.resume_audio = true;
        self.motion = s.motion;
        self.time = s.time;
        self.transition = s.transition;
        self.previous = s.previous.clone();
        self.pose = s.pose.clone();
        self.jumps = s.jumps;
        self.landings = s.landings;
        self.large = s.large;
        self.offset = s.offset;
        self.previous_offset = s.previous_offset;
    }
    fn new(clips: Vec<Animation>) -> Self {
        let pose = clips[6].sample(0., true);
        Self {
            last_feet: None,
            last_rope_length: None,
            scale: 1.,
            sound_clip: None,
            resume_audio: false,
            sound_span: None,
            clips: clips.into(),
            motion: Motion::Idle,
            time: 0.,
            transition: 1.,
            previous: pose.clone(),
            pose,
            jumps: 0,
            landings: 0,
            large: false,
            unarmed: false,
            offset: 0.,
            previous_offset: 0.,
        }
    }
    fn reset(&mut self, player: &Player) {
        self.last_feet = Some(player.feet);
        self.last_rope_length = rope_length(player);
        self.sound_clip = None;
        self.resume_audio = false;
        self.sound_span = None;
        self.motion = Motion::Idle;
        self.time = 0.;
        self.transition = 1.;
        self.jumps = player.jumps;
        self.landings = player.landings;
        self.pose = self.clips[6].sample(0., true);
        self.previous = self.pose.clone();
        self.offset = 0.;
        self.previous_offset = 0.;
    }
    fn update(&mut self, dt: f32, player: &Player, running: bool) {
        self.sound_span = None;
        if dt <= 0. {
            return;
        }
        let previous_feet = self.last_feet.replace(player.feet);
        let length = rope_length(player);
        let rope_travel = length
            .zip(self.last_rope_length)
            .map_or(0., |(now, before)| (now - before).abs());
        self.last_rope_length = length;
        let displacement = previous_feet.map_or(player.velocity * dt, |p| player.feet - p);
        // Warps reset gait; authored root motion must never pull a collision body.
        let displacement = if displacement.length() <= 128. {
            displacement
        } else {
            Vec3::ZERO
        };
        // A lift can carry the body while Alice stands still. Only her own
        // controller velocity selects a gait; resolved travel still sets its pace.
        let speed = if player.velocity.truncate().length() > 1. {
            displacement.truncate().length() / dt
        } else {
            0.
        };
        let old_motion = self.motion;
        let old_clip = self.sound_clip;
        let old_time = self.time;
        let jump = player.jumps != self.jumps;
        let land = player.landings != self.landings;
        self.jumps = player.jumps;
        self.landings = player.landings;
        let next = if self.motion == Motion::Death {
            Motion::Death
        } else if player.script_motion > 0 {
            match player.script_motion {
                1 => Motion::Rope,
                2 => Motion::Cart,
                _ => Motion::Airship,
            }
        } else if let Some(hang) = &player.ledge {
            if hang.pulling {
                Motion::PullUp
            } else {
                Motion::Hang
            }
        } else if player.climbing() {
            Motion::Climb
        } else if player.swimming {
            if player.velocity.length() > 20. {
                Motion::Swim
            } else {
                Motion::Tread
            }
        } else if !player.grounded {
            if jump
                || (self.motion == Motion::Takeoff && self.time < self.clips[3].duration() - 0.05)
            {
                Motion::Takeoff
            } else {
                Motion::Air
            }
        } else if speed > 1.
            || (dt < crate::movement::FIXED_DT
                && player.velocity.truncate().length() > 1.
                && matches!(self.motion, Motion::Walk | Motion::Run))
        {
            if running {
                Motion::Run
            } else {
                Motion::Walk
            }
        } else if land || (self.motion == Motion::Land && self.time < 0.4) {
            Motion::Land
        } else {
            Motion::Idle
        };
        if next != self.motion || jump || land {
            self.previous.clone_from(&self.pose);
            self.previous_offset = self.offset;
            self.transition = 0.;
            self.motion = next;
            // The controller jumps immediately; skip the original pre-jump anticipation.
            self.time = if next == Motion::Takeoff { 0.25 } else { 0. };
        }
        let index = match self.motion {
            Motion::Hang | Motion::PullUp => player.ledge.as_ref().map_or(26, |h| h.clip()),
            Motion::Death => {
                if player.immersion.level == 3 {
                    18
                } else if !player.grounded {
                    17
                } else {
                    16
                }
            }
            Motion::Rope => match player
                .rope
                .as_ref()
                .map_or(player.script_rope_rise, |g| g.rise)
            {
                n if n > 0.001 => 22,
                n if n < -0.001 => 23,
                _ => 19,
            },
            Motion::Cart => 20,
            Motion::Airship => 21,
            Motion::Swim => 11,
            Motion::Tread => {
                if self.unarmed {
                    25
                } else {
                    12
                }
            }
            Motion::Climb => {
                if player.climb_height <= 34. {
                    13
                } else if player.climb_height <= 50. {
                    14
                } else {
                    15
                }
            }
            Motion::Idle => {
                if self.unarmed {
                    0
                } else if self.large {
                    24
                } else {
                    6
                }
            }
            Motion::Walk => {
                if self.unarmed {
                    1
                } else if self.large {
                    9
                } else {
                    7
                }
            }
            Motion::Run => {
                if self.unarmed {
                    2
                } else if self.large {
                    10
                } else {
                    8
                }
            }
            _ => self.motion.index(),
        };
        let entered = self.sound_clip != Some(index) && !self.resume_audio;
        if entered && !matches!(self.motion, Motion::Takeoff) {
            self.previous.clone_from(&self.pose);
            self.previous_offset = self.offset;
            self.transition = 0.;
            self.time = 0.;
        }
        self.sound_clip = Some(index);
        self.resume_audio = false;
        let clip = &self.clips[index];
        let duration = self.duration(index);
        let preserve_stride = matches!(old_motion, Motion::Walk | Motion::Run)
            && matches!(self.motion, Motion::Walk | Motion::Run);
        if preserve_stride && old_clip.is_some_and(|old| old != index) {
            self.time = old_time / self.clips[old_clip.unwrap()].duration() * clip.duration();
        }
        let end = if let Some(hang) = &player.ledge {
            if hang.elapsed + 0.001 < self.time {
                self.time -= clip.duration();
            }
            hang.elapsed
        } else if matches!(self.motion, Motion::Walk | Motion::Run) {
            clip.advance_distance(
                self.time,
                displacement.truncate().length() / self.scale,
                false,
            )
        } else if self.motion == Motion::Rope && matches!(index, 22 | 23) {
            // A climb stroke includes stationary hand-placement frames. Inverting
            // each frame's root delta skips these poses and snaps the limbs. Pace
            // the complete alternating-hand cycle by actual travel along the rope.
            let travel: f32 = [index, rope_pair(index).unwrap()]
                .into_iter()
                .flat_map(|i| &self.clips[i].frames)
                .map(|f| f.delta.z.abs())
                .sum();
            self.time + rope_travel / self.scale / travel.max(0.001) * duration
        } else if let Some(progress) = player
            .climb_progress()
            .filter(|_| self.motion == Motion::Climb)
        {
            progress * clip.duration()
        } else {
            self.time + dt
        };
        self.sound_span = Some((
            index,
            crate::audio::events::Span {
                start: self.time,
                end,
                duration,
                frame_time: clip.frame_time,
                looping: self.motion.looping(),
                entered: entered && !preserve_stride,
            },
        ));
        self.time = end;
        if self.motion.looping() {
            self.time = self.time.rem_euclid(duration);
        } else {
            self.time = self.time.min(clip.duration());
        }
        self.transition = (self.transition + dt / 0.12).min(1.);
        let sample = self.sample(index);
        let t = self.transition * self.transition * (3. - 2. * self.transition);
        // The frog stroke is authored ~24 units lower than the upright tread pose.
        // Blend its presentation origin with the pose, not the collision body.
        let offset = if self.motion == Motion::Swim { 24. } else { 0. };
        self.offset = self.previous_offset + (offset - self.previous_offset) * t;
        self.pose = self
            .previous
            .iter()
            .zip(sample)
            .map(|(&a, b)| a.blend(b, t))
            .collect();
    }

    fn duration(&self, index: usize) -> f32 {
        self.clips[index].duration() + rope_pair(index).map_or(0., |i| self.clips[i].duration())
    }

    fn sample(&self, index: usize) -> Vec<Transform> {
        let Some(other) = rope_pair(index) else {
            return self.clips[index].sample(self.time, self.motion.looping());
        };
        let (current, next, time) = if self.time < self.clips[index].duration() {
            (index, other, self.time)
        } else {
            (other, index, self.time - self.clips[index].duration())
        };
        let clip = &self.clips[current];
        let last = clip.duration() - clip.frame_time;
        if time < last {
            clip.sample(time, false)
        } else {
            clip.frames
                .last()
                .unwrap()
                .pose
                .iter()
                .zip(&self.clips[next].frames[0].pose)
                .map(|(&a, &b)| a.blend(b, ((time - last) / clip.frame_time).clamp(0., 1.)))
                .collect()
        }
    }

    fn audio_spans(
        &self,
        index: usize,
        span: crate::audio::events::Span,
    ) -> Vec<(usize, crate::audio::events::Span)> {
        let Some(other) = rope_pair(index) else {
            return vec![(index, span)];
        };
        let mut spans = Vec::new();
        let duration = self.duration(index);
        let mut start = span.start;
        // Split at each handoff so both original hand cues keep their own frames.
        while start < span.end && spans.len() < 32 {
            let cycle = (start / duration).floor() * duration;
            let (i, offset) = if start - cycle < self.clips[index].duration() {
                (index, cycle)
            } else {
                (other, cycle + self.clips[index].duration())
            };
            let end = span.end.min(offset + self.clips[i].duration());
            if end <= start {
                break;
            }
            spans.push((
                i,
                crate::audio::events::Span {
                    start: start - offset,
                    end: end - offset,
                    duration: self.clips[i].duration(),
                    frame_time: self.clips[i].frame_time,
                    looping: false,
                    entered: (span.entered && start == span.start) || start > span.start,
                },
            ));
            start = end;
        }
        spans
    }
}

/// Single renderer resource slot, qualified by the Character lifetime/restore generation.
/// No controller, texture, mesh or pointer is carried by this input.
struct BodyInput {
    visit: u64,
    slot:usize,
    pose: Vec<Transform>,
    transform: Transform,
    scale: f32,
    face_time: f32,
    alive: bool,
    water_time: f32,
    water: (bool, Option<f32>),
    power: crate::power_pose::State,
    power_remaining: f32,
    frozen: bool,
    effects: crate::weapons::EffectsInput,
    held: crate::weapons::HeldInput,
    show_held: bool,
    attachments: Vec<(usize, usize, f32, f32)>,
}
impl Default for BodyInput {
    fn default() -> Self {
        Self { visit:0, slot:0, pose:Vec::new(), transform:Transform {translation:Vec3::ZERO,rotation:Quat::IDENTITY},
            scale:1., face_time:0., alive:true, water_time:0., water:(false,None), power:Default::default(), power_remaining:0., frozen:false,
            effects:Default::default(), held:Default::default(), show_held:false, attachments:Vec::new() }
    }
}
impl BodyInput {
    fn valid(&self, visit: u64, bones: usize) -> bool {
        self.visit == visit && self.slot==0 && self.pose.len() == bones && bones <= 65536 && self.attachments.len() <= 1024
            && self.attachments.iter().all(|&(_, bone, _, _)| bone < bones)
    }
    fn bytes(&self) -> usize {
        std::mem::size_of::<Self>() + self.pose.len()*std::mem::size_of::<Transform>()
            + self.attachments.len()*std::mem::size_of::<(usize, usize, f32, f32)>()
            + self.effects.bytes()-std::mem::size_of::<crate::weapons::EffectsInput>()
            + self.held.bytes()-std::mem::size_of::<crate::weapons::HeldInput>()
    }
}
pub struct Character {
    sim: AliceSim,
    art: AliceArt,
}

pub(crate) struct AliceSim {
    body_arena: BodyInput,
    body_scratch: Vec<Transform>,
    weapon_notice: Option<&'static str>,
    foot_rig: crate::footing::Rig,
    foot_state: crate::footing::State,
    acting: crate::acting::Acting,
    face: crate::facial::Rig,
    mouth: f32,
    face_time: f32,
    sound_events: crate::audio::events::Model,
    power_pose: crate::power_pose::Performance,
    power_remaining: f32,
    skeleton: Arc<Skeleton>,
    animator: Animator,
    scale: f32,
    facing: f32,
    actions: Actions,
    visuals: Visuals,
    upper: Vec<bool>,
    tags: [usize; 3],
    anchors: [Transform; 3],
    // Saved sway state also drives simulation-side muzzle and trail anchors.
    viewmodel: crate::viewmodel::ViewModel,
    water: crate::water_art::State,
}

/// Alice's render resources must be dropped on the owner thread. Texture2D is
/// Send in our macroquad revision, but dropping it touches the GL context.
/// The Rc-backed material layers keep AliceArt confined to the owner.
pub(crate) struct AliceArt {
    snapshot_visit: crate::render_snapshot::RenderVisit,
    body_input: Option<BodyInput>,
    skin_identity: skinning::identity::ActorId,
    _material_layers: Vec<std::rc::Rc<crate::render_fx::Surface>>,
    power_art: crate::power_pose::Art,
    water_art: crate::water_art::Art,
    meshes: Vec<Option<Mesh>>,
    material: SkinMaterial,
    visual_art: VisualArt,
    skeleton: Arc<Skeleton>,
    face: crate::facial::FaceArt,
    acting: crate::acting::Art,
    #[cfg(any(debug_assertions, not(target_os = "vita")))]
    require_body_input: bool,
}

const _: fn() = || {
    fn assert_send<T: Send>() {}
    assert_send::<AliceSim>();

    // This lookup becomes ambiguous if AliceArt gains Send. The live owner
    // has Rc-backed material layers and SkinMaterial, both of which forbid it.
    trait AmbiguousIfSend<Marker> {
        fn check() {}
    }
    impl<T: ?Sized> AmbiguousIfSend<()> for T {}
    struct IsSend;
    impl<T: ?Sized + Send> AmbiguousIfSend<IsSend> for T {}
    let _ = <AliceArt as AmbiguousIfSend<_>>::check;
};

impl AliceSim {
    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            power: self.power_pose.state.clone(),
            feet: self.foot_state.clone(),
            acting: self.acting.state.clone(),
            face_time: self.face_time,
            animator: self.animator.snapshot(),
            facing: self.facing,
            actions: self.actions.snapshot(),
            projectiles: self.visuals.snapshot(),
            anchors: self.anchors,
            viewmodel: self.viewmodel.clone(),
        }
    }

    pub fn restore(&mut self, s: &Snapshot) -> Result<()> {
        s.validate(self.skeleton.bones.len())?;
        self.power_pose.state = s.power.clone();
        self.foot_state = s.feet.clone();
        self.actions.restore(&s.actions)?;
        self.acting.restore(&s.acting)?;
        self.visuals.restore(&s.projectiles)?;
        self.animator.restore(&s.animator);
        self.face_time = s.face_time;
        self.mouth = 0.;
        self.facing = s.facing;
        self.anchors = s.anchors;
        self.viewmodel = s.viewmodel.clone();
        self.water = Default::default();
        Ok(())
    }

    #[must_use = "true means the owner must clear the heavy, ice and bomb emitter history (art-side write)"]
    fn die(&mut self) -> bool {
        self.power_pose.state.sync(None);
        if !matches!(
            self.animator.motion,
            Motion::Death | Motion::Rope | Motion::Cart | Motion::Airship
        ) {
            self.animator.previous.clone_from(&self.animator.pose);
            self.animator.previous_offset = self.animator.offset;
            self.animator.motion = Motion::Death;
            self.animator.transition = 0.;
            self.animator.time = 0.;
            self.actions.reset(&self.animator.pose);
            self.visuals.cancel_staff();
            self.visuals.clear();
            return true;
        }
        false
    }

    pub fn reset(&mut self, player: &Player, yaw: f32) {
        self.power_pose.state.interrupt();
        self.foot_state = Default::default();
        self.acting.reset();
        self.face_time = 0.;
        self.mouth = 0.;
        self.facing = yaw;
        self.animator.reset(player);
        self.actions.reset(&self.animator.pose);
        self.visuals.clear();
        self.viewmodel.reset();
    }

    pub fn notarget(&mut self, value: bool) {
        self.visuals.dice.notarget = value;
    }

    pub fn facing(&self) -> f32 {
        self.facing
    }

    pub fn rage_scene(&mut self, time: f32, finished: bool, player: &Player) {
        self.power_pose
            .rage_scene(time, finished, &mut self.actions.pose);
        if !finished {
            self.facing = player.script_facing;
            self.acting.reset();
            self.foot_state = Default::default();
            self.visuals.cancel_staff();
        }
    }

    /// Resume from the actual last cinematic pose without clearing live effects.
    /// A distant cutaway or a scaled Alice cannot seed an ordinary body pose.
    pub fn resume_scene(
        &mut self,
        player: &Player,
        yaw: f32,
        pose: Option<&crate::cinematic::ActorPose>,
    ) {
        self.power_pose.state.interrupt();
        self.foot_state = Default::default();
        self.acting.reset();
        self.mouth = 0.;
        self.facing = yaw;
        self.animator.reset(player);
        if let Some(pose) = pose.filter(|p| {
            p.local.len() == self.skeleton.bones.len()
                && (p.scale - 1.).abs() < 0.01
                && p.transform.translation.distance(player.feet) <= 16.
        }) {
            self.facing = pose.transform.rotation.to_euler(EulerRot::ZYX).0;
            self.animator.pose.clone_from(&pose.local);
            self.animator.previous.clone_from(&pose.local);
            self.animator.transition = 0.;
            self.animator.resume_audio = true;
        }
        self.actions.reset(&self.animator.pose);
    }

    pub fn cancel_weapon_action(&mut self) {
        self.actions.reset(&self.animator.pose);
        self.visuals.cancel_staff();
        self.weapon_notice = None;
    }

    pub fn update_funded(
        &mut self,
        dt: f32,
        player: &Player,
        running: bool,
        mut weapon: WeaponInput,
        context: &crate::combat::Context<'_>,
        mut stats: Option<&mut crate::inventory::Stats>,
    ) -> Vec<&'static str> {
        if let Some(stats) = stats.as_deref() {
            self.power_appearance(stats);
        }
        if !dt.is_finite() || dt <= 0. {
            self.animator.sound_span = None;
            return Vec::new();
        }
        self.visuals.dice_count = weapon.dice.min(3);
        if weapon.selected == 6 && !self.visuals.dice.ready() {
            weapon.click = None;
        }
        self.animator.large = [4, 7, 8].contains(&weapon.selected);
        self.animator.unarmed = weapon.selected == crate::weapons::UNARMED;
        self.animator.update(
            if self.visuals.ice_locked() { 0. } else { dt },
            player,
            running,
        );
        if self.animator.motion == Motion::Death
            || player.script_motion > 0
            || player.ledge.is_some()
        {
            self.power_pose
                .update(dt, false, false, &mut self.actions.pose, &self.upper);
            self.acting.update(dt, false, false, false, weapon.selected);
            self.actions.reset(&self.animator.pose);
            self.visuals.cancel_staff();
            if player.script_motion > 0 {
                self.facing = player.script_facing;
            }
            if let Some(hang) = &player.ledge {
                self.facing = hang.forward.y.atan2(hang.forward.x);
                let pose = self.skeleton.global_pose(&self.actions.pose);
                let rotation = Quat::from_rotation_z(self.facing);
                self.anchors = self.tags.map(|i| Transform {
                    rotation: rotation * pose[i].rotation,
                    translation: player.feet + rotation * (pose[i].translation * self.scale),
                });
            }
            if self.animator.motion == Motion::Rope || player.ledge.is_some() {
                // Hanging suppresses new attacks, not already released projectiles
                // or a summoned demon. Their ordinary pause gate still applies.
                return self.visuals.update(
                    dt,
                    &self.actions,
                    Default::default(),
                    &self.anchors,
                    player.eye(),
                    context,
                );
            }
            return Vec::new();
        }
        let attack_aim = self
            .actions
            .playing
            .as_ref()
            .filter(|p| matches!(p.action, Action::Attack { .. }))
            .map(|_| weapon.aim)
            .or_else(|| {
                (weapon.click.is_some()
                    && crate::weapons::supported(weapon.selected)
                    && self.actions.playing.is_none())
                .then_some(weapon.aim)
            });
        if dt > 0. && player.climbing() {
            self.facing = player.climb_direction.y.atan2(player.climb_direction.x);
        } else if dt > 0. && attack_aim.is_some() {
            let aim = attack_aim.unwrap();
            self.facing = aim.y.atan2(aim.x);
        } else if dt > 0. && player.velocity.truncate().length() > 12. {
            let target = player.velocity.y.atan2(player.velocity.x);
            let difference = (target - self.facing + std::f32::consts::PI)
                .rem_euclid(std::f32::consts::TAU)
                - std::f32::consts::PI;
            self.facing = (self.facing + difference.clamp(-dt * 12., dt * 12.))
                .rem_euclid(std::f32::consts::TAU);
        }
        if self.visuals.ice_locked() {
            weapon.click = None;
        }
        self.visuals.owner_immersed =
            player.immersion.level > 1 && player.immersion.kind != crate::water::Liquid::Lava;
        self.visuals.follow_ice(player.feet);
        let first_person = weapon.first_person;
        let selected = weapon.selected;
        if selected != 4
            || (weapon.click != Some(false)
                && self.actions.playing.as_ref().is_none_or(|p| p.clip != 13))
        {
            self.visuals.stop_ice();
        }
        let aim = weapon.aim;
        let mut events = self.actions.update_funded(
            dt,
            weapon,
            &self.animator.pose,
            &self.upper,
            player.grounded && player.velocity.truncate().length() < 12.,
            stats.as_deref_mut(),
        );
        self.weapon_notice = events.notice;
        let idle = self.animator.motion == Motion::Idle;
        self.acting.update(
            dt,
            idle,
            player.grounded && !player.climbing() && !first_person,
            self.actions.playing.is_some(),
            selected,
        );
        if dt > 0. || self.animator.unarmed {
            self.acting.apply(&mut self.actions.pose, &self.upper, idle);
        }
        let mut power_mask = self.upper.clone();
        if let Some(i) = self.skeleton.bones.iter().position(|b| b.name == "tag_03") {
            power_mask[i] = true;
        }
        self.power_pose.update(
            dt,
            self.actions.playing.is_none()
                && player.grounded
                && !player.climbing()
                && !player.swimming,
            idle,
            &mut self.actions.pose,
            &power_mask,
        );
        self.foot_rig.apply(
            &self.skeleton,
            &mut self.actions.pose,
            &mut self.foot_state,
            context.world,
            Transform {
                translation: player.feet,
                rotation: Quat::from_rotation_z(self.facing),
            },
            self.scale,
            dt,
            player.grounded
                && !player.climbing()
                && !player.swimming
                && !first_person
                && matches!(
                    self.animator.motion,
                    Motion::Idle | Motion::Walk | Motion::Run | Motion::Land
                ),
        );
        let pose = self.skeleton.global_pose(&self.actions.pose);
        let rotation = Quat::from_rotation_z(self.facing);
        self.anchors = self.tags.map(|i| Transform {
            rotation: rotation * pose[i].rotation,
            translation: player.feet
                + Vec3::Z * self.animator.offset
                + rotation * (pose[i].translation * self.scale),
        });
        self.visuals.set_combat_anchors(self.anchors, aim);
        events.attach_contacts(|local| {
            let pose = self.skeleton.global_pose(local);
            self.tags.map(|i| Transform {
                rotation: rotation * pose[i].rotation,
                translation: player.feet
                    + Vec3::Z * self.animator.offset
                    + rotation * (pose[i].translation * self.scale),
            })
        });
        let mut view_scale = 1.;
        if first_person {
            let view = self.viewmodel.pose(
                dt,
                player.velocity.length(),
                &self.actions,
                crate::viewmodel::camera_frame(player.eye(), aim),
            );
            self.anchors = view.anchors;
            view_scale = view.scale;
        }
        self.visuals.set_view(first_person, view_scale);
        self.visuals.owner_velocity = player.velocity;
        let sounds = self.visuals.update_funded(
            dt,
            &self.actions,
            events,
            &self.anchors,
            player.eye(),
            context,
            stats.as_deref_mut(),
        );
        if let Some(stats) = stats {
            if self.visuals.ice_drain {
                stats.spend_will(stats.will());
            } else if self.visuals.ice_refund > 0. {
                stats.apply(crate::inventory::PickupKind::Will, self.visuals.ice_refund);
            }
        }
        self.visuals.ice_drain = false;
        self.visuals.ice_refund = 0.;
        self.visuals.finish_staff(&mut self.actions);
        self.actions.finish_frame();
        sounds
    }

    pub fn ice_locked(&self) -> bool {
        self.visuals.ice_locked()
    }

    pub fn attack_movement_locked(&self) -> bool {
        self.actions.movement_locked()
    }

    pub fn ice_targets(&self) -> Vec<crate::combat::Target> {
        self.visuals.ice_targets()
    }

    pub fn croquet_contacts(&self) -> bool {
        self.actions.equipment().0 == 2 || self.visuals.croquet_in_flight()
    }

    pub fn hit_ice(&mut self, hit: crate::combat::Hit) -> bool {
        self.visuals.hit_ice(hit)
    }

    pub fn audio_loops(&self) -> Vec<crate::audio::LoopCue> {
        self.visuals.audio_loops()
    }

    pub fn take_world_audio(&mut self) -> Vec<(&'static str, Vec3)> {
        std::mem::take(&mut self.visuals.spatial_sounds)
    }

    pub fn take_audio(&mut self) -> Vec<crate::audio::events::Cue> {
        let mut cues = self.acting.take_audio();
        let Some((index, span)) = self.animator.sound_span.take() else {
            return cues;
        };
        for (index, span) in self.animator.audio_spans(index, span) {
            cues.extend(
                self.sound_events
                    .between(sound_alias(CLIPS[index]), span)
                    .into_iter()
                    .filter(|cue| {
                        // Immediate controller jump/landing contacts already own these cues.
                        !cue.path.ends_with("/jump.wav") && !cue.path.ends_with("/small_land.wav")
                    }),
            );
        }
        cues
    }

    pub fn ready_to_attack(&self, selected: usize) -> bool {
        self.actions.ready_to_attack(selected)
            && self.animator.motion != Motion::Death
            && (selected != 6 || self.visuals.dice.ready())
    }

    pub fn summon_target(&self) -> Option<crate::combat::Target> {
        self.visuals.dice.target()
    }

    pub fn ally_target(&self) -> Option<crate::combat::Target> {
        self.visuals.dice.ally_target()
    }

    pub fn dismiss_summon(&mut self) {
        self.visuals.dice.dismiss();
    }

    pub fn hit_summon(&mut self, hit: crate::combat::Hit) {
        self.visuals.dice.hit(hit);
    }

    pub fn threatens(&self, world: &World, target: crate::combat::Target) -> bool {
        self.visuals.threatens(world, target)
    }

    pub fn take_hits(&mut self) -> Vec<crate::combat::Hit> {
        std::mem::take(&mut self.visuals.hits)
    }

    pub fn world_time(&mut self, dt: f32, world_dt: f32) {
        let frozen = world_dt == 0. && dt > 0.;
        self.visuals.time_stopped = frozen;
        self.visuals.world_ratio = Some(if dt > 0. {
            (world_dt / dt).clamp(0., 1.)
        } else {
            0.
        });
        self.actions.time_stopped = frozen;
    }

    pub fn power_appearance(&mut self, stats: &crate::inventory::Stats) {
        self.power_remaining=stats.powers.rage;
        self.visuals.dice.rage = stats.powers.rage > 0.;
        self.visuals.dice.difficulty = stats.difficulty;
        if !stats.alive() {
            self.visuals.dice.dismiss();
        }
        self.acting.health(stats.sanity());
        self.power_pose.state.sync(power_form(stats));
        if stats.equipped().is_none() {
            self.actions.unarm(&self.animator.pose);
        }
    }

    pub fn story_pose(&mut self, story: &crate::story::Story, dt: f32) {
        self.acting.dialogue(story);
        self.face_time += dt.max(0.);
        self.mouth = if self.animator.motion == Motion::Death {
            0.
        } else {
            story.mouth(&["fakeplayer", "player", "alice"])
        };
    }

    fn extract_body(
        &mut self,
        feet: Vec3,
        visit: u64,
        summon_art: Option<&crate::dice::Art>,
    ) -> (usize, bool) {
        let bones = self.skeleton.bones.len();
        if self.actions.pose.len() != bones || bones > 65536 || self.visuals.render_effects_overflow() {
            return (0, true);
        }
        let input = &mut self.body_arena;
        input.visit = visit;
        input.slot = 0;
        self.face.pose_into(
            &self.skeleton,
            &self.actions.pose,
            self.mouth,
            &mut self.body_scratch,
            &mut input.pose,
        );
        input.transform = Transform {
            translation: feet + Vec3::Z * self.animator.offset,
            rotation: Quat::from_rotation_z(self.facing),
        };
        input.scale = self.scale;
        input.face_time = self.face_time;
        input.alive = self.animator.motion != Motion::Death;
        input.water_time = self.animator.time;
        input.water = self.water.render_state();
        input.power_remaining = self.power_remaining;
        input.power = self.power_pose.state.render_state();
        input.frozen = self.visuals.ice_locked();
        self.acting.render_attachments(&mut input.attachments, &self.skeleton);
        input.show_held = !matches!(self.animator.motion, Motion::Death | Motion::Rope | Motion::Cart | Motion::Airship)
            && !self.acting.hide_weapon() && !input.power.performing;
        self.visuals.extract_held(&self.actions, &self.anchors, &mut input.held);
        self.visuals.extract_effects(&mut input.effects, summon_art);

        let overflow = !input.valid(visit, bones)
            || !self.visuals.valid_body_inputs(&input.held, &input.effects, summon_art)
            || !self.acting.valid_attachments(&input.attachments);
        (input.bytes(), overflow)
    }

    pub fn visual_counts(&self) -> (u64, u64) {
        (self.visuals.shots, self.visuals.impacts)
    }

    pub fn lights(&self) -> Vec<crate::lighting::Light> {
        self.visuals.lights()
    }
}

impl AliceArt {
    pub fn atmosphere(&self, atmosphere: &crate::environment::Atmosphere, camera: Vec3) {
        self.water_art.atmosphere(atmosphere, camera);
        self.material.atmosphere(atmosphere, camera);
        self.visual_art.atmosphere(atmosphere, camera);
        self.power_art.atmosphere(camera);
    }

    fn draw_live(&mut self, sim: &AliceSim, feet: Vec3, fullbright: bool, ghost: bool) {
        self.power_art.remaining(sim.power_remaining);
        let _identity = self.skin_identity.scope();
        let feet = feet + Vec3::Z * sim.animator.offset;
        let prepared = skinning::prepared::pose(
            (Arc::as_ptr(&self.skeleton) as usize, &sim.face as *const _ as usize),
            &sim.actions.pose, sim.mouth, &[],
            || (sim.face.pose(&self.skeleton, &sim.actions.pose, sim.mouth, None), Vec::new()),
        );
        let pose = &prepared.pose;
        self.power_art
            .skin(&mut self.meshes, &sim.power_pose.state);
        if sim.power_pose.state.skin().is_none() {
            self.face.blink(
                &mut self.meshes,
                sim.face_time,
                sim.animator.motion != Motion::Death,
            );
        }
        let mut hidden = Vec::new();
        for (i, surface) in self.skeleton.surfaces.iter().enumerate() {
            if sim.power_pose.state.hidden(&surface.name) {
                hidden.push((i, self.meshes[i].take()));
            }
        }
        let rotation = Quat::from_rotation_z(sim.facing);
        let _skin_guard = skinning::CullGuard::preserve(sim.visuals.ice_locked()
            || sim.power_pose.state.form == Some(crate::power_pose::Form::Rage));
        let skin_visible = draw_skin(
            &self.skeleton,
            &mut self.meshes,
            &pose,
            Transform {
                translation: feet,
                rotation,
            },
            sim.scale,
            fullbright,
        );
        if skinning::overlap::gather_only() {
            for (i, mesh) in hidden { self.meshes[i] = mesh; }
            return;
        }
        if skin_visible {
            self.visual_art.frozen_body_input(&self.meshes, sim.visuals.ice_locked());
            self.power_art.surface_effects(&self.meshes, &sim.power_pose.state);
        }
        for (i, mesh) in hidden {
            self.meshes[i] = mesh;
        }
        self.power_art.draw(
            &sim.power_pose.state,
            &pose,
            Transform {
                translation: feet,
                rotation,
            },
            sim.scale,
            fullbright,
        );
        self.material.bind_appearance(Appearance {
            ghost,
            ..Default::default()
        });
        self.acting.draw(
            &sim.acting,
            &self.skeleton,
            &pose,
            Transform {
                translation: feet,
                rotation,
            },
            sim.scale,
            fullbright,
        );
        if sim.animator.motion != Motion::Death {
            self.water_art.draw_input(
                sim.water.render_state(),
                &pose,
                Transform {
                    translation: feet,
                    rotation,
                },
                sim.scale,
                sim.animator.time,
                fullbright,
            );
            self.material.bind_appearance(Appearance { ghost, ..Default::default() });
        }
        if !matches!(
            sim.animator.motion,
            Motion::Death | Motion::Rope | Motion::Cart | Motion::Airship
        ) && !sim.acting.hide_weapon()
            && !sim.power_pose.state.performing
        {
            self.visual_art
                .draw_held(&sim.visuals, &sim.actions, &sim.anchors, fullbright);
        }
        gl_use_default_material();
    }

    fn draw(&mut self, input: &BodyInput, fullbright: bool, ghost: bool) {
        self.power_art.remaining(input.power_remaining);
        let _identity = self.skin_identity.scope();
        let pose = &input.pose;
        let feet = input.transform.translation;
        self.power_art
            .skin(&mut self.meshes, &input.power);
        if input.power.skin().is_none() {
            self.face.blink(
                &mut self.meshes,
                input.face_time,
                input.alive,
            );
        }
        let mut hidden = Vec::new();
        for (i, surface) in self.skeleton.surfaces.iter().enumerate() {
            if input.power.hidden(&surface.name) {
                hidden.push((i, self.meshes[i].take()));
            }
        }
        let rotation = input.transform.rotation;
        let _skin_guard = skinning::CullGuard::preserve(input.frozen
            || input.power.form == Some(crate::power_pose::Form::Rage));
        let skin_visible = draw_skin(
            &self.skeleton,
            &mut self.meshes,
            &pose,
            Transform {
                translation: feet,
                rotation,
            },
            input.scale,
            fullbright,
        );
        if skinning::overlap::gather_only() {
            for (i, mesh) in hidden { self.meshes[i] = mesh; }
            return;
        }
        if skin_visible {
            self.visual_art.frozen_body_input(&self.meshes, input.frozen);
            self.power_art.surface_effects(&self.meshes, &input.power);
        }
        for (i, mesh) in hidden {
            self.meshes[i] = mesh;
        }
        self.power_art.draw(
            &input.power,
            &pose,
            Transform {
                translation: feet,
                rotation,
            },
            input.scale,
            fullbright,
        );
        self.material.bind_appearance(Appearance {
            ghost,
            ..Default::default()
        });
        self.acting.draw_attachments(&input.attachments, pose, input.transform, input.scale, fullbright);
        if input.alive {
            self.water_art.draw_input(
                input.water, &pose,
                Transform {
                    translation: feet,
                    rotation,
                },
                input.scale,
                input.water_time,
                fullbright,
            );
            self.material.bind_appearance(Appearance { ghost, ..Default::default() });
        }
        if input.show_held { self.visual_art.draw_held_input(&input.held, fullbright); }
        gl_use_default_material();
    }

    fn valid_body_input(&self, input: &BodyInput) -> bool {
        input.valid(self.snapshot_visit.id(), self.skeleton.bones.len())
            && self.visual_art.valid_effects_input(&input.effects)
            && self.visual_art.valid_held_input(&input.held)
            && self.acting.valid_attachments(&input.attachments)
    }

    fn assert_live_fallback(&self) {
        #[cfg(any(debug_assertions, not(target_os = "vita")))]
        assert!(!self.require_body_input, "world-free Alice draw fell back to live simulation");
    }

    fn draw_first_person_live(&mut self, sim: &AliceSim, fullbright: bool) {
        self.material.bind();
        self.visual_art
            .draw_held(&sim.visuals, &sim.actions, &sim.anchors, fullbright);
        crate::render::depth_read_only(|| self.visual_art.draw_view_trail(&sim.visuals));
        gl_use_default_material();
    }

    fn draw_viewmodel(&mut self, input: &BodyInput, fullbright: bool) {
        self.material.bind();
        self.visual_art.draw_held_input(&input.held, fullbright);
        crate::render::depth_read_only(|| self.visual_art.draw_view_trail_input(&input.effects));
        gl_use_default_material();
    }
}

impl Character {
    pub fn snapshot(&self) -> Snapshot {
        self.sim.snapshot()
    }

    pub fn restore(&mut self, snapshot: &Snapshot) -> Result<()> {
        self.retire_body_input();
        self.sim.restore(snapshot)?;
        self.art.visual_art.clear();
        self.art.water_art.reset_emitter();
        Ok(())
    }

    pub fn die(&mut self) {
        if self.sim.die() {
            self.art.visual_art.clear();
        }
    }

    pub fn load(assets: &mut Assets) -> Result<Self> {
        let _r13=crate::allocation_census::Scope::new(4);
        let def = Definition::alice(assets)?;
        let skeleton = Arc::new(Skeleton::parse(&assets.read(&format!("{}/{}", def.path, def.model))?)?);
        let mut clips = Vec::new();
        for name in CLIPS {
            let path = format!("{}/{name}.ska", def.path);
            clips.push(
                Animation::parse(&assets.read(&path)?, skeleton.bones.len())
                    .with_context(|| path)?,
            );
        }
        let specs = texture::read_materials(assets)?;
        let mut textures = BTreeMap::<String, Texture2D>::new();
        let mut meshes = Vec::new();
        let mut material_layers = Vec::new();
        for surface in &skeleton.surfaces {
            // The extra cap surfaces are only used by the original dismemberment system.
            if ["material15", "material16", "material17"].contains(&surface.name.as_str()) {
                meshes.push(None);
                continue;
            }
            let skin = def
                .skins
                .get(&surface.name)
                .context("Alice surface has no skin")?;
            let path = texture::resolve(assets, &format!("{}/{skin}", def.path), &specs)
                .or_else(|| texture::resolve(assets, skin, &specs))
                .with_context(|| format!("Alice skin not found: {skin}"))?;
            let tex = if let Some(tex) = textures.get(&path) {
                tex.clone()
            } else {
                let image = texture::decode(assets, &path)?;
                let tex = crate::texture::upload_rgba8(image.width, image.height, &image.pixels);
                tex.set_filter(FilterMode::Linear);
                textures.insert(path, tex.clone());
                tex
            };
            material_layers.push(crate::render_fx::register(
                assets,
                &tex,
                &[format!("{}/{skin}", def.path), skin.clone()],
                &specs,
            )?);
            meshes.push(Some(Mesh { upload_identity: None, surface: material_layers.last().cloned(),
                vertices: surface
                    .vertices
                    .iter()
                    .map(|v| Vertex {
                        position: Vec3::ZERO,
                        uv: v.uv,
                        color: [255; 4],
                        normal: Vec4::ZERO,
                    })
                    .collect(),
                indices: surface.indices.clone(),
                texture: Some(tex),
            }));
        }
        let (face, face_art) = crate::facial::Face::load(
            assets,
            "models/alice.tik",
            &def,
            &skeleton,
            &meshes,
            &specs,
        )?.into_parts();
        let material = skin_material()?;
        println!(
            "Alice: {} bones, {} visible surfaces, {} locomotion clips",
            skeleton.bones.len(),
            meshes.iter().flatten().count(),
            clips.len()
        );
        let tag = |name| {
            skeleton
                .bones
                .iter()
                .position(|b| b.name == name)
                .with_context(|| format!("Missing Alice attachment {name}"))
        };
        let tags = [
            tag("tag_weapon")?,
            tag("tag_ball_linked")?,
            tag("tag_ball_free")?,
        ];
        let power_pose = crate::power_pose::Performance::load(assets, &def, skeleton.bones.len())?;
        let power_art = crate::power_pose::Art::load(assets, &skeleton, &meshes, &specs)?;
        let water_art = crate::water_art::Art::load(assets, &skeleton, &specs)?;
        let mut upper = Vec::new();
        for b in &skeleton.bones {
            upper.push(b.name == "Bip01 Spine" || b.parent.is_some_and(|p| upper[p]));
        }
        let mut animator = Animator::new(clips);
        animator.scale = def.scale;
        let foot_rig = crate::footing::Rig::new(&skeleton, &animator.pose, def.scale);
        let actions = Actions::load(assets, skeleton.bones.len(), &animator.pose)?;
        let (visuals, visual_art) = Visuals::load(assets)?;
        let (acting, acting_art) = crate::acting::Acting::load(assets, &def, &skeleton, &specs)?;
        let sim = AliceSim {
            body_arena: Default::default(),
            body_scratch: Vec::new(),
            weapon_notice: None,
            foot_rig,
            foot_state: Default::default(),
            acting,
            face,
            mouth: 0.,
            face_time: 0.,
            sound_events: crate::audio::events::Model::load(assets, "models/alice.tik")?,
            power_pose,
            power_remaining: 0.,
            skeleton: Arc::clone(&skeleton),
            animator,
            scale: def.scale,
            facing: 0.,
            actions,
            visuals,
            upper,
            tags,
            anchors: [Transform {
                rotation: Quat::IDENTITY,
                translation: Vec3::ZERO,
            }; 3],
            viewmodel: crate::viewmodel::ViewModel::default(),
            water: Default::default(),
        };
        let art = AliceArt {
            snapshot_visit: Default::default(),
            body_input: None,
            skin_identity: Default::default(),
            _material_layers: material_layers,
            power_art,
            water_art,
            meshes,
            material,
            visual_art,
            skeleton,
            face: face_art,
            acting: acting_art,
            #[cfg(any(debug_assertions, not(target_os = "vita")))]
            require_body_input: false,
        };
        Ok(Self { sim, art })
    }

    pub fn reset(&mut self, player: &Player, yaw: f32) {
        self.retire_body_input();
        self.sim.water = Default::default();
        self.art.water_art.reset_emitter();
        self.water_appearance(player);
        self.sim.reset(player, yaw);
        self.art.visual_art.clear();
    }

    pub fn notarget(&mut self, value: bool) {
        self.sim.notarget(value)
    }

    pub fn facing(&self) -> f32 {
        self.sim.facing()
    }

    pub fn rage_scene(&mut self, time: f32, finished: bool, player: &Player) {
        self.sim.rage_scene(time, finished, player)
    }

    /// Resume from the actual last cinematic pose without clearing live effects.
    /// A distant cutaway or a scaled Alice cannot seed an ordinary body pose.
    pub fn resume_scene(
        &mut self,
        player: &Player,
        yaw: f32,
        pose: Option<&crate::cinematic::ActorPose>,
    ) {
        self.sim.resume_scene(player, yaw, pose)
    }

    pub fn cancel_weapon_action(&mut self) {
        self.sim.cancel_weapon_action()
    }

    pub fn update(
        &mut self,
        dt: f32,
        player: &Player,
        running: bool,
        weapon: WeaponInput,
        context: &crate::combat::Context<'_>,
    ) -> Vec<&'static str> {
        if !dt.is_finite() || dt <= 0. {
            self.sim.animator.sound_span = None;
            return Vec::new();
        }
        self.update_funded(dt, player, running, weapon, context, None)
    }

    pub fn update_funded(
        &mut self,
        dt: f32,
        player: &Player,
        running: bool,
        weapon: WeaponInput,
        context: &crate::combat::Context<'_>,
        stats: Option<&mut crate::inventory::Stats>,
    ) -> Vec<&'static str> {
        self.water_appearance(player);
        self.sim.update_funded(dt, player, running, weapon, context, stats)
    }

    pub fn ice_locked(&self) -> bool {
        self.sim.ice_locked()
    }

    pub fn attack_movement_locked(&self) -> bool {
        self.sim.attack_movement_locked()
    }

    pub fn ice_targets(&self) -> Vec<crate::combat::Target> {
        self.sim.ice_targets()
    }

    pub fn croquet_contacts(&self) -> bool {
        self.sim.croquet_contacts()
    }

    pub fn hit_ice(&mut self, hit: crate::combat::Hit) -> bool {
        self.sim.hit_ice(hit)
    }

    pub fn audio_loops(&self) -> Vec<crate::audio::LoopCue> {
        self.sim.audio_loops()
    }

    pub fn take_world_audio(&mut self) -> Vec<(&'static str, Vec3)> {
        self.sim.take_world_audio()
    }

    pub fn take_audio(&mut self) -> Vec<crate::audio::events::Cue> {
        self.sim.take_audio()
    }

    pub fn ready_to_attack(&self, selected: usize) -> bool {
        self.sim.ready_to_attack(selected)
    }

    pub fn summon_target(&self) -> Option<crate::combat::Target> {
        self.sim.summon_target()
    }

    pub fn ally_target(&self) -> Option<crate::combat::Target> {
        self.sim.ally_target()
    }

    pub fn dismiss_summon(&mut self) {
        self.sim.dismiss_summon()
    }

    pub fn hit_summon(&mut self, hit: crate::combat::Hit) {
        self.sim.hit_summon(hit)
    }

    pub fn threatens(&self, world: &World, target: crate::combat::Target) -> bool {
        self.sim.threatens(world, target)
    }

    pub fn take_hits(&mut self) -> Vec<crate::combat::Hit> {
        self.sim.take_hits()
    }

    pub fn atmosphere(&self, atmosphere: &crate::environment::Atmosphere, camera: Vec3) {
        self.art.atmosphere(atmosphere, camera);
    }

    pub fn world_time(&mut self, dt: f32, world_dt: f32) {
        self.sim.world_time(dt, world_dt)
    }

    pub fn power_appearance(&mut self, stats: &crate::inventory::Stats) {
        self.sim.power_appearance(stats)
    }

    pub fn water_appearance(&mut self, player: &Player) {
        // Emitter history stays on the serial path at the original sync point.
        if self.sim.water.sync(player) {
            self.art.water_art.reset_emitter();
        }
    }

    pub fn story_pose(&mut self, story: &crate::story::Story, dt: f32) {
        self.sim.story_pose(story, dt)
    }

    fn retire_body_input(&mut self) {
        self.clear_body_input();
        self.art.snapshot_visit = Default::default();
    }

    pub(crate) fn clear_body_input(&mut self) {
        if let Some(input) = self.art.body_input.take() {
            self.sim.body_arena = input;
        }
    }

    pub(crate) fn extract_body(&mut self, feet: Vec3) -> (usize, bool) {
        self.clear_body_input();
        let summon_art = self.has_live_summon().then(|| self.art.visual_art.summon_art());
        let (bytes, overflow) = self.sim.extract_body(feet, self.art.snapshot_visit.id(), summon_art);
        if !overflow {
            self.art.body_input = Some(std::mem::take(&mut self.sim.body_arena));
        }
        (bytes, overflow)
    }

    pub(crate) fn has_live_summon(&self) -> bool {
        self.sim.visuals.dice.demon.is_some()
    }

    pub(crate) fn take_weapon_notice(&mut self) -> Option<&'static str> {
        self.sim.weapon_notice.take()
    }

    #[cfg(any(debug_assertions, not(target_os = "vita")))]
    pub(crate) fn require_body_input(&mut self, required: bool) {
        self.art.require_body_input = required;
    }

    pub fn draw(&mut self, feet: Vec3, fullbright: bool) {
        self.draw_ghost(feet, fullbright, false);
    }

    pub fn draw_ghost(&mut self, feet: Vec3, fullbright: bool, ghost: bool) {
        if crate::draw_diagnostics::skip(crate::draw_diagnostics::Class::LiveActors) { return; }
        let material = self.art.material.clone();
        material.draw(Appearance { ghost, ..Default::default() }, || {
            let input = self.art.body_input.take();
            if let Some(input) = input.as_ref().filter(|input| self.art.valid_body_input(input)) {
                self.art.draw(input, fullbright, ghost);
            } else {
                self.art.assert_live_fallback();
                self.art.draw_live(&self.sim, feet, fullbright, ghost);
            }
            self.art.body_input = input;
        });
        if !skinning::overlap::gather_only() { gl_use_default_material(); }
    }

    pub fn draw_effects(&mut self, camera: Vec3, fullbright: bool, show_alice: bool) {
        let art = &mut self.art;
        let input = art.body_input.take();
        if let Some(input) = input.as_ref().filter(|input| art.valid_body_input(input)) {
            art.visual_art.draw_effects_input(&input.effects, &art.material, camera, fullbright, show_alice);
        } else {
            art.assert_live_fallback();
            art.visual_art.draw_effects(&self.sim.visuals, &art.material, camera, fullbright, show_alice);
        }
        art.body_input = input;
    }

    pub fn draw_first_person(&mut self, fullbright: bool) {
        let input = self.art.body_input.take();
        if let Some(input) = input.as_ref().filter(|input| self.art.valid_body_input(input)) {
            self.art.draw_viewmodel(input, fullbright);
        } else {
            self.art.assert_live_fallback();
            self.art.draw_first_person_live(&self.sim, fullbright);
        }
        self.art.body_input = input;
    }

    pub fn visual_counts(&self) -> (u64, u64) {
        self.sim.visual_counts()
    }

    pub fn lights(&self) -> Vec<crate::lighting::Light> {
        self.sim.lights()
    }

    /// Renderer regression after a staged scene; keep the live shared pipeline.
    pub(crate) async fn check_visible(&mut self, label: &str) -> Result<()> {
        let player = crate::movement::Player::new(Vec3::ZERO);
        self.reset(&player, 0.);
        visibility_camera(Vec3::ZERO, 1.);
        self.atmosphere(&crate::environment::Atmosphere::default(), player.eye());
        let before = get_screen_data();
        self.draw_ghost(Vec3::ZERO, true, false);
        let pixels = visible_pixels(&before, &visibility_image());
        anyhow::ensure!(
            pixels > 200,
            "Gameplay Alice missing after {label}: {pixels} pixels"
        );
        gl_use_default_material();
        set_default_camera();
        next_frame().await;
        println!("PASS gameplay Alice after {label}: {pixels} pixels");
        Ok(())
    }
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct Snapshot {
    #[serde(default)]
    power: crate::power_pose::State,
    #[serde(default)]
    feet: crate::footing::State,
    #[serde(default)]
    acting: crate::acting::State,
    #[serde(default)]
    face_time: f32,
    animator: AnimatorSave,
    facing: f32,
    actions: crate::weapons::Snapshot,
    projectiles: crate::weapons::ProjectileSave,
    anchors: [Transform; 3],
    viewmodel: crate::viewmodel::ViewModel,
}
/// The transformation Alice wears for these stats: none while dead or without a power-up.
pub fn power_form(stats: &crate::inventory::Stats) -> Option<crate::power_pose::Form> {
    if !stats.alive() {
        None
    } else if stats.powers.rage > 0. {
        Some(crate::power_pose::Form::Rage)
    } else if stats.powers.tea > 0. {
        Some(crate::power_pose::Form::Tea)
    } else {
        None
    }
}
impl Snapshot {
    /// The shape checks `Character::restore` applies before it accepts a saved block; they
    /// depend only on the rig's bone count.
    pub fn validate(&self, bones: usize) -> Result<()> {
        self.power.validate(bones)?;
        self.feet.validate()?;
        anyhow::ensure!(
            self.animator.pose.len() == bones
                && self.animator.previous.len() == bones
                && self.animator.last_feet.is_none_or(|p| p.is_finite())
                && self
                    .animator
                    .last_rope_length
                    .is_none_or(|n| n.is_finite() && n >= 0.)
                && self.animator.time.is_finite()
                && self.animator.time >= 0.
                && (0.0..=1.).contains(&self.animator.transition),
            "Invalid saved Alice animation"
        );
        anyhow::ensure!(
            self.face_time.is_finite() && self.face_time >= 0.,
            "Invalid saved facial clock"
        );
        self.viewmodel.validate()
    }
    /// What `Character::restore` followed by `Restored::build`'s `power_appearance` leaves in
    /// Alice, computed without a window: the same validation, the real `Actions::restore` on
    /// `actions` (a rig of `bones` bones), and the same upgrades. The idle-gesture clip lookup
    /// of `Acting::restore` needs the loaded animations and is not repeated.
    pub fn restored_headless(
        &self,
        bones: usize,
        actions: &mut crate::weapons::Actions,
        stats: &crate::inventory::Stats,
    ) -> Result<Self> {
        self.validate(bones)?;
        self.projectiles.validate()?;
        actions.restore(&self.actions)?;
        let mut out = self.clone();
        out.projectiles = self.projectiles.migrated();
        out.power.sync(power_form(stats));
        if !stats.alive() {
            out.projectiles = out.projectiles.dismissed();
        }
        if stats.equipped().is_none() {
            actions.unarm(&self.animator.pose);
        }
        out.actions = actions.snapshot();
        Ok(out)
    }
    /// The block a restored Alice saves again for a player with these stats.
    ///
    /// Restoring never rewrites a valid block, except for these deliberate upgrades of state
    /// that older saves lack: the power-up presentation absent from saves before the
    /// transformation performances is initialised to the stats' current form without
    /// replaying its pickup performance (`power_appearance`); an unequipped player drops the
    /// toy pose; a Blade cooldown from before the recovery clock becomes that clock; legacy
    /// projectiles take their current kinds; and a dead player's summon is dismissed. Every
    /// other field must come back exactly as saved, so legacy-fixture checks compare against
    /// this and never against the raw old block.
    pub fn migrated(&self, stats: &crate::inventory::Stats) -> Self {
        let mut s = self.clone();
        s.power.sync(power_form(stats));
        s.actions = s.actions.migrated();
        if stats.equipped().is_none() {
            s.actions = s.actions.unarmed(&s.animator.pose);
        }
        s.projectiles = s.projectiles.migrated();
        if !stats.alive() {
            s.projectiles = s.projectiles.dismissed();
        }
        s
    }
}
#[derive(Clone, serde::Serialize, serde::Deserialize)]
struct AnimatorSave {
    #[serde(default)]
    last_feet: Option<Vec3>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    last_rope_length: Option<f32>,
    motion: Motion,
    time: f32,
    transition: f32,
    previous: Vec<Transform>,
    pose: Vec<Transform>,
    jumps: u64,
    landings: u64,
    large: bool,
    offset: f32,
    previous_offset: f32,
}
/// One GPU pipeline, with effects explicitly owned by each draw. Do not expose
/// the raw Material: a queued flush can restore an earlier actor's uniforms.
#[derive(Clone)]
pub(crate) struct SkinMaterial(std::rc::Rc<Material>);

#[derive(Clone, Copy, Default, PartialEq)]
struct Appearance {
    dissolve: f32,
    ghost: bool,
    power: f32,
}

impl SkinMaterial {
    pub(crate) fn atmosphere(&self, atmosphere: &crate::environment::Atmosphere, eye: Vec3) {
        if skinning::overlap::gather_only() { return; }
        #[cfg(feature="vita_renderer")]
        if crate::render_fx::retained_active()&&crate::render::vita_renderer::config().actor_fast_path{return;}
        atmosphere.apply(&self.0, eye);
    }

    /// Every ordinary actor/prop starts opaque, even after a frame/readback flush.
    pub(crate) fn bind(&self) {
        self.bind_appearance(Appearance::default());
    }

    fn bind_appearance(&self, appearance: Appearance) {
        if skinning::overlap::gather_only() { return; }
        crate::render_fx::appearance(crate::render_fx::Appearance {
            dissolve: appearance.dissolve,
            ghost: appearance.ghost,
            power: appearance.power,
        });
        #[cfg(feature="vita_renderer")]
        if crate::render_fx::retained_active()&&crate::render::vita_renderer::config().actor_fast_path{return;}
        self.0.set_uniform("Dissolve", appearance.dissolve);
        self.0
            .set_uniform("Ghost", if appearance.ghost { 1_f32 } else { 0_f32 });
        self.0.set_uniform("Power", appearance.power);
        gl_use_material(&self.0);
    }

    fn draw(&self, appearance: Appearance, draw: impl FnOnce()) {
        self.bind_appearance(appearance);
        draw();
        if skinning::overlap::gather_only() { return; }
        if appearance != Appearance::default() && !crate::render_fx::retained_active() {
            // Macroquad writes queued uniforms back into the shared pipeline.
            // Flush before resetting, including when no later actor is drawn.
            unsafe {
                {
                    #[cfg(target_os = "vita")]
                    crate::frame_profile::flush_boundary("flush_skin_appearance");
                    #[cfg(target_os = "vita")]
                    let _flush_profile = crate::frame_profile::span("gl_flush");
                    #[cfg(target_os = "vita")]
                    let _flush_site_profile = crate::frame_profile::span("flush_skin_appearance");

                    get_internal_gl().flush();
                }
            }
            self.bind();
        } else if appearance != Appearance::default() {
            // Each retained packet already captured Appearance; reset without a flush.
            self.bind();
        }
    }

    pub(crate) fn draw_dissolving(&self, amount: f32, draw: impl FnOnce()) {
        self.draw(
            Appearance {
                dissolve: amount.clamp(0., 1.),
                ..Default::default()
            },
            draw,
        );
    }
}

/// Staged screenshots exercise the same skin, attachment and gesture rendering as play.
pub async fn render_animations(assets: &mut Assets) -> Result<()> {
    let mut alice = Character::load(assets)?;
    let mut legacy = serde_json::to_value(alice.snapshot())?;
    legacy.as_object_mut().unwrap().remove("acting");
    alice.restore(&serde_json::from_value(legacy)?)?;
    let specs = texture::read_materials(assets)?;
    for (clip, time, weapon) in [
        ("knife_idle3", 1., 0),
        ("cards_idle2", 2., 1),
        ("mallet_idle3", 1.5, 2),
        ("jacks_idle2", 0.8, 5),
        ("dice_idle2", 2., 6),
        ("staff_idle2", 0.8, 7),
        ("talk_gnomes_01", 0.8, crate::weapons::UNARMED),
        ("pain_front", 0.2, 0),
    ] {
        alice.sim.actions.selected = weapon;
        alice.sim.actions.playing = None;
        alice.sim.acting.stage(clip, time, weapon)?;
        alice.sim.actions.pose = alice.sim.animator.pose.clone();
        alice
            .sim.acting
            .apply(&mut alice.sim.actions.pose, &alice.sim.upper, true);
        let pose = alice.sim.skeleton.global_pose(&alice.sim.actions.pose);
        alice.sim.anchors = alice.sim.tags.map(|i| Transform {
            rotation: pose[i].rotation,
            translation: pose[i].translation * alice.sim.scale,
        });
        for frame in 0..3 {
            animation_camera(clip);
            alice.draw(Vec3::ZERO, true);
            if frame == 2 {
                crate::viewer::save_capture(std::path::Path::new(&format!(
                    "private/animation-{clip}.png"
                )))?;
            }
            next_frame().await;
        }
    }
    for name in ["c_torchgnome", "c_cheshire", "c_madhatter"] {
        let mut actor = crate::npc::Puppet::load(assets, name, &[], &specs)?;
        let def = Definition::load(assets, &format!("models/{name}.tik"))?;
        let skeleton = Skeleton::parse(&assets.read(&format!("{}/{}", def.path, def.model))?)?;
        let idle = Animation::parse(
            &assets.read(&format!(
                "{}/{}",
                def.path,
                def.animations[actor.idle_clip()]
            ))?,
            skeleton.bones.len(),
        )?;
        let (min, max) = (
            idle.frames[0].min * def.scale,
            idle.frames[0].max * def.scale,
        );
        for (label, speaking, time) in [("rest", false, 0.), ("gesture", true, 3.)] {
            for frame in 0..3 {
                animation_camera_bounds(name, min, max);
                alice.art.material.bind();
                actor.draw_performance(
                    speaking,
                    time,
                    Transform {
                        translation: Vec3::ZERO,
                        rotation: Quat::IDENTITY,
                    },
                    1.,
                    true,
                );
                gl_use_default_material();
                if frame == 2 {
                    crate::viewer::save_capture(std::path::Path::new(&format!(
                        "private/animation-{name}-{label}.png"
                    )))?;
                }
                next_frame().await;
            }
        }
    }
    let school = crate::bsp::Bsp::parse(&assets.read("maps/skool1.bsp")?)?;
    let books = school
        .entities
        .iter()
        .find(|e| e.get("targetname").is_some_and(|n| n == "book_stack"))
        .and_then(|e| e.get("model"))
        .context("Missing authored book stack")?
        .trim_start_matches("models/")
        .trim_end_matches(".tik");
    for (name, clip) in [(books, "sway"), ("sky_watch", "handsmove")] {
        let def = Definition::load(assets, &format!("models/{name}.tik"))?;
        let model = crate::ambient_animation::read(assets, &def, clip)?;
        let (min, max) = model
            .surfaces
            .iter()
            .flat_map(|s| s.frames.iter().flatten())
            .fold(
                (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY)),
                |(min, max), p| (min.min(*p * def.scale), max.max(*p * def.scale)),
            );
        let mut prop = crate::weapons::Prop::load_animation(assets, name, clip, &specs)?;
        for (label, time) in [("start", 0.), ("moving", 1.5)] {
            for frame in 0..3 {
                animation_camera_bounds(name, min, max);
                alice.art.material.bind();
                prop.draw_frame(
                    Transform {
                        translation: Vec3::ZERO,
                        rotation: Quat::IDENTITY,
                    },
                    1.,
                    true,
                    time,
                    true,
                );
                gl_use_default_material();
                if frame == 2 {
                    crate::viewer::save_capture(std::path::Path::new(&format!(
                        "private/animation-{name}-{label}.png"
                    )))?;
                }
                next_frame().await;
            }
        }
    }
    println!("PASS animation render: eight Alice gestures with props, three NPCs and two moving scenery models");
    Ok(())
}
fn animation_camera(label: &str) {
    animation_camera_bounds(label, vec3(-25., -25., 0.), vec3(25., 25., 80.));
}
fn animation_camera_bounds(label: &str, min: Vec3, max: Vec3) {
    clear_background(Color::new(0.10, 0.12, 0.15, 1.));
    set_default_camera();
    draw_text(label, 20., 30., 24., WHITE);
    let target = (min + max) * 0.5;
    let distance = (max - min).length().max(1.) * 1.8;
    set_camera(&Camera3D {
        position: target + vec3(1., -1., 0.35).normalize() * distance,
        target,
        up: Vec3::Z,
        fovy: 40_f32.to_radians(),
        z_near: 0.5,
        z_far: distance * 10.,
        ..Default::default()
    });
}
thread_local! {
    // Every actor/prop uses this same pipeline. Keep ownership weak in the cache
    // so it is released with the last scene, but never allocate one per actor or
    // duplicate it while a quickload retains the previous complete game.
    static SKIN_MATERIAL: std::cell::RefCell<std::rc::Weak<Material>> = Default::default();
}
pub(crate) fn skin_material() -> Result<SkinMaterial> {
    SKIN_MATERIAL.with(|cache| {
        if let Some(material) = cache.borrow().upgrade() {
            return Ok(SkinMaterial(material));
        }
        let material = crate::frame_profile::load_material(
            ShaderSource::Glsl {
                vertex: VERTEX,
                fragment: &crate::environment::fragment(FRAGMENT),
            },
            MaterialParams {
                uniforms: {
                    let mut u = crate::environment::uniforms();
                    u.push(UniformDesc::new("Dissolve", UniformType::Float1));
                    u.push(UniformDesc::new("Ghost", UniformType::Float1));
                    u.push(UniformDesc::new("Power", UniformType::Float1));
                    u
                },
                pipeline_params: PipelineParams {
                    depth_test: Comparison::LessOrEqual,
                    depth_write: true,
                    ..Default::default()
                },
                ..Default::default()
            },
        )
        .map_err(|e| anyhow::anyhow!("Character shader: {e:?}"))?;
        let material = std::rc::Rc::new(material);
        *cache.borrow_mut() = std::rc::Rc::downgrade(&material);
        Ok(SkinMaterial(material))
    })
}

/// Shared pipelines must retain per-draw uniforms: Alice's powers must not tint
/// later NPCs/props, and no owner may consume another pipeline slot.
pub async fn check_shared_material() -> Result<()> {
    let owners = (0..64)
        .map(|_| skin_material())
        .collect::<Result<Vec<_>>>()?;
    anyhow::ensure!(
        owners
            .iter()
            .all(|m| std::rc::Rc::ptr_eq(&m.0, &owners[0].0)),
        "Actor shaders were duplicated"
    );
    clear_background(BLACK);
    let camera = Camera3D {
        position: vec3(0., 0., 10.),
        target: Vec3::ZERO,
        up: Vec3::Y,
        projection: Projection::Orthographics,
        fovy: 4.,
        z_near: 0.1,
        z_far: 50.,
        ..Default::default()
    };
    set_camera(&camera);
    let white = crate::texture::upload_rgba8(1, 1, &[255; 4]);
    let quad = |x: f32| Mesh { upload_identity: None, surface: None,
        vertices: [
            vec3(x - 0.7, -1., 0.),
            vec3(x + 0.7, -1., 0.),
            vec3(x + 0.7, 1., 0.),
            vec3(x - 0.7, 1., 0.),
        ]
        .into_iter()
        .map(|position| Vertex {
            position,
            uv: Vec2::ZERO,
            normal: Vec4::ZERO,
            color: [255; 4],
        })
        .collect(),
        indices: vec![0, 1, 2, 0, 2, 3],
        texture: Some(white.clone()),
    };
    for (index, (x, power)) in [(-1_f32, 1_f32), (1., 0.)].into_iter().enumerate() {
        let m = &owners[index];
        m.atmosphere(
            &crate::environment::Atmosphere::default(),
            vec3(0., 0., 10.),
        );
        m.bind_appearance(Appearance {
            power,
            ..Default::default()
        });
        draw_mesh(&quad(x));
    }
    let frame = get_screen_data();
    let sample = |x: f32| {
        frame.get_pixel(
            (frame.width as f32 * 0.5 + x * frame.height as f32 / 4.) as u32,
            frame.height as u32 / 2,
        )
    };
    let (powered, plain) = (sample(-1.), sample(1.));
    anyhow::ensure!(
        powered.r > 0.98
            && powered.g < 0.8
            && powered.b < 0.7
            && plain.r > 0.98
            && plain.g > 0.98
            && plain.b > 0.98,
        "Shared character power leaked between draws: {powered:?} / {plain:?}"
    );
    gl_use_default_material();
    set_default_camera();
    next_frame().await;
    for amount in [0.5_f32, 1.] {
        clear_background(BLACK);
        set_camera(&camera);
        let material = &owners[0];
        material.draw_dissolving(amount, || draw_mesh(&quad(0.)));
        let faded = get_screen_data();
        let mut visible = 0;
        for y in 0..32 {
            for x in 0..32 {
                visible += usize::from(
                    faded
                        .get_pixel(faded.width as u32 / 2 + x, faded.height as u32 / 2 + y)
                        .r
                        > 0.9,
                );
            }
        }
        anyhow::ensure!(
            if amount == 1. {
                visible == 0
            } else {
                visible == 512
            },
            "Cat dissolve lost its intended coverage: {amount}, {visible} pixels"
        );
        next_frame().await;
        clear_background(BLACK);
        set_camera(&camera);
        // Another actor shares this material and must not inherit the prior frame's fade.
        owners[1].bind();
        draw_mesh(&quad(0.));
        let restored = get_screen_data();
        anyhow::ensure!(
            restored
                .get_pixel(restored.width as u32 / 2, restored.height as u32 / 2)
                .r
                > 0.98,
            "Cat fade leaked into another actor after the frame boundary"
        );
        next_frame().await;
    }
    gl_use_default_material();
    set_default_camera();
    println!("PASS 64 character/prop owners share one pipeline; power tint remains per draw");
    println!("PASS partial/full Cat dissolve clears before the next actor/frame");
    Ok(())
}

/// GPU regression using the actual player and cinematic Alice meshes. Deliberately
/// leave stale uniforms behind, as a final effect draw or readback can do.
pub async fn check_visibility(assets: &mut Assets) -> Result<()> {
    check_shared_material().await?;
    for queued in [false, true] {
        let material = skin_material()?;
        let mut alice = Character::load(assets)?;
        let specs = texture::read_materials(assets)?;
        let mut puppet = crate::npc::Puppet::load(assets, "alice", &["idle_stand"], &specs)?;
        let player = crate::movement::Player::new(Vec3::ZERO);
        alice.reset(&player, 0.);
        let mut counts = Vec::new();
        let mut rendered = Vec::new();
        for poison in [
            Appearance::default(),
            Appearance {
                dissolve: 1.,
                ghost: true,
                power: 2.,
            },
        ] {
            for kind in 0..4 {
                material.bind_appearance(poison);
                // Survives a frame boundary and a readback before the actor binds.
                next_frame().await;
                visibility_camera(Vec3::ZERO, 1.);
                if !queued {
                    crate::render_fx::finish();
                }
                let before = get_screen_data();
                material.bind_appearance(poison);
                material.atmosphere(&crate::environment::Atmosphere::default(), Vec3::ZERO);
                match kind {
                    0 => alice.draw(Vec3::ZERO, true),
                    1 => alice.draw_ghost(Vec3::ZERO, true, false),
                    2 => puppet.draw(
                        "idle_stand",
                        0.,
                        true,
                        Transform {
                            translation: Vec3::ZERO,
                            rotation: Quat::IDENTITY,
                        },
                        1.,
                        true,
                    ),
                    _ => puppet.draw_performance(
                        false,
                        0.,
                        Transform {
                            translation: Vec3::ZERO,
                            rotation: Quat::IDENTITY,
                        },
                        1.,
                        true,
                    ),
                }
                let after = visibility_image();
                let pixels = visible_pixels(&before, &after);
                anyhow::ensure!(
                    pixels > 200,
                    "Alice draw path {kind} invisible: {pixels} pixels"
                );
                counts.push(pixels);
                rendered.push(after.bytes);
                next_frame().await;
            }
        }
        anyhow::ensure!(
            counts[..4] == counts[4..] && rendered[..4] == rendered[4..],
            "Effects changed later Alice coverage: {counts:?}"
        );
        // Intentional invisibility remains partial; it must not affect the next body.
        visibility_camera(Vec3::ZERO, 1.);
        if !queued {
            crate::render_fx::finish();
        }
        let before = get_screen_data();
        alice.draw_ghost(Vec3::ZERO, true, true);
        let ghost = visible_pixels(&before, &visibility_image());
        anyhow::ensure!(
            ghost > 0 && ghost < counts[0] / 2,
            "Glass ghost coverage invalid: {ghost}"
        );
        next_frame().await;
        visibility_camera(Vec3::ZERO, 1.);
        if !queued {
            crate::render_fx::finish();
        }
        let before = get_screen_data();
        alice.draw(Vec3::ZERO, true);
        anyhow::ensure!(
            visible_pixels(&before, &visibility_image()) == counts[0],
            "Ghost leaked to next frame"
        );
        gl_use_default_material();
        set_default_camera();
        next_frame().await;
        println!("PASS actual Alice/player/puppet meshes recover from stale dissolve, ghost and power at frame/readback boundaries; intentional Glass ghost preserved (queued={queued})");
        let end_pose = puppet
            .handoff_pose()
            .context("Missing cinematic handoff pose")?;
        alice.resume_scene(&player, 0., Some(end_pose));
        anyhow::ensure!(
            alice
                .sim.animator
                .pose
                .iter()
                .zip(&end_pose.local)
                .all(|(a, b)| a.translation == b.translation && a.rotation == b.rotation),
            "Cinematic pose was lost before gameplay resumed"
        );
        let saved = alice.snapshot();
        alice.restore(&serde_json::from_slice(&serde_json::to_vec(&saved)?)?)?;
        anyhow::ensure!(
            alice.sim.animator.transition == 0.,
            "Pose blend lost after saving"
        );
        visibility_camera(player.feet, 1.);
        let before = get_screen_data();
        alice.draw(player.feet, true);
        anyhow::ensure!(
            visible_pixels(&before, &visibility_image()) > 200,
            "Alice vanished during pose handoff"
        );
        next_frame().await;
        println!("PASS cinematic pose survives handoff and saved blend (queued={queued})");
    }
    Ok(())
}

pub(crate) fn visibility_camera(feet: Vec3, scale: f32) {
    clear_background(BLACK);
    let scale = scale.max(0.1);
    let target = feet + Vec3::Z * 35. * scale;
    set_camera(&Camera3D {
        position: target + vec3(120., -140., 45.) * scale,
        target,
        up: Vec3::Z,
        fovy: 50_f32.to_radians(),
        z_near: 0.1,
        z_far: 30000.,
        ..Default::default()
    });
    let eye = target + vec3(120., -140., 45.) * scale;
    crate::render_fx::begin(
        eye,
        (target - eye).normalize_or_zero(),
        0.,
        &crate::environment::Atmosphere::default(),
        true,
    );
}

pub(crate) fn visibility_image() -> Image {
    if crate::render_fx::active() {
        crate::render_fx::finish();
    }
    get_screen_data()
}

pub(crate) fn visible_pixels(before: &Image, after: &Image) -> usize {
    before
        .bytes
        .chunks_exact(4)
        .zip(after.bytes.chunks_exact(4))
        .filter(|(a, b)| a[..3] != b[..3])
        .count()
}

pub(crate) fn draw_skin(
    skeleton: &Skeleton,
    meshes: &mut [Option<Mesh>],
    pose: &[Transform],
    transform: Transform,
    scale: f32,
    fullbright: bool,
) -> bool {
    draw_skin_scaled(skeleton, meshes, pose, transform, scale, fullbright, &[])
}
pub(crate) fn draw_skin_scaled(
    skeleton: &Skeleton, meshes: &mut [Option<Mesh>], pose: &[Transform],
    transform: Transform, scale: f32, fullbright: bool, bone_scales: &[f32],
) -> bool {
    draw_skin_deformed(
        skeleton,
        meshes,
        pose,
        transform,
        scale,
        fullbright,
        (bone_scales, 1.),
    )
}
pub(crate) fn draw_skin_alpha(
    skeleton: &Skeleton, meshes: &mut [Option<Mesh>], pose: &[Transform],
    transform: Transform, scale: f32, fullbright: bool, alpha: f32,
) -> bool {
    draw_skin_deformed(
        skeleton,
        meshes,
        pose,
        transform,
        scale,
        fullbright,
        (&[], alpha),
    )
}
fn draw_skin_deformed(
    skeleton: &Skeleton, meshes: &mut [Option<Mesh>], pose: &[Transform],
    transform: Transform, scale: f32, fullbright: bool, (bone_scales, alpha): (&[f32], f32),
) -> bool {
    #[cfg(feature = "vita_renderer")]
    let use_palette = crate::render::vita_renderer::config().bone_palette;
    #[cfg(not(feature = "vita_renderer"))]
    let use_palette = false;
    #[cfg(feature = "vita_renderer")]
    let cull = crate::render::vita_renderer::config().skin_frustum_cull;
    #[cfg(not(feature = "vita_renderer"))]
    let cull = false;
    #[cfg(feature = "vita_renderer")]
    let flat = crate::render::vita_renderer::config().flat_actor_light;
    #[cfg(not(feature = "vita_renderer"))]
    let flat = false;
    let setup_profile = crate::frame_profile::span("skin_setup");
    let character_setup_profile = crate::frame_profile::characters_span("characters_skin_setup");
    let (cull, flat) = skinning::visual_options(cull, flat, meshes);
    let world_rotation = (use_palette || cull).then(|| skinning::rotation(transform.rotation));
    #[cfg(feature="vita_renderer")]
    if cull { crate::light_profile::render_add(crate::light_profile::RenderPhase::Actors,
        crate::light_profile::RenderMetrics { visibility_tests: 1, ..Default::default() }); }
    let visible = !(cull && skinning::outside_skin_bounds(cull, &skeleton.skin_bounds, pose, bone_scales,
        world_rotation.unwrap(), transform.translation, scale));
    #[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
    if std::env::var_os("LOOKING_GLASS_ACTOR_CENSUS").is_some() && crate::sim_maps::probe::frame()==300 {
        println!("[lg/skin-census] identity={:?} bones={} visible={visible} cull={cull} allowed={} preserved_materials={} palette={use_palette} mutable={} surfaces={} triangles={} gathering={}",
            skinning::identity::current(),pose.len(),skinning::culling_allowed(),meshes.iter().flatten().filter(|m|crate::render_fx::skin_requires_preservation(m)).count(),skinning::mutable_reasons(),meshes.iter().flatten().count(),meshes.iter().flatten().map(|m|m.indices.len()/3).sum::<usize>(),skinning::overlap::gather_only());
    }
    skinning::with_palette(use_palette, pose, bone_scales, |palette| {
        drop(character_setup_profile);
        drop(setup_profile);
        let params = skinning::Params { pose, scales: bone_scales, palette, transform,
            scale, flat, fullbright, alpha };
        if let Some(visible) = skinning::overlap::handle(skeleton, meshes, params, visible) {
            if !skinning::overlap::gathering() {
                crate::frame_profile::skinned_draw(!visible);
                if visible { skinning::overlap::submit(meshes); }
            }
            return visible;
        }
        crate::frame_profile::skinned_draw(!visible);
        if !visible { return false; }
        #[cfg(feature = "vita_renderer")]
        let workers = if crate::render::vita_renderer::config().skin_overlap { 0 }
            else { crate::render::vita_renderer::config().skin_workers };
        #[cfg(not(feature = "vita_renderer"))]
        let workers = 0;
        #[cfg(feature="vita_renderer")]
        let bench=crate::render::vita_renderer::config().skin_worker_bench;
        #[cfg(not(feature="vita_renderer"))]
        let bench=false;
        if (workers > 0 || bench) && skeleton.surfaces.len() <= 128 {
            let _batch = crate::frame_profile::span("skin_actor_batch");
            let mut source=skeleton.surfaces.iter().zip(meshes.iter_mut());
            let mut jobs: [Option<skinning::Job<'_>>;128]=std::array::from_fn(|_| {
                source.next().and_then(|(s,m)|m.as_mut().map(|m|skinning::Job {
                    input:&s.vertices,vertices:&mut m.vertices,indices:&m.indices }))
            });
            if bench { skinning::bench_capture(&jobs,params); }
            let [main,service,wait]=if workers>0 {skinning::run_slots(&mut jobs,params,workers)}
                else {for job in jobs.iter_mut().flatten() {skinning::kernel(job,params);} [0;3]};
            #[cfg(feature = "vita_renderer")]
            for (name, value) in [("skin_main_service_us", main),
                ("skin_worker_service_us", service), ("skin_join_wait_us", wait)] {
                crate::render_fx::retained::count(name, value);
            }
            #[cfg(not(feature = "vita_renderer"))]
            let _ = (main, service, wait);
            drop(jobs);
            drop(source);
            for mesh in meshes.iter().flatten() { crate::render_fx::skin(mesh); }
        } else {
            for (surface, mesh) in skeleton.surfaces.iter().zip(meshes.iter_mut()) {
                let Some(mesh) = mesh else { continue; };
                let _skin = crate::frame_profile::span("skinning");
                let positions = crate::frame_profile::characters_span("characters_skinning_positions");
                skinning::positions(&mut skinning::Job { input: &surface.vertices,
                    vertices: &mut mesh.vertices, indices: &mesh.indices }, params);
                drop(positions);
                skinning::shade(mesh, flat, fullbright, alpha);
                drop(_skin);
                crate::render_fx::skin(mesh);
            }
        }
        true
    })
}

/// Sweep a small camera box along the view arm to avoid looking through walls.
pub fn follow_camera(world: &World, feet: Vec3, direction: Vec3) -> (Vec3, Vec3, bool) {
    follow_camera_at(world, feet, direction, 132.)
}
pub fn follow_camera_at(
    world: &World,
    feet: Vec3,
    direction: Vec3,
    distance: f32,
) -> (Vec3, Vec3, bool) {
    crate::camera::Follow::default().update(world, feet, direction, distance, 0.)
}

pub(crate) const VERTEX: &str = r#"#version 100
attribute vec3 position;
attribute vec2 texcoord;
attribute vec4 color0;
uniform mat4 Model;
uniform mat4 Projection;
varying highp vec2 uv;
varying lowp vec4 color;
varying highp vec3 worldPosition;
void main() { worldPosition=(Model*vec4(position,1.0)).xyz; gl_Position = Projection * Model * vec4(position,1.0); uv=texcoord; color=color0/255.0; }
"#;
const FRAGMENT: &str = r#"#version 100
precision mediump float;
uniform sampler2D Texture;
varying highp vec2 uv;
varying lowp vec4 color;
// FOG
uniform float Dissolve;
uniform float Ghost;
uniform float Power;
void main() { if(Dissolve>0.0 && (mod(floor(gl_FragCoord.x)*3.0+floor(gl_FragCoord.y)*5.0,16.0)+0.5)/16.0<Dissolve) discard; vec4 t=texture2D(Texture,uv); if(t.a<0.35) discard; if(Ghost>0.5 && mod(floor(gl_FragCoord.x)+floor(gl_FragCoord.y),4.0)>0.5) discard; vec3 c=t.rgb*color.rgb; if(Power>1.5) c=mix(c,c*vec3(0.65,1.15,0.7),0.6); else if(Power>0.5) c=mix(c,c*vec3(1.3,0.55,0.4),0.6); gl_FragColor=vec4(fogged(c,0.0),1.0); }
"#;

/// Real clips, real swept controller, with presentation forbidden to move the body.
pub fn check_movement(assets: &mut Assets) -> Result<()> {
    use crate::movement::{Controls, FIXED_DT};
    use anyhow::ensure;
    let def = Definition::alice(assets)?;
    let skeleton = Skeleton::parse(&assets.read(&format!("{}/{}", def.path, def.model))?)?;
    let mut clips = Vec::new();
    for name in CLIPS {
        clips.push(Animation::parse(
            &assets.read(&format!("{}/{name}.ska", def.path))?,
            skeleton.bones.len(),
        )?);
    }
    for i in [1, 2, 7, 8, 9, 10, 22, 23, 30, 31] {
        let clip = &clips[i];
        let vertical = i >= 22;
        let distance = if vertical {
            clip.frames.iter().map(|f| f.delta.z.abs()).sum()
        } else {
            clip.distance
        };
        ensure!(distance > 0., "Missing movement data: {}", CLIPS[i]);
        let once = clip.advance_distance(0., distance * 2.37, vertical);
        for hz in [30, 60, 144] {
            let mut time = 0.;
            for _ in 0..hz {
                time = clip.advance_distance(time, distance * 2.37 / hz as f32, vertical);
            }
            ensure!(
                (once - time).abs() < 0.002,
                "Stride depends on frame rate: {} at {hz} Hz: {once} vs {time}",
                CLIPS[i]
            );
        }
    }
    crate::footing::check(&skeleton, &clips[0], &clips[7], def.scale)?;
    let mut a = Animator::new(clips);
    a.scale = def.scale;
    // Original paired strokes must keep the same phase with and without swing,
    // including render frames in which the fixed controller does not advance.
    for rise in [-1., 1.] {
        let mut reference: Option<f32> = None;
        for hz in [30, 60, 144] {
            let mut world = World::fixture(&[]);
            let anchor = Vec3::Z * 1000.;
            world.traversal.ropes.push(crate::traversal::Rope {
                id: crate::entity::Id(1),
                model: 1,
                enabled: true,
                origin: Vec3::ZERO,
                anchor,
                length: 1000.,
            });
            let mut p =
                Player::new(anchor + vec3(0.15, 0., -1.).normalize() * 500. - Vec3::Z * 40.);
            p.script_motion = 1;
            p.rope = Some(crate::movement::RopeGrip {
                id: crate::entity::Id(1),
                length: 500.,
                rise,
            });
            let mut clock = crate::movement::FixedClock::default();
            a.reset(&p);
            let index = if rise > 0. { 22 } else { 23 };
            let travel: f32 = [index, rope_pair(index).unwrap()]
                .into_iter()
                .flat_map(|i| &a.clips[i].frames)
                .map(|f| f.delta.z.abs())
                .sum();
            let mut phase = 0.;
            for _ in 0..hz * 2 {
                clock.advance(
                    1. / hz as f64,
                    &world,
                    &mut p,
                    Controls {
                        rise,
                        wish: Vec2::Y,
                        ..Default::default()
                    },
                );
                a.update(1. / hz as f32, &p, false);
                ensure!(
                    a.sound_clip == Some(index),
                    "Climb flickered to hang at {hz} Hz"
                );
                let span = a.sound_span.unwrap().1;
                phase += span.end - span.start;
            }
            let distance = (p.rope.as_ref().unwrap().length - 500.).abs();
            ensure!(
                (phase - distance / a.scale / travel * a.duration(index)).abs() < 0.002,
                "Swing changed climb cadence at {hz} Hz"
            );
            if let Some(previous) = reference {
                ensure!(
                    (phase - previous).abs() < 0.002,
                    "Climb cadence depends on render rate"
                );
            }
            reference = Some(phase);
        }
    }
    for (unarmed, large, index) in [(true, false, 1), (false, false, 7), (false, true, 9)] {
        a.unarmed = unarmed;
        a.large = large;
        println!(
            "Walk {}: {:.3} units in {:.3} s",
            CLIPS[index],
            a.clips[index].distance,
            a.duration(index)
        );
        for fraction in [0.125, 0.25, 0.5, 1., 2.] {
            let mut reference: Option<f32> = None;
            for hz in [30, 60, 144] {
                let world =
                    World::fixture(&[(vec3(-2000., -2000., -100.), vec3(2000., 2000., 0.))]);
                let mut p = Player::new(vec3(0., 0., 0.1));
                p.grounded = true;
                p.tea = fraction > 1.;
                p.velocity.x = crate::movement::WALK_SPEED * fraction;
                let mut clock = crate::movement::FixedClock::default();
                a.reset(&p);
                let mut elapsed = 0.;
                // Compare brisk cadence with real travel, including slow analog
                // input and render frames without a physics tick.
                for _ in 0..hz * 8 {
                    clock.advance(
                        1. / hz as f64,
                        &world,
                        &mut p,
                        Controls {
                            wish: Vec2::X * fraction.min(1.),
                            ..Default::default()
                        },
                    );
                    a.update(1. / hz as f32, &p, false);
                    if clock.ticks > 0 {
                        ensure!(
                            a.motion == Motion::Walk && a.sound_clip == Some(index),
                            "Gait stopped between physics ticks at {hz} Hz"
                        );
                        let span = a.sound_span.unwrap().1;
                        elapsed += span.end - span.start;
                    }
                }
                ensure!(
                    (p.feet.x - 8. * crate::movement::WALK_SPEED * fraction).abs() < 0.1,
                    "Walking speed changed at {hz} Hz"
                );
                let clip = &a.clips[index];
                let expected = clip.advance_distance(0., p.feet.x / a.scale, false);
                ensure!(
                    (elapsed - expected).abs() < 0.002,
                    "{} stride slipped at {fraction} input, {hz} Hz: {elapsed} vs {expected}",
                    CLIPS[index]
                );
                // Per-frame root curves are uneven (including stationary frames).
                // Check average playback over a complete stride, not the phase of
                // the short partial stride left at the end of this controller run.
                let rate = p.velocity.x / a.scale / clip.distance * clip.duration();
                ensure!(
                    (rate / (2. * fraction) - 1.).abs() < 0.02,
                    "{} full-stride cadence is {rate} at {fraction} input",
                    CLIPS[index]
                );
                if let Some(previous) = reference {
                    ensure!(
                        (elapsed - previous).abs() < 0.002,
                        "Walk cadence depends on render rate"
                    );
                }
                reference = Some(elapsed);
            }
        }
    }
    a.unarmed = false;
    a.large = false;
    let floor = (vec3(-2000., -2000., -100.), vec3(2000., 2000., 0.));
    let wall = World::fixture(&[floor, (vec3(180., -1000., 0.), vec3(200., 1000., 300.))]);
    let mut p = Player::new(vec3(0., 0., 0.1));
    a.reset(&p);
    let mut seen = std::collections::BTreeSet::new();
    for frame in 0..480 {
        p.tick(
            &wall,
            Controls {
                wish: Vec2::X,
                run: frame >= 55,
                jump: frame == 75,
                ..Default::default()
            },
        );
        let before = serde_json::to_vec(&p)?;
        a.update(FIXED_DT, &p, frame >= 55);
        ensure!(
            before == serde_json::to_vec(&p)? && wall.body_clear(p.feet),
            "Animation changed collision or entered solid"
        );
        seen.insert(format!("{:?}", a.motion));
        if frame == 130 {
            let saved = serde_json::to_vec(&a.snapshot())?;
            a.update(0., &p, true);
            ensure!(
                saved == serde_json::to_vec(&a.snapshot())?,
                "Paused locomotion changed"
            );
            a.restore(&serde_json::from_slice(&saved)?);
        }
    }
    for name in ["Walk", "Run", "Takeoff", "Air", "Idle"] {
        ensure!(
            seen.contains(name),
            "Missing movement phase {name}: {seen:?}"
        );
    }
    ensure!(
        p.feet.x < 165. && a.motion == Motion::Idle,
        "Blocked player kept walking"
    );
    let ledge = World::fixture(&[floor, (vec3(40., -100., 0.), vec3(180., 100., 48.))]);
    p = Player::new(vec3(0., 0., 0.1));
    a.reset(&p);
    let mut climbed = false;
    for _ in 0..240 {
        p.tick(
            &ledge,
            Controls {
                wish: Vec2::X,
                jump: true,
                ..Default::default()
            },
        );
        a.update(FIXED_DT, &p, false);
        ensure!(ledge.body_clear(p.feet), "Climb entered collision");
        if let Some(progress) = p.climb_progress() {
            climbed = true;
            ensure!(a.motion == Motion::Climb, "Climb clip missing");
            let duration = a.clips[a.sound_clip.unwrap()].duration();
            ensure!(
                (a.time - progress * duration).abs() < 0.001,
                "Climb animation fell behind controller"
            );
        }
    }
    ensure!(climbed, "Climb fixture never caught ledge");
    // Pandemonium owns its grip separately from Player::rope. Swinging changes
    // body height too, so only resolved travel along the rope selects a climb.
    p.rope = None;
    p.script_motion = 1;
    p.script_rope_length = Some(200.);
    for (rise, dz, expected) in [(1., 0.5, 22), (-1., -0.5, 23), (0., 0.5, 19), (0., 0., 19)] {
        p.script_rope_rise = rise;
        a.reset(&p);
        for _ in 0..60 {
            p.feet.z += dz;
            p.script_rope_length = p.script_rope_length.map(|l| l - rise * 0.5);
            a.update(FIXED_DT, &p, false);
        }
        ensure!(
            a.sound_clip == Some(expected),
            "Script rope selected wrong clip"
        );
    }
    for mode in [1, 2, 3] {
        p.script_motion = mode;
        p.rope = Some(crate::movement::RopeGrip {
            id: crate::entity::Id(0),
            length: 100.,
            rise: 1.,
        });
        a.reset(&p);
        for _ in 0..60 {
            p.feet += Vec3::Z * 0.5;
            a.update(FIXED_DT, &p, false);
        }
        ensure!(
            matches!(a.motion, Motion::Rope | Motion::Cart | Motion::Airship),
            "Missing script pose"
        );
        let saved = a.snapshot();
        let original = serde_json::to_vec(&saved)?;
        a.update(0., &p, false);
        ensure!(
            original == serde_json::to_vec(&a.snapshot())?,
            "Paused script pose changed"
        );
        p.feet += Vec3::Z * 0.5;
        a.update(FIXED_DT, &p, false);
        let expected = serde_json::to_vec(&a.snapshot())?;
        a.restore(&serde_json::from_slice(&original)?);
        a.update(FIXED_DT, &p, false);
        ensure!(
            expected == serde_json::to_vec(&a.snapshot())?,
            "Restored script movement differs"
        );
    }
    println!("PASS original stride curves and paired rope strokes at 30/60/144 Hz; swept walk/run/jump/wall/climb; rope/cart/airship pause and save continuation");
    Ok(())
}

pub async fn render_movement(assets: &mut Assets) -> Result<()> {
    use crate::movement::{Controls, FIXED_DT};
    use anyhow::ensure;
    check_movement(assets)?;
    let mut alice = Character::load(assets)?;
    let floor = (vec3(-2000., -2000., -100.), vec3(2000., 2000., 0.));
    for (label, solid, steps, jump) in [
        (
            "walk",
            (vec3(400., -100., 0.), vec3(450., 100., 300.)),
            43,
            false,
        ),
        (
            "wall",
            (vec3(65., -100., 0.), vec3(90., 100., 300.)),
            130,
            false,
        ),
        (
            "jump",
            (vec3(400., -100., 0.), vec3(450., 100., 300.)),
            25,
            true,
        ),
        (
            "climb",
            (vec3(40., -100., 0.), vec3(180., 100., 48.)),
            32,
            true,
        ),
    ] {
        let world = World::fixture(&[floor, solid]);
        let context = crate::combat::Context {
            world: &world,
            targets: &[],
        };
        let mut p = Player::new(vec3(0., 0., 0.1));
        alice.reset(&p, 0.);
        for frame in 0..steps {
            p.tick(
                &world,
                Controls {
                    wish: Vec2::X,
                    jump: jump && (frame == 1 || label == "climb"),
                    ..Default::default()
                },
            );
            alice.update(
                FIXED_DT,
                &p,
                false,
                WeaponInput {
                    selected: crate::weapons::UNARMED,
                    dice: 0,
                    click: None,
                    aim: Vec3::X,
                    first_person: false,
                },
                &context,
            );
        }
        let saved = serde_json::to_vec(&alice.snapshot())?;
        alice.update(
            0.,
            &p,
            false,
            WeaponInput {
                selected: crate::weapons::UNARMED,
                dice: 0,
                click: None,
                aim: Vec3::X,
                first_person: false,
            },
            &context,
        );
        ensure!(
            saved == serde_json::to_vec(&alice.snapshot())?,
            "Paused feet/pose changed"
        );
        let draw = |alice: &mut Character| {
            animation_camera_bounds(
                label,
                p.feet + vec3(-70., -70., 0.),
                p.feet + vec3(70., 70., 110.),
            );
            draw_cube(
                vec3(p.feet.x, p.feet.y, -1.),
                vec3(500., 500., 2.),
                None,
                Color::new(0.18, 0.22, 0.23, 1.),
            );
            draw_cube(
                (solid.0 + solid.1) * 0.5,
                solid.1 - solid.0,
                None,
                Color::new(0.28, 0.3, 0.32, 1.),
            );
            alice.draw(p.feet, true);
            gl_use_default_material();
        };
        for frame in 0..3 {
            draw(&mut alice);
            if frame == 2 {
                crate::viewer::save_capture(std::path::Path::new(&format!(
                    "private/movement-{label}.png"
                )))?;
            }
            next_frame().await;
        }
        draw(&mut alice);
        let original = get_screen_data();
        next_frame().await;
        alice.restore(&serde_json::from_slice(&saved)?)?;
        draw(&mut alice);
        let restored = get_screen_data();
        next_frame().await;
        ensure!(
            original.bytes == restored.bytes,
            "Restored {label} pose rendered differently"
        );
    }
    let world = World::fixture(&[]);
    let context = crate::combat::Context {
        world: &world,
        targets: &[],
    };
    for (name, rise) in [("up", 1.), ("down", -1.)] {
        let mut p = Player::new(Vec3::ZERO);
        p.script_motion = 1;
        p.script_rope_length = Some(500.);
        p.script_rope_rise = rise;
        alice.reset(&p, 0.);
        for frame in 0..120 {
            p.script_rope_length = p.script_rope_length.map(|l| l - rise * 100. * FIXED_DT);
            alice.update(
                FIXED_DT,
                &p,
                false,
                WeaponInput {
                    selected: crate::weapons::UNARMED,
                    dice: 0,
                    click: None,
                    aim: Vec3::X,
                    first_person: false,
                },
                &context,
            );
            if [20, 40, 60, 80, 100, 119].contains(&frame) {
                for draw_frame in 0..3 {
                    animation_camera_bounds("rope", vec3(-45., -45., -10.), vec3(45., 45., 100.));
                    draw_line_3d(vec3(10., 0., -20.), vec3(10., 0., 120.), BROWN);
                    alice.draw(p.feet, true);
                    if draw_frame == 2 {
                        crate::viewer::save_capture(std::path::Path::new(&format!(
                            "private/movement-rope-{name}-{frame}.png"
                        )))?;
                    }
                    next_frame().await;
                }
            }
        }
    }
    println!("PASS real Alice movement skin/feet, paired rope-stroke renders, paused pose and identical save readbacks");
    Ok(())
}

/// Native regression for power skins, saved transformation poses and weapon-only first person.
pub async fn render_presentation(assets: &mut Assets) -> Result<()> {
    use crate::power_pose::Form;
    use crate::powerups::Kind;
    let floor = World::fixture(&[(vec3(-1000., -1000., -50.), vec3(1000., 1000., 0.))]);
    let wall = World::fixture(&[
        (vec3(-1000., -1000., -50.), vec3(1000., 1000., 0.)),
        (vec3(32., -1000., 0.), vec3(40., 1000., 500.)),
    ]);
    let mut player = Player::new(Vec3::ZERO);
    player.grounded = true;
    let mut alice = Character::load(assets)?;
    let input = |selected, first_person, click| WeaponInput {
        selected,
        first_person,
        click,
        aim: Vec3::X,
        dice: 1,
    };
    for (kind, form, name) in [
        (Kind::Rage, Form::Rage, "rage"),
        (Kind::Tea, Form::Tea, "tea"),
    ] {
        alice.reset(&player, 0.);
        let mut stats = crate::inventory::Stats::for_level("skool1", None);
        anyhow::ensure!(stats.powerup(kind), "Fixture power activation failed");
        let mut frame = 0;
        for (label, target) in [("start", 0.8), ("growing", 4.8), ("active", 14.)] {
            while (frame as f32) / 60. < target {
                alice.update_funded(
                    1. / 60.,
                    &player,
                    false,
                    input(0, false, None),
                    &crate::combat::Context {
                        world: &floor,
                        targets: &[],
                    },
                    Some(&mut stats),
                );
                stats.update(1. / 60.);
                frame += 1;
            }
            anyhow::ensure!(
                alice.sim.power_pose.state.form == Some(form),
                "Power disappeared"
            );
            visibility_camera(Vec3::ZERO, 1.);
            alice.draw(Vec3::ZERO, true);
            let original = visibility_image();
            original.export_png(&format!("private/presentation-{name}-{label}.png"));
            let snapshot = serde_json::to_vec(&alice.snapshot())?;
            next_frame().await;
            alice.restore(&serde_json::from_slice(&snapshot)?)?;
            alice.power_appearance(&stats);
            visibility_camera(Vec3::ZERO, 1.);
            alice.draw(Vec3::ZERO, true);
            anyhow::ensure!(
                original.bytes == visibility_image().bytes,
                "Saved {name} {label} pixels differ"
            );
            next_frame().await;
            let paused = serde_json::to_vec(&alice.snapshot())?;
            alice.update_funded(
                0.,
                &player,
                false,
                input(0, false, None),
                &crate::combat::Context {
                    world: &floor,
                    targets: &[],
                },
                Some(&mut stats),
            );
            anyhow::ensure!(
                paused == serde_json::to_vec(&alice.snapshot())?,
                "Paused presentation changed"
            );
        }
        stats.update(60.);
        alice.power_appearance(&stats);
        anyhow::ensure!(
            alice.sim.power_pose.state.form.is_none(),
            "Expired skin remained"
        );
        visibility_camera(Vec3::ZERO, 1.);
        alice.draw(Vec3::ZERO, true);
        visibility_image().export_png(&format!("private/presentation-{name}-expired.png"));
        next_frame().await;
        stats.powerup(kind);
        alice.power_appearance(&stats);
        alice.die();
        anyhow::ensure!(
            alice.sim.power_pose.state.form.is_none(),
            "Power survived death"
        );
        println!("PASS native {name} growth, alternate skin, save pixel equality, pause, expiry and death");
    }
    for selected in 0..10 {
        alice.reset(&player, 0.);
        for _ in 0..100 {
            alice.update(
                1. / 60.,
                &player,
                false,
                input(selected, true, None),
                &crate::combat::Context {
                    world: &floor,
                    targets: &[],
                },
            );
        }
        for (label, world, click) in [
            ("idle", &floor, None),
            ("attack", &floor, Some(false)),
            ("wall", &wall, Some(false)),
        ] {
            for _ in 0..6 {
                alice.update(
                    1. / 60.,
                    &player,
                    false,
                    input(selected, true, click),
                    &crate::combat::Context {
                        world,
                        targets: &[],
                    },
                );
            }
            clear_background(Color::new(0.12, 0.14, 0.18, 1.));
            let camera = Camera3D {
                position: player.eye(),
                target: player.eye() + Vec3::X,
                up: Vec3::Z,
                fovy: 75_f32.to_radians(),
                z_near: 2.,
                z_far: 1000.,
                ..Default::default()
            };
            set_camera(&camera);
            if label == "wall" {
                draw_cube(vec3(34., 0., 70.), vec3(4., 180., 140.), None, GRAY);
            }
            let before = get_screen_data();
            crate::render::clear_view_depth();
            alice.draw_first_person(true);
            let rendered = get_screen_data();
            anyhow::ensure!(
                visible_pixels(&before, &rendered) > 50,
                "Invisible first-person pose {selected}/{label}"
            );
            rendered.export_png(&format!("private/presentation-fp-{selected}-{label}.png"));
            next_frame().await;
            let saved = serde_json::to_vec(&alice.snapshot())?;
            alice.restore(&serde_json::from_slice(&saved)?)?;
            anyhow::ensure!(
                saved == serde_json::to_vec(&alice.snapshot())?,
                "First-person restoration differs"
            );
        }
    }
    alice.check_visible("power-up presentation").await?;
    println!("PASS native weapon-only first person for ten toys, switch/attack/wall poses and restored view placement");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skeletal::Frame;
    fn animator() -> Animator {
        Animator::new(
            (0..CLIPS.len())
                .map(|_| Animation {
                    frames: (0..20)
                        .map(|_| Frame {
                            delta: Vec3::X * 5.,
                            pose: vec![Transform {
                                rotation: Quat::IDENTITY,
                                translation: Vec3::ZERO,
                            }],
                            min: Vec3::ZERO,
                            max: Vec3::ONE,
                        })
                        .collect(),
                    frame_time: 0.05,
                    distance: 100.,
                })
                .collect(),
        )
    }
    #[test]
    fn walking_cadence_tracks_resolved_speed_in_every_equipment_pose() {
        for (unarmed, large, index) in [(true, false, 1), (false, false, 7), (false, true, 9)] {
            for fraction in [0.05, 0.125, 0.25, 0.5, 1., 2.] {
                let mut a = animator();
                a.unarmed = unarmed;
                a.large = large;
                // A one-second walk with the short root travel from issue #2.
                Arc::get_mut(&mut a.clips).unwrap()[index].distance = 51.7;
                let mut p = Player::new(Vec3::ZERO);
                p.grounded = true;
                p.velocity = Vec3::X * crate::movement::WALK_SPEED * fraction;
                a.reset(&p);
                let mut elapsed = 0.;
                for _ in 0..120 {
                    p.feet += p.velocity / 120.;
                    a.update(1. / 120., &p, false);
                    assert_eq!(a.sound_clip, Some(index));
                    let span = a.sound_span.unwrap().1;
                    elapsed += span.end - span.start;
                }
                assert!(
                    (elapsed / (2. * fraction) - 1.).abs() < 0.02,
                    "{} at {fraction} speed advanced {elapsed} seconds",
                    CLIPS[index]
                );
                // Requested movement against a wall must not keep the legs stepping.
                a.update(1. / 120., &p, false);
                assert_eq!(a.motion, Motion::Idle);
            }
        }
    }
    #[test]
    fn walking_equipment_and_run_changes_preserve_stride_and_save_continuation() {
        let mut a = animator();
        a.unarmed = true;
        Arc::get_mut(&mut a.clips).unwrap()[1].distance = 51.7;
        Arc::get_mut(&mut a.clips).unwrap()[9].frame_time = 0.04;
        let mut p = Player::new(Vec3::ZERO);
        p.grounded = true;
        p.velocity = Vec3::X * crate::movement::WALK_SPEED;
        a.reset(&p);
        p.feet.x = a.clips[1].distance * 0.25;
        a.update(0.25, &p, false);
        assert!((a.time - 0.25).abs() < 0.001);
        a.unarmed = false;
        a.large = true;
        // A render-only frame switches equipment without consuming another step.
        a.update(1. / 144., &p, false);
        assert!((a.time / a.duration(9) - 0.25).abs() < 0.001);
        assert!(!a.sound_span.unwrap().1.entered);
        a.update(1. / 144., &p, true);
        assert!((a.time / a.duration(10) - 0.25).abs() < 0.001);
        p.feet.x += 10.;
        a.update(0.05, &p, true);
        // Running still uses authored distance: this fixture travels 100 units/cycle.
        assert!((a.time - 0.35).abs() < 0.001);
        a.update(1. / 144., &p, false);
        assert!((a.time / a.duration(9) - 0.35).abs() < 0.001);
        let saved = a.snapshot();
        a.update(0., &p, false);
        assert!(a.sound_span.is_none());
        assert_eq!(a.time, saved.time);
        p.feet.x += 10.;
        a.update(0.05, &p, false);
        let expected = serde_json::to_vec(&a.snapshot()).unwrap();
        a.restore(&saved);
        a.update(0.05, &p, false);
        assert_eq!(expected, serde_json::to_vec(&a.snapshot()).unwrap());
    }
    #[test]
    fn rope_cycle_uses_both_hands_and_only_distance_along_rope() {
        let mut a = animator();
        for i in [22, 23, 30, 31] {
            for (j, frame) in Arc::get_mut(&mut a.clips).unwrap()[i].frames.iter_mut().enumerate() {
                // Stationary root frames must not skip the hand-placement pose.
                frame.delta = if j % 2 == 0 { Vec3::ZERO } else { Vec3::Z * 5. };
                frame.pose[0].translation.x = if i < 30 { j as f32 } else { 20. - j as f32 };
            }
        }
        let mut p = Player::new(Vec3::ZERO);
        p.script_motion = 1;
        p.script_rope_length = Some(250.);
        p.script_rope_rise = 1.;
        a.reset(&p);
        for _ in 0..150 {
            p.script_rope_length = p.script_rope_length.map(|l| l - 0.5);
            // A swing can cancel the climb's world-Z movement completely.
            a.update(1. / 120., &p, false);
        }
        assert!((a.time - 1.5).abs() < 0.001);
        assert!((a.pose[0].translation.x - 10.).abs() < 0.01);
        let phase = a.time;
        p.feet.z += 20.;
        a.update(1. / 144., &p, false);
        assert_eq!(a.time, phase, "swinging advanced a climb stroke");
        let saved = a.snapshot();
        p.script_rope_length = Some(174.);
        a.update(1. / 120., &p, false);
        let expected = serde_json::to_vec(&a.snapshot()).unwrap();
        a.restore(&saved);
        a.update(1. / 120., &p, false);
        assert_eq!(serde_json::to_vec(&a.snapshot()).unwrap(), expected);
        let spans = a.audio_spans(
            22,
            crate::audio::events::Span {
                start: 0.9,
                end: 2.1,
                duration: 2.,
                frame_time: 0.05,
                looping: true,
                entered: false,
            },
        );
        assert_eq!(
            spans.iter().map(|(i, _)| *i).collect::<Vec<_>>(),
            vec![22, 30, 22]
        );
    }
    #[test]
    fn platform_transport_does_not_start_a_walking_gait() {
        let mut a = animator();
        let mut p = Player::new(Vec3::ZERO);
        p.grounded = true;
        a.reset(&p);
        for _ in 0..120 {
            p.feet += vec3(1., 0., 0.2);
            a.update(1. / 120., &p, false);
            assert_eq!(a.motion, Motion::Idle);
        }
        p.velocity = Vec3::X * 100.;
        p.feet += Vec3::X;
        a.update(0.01, &p, false);
        assert_eq!(a.motion, Motion::Walk);
    }
    #[test]
    fn equipment_selects_unarmed_small_and_large_movement_and_water_poses() {
        let mut a = animator();
        let mut p = Player::new(Vec3::ZERO);
        p.grounded = true;
        for (unarmed, large, idle, walk, run, tread) in [
            (true, false, 0, 1, 2, 25),
            (false, false, 6, 7, 8, 12),
            (false, true, 24, 9, 10, 12),
        ] {
            a.unarmed = unarmed;
            a.large = large;
            p.swimming = false;
            p.velocity = Vec3::ZERO;
            a.update(0.1, &p, false);
            assert_eq!(a.sound_clip, Some(idle));
            p.velocity = Vec3::X * 100.;
            p.feet += p.velocity * 0.1;
            a.update(0.1, &p, false);
            assert_eq!(a.sound_clip, Some(walk));
            p.feet += p.velocity * 0.1;
            a.update(0.1, &p, true);
            assert_eq!(a.sound_clip, Some(run));
            p.swimming = true;
            p.velocity = Vec3::ZERO;
            a.update(0.1, &p, false);
            assert_eq!(a.sound_clip, Some(tread));
        }
    }
    #[test]
    fn animation_sound_span_resumes_without_replaying_entry_or_paused_frames() {
        let mut a = animator();
        let mut p = Player::new(Vec3::ZERO);
        p.grounded = true;
        p.velocity = Vec3::X * 100.;
        a.update(0.2, &p, true);
        assert!(a.sound_span.unwrap().1.entered);
        let saved = a.snapshot();
        a.update(0., &p, true);
        assert!(a.sound_span.is_none());
        a.restore(&saved);
        p.feet += p.velocity * 0.1;
        a.update(0.1, &p, true);
        let (_, span) = a.sound_span.unwrap();
        assert!(!span.entered);
        assert_eq!(span.start, saved.time);
        assert!(span.end > span.start);
    }
    #[test]
    fn locomotion_jumping_landing_pause_and_reset() {
        let mut a = animator();
        let mut p = Player::new(Vec3::ZERO);
        p.grounded = true;
        a.update(0.1, &p, false);
        assert_eq!(a.motion, Motion::Idle);
        p.velocity = Vec3::X * 210.;
        p.feet += p.velocity * 0.1;
        a.update(0.1, &p, false);
        assert_eq!(a.motion, Motion::Walk);
        p.feet += p.velocity * 0.1;
        a.update(0.1, &p, true);
        assert_eq!(a.motion, Motion::Run);
        p.grounded = false;
        p.jumps += 1;
        a.update(0.1, &p, true);
        assert_eq!(a.motion, Motion::Takeoff);
        for _ in 0..12 {
            a.update(0.1, &p, true);
        }
        assert_eq!(a.motion, Motion::Air);
        let time = a.time;
        a.update(0., &p, true);
        assert_eq!(a.time, time);
        p.velocity = Vec3::ZERO;
        p.grounded = true;
        p.landings += 1;
        a.update(0.1, &p, false);
        assert_eq!(a.motion, Motion::Land);
        for _ in 0..8 {
            a.update(0.1, &p, false);
        }
        assert_eq!(a.motion, Motion::Idle);
        a.reset(&p);
        a.update(0.1, &p, false);
        assert_eq!(a.motion, Motion::Idle);
    }
    #[test]
    fn following_camera_stops_before_walls_and_hides_skin_when_cramped() {
        let wall = World::fixture(&[(vec3(-60., -100., -100.), vec3(-50., 100., 100.))]);
        let (camera, target, visible) = follow_camera(&wall, Vec3::ZERO, Vec3::X);
        assert!(camera.x > -46. && camera.x < -45. && visible);
        assert!(!wall.sweep(camera, camera, Vec3::splat(3.9)).start_solid);
        assert!(target.distance(camera) > 1.);
        let tight = World::fixture(&[(vec3(-60., -100., -100.), vec3(-20., 100., 100.))]);
        let (_, _, visible) = follow_camera(&tight, Vec3::ZERO, Vec3::X);
        assert!(!visible);
    }
    #[test]
    fn swimming_uses_full_body_clips_and_pause_preserves_pose() {
        let mut a = animator();
        let mut p = Player::new(Vec3::ZERO);
        p.swimming = true;
        a.update(0.1, &p, false);
        assert_eq!(a.motion, Motion::Tread);
        p.velocity = Vec3::Z * 150.;
        a.update(0.1, &p, false);
        assert_eq!(a.motion, Motion::Swim);
        a.update(0.1, &p, false);
        assert_eq!(a.offset, 24.);
        let time = a.time;
        a.update(0., &p, false);
        assert_eq!(a.time, time);
        p.swimming = false;
        p.grounded = true;
        p.velocity = Vec3::ZERO;
        a.update(0.1, &p, false);
        assert_eq!(a.motion, Motion::Idle);
    }
    #[test]
    fn death_finishes_once_and_respawn_clears_the_pose() {
        let mut a = animator();
        let p = Player::new(Vec3::ZERO);
        a.motion = Motion::Death;
        for _ in 0..40 {
            a.update(0.1, &p, false);
        }
        assert_eq!(a.motion, Motion::Death);
        assert_eq!(a.time, a.clips[16].duration());
        a.reset(&p);
        assert_eq!(a.motion, Motion::Idle);
        assert_eq!(a.time, 0.);
        assert_eq!(a.offset, 0.);
    }

    // Saved Alice blocks. The window-free helpers below stand in for `Character::restore`
    // followed by `power_appearance` on a rig with the fixture's bone count: they run the same
    // validation and the real `Actions::restore`, then the same upgrades the character applies.
    fn stock_transform() -> Transform {
        Transform {
            rotation: Quat::IDENTITY,
            translation: Vec3::ZERO,
        }
    }
    /// A block as written before the power-up, footing, idle-performance, facial-clock and
    /// foot-plant state existed: none of those fields are present.
    fn legacy_block(bones: usize) -> Snapshot {
        let pose = vec![stock_transform(); bones];
        serde_json::from_value(serde_json::json!({
            "animator": {
                "motion": "Idle", "time": 0.25, "transition": 1.0, "previous": pose, "pose": pose,
                "jumps": 3, "landings": 2, "large": false, "offset": 0.0, "previous_offset": 0.0
            },
            "facing": 0.5,
            "actions": crate::weapons::test_actions_with_bones(bones).snapshot(),
            "projectiles": {"projectiles": [], "shots": 0, "impacts": 0},
            "anchors": [stock_transform(), stock_transform(), stock_transform()],
            "viewmodel": crate::viewmodel::ViewModel::default()
        }))
        .unwrap()
    }
    fn restore_headless(old: &Snapshot, stats: &crate::inventory::Stats) -> Result<Snapshot> {
        let bones = old.animator.pose.len();
        old.restored_headless(
            bones,
            &mut crate::weapons::test_actions_with_bones(bones),
            stats,
        )
    }
    fn saved(s: &Snapshot) -> serde_json::Value {
        serde_json::to_value(s).unwrap()
    }
    fn changed_keys(a: &Snapshot, b: &Snapshot) -> Vec<String> {
        let (a, b) = (saved(a), saved(b));
        let mut keys: Vec<_> = a
            .as_object()
            .unwrap()
            .keys()
            .chain(b.as_object().unwrap().keys())
            .filter(|k| a[k.as_str()] != b[k.as_str()])
            .cloned()
            .collect();
        keys.sort();
        keys.dedup();
        keys
    }
    #[test]
    fn legacy_alice_block_restores_unchanged_except_for_the_power_presentation() {
        let old = legacy_block(4);
        let stats = crate::inventory::Stats::default();
        let restored = restore_headless(&old, &stats).unwrap();
        // The reproduced failure: the block written without a power-up presentation gains an
        // initialised one, so a comparison against the raw old block can never pass.
        assert_eq!(changed_keys(&old, &restored), ["power"]);
        assert_eq!(saved(&old)["power"]["initialized"], false);
        assert_eq!(saved(&restored)["power"]["initialized"], true);
        assert_eq!(restored.power.form, None);
        assert!(!restored.power.performing);
        assert!(
            restored.power.age >= 2.5,
            "an existing effect is not replayed"
        );
        // The documented migration predicts the restore exactly, and a current block is a fixed point.
        assert_eq!(saved(&old.migrated(&stats)), saved(&restored));
        let again = restore_headless(&restored, &stats).unwrap();
        assert!(changed_keys(&restored, &again).is_empty());
    }
    #[test]
    fn migrated_alice_block_predicts_forms_deaths_and_unequipped_restores() {
        let old = legacy_block(4);
        let mut rage = crate::inventory::Stats::default();
        rage.powers.rage = 10.;
        let mut tea = crate::inventory::Stats::default();
        tea.powers.tea = 10.;
        let mut dead = crate::inventory::Stats::default();
        dead.damage(1000.);
        let mut empty_handed = serde_json::to_value(crate::inventory::Stats::default()).unwrap();
        empty_handed["owned"] = serde_json::json!(vec![0u8; 10]);
        let mut empty_handed: crate::inventory::Stats =
            serde_json::from_value(empty_handed).unwrap();
        empty_handed.powers.rage = 5.;
        for (stats, form, armed) in [
            (rage, Some(crate::power_pose::Form::Rage), true),
            (tea, Some(crate::power_pose::Form::Tea), true),
            (dead, None, true),
            (empty_handed, Some(crate::power_pose::Form::Rage), false),
        ] {
            assert_eq!(power_form(&stats), form);
            let restored = restore_headless(&old, &stats).unwrap();
            assert_eq!(restored.power.form, form);
            // A legacy save applies its active effect without repeating the pickup performance.
            assert!(!restored.power.performing && restored.power.age >= 2.5);
            assert_eq!(
                saved(&old.migrated(&stats)),
                saved(&restored),
                "forms, deaths and an empty inventory"
            );
            assert_eq!(
                saved(&restored)["actions"]["selected"] == crate::weapons::UNARMED,
                !armed
            );
        }
    }
    /// Every retained legacy fixture that carries an Alice block. The fixtures are private
    /// local files: a missing one is skipped, exactly like `save_check` skips them.
    const LEGACY_FIXTURES: [&str; 15] = [
        "private/event-legacy-v1-school.json",
        "private/event-legacy-v1-battle.json",
        "private/dice-legacy-v2-school.json",
        "private/dice-legacy-v2-battle.json",
        "private/ladybug-legacy-v3/quick.json",
        "private/ladybug-legacy-v3/auto.json",
        "private/pand-legacy-v4/quick.json",
        "private/pand-legacy-v4/auto.json",
        "private/return-legacy-v5/quick.json",
        "private/cinema-legacy-v6/quick.json",
        "private/duchess-legacy-v7/quick.json",
        "private/duchess-legacy-v7/auto.json",
        "private/duchess-native-reward/quick.json",
        "private/duchess-native-reward/auto.json",
        "private/duchess-native-reward/auto.previous.json",
    ];
    #[test]
    fn retained_legacy_fixtures_restore_to_their_migrated_alice_blocks() {
        let mut checked = 0;
        for path in LEGACY_FIXTURES {
            let Ok(bytes) = std::fs::read(path) else {
                eprintln!("skipped {path}: private fixture not present");
                continue;
            };
            let envelope: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            let payload = &envelope["payload"];
            let old: Snapshot = serde_json::from_value(payload["character"].clone())
                .unwrap_or_else(|e| panic!("{path}: {e}"));
            let stats: crate::inventory::Stats = serde_json::from_value(payload["stats"].clone())
                .unwrap_or_else(|e| panic!("{path}: {e}"));
            let restored = restore_headless(&old, &stats).unwrap_or_else(|e| panic!("{path}: {e}"));
            let had_power = payload["character"].get("power").is_some();
            let changed = changed_keys(&old, &restored);
            assert!(
                changed
                    == if had_power {
                        vec![]
                    } else {
                        vec!["power".to_string()]
                    },
                "{path}: restore rewrote {changed:?}"
            );
            assert_eq!(
                saved(&old.migrated(&stats)),
                saved(&restored),
                "{path}: migrated prediction"
            );
            let again = restore_headless(&restored, &stats).unwrap();
            assert!(
                changed_keys(&restored, &again).is_empty(),
                "{path}: a current-format block must restore unchanged"
            );
            checked += 1;
        }
        eprintln!(
            "legacy Alice blocks checked: {checked} of {}",
            LEGACY_FIXTURES.len()
        );
    }
}

#[cfg(test)]
mod snapshot_input_tests {
    use super::*;
    #[test]
    fn body_input_owns_values_and_rejects_stale_generation_shape_and_bones() {
        fn owns_send_values<T:Send>(){} owns_send_values::<BodyInput>();owns_send_values::<crate::render_snapshot::DynamicInput>();
        let visit=crate::render_snapshot::RenderVisit::default();
        let replaced=crate::render_snapshot::RenderVisit::default();
        let mut input=BodyInput::default();input.visit=visit.id();input.pose.push(input.transform);
        assert!(input.valid(visit.id(),1));assert!(!input.valid(replaced.id(),1));assert!(!input.valid(visit.id(),2));
        input.slot=1;assert!(!input.valid(visit.id(),1));input.slot=0;
        input.attachments.push((0,1,1.,0.));assert!(!input.valid(visit.id(),1));
        input.attachments.clear();input.pose.resize(65537,input.transform);assert!(!input.valid(visit.id(),65537));
    }
}

/// Real-asset OFF/input mutation isolation; captures stay outside the repository.
pub(crate) async fn snapshot_render_check(assets:&mut Assets)->Result<()> {
    let world=World::fixture(&[(vec3(-4000.,-4000.,-40.),vec3(4000.,4000.,-10.))]);
    let ctx=crate::combat::Context{world:&world,targets:&[]};
    let atmosphere=crate::environment::Atmosphere::default();
    let weapons=[crate::weapons::UNARMED,0,1,2,3,4,5,7,8,9,6];
    let mut checked=0;
    for case in 0..16 {
        let mut reference=None;
        for captured in [false,true] {
            let mut alice=Character::load(assets)?;
            let mut player=Player::new(Vec3::ZERO);if case==15 {player.immersion.level=3;}alice.reset(&player,0.);
            let weapon=if case==15 {4}else{weapons[case%weapons.len()]};
            for tick in 0..if case==10 {480}else{150} {
                alice.update(1./60.,&player,false,WeaponInput{dice:3,selected:weapon,aim:Vec3::X,click:((case<11 && (case!=10 || tick<120)) || case==15).then_some(case%2==0 || case==15),first_person:case%3==0,..Default::default()},&ctx);
            }
            if case==11 || case==12 {
                alice.sim.power_pose.state.form=Some(if case==11 {crate::power_pose::Form::Rage}else{crate::power_pose::Form::Tea});
                alice.sim.power_pose.state.age=8.1;alice.sim.power_pose.state.performing=true;alice.sim.power_remaining=2.;
            }
            if case==13 {player.breath.shell=true;player.immersion.level=3;alice.water_appearance(&player);}
            if case==14 {
                let definition=Definition::alice(assets)?;
                let gesture=crate::acting::clip_names(&definition).into_iter().next().context("Missing gesture control")?;
                alice.sim.acting.stage(&gesture,0.4,weapon)?;
                alice.sim.acting.apply(&mut alice.sim.actions.pose,&alice.sim.upper,true);
            }
            if case==10 {anyhow::ensure!(alice.sim.visuals.dice.demon.is_some(),"Summon control did not summon");if let Some(d)=&mut alice.sim.visuals.dice.demon {d.feet=Vec3::Y*50.;}}
            if case==15 {anyhow::ensure!(alice.ice_locked(),"Ice shell control did not freeze");}
            alice.sim.mouth=0.7;alice.sim.face_time=case as f32*0.37;
            if captured {
                let (_,overflow)=alice.extract_body(player.feet);anyhow::ensure!(!overflow,"Alice control overflow {case}");
                alice.sim.actions.pose.clear();alice.sim.actions.selected=crate::weapons::UNARMED;alice.sim.scale=0.;alice.sim.facing=2.;
                alice.sim.animator.motion=Motion::Death;alice.sim.power_pose.state=Default::default();alice.sim.power_remaining=0.;alice.sim.acting.reset();
                alice.sim.visuals.clear();
                alice.art.visual_art.clear();
            }
            let view=Camera3D{position:vec3(150.,-190.,85.),target:Vec3::Z*38.,up:Vec3::Z,fovy:65_f32.to_radians(),z_near:2.,z_far:30000.,..Default::default()};
            clear_background(BLACK);set_camera(&view);crate::render_fx::begin_view(&view,2.5,&atmosphere,false);
            alice.atmosphere(&atmosphere,view.position);alice.draw_ghost(player.feet,true,case==15);
            alice.draw_effects(view.position,true,true);
            if case%3==0 {crate::render::clear_view_depth();alice.draw_first_person(true);}
            crate::render_fx::finish();set_default_camera();let image=get_screen_data();
            anyhow::ensure!(image.bytes.chunks_exact(4).filter(|p|p[..3]!=[0,0,0]).count()>20,"Empty Alice control {case}");
            if let Some(expected)=&reference {anyhow::ensure!(*expected==image.bytes,"Alice snapshot mutated control differs {case}");checked+=1;}
            else {reference=Some(image.bytes);}
            next_frame().await;
        }
    }
    println!("PASS snapshot Alice: {checked} exact real-asset body/weapon/effect/view controls with live-state poisoning");Ok(())
}
