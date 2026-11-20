//! Bounded declarative emitters from local TIKI definitions. No client script VM.
use crate::mesh::Mesh;
use crate::{assets::Assets, bsp::Bsp, collision::World, environment::Atmosphere, texture};
#[cfg(target_os = "vita")]
use crate::frame_profile::{add_counter, Counter};
use anyhow::Result;
use macroquad::{
    miniquad::{BlendFactor, BlendState, BlendValue, Equation, ShaderSource},
    prelude::*,
};

#[derive(Clone, Copy, Debug, Default)]
struct Range {
    low: f32,
    high: f32,
}
impl Range {
    fn fixed(v: f32) -> Self {
        Self { low: v, high: v }
    }
    fn sample(self, seed: &mut u32) -> f32 {
        self.low + (self.high - self.low) * random(seed)
    }
}
fn scalar(t: &[String], index: &mut usize) -> Option<Range> {
    let mut mode = "";
    while t
        .get(*index)
        .is_some_and(|s| s == "random" || s == "crandom")
    {
        mode = t[*index].as_str();
        *index += 1;
    }
    let v = t
        .get(*index)?
        .parse::<f32>()
        .ok()
        .filter(|v| v.is_finite() && v.abs() < 100000.)?;
    *index += 1;
    Some(match mode {
        "random" => Range { low: 0., high: v },
        "crandom" => Range {
            low: -v.abs(),
            high: v.abs(),
        },
        _ => Range::fixed(v),
    })
}
fn vector(t: &[String]) -> Option<[Range; 3]> {
    let mut i = 1;
    Some([scalar(t, &mut i)?, scalar(t, &mut i)?, scalar(t, &mut i)?])
}
fn sampled(v: [Range; 3], seed: &mut u32) -> Vec3 {
    vec3(v[0].sample(seed), v[1].sample(seed), v[2].sample(seed))
}
#[derive(Clone, Debug, Default)]
struct Spec {
    emission: EmissionSpec,
    visual: VisualSpec,
}
#[derive(Clone, Debug)]
struct EmissionSpec {
    no_deadtime: bool,
    burst: Option<(f32, u32)>,
    name: String,
    light: Option<(Vec3, f32, bool, bool)>,
    light_style: String,
    min_velocity: Vec3,
    constrain: Option<Vec3>,
    tag: Option<String>,
    rate: f32,
    size: Range,
    life: Range,
    offset: [Range; 3],
    axis_offset: [Range; 3],
    velocity: [Range; 3],
    tag_velocity: bool,
    accel: [Range; 3],
    forward: f32,
    image: String,
    random_roll: bool,
    sphere: bool,
    start_off: bool,
}
#[derive(Clone, Copy, Debug)]
struct VisualSpec {
    growth: f32,
    fade: bool,
    fade_in: f32,
    alpha: f32,
    color: Vec3,
    /// `scalerate` compounds, as the original client integrates it, instead of adding to 1.
    growth_compounds: bool,
}
impl Spec {
    /// Under `fx_bubble_true_size`, the bubble sprite only. The original draws a sprite as wide as
    /// its image times the entity scale, so this 16-pixel image is a quarter of the 64-pixel baseline.
    fn true_bubble_size(&mut self, width: Option<f32>) {
        if self.emission.image != "bubble" || !crate::sim_maps::fx_bubble_true_size() { return; }
        let factor = width.map_or(1., |w| w / 64.);
        self.emission.size.low *= factor;
        self.emission.size.high *= factor;
        self.visual.growth_compounds = true;
    }
}
impl VisualSpec {
    /// Radius of a billboard this old, from the spawned radius.
    fn grown(&self, size: f32, age: f32) -> f32 {
        if self.growth_compounds { size * (self.growth * age).exp() } else { size * (1. + self.growth * age).max(0.) }
    }
}
/// The original's `sphere` direction: three independent values in -1..1, not normalized.
fn sphere_axis(seed: &mut u32) -> Vec3 {
    vec3(random(seed) * 2. - 1., random(seed) * 2. - 1., random(seed) * 2. - 1.)
}
impl Default for EmissionSpec {
    fn default() -> Self {
        Self {
            no_deadtime: false,
            burst: None,
            name: String::new(),
            light: None,
            light_style: String::new(),
            min_velocity: Vec3::ZERO,
            constrain: None,
            tag: None,
            rate: 0.,
            size: Range::fixed(0.3),
            life: Range::fixed(1.),
            offset: [Range::default(); 3],
            axis_offset: [Range::default(); 3],
            velocity: [Range::default(); 3],
            tag_velocity: false,
            accel: [Range::default(); 3],
            forward: 0.,
            image: String::new(),
            random_roll: false,
            sphere: false,
            start_off: false,
        }
    }
}
impl Default for VisualSpec {
    fn default() -> Self {
        Self {
            growth: 0.,
            fade: false,
            fade_in: 0.,
            alpha: 1.,
            color: Vec3::ONE,
            growth_compounds: false,
        }
    }
}
fn parse(text: &str) -> Vec<Spec> {
    let mut out = Vec::new();
    let mut current: Option<Spec> = None;
    for t in crate::materials::lines(text) {
        if t[0] == "originemitter" || t[0] == "tagemitter" {
            current = Some(Spec {
                emission: EmissionSpec {
                    name: t
                        .get(if t[0] == "tagemitter" { 2 } else { 1 })
                        .cloned()
                        .unwrap_or_default(),
                    tag: if t[0] == "tagemitter" {
                        t.get(1).cloned()
                    } else {
                        None
                    },
                    ..Default::default()
                },
                visual: VisualSpec::default(),
            });
            continue;
        }
        let Some(s) = current.as_mut() else { continue };
        if t[0] == ")" {
            let s = current.take().unwrap();
            let emission = &s.emission;
            if ((emission.light.is_some() && emission.image.is_empty())
                || (emission.rate > 0.
                    && emission.rate <= 1024.
                    && emission.life.low >= 0.
                    && emission.life.high > 0.
                    && emission.life.high <= 30.
                    && emission.size.low > 0.
                    && emission.size.high <= 10.
                    && !emission.image.is_empty()))
                && out.len() < 16
            {
                out.push(s);
            }
            continue;
        }
        let n = |i: usize| {
            t.get(i)
                .and_then(|v| v.parse::<f32>().ok())
                .filter(|v| v.is_finite() && v.abs() < 100000.)
        };
        let Spec { emission, visual } = s;
        match t[0].as_str() {
            "no_deadtime" => emission.no_deadtime = true,
            "dlight" => {
                if let (Some(r), Some(g), Some(b), Some(radius)) = (n(1), n(2), n(3), n(4)) {
                    if radius > 0. && radius <= 8192. {
                        emission.light = Some((
                            vec3(r, g, b).clamp(Vec3::ZERO, Vec3::splat(2.)),
                            radius,
                            t.iter().any(|s| s == "onlylightents"),
                            radius <= 1.,
                        ));
                    }
                }
            }
            "lightstyle" => emission.light_style = t.get(1).cloned().unwrap_or_default(),
            "minvel" => {
                if let (Some(x), Some(y), Some(z)) = (n(1), n(2), n(3)) {
                    emission.min_velocity = vec3(x, y, z);
                }
            }
            "constrain" => {
                if let (Some(x), Some(y), Some(z)) = (n(1), n(2), n(3)) {
                    emission.constrain = Some(vec3(x, y, z));
                }
            }
            "spawnrate" => emission.rate = n(1).unwrap_or(0.),
            "model" => {
                emission.image = t
                    .get(1)
                    .map(|v| v.trim_end_matches(".spr").into())
                    .unwrap_or_default()
            }
            "scale" => emission.size = Range::fixed(n(1).unwrap_or(0.3)),
            "scalemin" => emission.size.low = n(1).unwrap_or(emission.size.low),
            "scalemax" => emission.size.high = n(1).unwrap_or(emission.size.high),
            "scalerate" => visual.growth = n(1).unwrap_or(0.),
            "life" => {
                let mut i = 1;
                emission.life = scalar(&t, &mut i).unwrap_or_default();
                if t.get(i).is_some_and(|v| v == "random") {
                    emission.life.high += n(i + 1).unwrap_or(0.);
                }
            }
            "offset" => {
                if let Some(v) = vector(&t) {
                    emission.offset = v;
                }
            }
            "offsetalongaxis" => {
                if let Some(v) = vector(&t) {
                    emission.axis_offset = v;
                }
            }
            "randvel" => {
                if let Some(v) = vector(&t) {
                    emission.velocity = v;
                }
            }
            "accel" => {
                if let Some(v) = vector(&t) {
                    emission.accel = v;
                }
            }
            "velocity" => emission.forward = n(1).unwrap_or(0.),
            "fade" => visual.fade = true,
            "fadein" => visual.fade_in = n(1).unwrap_or(0.).max(0.),
            "alpha" => visual.alpha = n(1).unwrap_or(1.).clamp(0., 1.),
            "color" if t.len() >= 4 => {
                visual.color = vec3(n(1).unwrap_or(1.), n(2).unwrap_or(1.), n(3).unwrap_or(1.))
            }
            "randomroll" => emission.random_roll = true,
            "sphere" => emission.sphere = true,
            "startoff" => emission.start_off = true,
            _ => {}
        }
    }
    out
}
fn burst_specs(text: &str, wanted: Option<&str>, frame_time: f32) -> Vec<Spec> {
    let mut definitions = Vec::new();
    let mut block = String::new();
    let mut current = None;
    let mut count = 1;
    let (mut depth, mut animations, mut client) = (0, false, false);
    let mut clip = String::new();
    for t in crate::materials::lines(text) {
        match t[0].as_str() {
            "{" => depth += 1,
            "}" => {
                depth -= 1;
                if depth < 3 {
                    client = false;
                }
                if depth == 0 {
                    animations = false;
                }
            }
            "animations" if depth == 0 => animations = true,
            "client" if animations && depth == 2 => client = true,
            "server" if animations && depth == 2 => client = false,
            _ if animations
                && depth == 1
                && t.get(1)
                    .is_some_and(|s| s.ends_with(".ska") || s.ends_with(".tan")) =>
            {
                clip.clone_from(&t[0])
            }
            _ => (),
        }
        if wanted.is_some_and(|wanted| !animations || !client || clip != wanted) {
            continue;
        }
        if t.len() >= 2 && (t[1] == "originspawn" || t[1] == "tagspawn") {
            let at = if t[0] == "entry" || t[0] == "enter" {
                0.
            } else {
                t[0].parse::<f32>().unwrap_or(0.) * frame_time
            };
            current = Some(at);
            count = 1;
            block = if t[1] == "tagspawn" {
                format!(
                    "tagemitter {} burst{}\n(\nspawnrate 1\n",
                    t.get(2).map_or("", String::as_str),
                    definitions.len()
                )
            } else {
                format!("originemitter burst{}\n(\nspawnrate 1\n", definitions.len())
            };
        } else if current.is_some() {
            if t[0] == "count" {
                count = t
                    .get(1)
                    .and_then(|v| v.parse::<u32>().ok())
                    .unwrap_or(1)
                    .min(512);
            }
            if t[0] != "(" {
                block.push_str(&t.join(" "));
                block.push('\n');
            }
            if t[0] == ")" {
                for mut spec in parse(&block) {
                    spec.emission.burst = Some((current.unwrap(), count));
                    definitions.push(spec);
                }
                current = None;
            }
        }
    }
    definitions
}

fn supported(model: &str) -> bool {
    model.ends_with(".tik")
        && (model.starts_with("altar_")
            || model.starts_with("fx_emitter_")
            || model.starts_with("fx_aaron_")
            || matches!(
                model,
                "fx_emitter_steam.tik"
                    | "firefly.tik"
                    | "lantern.tik"
                    | "p_h1.tik"
                    | "p_h2.tik"
                    | "p_m1.tik"
                    | "p_m2.tik"
                    | "fx_emitter_hotsteam.tik"
                    | "fx_emitter_steamslow.tik"
                    | "fx_emitter_steamrealslow.tik"
                    | "fx_emitter_steamrealslow_down.tik"
                    | "fx_mushroom_steam.tik"
                    | "al_fire1.tik"
                    | "fx_emitter_fire_expensive.tik"
                    | "fx_bubbles_noair.tik"
                    | "fx_bubbles_air.tik"
                    | "fx_mockturtle_launcher.tik"
                    | "fx_waterfall_g1.tik"
                    | "fx_waterfall_g2.tik"
                    | "fx_waterfall_g3.tik"
                    | "fx_wfall_face.tik"
                    | "fx_wfall_face_qlair.tik"
            ))
}
/// Model children use the same bounded declarative burst properties as sprites.
/// Sampling from the owning clock keeps pauses/restores free of repeated births.
pub(crate) struct ModelBurst(Spec);
pub(crate) struct ModelParticle {
    pub model: String,
    pub tag: Option<String>,
    pub offset: Vec3,
    pub age: f32,
    pub scale: f32,
    pub alpha: f32,
}
impl ModelBurst {
    pub(crate) fn parse(text: &str, frame_time: f32) -> Vec<Self> {
        burst_specs(text, Some("idle"), frame_time).into_iter()
            .filter(|s| s.emission.image.ends_with(".tik")).map(Self).collect()
    }
    pub(crate) fn model(&self) -> &str {
        self.0.emission.image.trim_start_matches("models/").trim_end_matches(".tik")
    }
    pub(crate) fn sample(&self, age: f32, seed: u32) -> Vec<ModelParticle> {
        let s = &self.0;
        let (birth, count) = s.emission.burst.unwrap();
        let age = age - birth;
        if age < 0. || age >= s.emission.life.high {
            return Vec::new();
        }
        (0..count.min(128))
            .filter_map(|i| {
                let mut seed = seed.wrapping_add(i.wrapping_mul(2654435761));
                let life = s.emission.life.sample(&mut seed);
                if age >= life {
                    return None;
                }
                let offset = sampled(s.emission.offset, &mut seed) + sampled(s.emission.axis_offset, &mut seed);
                let velocity = sampled(s.emission.velocity, &mut seed) + Vec3::X * s.emission.forward;
                let accel = sampled(s.emission.accel, &mut seed);
                Some(ModelParticle {
                    model: self.model().into(),
                    tag: s.emission.tag.clone(),
                    age,
                    offset: offset + velocity * age + accel * (0.5 * age * age),
                    scale: (s.emission.size.sample(&mut seed) + s.visual.growth * age).max(0.),
                    alpha: s.visual.alpha
                        * if s.visual.fade {
                            (1. - age / life).max(0.)
                        } else {
                            1.
                        },
                })
            })
            .collect()
    }
}
pub(crate) fn declared_lights(
    assets: &mut Assets,
    name: &str,
) -> Result<Vec<crate::lighting::Light>> {
    let text = String::from_utf8_lossy(&assets.read(&format!("models/{name}.tik"))?).into_owned();
    Ok(parse(&text)
        .into_iter()
        .filter(|s| !s.emission.start_off)
        .filter_map(|s| {
            s.emission.light.map(
                |(color, radius, only_models, flare)| crate::lighting::Light {
                    position: sampled(s.emission.offset, &mut 1),
                    color,
                    radius,
                    only_models,
                    flare,
                },
            )
        })
        .collect())
}
/// Preserve attachment names and start-off flags for a controller-owned actor.
pub(crate) fn attached_lights(
    assets: &mut Assets,
    name: &str,
) -> Result<Vec<(String, Option<String>, bool, crate::lighting::Light)>> {
    let text = String::from_utf8_lossy(&assets.read(&format!("models/{name}.tik"))?).into_owned();
    Ok(parse(&text)
        .into_iter()
        .filter_map(|s| {
            s.emission.light.map(|(color, radius, only_models, flare)| {
                (
                    s.emission.name,
                    s.emission.tag,
                    !s.emission.start_off,
                    crate::lighting::Light {
                        position: sampled(s.emission.offset, &mut 1),
                        color,
                        radius,
                        only_models,
                        flare,
                    },
                )
            })
        })
        .collect())
}
mod check;
mod light_style;
pub(crate) use check::ambient_render;

#[derive(Clone)]
struct EmitterSim {
    id: usize,
    pickup: bool,
    origin: Vec3,
    rotation: Quat,
    local: crate::skeletal::Transform,
    scale: f32,
    spec: EmissionSpec,
    light_style: Option<light_style::Style>,
    light_color: Option<Vec3>,
    carry: f32,
    enabled: bool,
    animation_enabled: Option<bool>,
}
#[derive(Clone)]
struct EmitterVisual {
    spec: VisualSpec,
    textures: Vec<Texture2D>,
    upload_identities: Vec<u64>,
    stage: crate::materials::Stage,
    additive: bool,
    #[cfg(all(feature="sim_maps_probe", not(target_os="vita")))]
    diagnostic: EmitterDiagnostic,
}

#[cfg(all(feature="sim_maps_probe", not(target_os="vita")))]
#[derive(Clone)]
struct EmitterDiagnostic {
    id: usize,
    name: String,
    image: String,
}
impl EmitterVisual {
    fn new(id: usize, spec: &Spec, textures: Vec<Texture2D>, upload_identities: Vec<u64>,
           stage: crate::materials::Stage, additive: bool) -> Self {
        let _ = id;
        Self {
            spec: spec.visual, textures, upload_identities, stage, additive,
            #[cfg(all(feature="sim_maps_probe", not(target_os="vita")))]
            diagnostic: EmitterDiagnostic {
                id, name: spec.emission.name.clone(), image: spec.emission.image.clone(),
            },
        }
    }
}
struct AnimationBinding {
    id: usize,
    events: crate::animation_events::Model,
    clip: String,
    model: Option<crate::tan::Model>,
    origin: crate::skeletal::Transform,
    scale: f32,
    duration: f32,
    frame_time: f32,
}
#[derive(Clone, Copy, Debug, Default)]
struct PuffSweep {
    start: Option<Vec3>,
    ticks: u32,
}
impl PuffSweep {
    fn segment(&mut self, config: &crate::vita_sim::Config, position: Vec3, next: Vec3, velocity: Vec3) -> Option<(Vec3, Vec3)> {
        use crate::vita_sim::PuffCollide;
        let start = *self.start.get_or_insert(position);
        self.ticks += 1;
        let moving = velocity.length_squared() >= 1e-6; // 0.001 world units/second.
        let enabled = match config.puff_collide {
            PuffCollide::All => true,
            PuffCollide::Moving => moving,
            PuffCollide::None => false,
        };
        if enabled && self.ticks >= config.puff_sweep_every {
            self.start = None;
            self.ticks = 0;
            Some((start, next))
        } else {
            if config.puff_collide == PuffCollide::None {
                *self = Self::default();
            }
            None
        }
    }
}
#[derive(Clone)]
struct Puff {
    sweep: PuffSweep,
    position: Vec3,
    constraint_center: Vec3,
    velocity: Vec3,
    accel: Vec3,
    age: f32,
    life: f32,
    size: f32,
    roll: f32,
    emitter: usize,
}
impl Puff {
    fn render_input(&self)->crate::render_snapshot::ParticleInput {
        crate::render_snapshot::ParticleInput{emitter:self.emitter,position:self.position,age:self.age,life:self.life,size:self.size,roll:self.roll}
    }
}
/// Borrowed serial fallback or owned snapshot values; neither iterator allocates.
#[derive(Clone,Copy)]
enum RenderPuffs<'a> { Live(&'a [Puff]), Input(&'a [crate::render_snapshot::ParticleInput]) }
enum RenderPuffIter<'a> { Live(std::slice::Iter<'a,Puff>), Input(std::slice::Iter<'a,crate::render_snapshot::ParticleInput>) }
impl<'a> RenderPuffs<'a> {
    fn len(self)->usize {match self {Self::Live(p)=>p.len(),Self::Input(p)=>p.len()}}
    fn is_empty(self)->bool {self.len()==0}
    fn at(self,index:usize)->crate::render_snapshot::ParticleInput {match self {Self::Live(p)=>p[index].render_input(),Self::Input(p)=>p[index]}}
    fn iter(self)->RenderPuffIter<'a> {match self {Self::Live(p)=>RenderPuffIter::Live(p.iter()),Self::Input(p)=>RenderPuffIter::Input(p.iter())}}
}
impl Iterator for RenderPuffIter<'_> {
    type Item=crate::render_snapshot::ParticleInput;
    fn next(&mut self)->Option<Self::Item> {match self {Self::Live(p)=>p.next().map(Puff::render_input),Self::Input(p)=>p.next().copied()}}
    fn size_hint(&self)->(usize,Option<usize>) {let n=match self {Self::Live(p)=>p.len(),Self::Input(p)=>p.len()};(n,Some(n))}
}
impl ExactSizeIterator for RenderPuffIter<'_> {}
#[cfg(feature="vita_renderer")]
mod quad;
#[derive(Default)]
pub(crate) struct SteamInput {
    visit: u64,
    puffs: Vec<crate::render_snapshot::ParticleInput>,
}

pub struct Steam {
    sim: SteamSim,
    art: SteamArt,
}

pub(crate) struct SteamSim {
    animations: Vec<AnimationBinding>,
    emitters: Vec<EmitterSim>,
    puffs: Vec<Puff>,
    seed: u32,
}

pub(crate) struct SteamArt {
    input: Option<SteamInput>,
    arena: SteamInput,
    #[cfg(feature="vita_renderer")]
    quads: std::cell::RefCell<quad::Buffer>,
    emitters: Vec<EmitterVisual>,
    materials: Vec<std::rc::Rc<Material>>,
}

const _: fn() = || {
    fn check<T: Send>() {}
    check::<SteamSim>();
    check::<SteamInput>();
};

// If SteamArt ever implements Send, the inferred parameter becomes ambiguous.
const _: fn() = || {
    trait AmbiguousIfSend<A> { fn some_item() {} }
    impl<T: ?Sized> AmbiguousIfSend<()> for T {}
    struct Invalid;
    impl<T: ?Sized + Send> AmbiguousIfSend<Invalid> for T {}
    let _ = <SteamArt as AmbiguousIfSend<_>>::some_item;
};

/// Rendering borrows one puff source chosen at the frame boundary.
pub(crate) struct SteamDraw<'a> {
    art: &'a SteamArt,
    puffs: RenderPuffs<'a>,
}

/// Live particle state without any texture, material, model or GPU reference.
pub(crate) struct Runtime {
    puffs: Vec<Puff>, seed: u32,
    emitters: Vec<(Vec3, Quat, crate::skeletal::Transform, f32, f32, bool, Option<bool>)>,
}
fn random(seed: &mut u32) -> f32 {
    *seed ^= *seed << 13;
    *seed ^= *seed >> 17;
    *seed ^= *seed << 5;
    (*seed >> 8) as f32 / 16777216.
}
impl Steam {
    pub(crate) fn art(&self) -> &SteamArt { &self.art }

    pub(crate) fn sim_mut(&mut self) -> &mut SteamSim { &mut self.sim }

    pub(crate) fn into_parts(self) -> (SteamSim, SteamArt) {
        (self.sim, self.art)
    }

    pub(crate) fn from_parts(sim: SteamSim, art: SteamArt) -> Self {
        Self { sim, art }
    }

    fn new(sim: SteamSim, emitters: Vec<EmitterVisual>, materials: Vec<std::rc::Rc<Material>>) -> Self {
        Self {
            sim,
            art: SteamArt {
                input: None,
                arena: SteamInput::default(),
                #[cfg(feature="vita_renderer")]
                quads: Default::default(),
                emitters,
                materials,
            },
        }
    }

    pub(crate) fn clear_render_input(&mut self) { self.art.clear_input(); }

    pub(crate) fn extract_render_input(&mut self, visit: u64) -> (usize, bool) {
        self.art.clear_input();
        let Some(bytes) = self.sim.extract(visit, &mut self.art.arena) else { return (0, true); };
        self.art.publish();
        (bytes, false)
    }

    pub(crate) fn draw_input(&self, visit: u64) -> Option<SteamDraw<'_>> {
        self.art.draw_input(visit)
    }

    pub(crate) fn live_draw(&self) -> SteamDraw<'_> {
        SteamDraw { art: &self.art, puffs: RenderPuffs::Live(&self.sim.puffs) }
    }

    #[cfg(all(feature = "sim_maps_probe", not(target_os = "vita")))]
    pub(crate) fn poison_sim_for_draw(&mut self, frame: u64) -> SteamSim {
        assert!(!self.sim.emitters.is_empty() && !self.sim.puffs.is_empty(),
            "steam poison requires live map emitters and puffs");
        let mut emitter = self.sim.emitters[0].clone();
        emitter.id = usize::MAX;
        emitter.origin = Vec3::splat(-12345.);
        emitter.rotation = Quat::from_rotation_z(3.);
        emitter.local.translation = Vec3::splat(12345.);
        emitter.scale = 0.;
        emitter.enabled = false;
        emitter.animation_enabled = Some(false);
        emitter.carry = -1.;
        emitter.spec = EmissionSpec::default();
        let mut puff = self.sim.puffs[0].clone();
        puff.emitter = 0;
        puff.position = emitter.origin;
        puff.age = 0.;
        puff.size = 0.;
        puff.roll = 3.;
        println!("[lg/snapshot-steam-poison] frame={frame} emitters={} puffs={}",
            self.sim.emitters.len(), self.sim.puffs.len());
        std::mem::replace(&mut self.sim, SteamSim {
            animations: Vec::new(), emitters: vec![emitter], puffs: vec![puff], seed: 0,
        })
    }

    #[cfg(all(feature = "sim_maps_probe", not(target_os = "vita")))]
    pub(crate) fn restore_sim_after_draw(&mut self, sim: SteamSim) { self.sim = sim; }

    pub(crate) fn runtime(&self) -> Runtime { self.sim.runtime() }
    pub(crate) fn restore_runtime(&mut self, runtime: Runtime) {
        self.clear_render_input();
        self.sim.restore_runtime(runtime);
    }
    pub fn emission(&self, id: crate::entity::Id) -> Option<(bool, usize)> { self.sim.emission(id) }
    pub fn animate(&mut self, clock: f32) { self.sim.animate(clock); }
    pub fn lights(&self) -> Vec<crate::lighting::Light> { self.sim.lights() }
    pub fn check_runtime(&mut self, world: &World, state: &mut crate::event::Runtime) -> Result<()> {
        self.sim.check_runtime(world, state)
    }
    pub fn stop(&mut self) { self.sim.stop(); }
    pub fn sync(&mut self, state: &crate::event::Runtime) { self.sim.sync(state); }
    pub fn gate(&mut self, intervals: &[(crate::entity::Id, bool)]) { self.sim.gate(intervals); }
    pub fn collected(&mut self, items: &[crate::inventory::Pickup], stats: &crate::inventory::Stats) {
        self.sim.collected(items, stats);
    }
    pub fn place(&mut self, poses: &[(crate::entity::Id, crate::skeletal::Transform)]) {
        self.sim.place(poses);
    }
    pub fn update(&mut self, dt: f32, eye: Vec3, world: &World) { self.sim.update(dt, eye, world); }
    pub fn update_clocks(&mut self, dt: f32, world_dt: f32, eye: Vec3, world: &World) {
        self.sim.update_clocks(dt, world_dt, eye, world);
    }
    #[cfg(test)]
    pub(crate) fn update_clocks_config(&mut self, dt: f32, world_dt: f32, eye: Vec3, world: &World, config: &crate::vita_sim::Config) {
        self.sim.update_clocks_config(dt, world_dt, eye, world, config);
    }
    #[cfg(all(feature="vita_renderer", not(target_os="vita")))]
    pub(crate) fn host_steam_puff(&self) -> Option<(usize, Vec3)> { self.sim.host_steam_puff() }
    pub fn prepare_draw(&self, camera: Vec3, atmosphere: &Atmosphere) {
        self.live_draw().prepare_draw(camera, atmosphere);
    }
    pub fn depths(&self, camera: Vec3, direction: Vec3) -> impl Iterator<Item = (usize, f32)> + '_ {
        self.live_draw().depths(camera, direction)
    }
    #[cfg(feature="vita_renderer")]
    pub(crate) fn prepare_quads(&self, camera: Vec3, direction: Option<Vec3>) {
        self.live_draw().prepare_quads(camera, direction);
    }
    pub fn draw_one(&self, index: usize, camera: Vec3, direction: Vec3) {
        self.live_draw().draw_one(index, camera, direction);
    }
}

impl SteamArt {
    fn publish(&mut self) {
        self.input = Some(std::mem::take(&mut self.arena));
    }

    fn clear_input(&mut self) {
        if let Some(input) = self.input.take() { self.arena = input; }
    }

    pub(crate) fn draw_input(&self, visit: u64) -> Option<SteamDraw<'_>> {
        let input = self.input.as_ref().filter(|input| input.visit == visit)?;
        Some(SteamDraw { art: self, puffs: RenderPuffs::Input(&input.puffs) })
    }
}

impl SteamSim {
    fn extract(&self, visit: u64, input: &mut SteamInput) -> Option<usize> {
        if self.puffs.len() > 16384 || self.puffs.iter().any(|p| p.emitter >= self.emitters.len()) {
            return None;
        }
        input.visit = visit;
        input.puffs.clear();
        input.puffs.extend(self.puffs.iter().map(Puff::render_input));
        Some(std::mem::size_of::<SteamInput>() + std::mem::size_of_val(input.puffs.as_slice()))
    }

    pub(crate) fn runtime(&self) -> Runtime {
        Runtime { puffs: self.puffs.clone(), seed: self.seed,
            emitters: self.emitters.iter().map(|e| (e.origin, e.rotation, e.local,
                e.scale, e.carry, e.enabled, e.animation_enabled)).collect() }
    }
    pub(crate) fn restore_runtime(&mut self, runtime: Runtime) {
        self.puffs = runtime.puffs; self.seed = runtime.seed;
        for (e, (origin, rotation, local, scale, carry, enabled, animation_enabled))
            in self.emitters.iter_mut().zip(runtime.emitters) {
            e.origin = origin; e.rotation = rotation; e.local = local; e.scale = scale;
            e.carry = carry; e.enabled = enabled; e.animation_enabled = animation_enabled;
        }
    }
    /// Emission and living puffs for a placed entity, used by mechanism checks.
    pub fn emission(&self, id: crate::entity::Id) -> Option<(bool, usize)> {
        self.emitters.iter().any(|e| e.id == id.0).then(|| {
            (
                self.emitters.iter().any(|e| e.id == id.0 && e.enabled),
                self.puffs
                    .iter()
                    .filter(|p| self.emitters[p.emitter].id == id.0)
                    .count(),
            )
        })
    }
    /// Reconstruct emitter switches from the saved animation clock. No historical
    /// bursts are replayed on loading, and paused clocks never advance emission.
    pub fn animate(&mut self, clock: f32) {
        for emitter in &mut self.emitters {
            emitter.light_color = emitter.light_style.as_ref().map(|s| s.sample(clock));
        }
        for binding in &self.animations {
            let visual = binding.events.visual(
                &binding.clip,
                clock,
                binding.duration,
                binding.frame_time,
                true,
            );
            for e in self.emitters.iter_mut().filter(|e| e.id == binding.id) {
                e.animation_enabled = visual.emitters.get(&e.spec.name).copied();
                if let (Some(tag), Some(model)) = (&e.spec.tag, &binding.model) {
                    if let (Some(points), Some(axes)) =
                        (model.tags.get(tag), model.tag_rotations.get(tag))
                    {
                        let f =
                            (clock.max(0.) / binding.frame_time).rem_euclid(points.len() as f32);
                        let a = f.floor() as usize;
                        let b = (a + 1) % points.len();
                        e.local.translation = points[a].lerp(points[b], f.fract()) * binding.scale;
                        e.local.rotation = axes[a].slerp(axes[b], f.fract());
                        e.origin = binding.origin.point(e.local.translation);
                        e.rotation = binding.origin.rotation * e.local.rotation;
                    }
                }
            }
        }
    }
    #[cfg(all(feature="vita_renderer", not(target_os="vita")))]
    pub(crate) fn host_steam_puff(&self) -> Option<(usize, Vec3)> {
        self.puffs.iter().enumerate().find(|(_,p)| self.emitters[p.emitter].spec.image.contains("steam"))
            .map(|(i,p)| (i,p.position))
    }
    pub fn lights(&self) -> Vec<crate::lighting::Light> {
        self.emitters
            .iter()
            .filter(|e| e.enabled)
            .filter_map(|e| {
                e.spec.light.map(
                    |(color, radius, only_models, flare)| crate::lighting::Light {
                        position: e.origin
                            + sampled(e.spec.offset, &mut 1) * e.scale
                            + e.rotation * sampled(e.spec.axis_offset, &mut 1) * e.scale,
                        color: e.light_color.unwrap_or(color),
                        radius: radius * e.scale,
                        only_models,
                        flare,
                    },
                )
            })
            .collect()
    }
    pub fn check_runtime(
        &mut self,
        world: &World,
        state: &mut crate::event::Runtime,
    ) -> Result<()> {
        use anyhow::ensure;
        ensure!(
            !self.emitters.is_empty(),
            "Particle runtime fixture needs emitters"
        );
        let saved = state.snapshot();
        let eye = self.emitters[0].origin;
        self.sync(state);
        self.update(0.05, eye, world);
        ensure!(!self.puffs.is_empty(), "Active waterfall did not emit");
        let before = self
            .puffs
            .iter()
            .map(|p| (p.position, p.age))
            .collect::<Vec<_>>();
        self.update(0., eye, world);
        ensure!(
            before
                == self
                    .puffs
                    .iter()
                    .map(|p| (p.position, p.age))
                    .collect::<Vec<_>>(),
            "Pause moved particles"
        );
        for e in &self.emitters {
            state.set_enabled(crate::entity::Id(e.id), false)?;
        }
        // Round-trip the shared saved state, then verify it controls rebuilt emitters.
        let encoded = serde_json::to_vec(&state.snapshot())?;
        state.restore(&serde_json::from_slice(&encoded)?)?;
        self.sync(state);
        self.puffs.clear();
        for _ in 0..120 {
            self.update(1. / 60., eye, world);
        }
        ensure!(self.puffs.is_empty(), "Disabled saved emitter restarted");
        state.restore(&saved)?;
        self.sync(state);
        self.update(0.05, eye, world);
        ensure!(
            !self.puffs.is_empty(),
            "Re-enabled emitter remained stopped"
        );
        self.stop();
        ensure!(
            self.emitters.iter().any(|e| e.enabled),
            "School steam shutdown disabled waterfall spray"
        );
        for _ in 0..900 {
            self.update(1. / 60., eye, world);
        }
        ensure!(
            self.puffs.len() <= 4096
                && self
                    .puffs
                    .iter()
                    .all(|p| p.position.is_finite() && p.age < p.life),
            "Unbounded/invalid particle lifetime"
        );
        self.puffs.clear();
        println!("PASS emitter pause, persisted activation, reactivation, selective steam stop and bounded expiry");
        Ok(())
    }
    pub fn stop(&mut self) {
        for e in &mut self.emitters {
            if e.spec.image.contains("steam") {
                e.spec.start_off = true;
                e.enabled = false;
                e.animation_enabled = Some(false);
            }
        }
        self.puffs
            .retain(|p| !self.emitters[p.emitter].spec.start_off);
    }
    pub fn sync(&mut self, state: &crate::event::Runtime) {
        for e in &mut self.emitters {
            e.enabled = state.enabled(crate::entity::Id(e.id))
                && e.animation_enabled.unwrap_or(!e.spec.start_off);
        }
    }
    /// Apply a mechanism's current emission interval after the saved activation
    /// flags. Existing puffs finish naturally; this only stops new emission.
    pub fn gate(&mut self, intervals: &[(crate::entity::Id, bool)]) {
        for e in &mut self.emitters {
            if let Some((_, active)) = intervals.iter().find(|(id, _)| id.0 == e.id) {
                e.enabled &= active;
                if !e.enabled {
                    e.carry = 0.;
                }
            }
        }
    }
    /// Pickup emitters belong to the collectible's entity, including its light.
    pub fn collected(
        &mut self,
        items: &[crate::inventory::Pickup],
        stats: &crate::inventory::Stats,
    ) {
        let available = items.iter().filter(|p| !stats.collected.contains(&p.id))
            .filter_map(|p| p.id.rsplit(':').next()?.parse::<usize>().ok())
            .collect::<std::collections::BTreeSet<_>>();
        let ids = items
            .iter()
            .filter(|p| stats.collected.contains(&p.id))
            .filter_map(|p| p.id.rsplit(':').next()?.parse::<usize>().ok())
            .collect::<std::collections::BTreeSet<_>>();
        for e in &mut self.emitters {
            if ids.contains(&e.id) || (e.pickup && !available.contains(&e.id)) {
                e.enabled = false;
                e.carry = 0.;
            }
        }
        self.puffs
            .retain(|p| {
                let e = &self.emitters[p.emitter];
                !ids.contains(&e.id) && (!e.pickup || available.contains(&e.id))
            });
    }
    /// Move future emission with its owning entity. Already emitted particles
    /// remain in world space; tag offsets/orientation stay local to the entity.
    pub fn place(&mut self, poses: &[(crate::entity::Id, crate::skeletal::Transform)]) {
        for e in &mut self.emitters {
            if let Some((_, pose)) = poses.iter().find(|(id, _)| id.0 == e.id) {
                e.origin = pose.point(e.local.translation);
                e.rotation = pose.rotation * e.local.rotation;
            }
        }
    }
}

impl Steam {
    pub fn load(assets: &mut Assets, map: &Bsp) -> Result<Self> {
        let specs = texture::read_materials(assets)?;
        let mut emitters = Vec::new();
        let mut visuals = Vec::new();
        let mut animations = Vec::new();
        for (id, e) in map.entities.iter().enumerate() {
            let Some(model) = e.get("model") else {
                continue;
            };
            let model = model.trim_start_matches("models/");
            if !supported(model) {
                continue;
            }
            let flags = e
                .get("spawnflags")
                .and_then(|v| v.parse::<u32>().ok())
                .unwrap_or(0);
            if !map.difficulty.allows(flags) {
                continue;
            }
            let Some(origin) = e.get("origin").and_then(|v| crate::interaction::vector(v)) else {
                continue;
            };
            let text =
                String::from_utf8_lossy(&assets.read(&format!("models/{model}"))?).into_owned();
            let definitions = parse(&text);
            let selected_clip = e.get("anim").map(String::as_str)
                .or_else(|| (model == "lantern.tik").then_some("lantern"));
            let tan = crate::weapons::read_model_clip(
                assets, model.trim_end_matches(".tik"), selected_clip,
            ).ok();
            let scale = e
                .get("scale")
                .and_then(|v| v.parse::<f32>().ok())
                .filter(|v| v.is_finite())
                .unwrap_or(1.)
                .clamp(0.01, 10.)
                * tan.as_ref().map_or(1., |(d, _)| d.scale);
            let rotation = crate::decorations::rotation(e);
            let events = crate::animation_events::Model::parse(&text)?;
            let clip = selected_clip.unwrap_or("idle").to_owned();
            let frame_time = tan.as_ref().map_or(0.05, |(_, m)| m.frame_time);
            let duration = tan
                .as_ref()
                .and_then(|(_, m)| {
                    m.surfaces
                        .first()
                        .map(|s| s.frames.len() as f32 * m.frame_time)
                })
                .unwrap_or(1.)
                .max(0.001);
            for mut spec in definitions {
                // The original 200+100/s additive altar plume saturates this
                // renderer's LDR target. Keep the weapon readable without
                // reducing the authored motion, colours or other emitters.
                if crate::inventory::WEAPONS
                    .iter()
                    .any(|(id, _)| model == format!("altar_{id}.tik"))
                {
                    spec.visual.alpha *= 0.12;
                }
                let mut point = origin;
                let mut axes = rotation;
                if let Some(tag) = &spec.emission.tag {
                    let Some((_, m)) = tan.as_ref() else {
                        continue;
                    };
                    let Some(offset) = m.tags.get(tag).and_then(|v| v.first()) else {
                        continue;
                    };
                    point += rotation * *offset * scale;
                    axes *= m.tag_rotations[tag][0];
                }
                let stage = specs
                    .get(&spec.emission.image)
                    .and_then(|s| s.stages.first())
                    .cloned()
                    .unwrap_or_else(|| crate::materials::Stage {
                        images: vec![spec.emission.image.clone()],
                        ..Default::default()
                    });
                let mut textures = Vec::new();
                let mut upload_identities = Vec::new();
                for frame in &stage.images {
                    let Some(path) = texture::resolve(assets, frame, &specs) else {
                        continue;
                    };
                    let shared = texture::decode_upload_shared_full(assets, &path)?;
                    #[cfg(all(feature = "sim_maps_probe", not(target_os = "vita")))]
                    crate::sim_maps::probe::draw_query::label_texture(shared.texture.raw_miniquad_id(), &path);
                    #[cfg(feature = "vita_renderer")]
                    crate::render_fx::retained::note_particle_texture_zero_alpha(&shared.texture, shared.zero_alpha);
                    upload_identities.push(shared.upload_identity);
                    textures.push(shared.texture);
                }
                if spec.emission.light.is_none()
                    && (textures.len() != stage.images.len() || textures.is_empty())
                {
                    continue;
                }
                let additive = matches!(
                    stage.blend,
                    crate::materials::Blend::Add | crate::materials::Blend::AlphaAdd
                );
                let light_style = light_style::Style::load(assets, &spec.emission.light_style)?;
                let light_color = light_style.as_ref().map(|s| s.sample(0.));
                if model == "lantern.tik" {
                    // The source sprite is 16 pixels, not the legacy 64-pixel baseline.
                    let factor = textures.first().map_or(1., |t| t.width() / 64.);
                    spec.emission.size.low *= factor;
                    spec.emission.size.high *= factor;
                }
                spec.true_bubble_size(textures.first().map(Texture2D::width));
                let enabled = !spec.emission.start_off;
                visuals.push(EmitterVisual::new(id, &spec, textures, upload_identities, stage, additive));
                emitters.push(EmitterSim {
                    id,
                    pickup: e.get("classname").is_some_and(|c| c.to_ascii_lowercase().starts_with("item_")),
                    origin: point,
                    rotation: axes,
                    local: crate::skeletal::Transform {
                        translation: rotation.inverse() * (point - origin),
                        rotation: rotation.inverse() * axes,
                    },
                    scale,
                    spec: spec.emission,
                    light_style,
                    light_color,
                    // One ten-second pixie is alive from entry, then replaced.
                    carry: if model == "lantern.tik" { 1. } else { 0. },
                    enabled,
                    animation_enabled: None,
                });
            }
            animations.push(AnimationBinding {
                id,
                events,
                clip,
                model: tan.map(|(_, m)| m),
                origin: crate::skeletal::Transform {
                    translation: origin,
                    rotation,
                },
                scale,
                duration,
                frame_time,
            });
        }
        let materials = shared_materials()?;
        println!("Particles: {} authored emitters", emitters.len());
        Ok(Self::new(SteamSim {
            animations,
            emitters,
            puffs: Vec::new(),
            seed: 0x56789abc,
        }, visuals, materials))
    }
}

impl SteamSim {
    pub fn update(&mut self, dt: f32, eye: Vec3, world: &World) {
        self.update_clocks(dt, dt, eye, world);
    }
    pub fn update_clocks(&mut self, dt: f32, world_dt: f32, eye: Vec3, world: &World) {
        self.update_clocks_config(dt, world_dt, eye, world, crate::vita_sim::effective());
    }
    pub(crate) fn update_clocks_config(&mut self, dt: f32, world_dt: f32, eye: Vec3, world: &World, config: &crate::vita_sim::Config) {
        let _profile = crate::frame_profile::span("particles");
        #[cfg(target_os = "vita")]
        let _counter_owner =
            crate::frame_profile::counter_owner(crate::frame_profile::Owner::Particles);
        if !dt.is_finite() || dt <= 0. {
            return;
        }
        let dt = dt.min(0.05);
        #[cfg(target_os = "vita")]
        let mut skipped = 0u32;
        #[cfg(target_os = "vita")]
        let (before, mut sweeps, mut constrained, mut frozen, mut stationary, mut impacts) =
            (self.puffs.len() as u32, 0u32, 0u32, 0u32, 0u32, 0u32);
        for p in &mut self.puffs {
            #[cfg(target_os = "vita")]
            let _integrate_profile = crate::frame_profile::span("particles_integrate");

            let dt = if self.emitters[p.emitter].spec.no_deadtime {
                dt
            } else {
                world_dt.clamp(0., dt)
            };
            if dt == 0. {
                #[cfg(target_os = "vita")]
                {
                    frozen += 1;
                }
                continue;
            }
            p.age += dt;
            p.velocity += p.accel * dt;
            let mut next = p.position + p.velocity * dt;
            if let Some(bounds) = self.emitters[p.emitter].spec.constrain {
                for axis in 0..3 {
                    if bounds[axis] >= 0.
                        && (next[axis] - p.constraint_center[axis]).abs() > bounds[axis]
                    {
                        p.velocity[axis] = -p.velocity[axis];
                        p.accel[axis] = -p.accel[axis];
                        next[axis] += p.velocity[axis] * dt;
                    }
                }
            }
            // The enclosed pixie requests axis bounds, not world collision.
            // Its casing still occludes the sprite through the depth buffer.
            #[cfg(target_os = "vita")]
            drop(_integrate_profile);
            if self.emitters[p.emitter].spec.constrain.is_none() {
                if let Some((start, end)) = p.sweep.segment(config, p.position, next, p.velocity) {
                    #[cfg(target_os = "vita")]
                    {
                        sweeps += 1;
                        stationary += u32::from(start == end);
                    }
                    if world.sweep(start, end, Vec3::ZERO).fraction < 1. {
                        p.age = p.life;
                        #[cfg(target_os = "vita")]
                        { impacts += 1; }
                    }
                } else {
                    #[cfg(target_os = "vita")]
                    { skipped += 1; }
                }
            } else {
                #[cfg(target_os = "vita")]
                {
                    constrained += 1;
                }
            }
            if self.emitters[p.emitter].spec.image == "bubble" {
                #[cfg(target_os = "vita")]
                let _hotspot = crate::frame_profile::span("particles_liquid");
                if world.liquid_at(next) == 0 {
                    p.age = p.life;
                }
            }
            p.position = next;
        }
        {
            #[cfg(target_os = "vita")]
            let _hotspot = crate::frame_profile::span("particles_retain");
            self.puffs.retain(|p| p.age < p.life);
        }
        let retained = self.puffs.len() as u32;
        #[cfg(target_os = "vita")]
        let _emit_profile = crate::frame_profile::span("particles_emit");

        for (index, e) in self.emitters.iter_mut().enumerate() {
            let dt = if e.spec.no_deadtime {
                dt
            } else {
                world_dt.min(dt)
            };
            if !e.enabled
                || e.spec.rate <= 0.
                || (e.spec.image.is_empty() && e.spec.light.is_some())
                || e.origin.distance_squared(eye) > 3000. * 3000.
            {
                // Keep the enclosed light ready when its lantern comes into range.
                e.carry = if e.enabled && e.spec.constrain.is_some() {
                    (e.carry + dt * e.spec.rate).min(1.)
                } else { 0. };
                continue;
            }
            e.carry = (e.carry + dt * e.spec.rate).min(32.);
            while e.carry >= 1. {
                e.carry -= 1.;
                if self.puffs.len() >= 4096 {
                    break;
                }
                let offset = sampled(e.spec.offset, &mut self.seed)
                    + e.rotation * sampled(e.spec.axis_offset, &mut self.seed);
                let axis = if e.spec.sphere {
                    vec3(
                        random(&mut self.seed) * 2. - 1.,
                        random(&mut self.seed) * 2. - 1.,
                        random(&mut self.seed) * 2. - 1.,
                    )
                    .normalize_or_zero()
                } else {
                    e.rotation * Vec3::X
                };
                let velocity = sampled(e.spec.velocity, &mut self.seed) + axis * e.spec.forward;
                self.puffs.push(Puff {
                    sweep: PuffSweep::default(),
                    position: e.origin + offset * e.scale,
                    constraint_center: e.origin + offset,
                    velocity: e.spec.min_velocity + velocity * e.scale,
                    accel: sampled(e.spec.accel, &mut self.seed),
                    age: 0.,
                    life: e.spec.life.sample(&mut self.seed),
                    size: 32. * e.spec.size.sample(&mut self.seed) * e.scale,
                    roll: if e.spec.random_roll {
                        random(&mut self.seed) * std::f32::consts::TAU
                    } else {
                        0.
                    },
                    emitter: index,
                });
            }
        }
        // Stable retain and append order is birth order for world puffs. Drop
        // the oldest live births first; emitter RNG/credit still run normally.
        crate::phase_census::path_count(crate::phase_census::Counter::WorldNew,self.puffs.len() as u64 - retained as u64);
        #[cfg(target_os = "vita")]
        let births = self.puffs.len() as u32 - retained;
        let dropped = self.puffs.len().saturating_sub(config.puff_cap);
        if dropped > 0 { self.puffs.drain(..dropped); }
        #[cfg(target_os = "vita")]
        drop(_emit_profile);
        #[cfg(target_os = "vita")]
        {
            add_counter(Counter::PuffsBefore, before);
            add_counter(Counter::PuffsAfter, self.puffs.len() as u32);
            add_counter(Counter::PuffsSweeps, sweeps);
            add_counter(Counter::PuffsConstrained, constrained);
            add_counter(Counter::PuffsFrozen, frozen);
            add_counter(Counter::PuffsStationarySweeps, stationary);
            add_counter(Counter::PuffsImpacts, impacts);
            add_counter(Counter::PuffsDeaths, before - retained);
            add_counter(Counter::PuffsBirths, births);
            add_counter(Counter::PuffsSkippedSweeps, skipped);
            add_counter(Counter::PuffsDropped, dropped as u32);
        }
    }
    #[cfg(test)]
    fn update_clocks_sim8(&mut self, dt: f32, world_dt: f32, eye: Vec3, world: &World) {
        let _profile = crate::frame_profile::span("particles");
        #[cfg(target_os = "vita")]
        let _counter_owner =
            crate::frame_profile::counter_owner(crate::frame_profile::Owner::Particles);
        if !dt.is_finite() || dt <= 0. {
            return;
        }
        let dt = dt.min(0.05);
        #[cfg(target_os = "vita")]
        let (before, mut sweeps, mut constrained, mut frozen, mut stationary, mut impacts) =
            (self.puffs.len() as u32, 0u32, 0u32, 0u32, 0u32, 0u32);
        for p in &mut self.puffs {
            #[cfg(target_os = "vita")]
            let _integrate_profile = crate::frame_profile::span("particles_integrate");

            let dt = if self.emitters[p.emitter].spec.no_deadtime {
                dt
            } else {
                world_dt.clamp(0., dt)
            };
            if dt == 0. {
                #[cfg(target_os = "vita")]
                {
                    frozen += 1;
                }
                continue;
            }
            p.age += dt;
            p.velocity += p.accel * dt;
            let mut next = p.position + p.velocity * dt;
            if let Some(bounds) = self.emitters[p.emitter].spec.constrain {
                for axis in 0..3 {
                    if bounds[axis] >= 0.
                        && (next[axis] - p.constraint_center[axis]).abs() > bounds[axis]
                    {
                        p.velocity[axis] = -p.velocity[axis];
                        p.accel[axis] = -p.accel[axis];
                        next[axis] += p.velocity[axis] * dt;
                    }
                }
            }
            // The enclosed pixie requests axis bounds, not world collision.
            // Its casing still occludes the sprite through the depth buffer.
            #[cfg(target_os = "vita")]
            drop(_integrate_profile);
            if self.emitters[p.emitter].spec.constrain.is_none() {
                #[cfg(target_os = "vita")]
                {
                    sweeps += 1;
                    stationary += u32::from(p.position == next);
                }
                if world.sweep(p.position, next, Vec3::ZERO).fraction < 1. {
                    p.age = p.life;
                    #[cfg(target_os = "vita")]
                    {
                        impacts += 1;
                    }
                }
            } else {
                #[cfg(target_os = "vita")]
                {
                    constrained += 1;
                }
            }
            if self.emitters[p.emitter].spec.image == "bubble" {
                #[cfg(target_os = "vita")]
                let _hotspot = crate::frame_profile::span("particles_liquid");
                if world.liquid_at(next) == 0 {
                    p.age = p.life;
                }
            }
            p.position = next;
        }
        {
            #[cfg(target_os = "vita")]
            let _hotspot = crate::frame_profile::span("particles_retain");
            self.puffs.retain(|p| p.age < p.life);
        }
        #[cfg(target_os = "vita")]
        let retained = self.puffs.len() as u32;
        #[cfg(target_os = "vita")]
        let _emit_profile = crate::frame_profile::span("particles_emit");

        for (index, e) in self.emitters.iter_mut().enumerate() {
            let dt = if e.spec.no_deadtime {
                dt
            } else {
                world_dt.min(dt)
            };
            if !e.enabled
                || e.spec.rate <= 0.
                || (e.spec.image.is_empty() && e.spec.light.is_some())
                || e.origin.distance_squared(eye) > 3000. * 3000.
            {
                // Keep the enclosed light ready when its lantern comes into range.
                e.carry = if e.enabled && e.spec.constrain.is_some() {
                    (e.carry + dt * e.spec.rate).min(1.)
                } else { 0. };
                continue;
            }
            e.carry = (e.carry + dt * e.spec.rate).min(32.);
            while e.carry >= 1. {
                e.carry -= 1.;
                if self.puffs.len() >= 4096 {
                    break;
                }
                let offset = sampled(e.spec.offset, &mut self.seed)
                    + e.rotation * sampled(e.spec.axis_offset, &mut self.seed);
                let axis = if e.spec.sphere {
                    vec3(
                        random(&mut self.seed) * 2. - 1.,
                        random(&mut self.seed) * 2. - 1.,
                        random(&mut self.seed) * 2. - 1.,
                    )
                    .normalize_or_zero()
                } else {
                    e.rotation * Vec3::X
                };
                let velocity = sampled(e.spec.velocity, &mut self.seed) + axis * e.spec.forward;
                self.puffs.push(Puff {
                    sweep: PuffSweep::default(),
                    position: e.origin + offset * e.scale,
                    constraint_center: e.origin + offset,
                    velocity: e.spec.min_velocity + velocity * e.scale,
                    accel: sampled(e.spec.accel, &mut self.seed),
                    age: 0.,
                    life: e.spec.life.sample(&mut self.seed),
                    size: 32. * e.spec.size.sample(&mut self.seed) * e.scale,
                    roll: if e.spec.random_roll {
                        random(&mut self.seed) * std::f32::consts::TAU
                    } else {
                        0.
                    },
                    emitter: index,
                });
            }
        }
        #[cfg(target_os = "vita")]
        drop(_emit_profile);
        #[cfg(target_os = "vita")]
        {
            add_counter(Counter::PuffsBefore, before);
            add_counter(Counter::PuffsAfter, self.puffs.len() as u32);
            add_counter(Counter::PuffsSweeps, sweeps);
            add_counter(Counter::PuffsConstrained, constrained);
            add_counter(Counter::PuffsFrozen, frozen);
            add_counter(Counter::PuffsStationarySweeps, stationary);
            add_counter(Counter::PuffsImpacts, impacts);
            add_counter(Counter::PuffsDeaths, before - retained);
            add_counter(Counter::PuffsBirths, self.puffs.len() as u32 - retained);
        }
    }
}

impl<'a> SteamDraw<'a> {
    pub(crate) fn live_puffs(&self)->u64 {self.puffs.len() as u64}
    pub fn depths(&self, camera: Vec3, direction: Vec3) -> impl Iterator<Item = (usize, f32)> + 'a {
        self.puffs
            .iter()
            .enumerate()
            .map(move |(i, p)| (i, (p.position - camera).dot(direction)))
    }
    pub fn prepare_draw(&self, camera: Vec3, atmosphere: &Atmosphere) {
        #[cfg(feature="vita_renderer")]
        crate::frame_profile::route_live_puffs(self.puffs.len());
        if crate::pickup_effects::cached() && crate::render_fx::retained_active() { return; }
        for m in &self.art.materials {
            atmosphere.apply(m, camera);
        }
    }
    #[cfg(feature="vita_renderer")]
    pub(crate) fn prepare_quads(&self, camera: Vec3, direction: Option<Vec3>) {
        let _quads=crate::phase_census::span(crate::phase_census::Region::FxQuadBuild);
        crate::phase_census::count_under(crate::phase_census::Region::Fx,crate::phase_census::Counter::QuadSets,1);
        if crate::render_fx::retained_active() && crate::render::vita_renderer::config().particle_producer_reuse {
            if let Some(direction) = direction.filter(|_| crate::render_fx::world_puff_batches_active()) {
                self.art.quads.borrow_mut().prepare_world(self.puffs, &self.art.emitters, camera, direction);
                return;
            }
            // Preserve the same sorted per-puff registration order, but avoid
            // one Rc geometry/pass publication per additive attached sprite.
            // The joint-alpha bank keeps world/actor/material barriers and owns
            // the final immutable arrays and unchanged quad indices.
            if direction.is_none() && crate::render_fx::world_puff_batches_active()
                && crate::render::vita_renderer::config().attached_puff_batches
                && (crate::sim_maps::fx_attached_persist()
                    || self.puffs.iter().all(|p| self.art.emitters[p.emitter].additive))
            {
                self.art.quads.borrow_mut().prepare_attached(self.puffs, &self.art.emitters, camera);
                return;
            }
            self.art.quads.borrow_mut().prepare(self.puffs, &self.art.emitters, camera, direction);
        }
    }
    /// One attached set in registration order, when `prepare_quads` took the raw path. The
    /// frame's list receives what `draw_one` would have pushed, quad by quad.
    #[cfg(feature="vita_renderer")]
    fn register_raw(&self, order: &[(usize, f32)]) -> bool {
        let quads = self.art.quads.borrow();
        if !crate::sim_maps::fx_attached_persist() || !crate::render_fx::retained_active()
            || !crate::render::vita_renderer::config().particle_producer_reuse
            || order.iter().any(|&(i, _)| quads.raw_ref(i).is_none()) { return false; }
        crate::sim_maps::count("fx_funnel_registered_raw", order.len() as u64);
        crate::render_fx::world_puff_quads(order.iter().map(|&(i, _)| {
            let p = self.puffs.at(i);
            let e = &self.art.emitters[p.emitter];
            // Stage::frame is 0 for at most one image, whatever the age.
            let frame = if e.stage.images.len() <= 1 { 0 } else { e.stage.frame(p.age) };
            (quads.raw_ref(i).unwrap(), e.textures.get(frame).cloned(), e.upload_identities.get(frame).copied(),
                if e.additive || cfg!(all(feature="sim_maps_probe",not(target_os="vita"))) && negative("persist-blend") { crate::materials::Blend::Add } else { crate::materials::Blend::Alpha })
        }));
        true
    }
    pub fn draw_one(&self, index: usize, camera: Vec3, direction: Vec3) {
        if crate::draw_diagnostics::skip(crate::draw_diagnostics::Class::Fx) { return; }
        #[cfg(feature="vita_renderer")]
        if crate::render_fx::retained_active() && crate::render::vita_renderer::config().particle_producer_reuse {
            if let Some(vertices) = self.art.quads.borrow().raw_quad(index) {
                crate::sim_maps::count("fx_funnel_registered_raw", 1);
                let e = &self.art.emitters[self.puffs.at(index).emitter];
                crate::render_fx::world_puff_quad_identified(vertices, e.textures.get(e.stage.frame(self.puffs.at(index).age)).cloned(), e.upload_identities.get(e.stage.frame(self.puffs.at(index).age)).copied(),
                    if e.additive { crate::materials::Blend::Add } else { crate::materials::Blend::Alpha });
                return;
            }
            if let Some(geometry) = self.art.quads.borrow().geometry(index) {
                crate::sim_maps::count("fx_funnel_registered_publication", 1);
                let e = &self.art.emitters[self.puffs.at(index).emitter];
                crate::render_fx::effect_geometry_identified(geometry, e.textures.get(e.stage.frame(self.puffs.at(index).age)).cloned(), e.upload_identities.get(e.stage.frame(self.puffs.at(index).age)).copied(),
                    if e.additive { crate::materials::Blend::Add } else { crate::materials::Blend::Alpha });
                return;
            }
        }
        crate::sim_maps::count("fx_funnel_registered_legacy", 1);
        let e = &self.art.emitters[self.puffs.at(index).emitter];
        #[cfg(feature="vita_renderer")]
        if !crate::render_fx::retained_active(){gl_use_material(&self.art.materials[usize::from(e.additive)]);}
        #[cfg(not(feature="vita_renderer"))]
        gl_use_material(&self.art.materials[usize::from(e.additive)]);
        let mesh = self.puff_mesh(index, camera, direction);
        crate::render_fx::effect(&mesh, if e.additive { crate::materials::Blend::Add } else { crate::materials::Blend::Alpha });
    }
    fn puff_mesh(&self, index: usize, camera: Vec3, direction: Vec3) -> Mesh {
        let right = direction.cross(Vec3::Z).try_normalize().unwrap_or(Vec3::X);
        let up = right.cross(direction).normalize_or_zero();
        let p = self.puffs.at(index);
        let e = &self.art.emitters[p.emitter];
        let radius = e.spec.grown(p.size, p.age);
        let x = (right * p.roll.cos() + up * p.roll.sin()) * radius;
        let y = (-right * p.roll.sin() + up * p.roll.cos()) * radius;
        let opacity = e.spec.alpha
            * if e.spec.fade { 1. - p.age / p.life } else { 1. }
            * (p.age / e.spec.fade_in.max(0.02)).min(1.)
            * (camera.distance(p.position) / 28.).min(1.);
        let color = Color::new(e.spec.color.x, e.spec.color.y, e.spec.color.z, opacity);
        let vertices = [
            p.position - x - y,
            p.position + x - y,
            p.position + x + y,
            p.position - x + y,
        ]
        .into_iter()
        .zip([vec2(0., 1.), vec2(1., 1.), vec2(1., 0.), vec2(0., 0.)])
        .map(|(position, uv)| Vertex {
            position,
            uv,
            color: color.into(),
            normal: Vec4::ZERO,
        })
        .collect();
        Mesh { upload_identity: e.upload_identities.get(e.stage.frame(p.age)).copied(), surface: None, vertices, indices: vec![0, 1, 2, 0, 2, 3],
            texture: e.textures.get(e.stage.frame(p.age)).cloned() }

    }
}
const FRAGMENT: &str = r#"#version 100
precision mediump float; uniform sampler2D Texture; uniform float Additive; varying highp vec2 uv; varying lowp vec4 color;
// FOG
void main(){vec4 t=texture2D(Texture,uv)*color; if(Additive>0.5)t.rgb*=t.a;gl_FragColor=vec4(fogged(t.rgb,Additive),t.a);}
"#;

/// Gnome disappearance: the original pickup sprite and a deterministic, finite
/// 200-particle burst. Sampling the saved scene age also reconstructs a mid-burst load.
pub struct PickupBurst {
    texture: Texture2D,
    material: std::rc::Rc<Material>,
}
thread_local! {
    static BURST_MATERIAL: std::cell::RefCell<std::rc::Weak<Material>>=Default::default();
}
impl PickupBurst {
    pub fn load(assets: &mut Assets) -> Result<Self> {
        let specs = texture::read_materials(assets)?;
        let path = texture::resolve(assets, "supra_p", &specs)
            .ok_or_else(|| anyhow::anyhow!("Missing gnome disappearance sprite"))?;
        let image = texture::decode(assets, &path)?;
        let texture = crate::texture::upload_rgba8(image.width, image.height, &image.pixels);
        texture.set_filter(FilterMode::Linear);
        let material = BURST_MATERIAL.with(|cache| -> Result<std::rc::Rc<Material>> {
            if let Some(m) = cache.borrow().upgrade() {
                return Ok(m);
            }
            let mut uniforms = crate::environment::uniforms();
            uniforms.push(UniformDesc::new("Additive", UniformType::Float1));
            let m = crate::frame_profile::load_material(
                ShaderSource::Glsl {
                    vertex: crate::character::VERTEX,
                    fragment: &crate::environment::fragment(FRAGMENT),
                },
                MaterialParams {
                    uniforms,
                    pipeline_params: crate::render::depth_pipeline(Some(BlendState::new(
                        Equation::Add,
                        BlendFactor::One,
                        BlendFactor::One,
                    ))),
                    ..Default::default()
                },
            )
            .map_err(|e| anyhow::anyhow!("Burst shader: {e:?}"))?;
            m.set_uniform("Additive", 1_f32);
            let m = std::rc::Rc::new(m);
            *cache.borrow_mut() = std::rc::Rc::downgrade(&m);
            Ok(m)
        })?;
        Ok(Self { texture, material })
    }
    pub fn draw(&self, origin: Vec3, age: f32, camera: Vec3, atmosphere: &Atmosphere) {
        if crate::draw_diagnostics::skip(crate::draw_diagnostics::Class::Fx) { return; }
        if !(0. ..4.).contains(&age) {
            return;
        }
        atmosphere.apply(&self.material, camera);
        gl_use_material(&self.material);
        let direction = (origin - camera).normalize_or_zero();
        let right = direction.cross(Vec3::Z).try_normalize().unwrap_or(Vec3::X);
        let up = right.cross(direction).normalize_or_zero();
        let mut seed = 0x4824acd;
        let mut vertices = Vec::new();
        let mut indices = Vec::new();
        for _ in 0..200 {
            let offset = vec3(
                (random(&mut seed) * 2. - 1.) * 20.,
                (random(&mut seed) * 2. - 1.) * 20.,
                random(&mut seed) * 60.,
            );
            let life = 3. + random(&mut seed);
            let accel = random(&mut seed) * 50.;
            let size = (0.25 + random(&mut seed) * 0.25) * 32.;
            if age >= life {
                continue;
            }
            let center = origin + offset + Vec3::Z * (0.5 * accel * age * age);
            let alpha = (age / 0.2).min(1.) * (1. - age / life);
            let color: [u8; 4] = Color::new(1., 1., 1., alpha).into();
            let base = vertices.len() as u16;
            for (delta, uv) in [
                (-right - up, vec2(0., 1.)),
                (right - up, vec2(1., 1.)),
                (right + up, vec2(1., 0.)),
                (-right + up, vec2(0., 0.)),
            ] {
                vertices.push(Vertex {
                    position: center + delta * size,
                    uv,
                    normal: Vec4::ZERO,
                    color,
                });
            }
            indices.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
        }
        crate::render_fx::effect(
            &Mesh { upload_identity: None, surface: None,
                vertices,
                indices,
                texture: Some(self.texture.clone()),
            },
            crate::materials::Blend::Add,
        );
        gl_use_default_material();
    }
}
pub fn check(assets: &mut Assets) -> Result<()> {
    let names = assets
        .names()
        .filter(|n| n.starts_with("models/") && supported(n.trim_start_matches("models/")))
        .map(str::to_owned)
        .collect::<Vec<_>>();
    let mut total = 0;
    for name in names {
        let specs = parse(&String::from_utf8_lossy(&assets.read(&name)?));
        if specs.is_empty() {
            println!("Emitter declaration deferred (no supported sprite/light): {name}");
        }
        total += specs.len();
    }
    println!("PASS {total} supported emitter definitions including multi-emitter fire and tagged waterfall spray");
    Ok(())
}

// Emitters and attached animation effects share two pipelines across model/map reloads.
thread_local! {
    static PARTICLE_MATERIALS: std::cell::RefCell<Vec<std::rc::Weak<Material>>> = const { std::cell::RefCell::new(Vec::new()) };
}
fn shared_materials() -> Result<Vec<std::rc::Rc<Material>>> {
    PARTICLE_MATERIALS.with(|cache| {
        let existing: Option<Vec<_>> = cache.borrow().iter().map(|m| m.upgrade()).collect();
        if let Some(m) = existing.filter(|m| m.len() == 2) {
            return Ok(m);
        }
        let fragment = crate::environment::fragment(FRAGMENT);
        let mut materials = Vec::new();
        for additive in [false, true] {
            let mut uniforms = crate::environment::uniforms();
            uniforms.push(UniformDesc::new("Additive", UniformType::Float1));
            let material = crate::frame_profile::load_material(
                ShaderSource::Glsl {
                    vertex: crate::character::VERTEX,
                    fragment: &fragment,
                },
                MaterialParams {
                    uniforms,
                    pipeline_params: crate::render::depth_pipeline(Some(BlendState::new(
                        Equation::Add,
                        if additive {
                            BlendFactor::One
                        } else {
                            BlendFactor::Value(BlendValue::SourceAlpha)
                        },
                        if additive {
                            BlendFactor::One
                        } else {
                            BlendFactor::OneMinusValue(BlendValue::SourceAlpha)
                        },
                    ))),
                    ..Default::default()
                },
            )
            .map_err(|e| anyhow::anyhow!("Particle shader: {e:?}"))?;
            material.set_uniform("Additive", if additive { 1_f32 } else { 0. });
            materials.push(std::rc::Rc::new(material));
        }

        *cache.borrow_mut() = materials.iter().map(std::rc::Rc::downgrade).collect();
        Ok(materials)
    })
}

/// A pure sample of an animation-owned emitter. Birth IDs provide repeatable
/// randomness; sample the tag at birth so existing particles do not follow it.
pub struct Attached {
    persisted: Vec<PersistedEmitter>,
    // Both parts remain on the art owner and are sampled together for attached effects.
    steam: Steam,
    // Keep emitted world poses while time advances: moving the owning actor
    // must not drag smoke already in the air. Rewinds rebuild cosmetic births.
    births: std::collections::BTreeMap<(usize, u32), crate::skeletal::Transform>,
    age: f32,
    reuse_scratch: bool,
    order: Vec<(usize, f32)>,
    live_births: Vec<(usize, u32)>,
}
/// Host oracle sabotage, read through the existing control variable. False in other builds.
#[cfg(any(feature="vita_renderer", all(feature="sim_maps_probe", not(target_os="vita"))))]
fn negative(_name: &str) -> bool {
    #[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
    { crate::sim_maps::probe::negative_control() == Some(_name) }
    #[cfg(not(all(feature="sim_maps_probe",not(target_os="vita"))))]
    { false }
}
/// What `Attached::sample` derives from one birth, kept until the birth leaves the window.
/// A birth's constants depend only on its number, its emitter's spec and its pose.
#[derive(Clone, Copy)]
struct PersistedBirth {
    at: f32,
    stage: BirthStage,
    life: f32,
    seed: u32,
    translation: Vec3,
    rotation: Quat,
    offset: Vec3,
    velocity: Vec3,
    accel: Vec3,
    size: f32,
    roll: f32,
}
#[derive(Clone, Copy, PartialEq)]
enum BirthStage {
    /// Nothing known but `at`. A disabled birth returns here and forgets its pose, as the map does.
    Unknown,
    /// `life` and the seed after it.
    Life,
    /// Also the pose. A spec change falls back here: the pose survives it, the rest does not.
    Pose,
    Complete,
}
/// Consecutive births from `first`. Bounded by the sampler's own window of 1025.
#[derive(Default)]
struct PersistedEmitter {
    first: u32,
    slots: std::collections::VecDeque<PersistedBirth>,
}
impl PersistedEmitter {
    fn window(&mut self, start: u32, last: u32, at: impl Fn(u32) -> f32) {
        if start > last { self.slots.clear(); self.first = start; return; }
        let end = self.first as u64 + self.slots.len() as u64; // exclusive
        if self.slots.is_empty() || start as u64 >= end || start < self.first {
            self.slots.clear();
            self.first = start;
        } else {
            self.slots.drain(..(start - self.first) as usize);
            self.first = start;
            self.slots.truncate((last - start) as usize + 1);
        }
        for birth in self.first as u64 + self.slots.len() as u64..=last as u64 {
            self.slots.push_back(PersistedBirth { at: at(birth as u32), stage: BirthStage::Unknown, life: 0., seed: 0,
                translation: Vec3::ZERO, rotation: Quat::IDENTITY, offset: Vec3::ZERO, velocity: Vec3::ZERO,
                accel: Vec3::ZERO, size: 0., roll: 0. });
        }
    }
}
impl Attached {
    /// A spec edit changes what the sampler derives from a birth, never its pose.
    fn persisted_spec_changed(&mut self) {
        for emitter in &mut self.persisted {
            for slot in &mut emitter.slots {
                if slot.stage == BirthStage::Complete { slot.stage = BirthStage::Pose; }
            }
        }
    }
    /// Item sprites use their image dimensions as the unscaled billboard size.
    /// Keep this opt-in: older effect owners retain their existing size tuning.
    pub(crate) fn use_sprite_dimensions(&mut self) {
        self.persisted_spec_changed();
        for (e, visual) in self.steam.sim.emitters.iter_mut().zip(&self.steam.art.emitters) {
            let diameter = visual.textures.first().map_or(64., Texture2D::width);
            let factor = diameter / 64.; // Puff's legacy unit diameter is 64.
            e.spec.size.low *= factor;
            e.spec.size.high *= factor;
        }
    }
    /// Attenuate this instance without changing shared materials or other effects.
    pub(crate) fn attenuate(&mut self, opacity: f32) {
        self.persisted_spec_changed();
        for visual in &mut self.steam.art.emitters {
            visual.spec.alpha *= opacity.clamp(0., 1.);
        }
    }
    /// Change only billboard size, preserving tag placement and particle travel.
    pub(crate) fn resize(&mut self, factor: f32) {
        self.persisted_spec_changed();
        for e in &mut self.steam.sim.emitters {
            e.spec.size.low *= factor.clamp(0., 1.);
            e.spec.size.high *= factor.clamp(0., 1.);
        }
    }
    /// Mouth jets use the animated attachment frame for their spread.
    pub(crate) fn orient_velocity(&mut self, name: &str) {
        self.persisted_spec_changed();
        for e in &mut self.steam.sim.emitters {
            if e.spec.name == name {
                e.spec.tag_velocity = true;
            }
        }
    }
    /// A separate emitter clock sharing immutable textures/materials.
    pub(crate) fn fork(&self) -> Self {
        Self {
            persisted: Vec::new(),
            births: Default::default(),
            age: 0.,
            reuse_scratch: false,
            order: Vec::new(),
            live_births: Vec::new(),
            steam: Steam::new(SteamSim {
                animations: Vec::new(),
                emitters: self.steam.sim.emitters.clone(),
                puffs: Vec::new(),
                seed: 1,
            }, self.steam.art.emitters.clone(), self.steam.art.materials.clone()),
        }
    }
    pub(crate) fn fork_cached(&self) -> Self {
        let mut instance = self.fork();
        instance.reuse_scratch = true;
        instance
    }
    pub fn load(
        assets: &mut Assets,
        name: &str,
        materials: &std::collections::BTreeMap<String, texture::MaterialSpec>,
    ) -> Result<Option<Self>> {
        let definitions = parse(&String::from_utf8_lossy(
            &assets.read(&format!("models/{name}.tik"))?,
        ));
        Self::from_specs(assets, definitions, materials)
    }
    /// One-shot client animation commands, sampled from a saved owning clock.
    pub(crate) fn load_bursts(
        assets: &mut Assets,
        name: &str,
        frame_time: f32,
        materials: &std::collections::BTreeMap<String, texture::MaterialSpec>,
    ) -> Result<Option<Self>> {
        Self::load_clip_bursts(assets, name, None, frame_time, materials)
    }
    pub(crate) fn load_clip_bursts(
        assets: &mut Assets,
        name: &str,
        clip: Option<&str>,
        frame_time: f32,
        materials: &std::collections::BTreeMap<String, texture::MaterialSpec>,
    ) -> Result<Option<Self>> {
        let text =
            String::from_utf8_lossy(&assets.read(&format!("models/{name}.tik"))?).into_owned();
        Self::from_specs(assets, burst_specs(&text, clip, frame_time), materials)
    }
    fn from_specs(
        assets: &mut Assets,
        definitions: Vec<Spec>,
        materials: &std::collections::BTreeMap<String, texture::MaterialSpec>,
    ) -> Result<Option<Self>> {
        if definitions.is_empty() {
            return Ok(None);
        }
        let mut emitters = Vec::new();
        let mut visuals = Vec::new();
        for mut spec in definitions {
            let stage = materials
                .get(&spec.emission.image)
                .and_then(|s| s.stages.first())
                .cloned()
                .unwrap_or_else(|| crate::materials::Stage {
                    images: vec![spec.emission.image.clone()],
                    ..Default::default()
                });
            let mut textures = Vec::new();
            let mut upload_identities = Vec::new();
            for frame in &stage.images {
                let Some(path) = texture::resolve(assets, frame, materials) else {
                    continue;
                };
                let shared = texture::decode_upload_shared_full(assets, &path)?;
                #[cfg(all(feature = "sim_maps_probe", not(target_os = "vita")))]
                crate::sim_maps::probe::draw_query::label_texture(shared.texture.raw_miniquad_id(), &path);
                #[cfg(feature = "vita_renderer")]
                crate::render_fx::retained::note_particle_texture_zero_alpha(&shared.texture, shared.zero_alpha);
                upload_identities.push(shared.upload_identity);
                textures.push(shared.texture);
            }
            if textures.is_empty() || textures.len() != stage.images.len() {
                continue;
            }
            let additive = matches!(
                stage.blend,
                crate::materials::Blend::Add | crate::materials::Blend::AlphaAdd
            );
            spec.true_bubble_size(textures.first().map(Texture2D::width));
            visuals.push(EmitterVisual::new(0, &spec, textures, upload_identities, stage, additive));
            emitters.push(EmitterSim {
                id: 0,
                pickup: false,
                origin: Vec3::ZERO,
                rotation: Quat::IDENTITY,
                local: crate::skeletal::Transform {
                    translation: Vec3::ZERO,
                    rotation: Quat::IDENTITY,
                },
                scale: 1.,
                spec: spec.emission,
                light_style: None,
                light_color: None,
                carry: 0.,
                enabled: true,
                animation_enabled: None,
            });
        }
        Ok(Some(Self {
            persisted: Vec::new(),
            births: Default::default(),
            age: 0.,
            reuse_scratch: false,
            order: Vec::new(),
            live_births: Vec::new(),
            steam: Steam::new(SteamSim {
                animations: Vec::new(),
                emitters,
                puffs: Vec::new(),
                seed: 1,
            }, visuals, shared_materials()?),
        }))
    }
    pub fn draw(
        &mut self,
        age: f32,
        scale: f32,
        birth_pose: impl Fn(f32, Option<&str>) -> crate::skeletal::Transform,
        enabled: impl Fn(&str, f32, bool) -> bool,
        camera: Vec3,
        atmosphere: &Atmosphere,
    ) {
        if crate::character::skinning::overlap::gathering() { return; }
        {
            let _sampling = crate::frame_profile::object_timer(crate::frame_profile::ObjectWork::Sampling);
            self.sample(age, scale, birth_pose, enabled);
        }
        self.draw_sampled(camera, atmosphere);
    }
    #[cfg(not(target_os="vita"))]
    pub(crate) fn live_puffs(&self) -> usize { self.steam.sim.puffs.len() }
    pub(crate) fn draw_sampled(&mut self, camera: Vec3, atmosphere: &Atmosphere) {
        if crate::draw_diagnostics::skip(crate::draw_diagnostics::Class::Fx) { return; }
        if crate::character::skinning::overlap::gathering() { return; }
        #[cfg(feature="vita_renderer")]
        let _producer = crate::light_profile::fx_timer(crate::light_profile::FxMetric::Producer);
        let _packets = crate::frame_profile::object_timer(crate::frame_profile::ObjectWork::Packets);
        self.steam.prepare_draw(camera, atmosphere);
        #[cfg(feature="vita_renderer")] {
            let _time = crate::sim_maps::timer("r26_attached_quads_ns");
            self.steam.prepare_quads(camera, None);
        }
        let register_time = crate::sim_maps::timer("r26_attached_register_ns");
        let _registration=crate::phase_census::span(crate::phase_census::Region::FxRegistration);
        crate::phase_census::count_under(crate::phase_census::Region::Fx,crate::phase_census::Counter::Registrations,1);
        let mut order = if self.reuse_scratch { std::mem::take(&mut self.order) } else { Vec::new() };
        order.clear();
        order.extend(self
            .steam
            .sim.puffs
            .iter()
            .enumerate()
            .map(|(i, p)| (i, p.position.distance_squared(camera)))
            );
        order.sort_by(|a, b| b.1.total_cmp(&a.1));
        #[cfg(feature="vita_renderer")]
        let registered = self.steam.live_draw().register_raw(&order);
        #[cfg(not(feature="vita_renderer"))]
        let registered = false;
        if !registered {
            for &(i, _) in &order {
                self.steam.draw_one(
                    i,
                    camera,
                    (self.steam.sim.puffs[i].position - camera).normalize_or_zero(),
                );
            }
        }
        if self.reuse_scratch { self.order = order; }
        drop(register_time);
        gl_use_default_material();
    }
    #[cfg(test)]
    pub(crate) fn output_bits(&self, camera: Vec3) -> serde_json::Value {
        let mut order: Vec<_> = self.steam.sim.puffs.iter().enumerate()
            .map(|(i, p)| (i, p.position.distance_squared(camera))).collect();
        order.sort_by(|a, b| b.1.total_cmp(&a.1));
        serde_json::json!(order.into_iter().map(|(i, _)| {
            let p = &self.steam.sim.puffs[i];
            let mesh = self.steam.live_draw().puff_mesh(i, camera, (p.position - camera).normalize_or_zero());
            serde_json::json!({"emitter": p.emitter, "blend_add": self.steam.art.emitters[p.emitter].additive,
                "birth_age_life_size_roll": ([p.age, p.life, p.size, p.roll].map(f32::to_bits)),
                "vertices": mesh.vertices.iter().map(|v| (v.position.to_array().map(f32::to_bits), v.uv.to_array().map(f32::to_bits), v.color)).collect::<Vec<_>>(),
                "indices": mesh.indices})
        }).collect::<Vec<_>>())
    }
    /// `sample` with each birth's constants kept. Same puffs, bits and order, and `birth_pose`
    /// and `enabled` see the same calls. Only the position, age and scaled size are per frame.
    fn sample_persisted(
        &mut self,
        age: f32,
        scale: f32,
        birth_pose: impl Fn(f32, Option<&str>) -> crate::skeletal::Transform,
        enabled: impl Fn(&str, f32, bool) -> bool,
    ) {
        self.steam.sim.puffs.clear();
        if !age.is_finite() || age < 0. || !scale.is_finite() || scale <= 0. {
            self.persisted.clear();
            return;
        }
        if age < self.age { self.persisted.clear(); }
        self.age = age;
        self.persisted.resize_with(self.steam.sim.emitters.len(), Default::default);
        let mut full = false;
        let sphere = crate::sim_maps::fx_attached_sphere();
        for (index, e) in self.steam.sim.emitters.iter().enumerate() {
            let emitter = &mut self.persisted[index];
            if full || e.spec.burst.is_some_and(|(at, _)| age < at) {
                emitter.slots.clear();
                continue;
            }
            let last = e.spec.burst.map_or_else(
                || (age * e.spec.rate).floor() as u32,
                |(_, n)| n.saturating_sub(1),
            );
            let first = if e.spec.burst.is_some() {
                0
            } else {
                ((age - e.spec.life.high).max(0.) * e.spec.rate).ceil() as u32
            };
            emitter.window(first.max(last.saturating_sub(1024)), last, |birth| e.spec.burst
                .map_or(birth as f32 / e.spec.rate, |(at, _)| at));
            let window_first = emitter.first;
            for (position, slot) in emitter.slots.iter_mut().enumerate() {
                if full || self.steam.sim.puffs.len() >= 4096 {
                    full = true;
                    slot.stage = BirthStage::Unknown;
                    continue;
                }
                let at = slot.at;
                if !enabled(&e.spec.name, at, !e.spec.start_off) {
                    slot.stage = BirthStage::Unknown;
                    continue;
                }
                if slot.stage == BirthStage::Unknown {
                    let birth = window_first.wrapping_add(position as u32);
                    let mut seed =
                        birth.wrapping_add(1).wrapping_mul(0x9e3779b9) ^ ((index as u32 + 1) * 7919);
                    slot.life = e.spec.life.sample(&mut seed);
                    slot.seed = seed;
                    slot.stage = BirthStage::Life;
                }
                let elapsed = age - at;
                if elapsed >= slot.life {
                    continue;
                }
                if slot.stage == BirthStage::Life {
                    crate::phase_census::count_under(crate::phase_census::Region::Fx,crate::phase_census::Counter::AttachedNew,1);
                    let pose = birth_pose(at, e.spec.tag.as_deref());
                    slot.translation = pose.translation;
                    slot.rotation = pose.rotation;
                    slot.stage = BirthStage::Pose;
                }
                if slot.stage == BirthStage::Pose {
                    let mut seed = slot.seed;
                    slot.offset = sampled(e.spec.offset, &mut seed)
                        + slot.rotation * sampled(e.spec.axis_offset, &mut seed);
                    let random_velocity = sampled(e.spec.velocity, &mut seed);
                    let forward = if sphere && e.spec.sphere { sphere_axis(&mut seed) } else { slot.rotation * Vec3::X };
                    slot.velocity = if e.spec.tag_velocity {
                        slot.rotation * random_velocity
                    } else {
                        random_velocity
                    } + forward * e.spec.forward;
                    slot.accel = sampled(e.spec.accel, &mut seed);
                    slot.size = 32. * e.spec.size.sample(&mut seed);
                    slot.roll = if e.spec.random_roll {
                        random(&mut seed) * std::f32::consts::TAU
                    } else {
                        0.
                    };
                    slot.stage = BirthStage::Complete;
                }
                #[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
                if negative("persist-size") { slot.size *= 1.0001; }
                self.steam.sim.puffs.push(Puff {
                    sweep: PuffSweep::default(),
                    constraint_center: slot.translation + slot.offset,
                    position: slot.translation
                        + slot.offset * scale
                        + (e.spec.min_velocity + slot.velocity * scale) * elapsed
                        + slot.accel * (0.5 * elapsed * elapsed),
                    velocity: Vec3::ZERO,
                    accel: Vec3::ZERO,
                    age: elapsed,
                    life: slot.life,
                    size: slot.size * scale,
                    roll: slot.roll,
                    emitter: index,
                });
            }
        }
    }
    pub(crate) fn sample(
        &mut self,
        age: f32,
        scale: f32,
        birth_pose: impl Fn(f32, Option<&str>) -> crate::skeletal::Transform,
        enabled: impl Fn(&str, f32, bool) -> bool,
    ) {
        #[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
        let _profile = crate::frame_profile::span("attached_sample");
        let _sample=crate::phase_census::span(crate::phase_census::Region::FxAttachedSample);
        let mut diagnostic_new=0u64;
        crate::phase_census::count_under(crate::phase_census::Region::Fx,crate::phase_census::Counter::AttachedSamples,1);
        let _time = crate::sim_maps::timer("r26_attached_sample_ns");
        if crate::sim_maps::fx_attached_persist() {
            self.sample_persisted(age, scale, birth_pose, enabled);
            crate::phase_census::count_under(crate::phase_census::Region::Fx,crate::phase_census::Counter::AttachedLive,self.steam.sim.puffs.len() as u64);
            crate::sim_maps::count("fx_funnel_attached_sets", 1);
            crate::sim_maps::count("fx_funnel_attached_sampled", self.steam.sim.puffs.len() as u64);
            return;
        }
        self.steam.sim.puffs.clear();
        self.live_births.clear();
        let mut previous = if self.reuse_scratch { Default::default() } else { std::mem::take(&mut self.births) };
        if !age.is_finite() || age < 0. || !scale.is_finite() || scale <= 0. {
            self.births.clear();
            return;
        }
        if age < self.age {
            previous.clear();
            self.births.clear();
        }
        self.age = age;
        let sphere = crate::sim_maps::fx_attached_sphere();
        'emitters: for (index, e) in self.steam.sim.emitters.iter().enumerate() {
            if e.spec.burst.is_some_and(|(at, _)| age < at) {
                continue;
            }
            let last = e.spec.burst.map_or_else(
                || (age * e.spec.rate).floor() as u32,
                |(_, n)| n.saturating_sub(1),
            );
            let first = if e.spec.burst.is_some() {
                0
            } else {
                ((age - e.spec.life.high).max(0.) * e.spec.rate).ceil() as u32
            };
            for birth in first.max(last.saturating_sub(1024))..=last {
                if self.steam.sim.puffs.len() >= 4096 {
                    break 'emitters;
                }
                let at = e
                    .spec
                    .burst
                    .map_or(birth as f32 / e.spec.rate, |(at, _)| at);
                if !enabled(&e.spec.name, at, !e.spec.start_off) {
                    continue;
                }
                let mut seed =
                    birth.wrapping_add(1).wrapping_mul(0x9e3779b9) ^ ((index as u32 + 1) * 7919);
                let life = e.spec.life.sample(&mut seed);
                let elapsed = age - at;
                if elapsed >= life {
                    continue;
                }
                let key = (index, birth);
                let pose = if self.reuse_scratch { self.births.get(&key).copied() } else { previous.remove(&key) }
                    .unwrap_or_else(|| {diagnostic_new+=1;birth_pose(at, e.spec.tag.as_deref())});
                self.births.insert(key, pose);
                if self.reuse_scratch { self.live_births.push(key); }
                let offset = sampled(e.spec.offset, &mut seed)
                    + pose.rotation * sampled(e.spec.axis_offset, &mut seed);
                let random_velocity = sampled(e.spec.velocity, &mut seed);
                let forward = if sphere && e.spec.sphere { sphere_axis(&mut seed) } else { pose.rotation * Vec3::X };
                let velocity = if e.spec.tag_velocity {
                    pose.rotation * random_velocity
                } else {
                    random_velocity
                } + forward * e.spec.forward;
                let accel = sampled(e.spec.accel, &mut seed);
                self.steam.sim.puffs.push(Puff {
                    sweep: PuffSweep::default(),
                    constraint_center: pose.translation + offset,
                    position: pose.translation
                        + offset * scale
                        + (e.spec.min_velocity + velocity * scale) * elapsed
                        + accel * (0.5 * elapsed * elapsed),
                    velocity: Vec3::ZERO,
                    accel: Vec3::ZERO,
                    age: elapsed,
                    life,
                    size: 32. * e.spec.size.sample(&mut seed) * scale,
                    roll: if e.spec.random_roll {
                        random(&mut seed) * std::f32::consts::TAU
                    } else {
                        0.
                    },
                    emitter: index,
                });
            }
        }
        crate::phase_census::count_under(crate::phase_census::Region::Fx,crate::phase_census::Counter::AttachedLive,self.steam.sim.puffs.len() as u64);
        crate::phase_census::count_under(crate::phase_census::Region::Fx,crate::phase_census::Counter::AttachedNew,diagnostic_new);
        if self.reuse_scratch {
            self.births.retain(|key, _| self.live_births.binary_search(key).is_ok());
        }
        crate::sim_maps::count("fx_funnel_attached_sets", 1);
        crate::sim_maps::count("fx_funnel_attached_sampled", self.steam.sim.puffs.len() as u64);

    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    pub(crate) fn world_test_steam() -> Steam { attached_fixture(2).steam }
    #[test]
    fn sphere_axis_is_per_axis_and_growth_keeps_its_old_form_unless_compounding() {
        let mut seed = 7;
        let axes: Vec<Vec3> = (0..4096).map(|_| sphere_axis(&mut seed)).collect();
        assert!(axes.iter().all(|a| a.abs().cmple(Vec3::ONE).all()));
        assert!(axes.iter().any(|a| a.length() > 1.2) && axes.iter().any(|a| a.length() < 0.8), "not normalized");
        let mut spec = VisualSpec { growth: 0.1, ..Default::default() };
        assert_eq!(spec.grown(8., 5.).to_bits(), (8. * (1. + 0.1f32 * 5.)).to_bits());
        spec.growth = -1.;
        assert_eq!(spec.grown(8., 5.), 0.);
        spec.growth = 0.1;
        spec.growth_compounds = true;
        assert!((spec.grown(8., 5.) - 8. * 0.5f32.exp()).abs() < 1e-5);
    }
    #[cfg(feature="vita_renderer")]
    pub(crate) fn census_quad_fixture(attached:&mut Attached) {
        // CPU producer entry used by prepare_quads, no retained renderer/GL.
        let _quads=crate::phase_census::span(crate::phase_census::Region::FxQuadBuild);
        crate::phase_census::count_under(crate::phase_census::Region::Fx,crate::phase_census::Counter::QuadSets,1);
        attached.steam.art.quads.borrow_mut().prepare(RenderPuffs::Live(&attached.steam.sim.puffs),&attached.steam.art.emitters,Vec3::Z*100.,None);
    }

    /// A set that uses every per-birth branch of the sampler.
    fn persist_fixture() -> Attached {
        let mut a = attached_fixture(4);
        let range = |low: f32, high: f32| Range { low, high };
        let spread = [range(-3., 5.), range(-1., 2.), range(0., 7.)];
        for (i, e) in a.steam.sim.emitters.iter_mut().enumerate() {
            e.spec.name = format!("e{i}");
            e.spec.rate = 40. + 17. * i as f32;
            e.spec.life = range(0.4, 1.3 + i as f32 * 0.2);
            e.spec.offset = spread; e.spec.axis_offset = spread; e.spec.velocity = spread; e.spec.accel = spread;
            e.spec.size = range(0.2, 1.1);
            e.spec.min_velocity = vec3(1., 2., 3.);
            e.spec.forward = i as f32;
            e.spec.tag_velocity = i % 2 == 0;
            e.spec.random_roll = i != 1;
            e.spec.start_off = i == 2;
        }
        a.steam.sim.emitters[3].spec.burst = Some((0.9, 40));
        a
    }
    fn puff_bits(a: &Attached) -> Vec<[u32; 13]> {
        a.steam.sim.puffs.iter().map(|p| {
            let (x, c) = (p.position.to_array().map(f32::to_bits), p.constraint_center.to_array().map(f32::to_bits));
            [x[0], x[1], x[2], c[0], c[1], c[2], p.age.to_bits(), p.life.to_bits(), p.size.to_bits(), p.roll.to_bits(),
                p.velocity.length_squared().to_bits(), p.accel.length_squared().to_bits(), p.emitter as u32]
        }).collect()
    }
    /// One scripted frame on both arms. The pose depends on the frame it is asked in, so a pose
    /// kept too long or dropped too early shows in the bits, and the calls are compared too.
    fn persist_step(legacy: &mut Attached, persisted: &mut Attached, frame: u32, age: f32, scale: f32, enabled: &dyn Fn(&str, f32, bool) -> bool) {
        let calls = [std::cell::RefCell::new(Vec::new()), std::cell::RefCell::new(Vec::new())];
        let pose = |arm: usize| { let calls = &calls[arm]; move |at: f32, tag: Option<&str>| {
            calls.borrow_mut().push((at.to_bits(), tag.map(str::to_owned)));
            let f = frame as f32;
            crate::skeletal::Transform { translation: vec3(at * 3. + f, -f * 0.5, 2.),
                rotation: Quat::from_rotation_z(f * 0.1 + at) }
        }};
        crate::sim_maps::with_fx_attached_persist(false, || legacy.sample(age, scale, pose(0), enabled));
        crate::sim_maps::with_fx_attached_persist(true, || persisted.sample(age, scale, pose(1), enabled));
        assert_eq!(puff_bits(legacy), puff_bits(persisted), "frame {frame} age {age}");
        assert_eq!(*calls[0].borrow(), *calls[1].borrow(), "pose calls, frame {frame} age {age}");
    }
    #[test]
    fn persisted_sample_matches_the_map_through_time_rewind_gates_scale_and_spec_edits() {
        for reuse in [false, true] {
            let (mut legacy, mut persisted) = (persist_fixture(), persist_fixture());
            legacy.reuse_scratch = reuse;
            persisted.reuse_scratch = reuse;
            let all = |_: &str, _: f32, _: bool| true;
            let mut frame = 0;
            let mut clock = 0.;
            let mut run = |legacy: &mut Attached, persisted: &mut Attached, steps: u32, dt: f32, scale: f32, enabled: &dyn Fn(&str, f32, bool) -> bool| {
                for _ in 0..steps {
                    clock += dt;
                    frame += 1;
                    persist_step(legacy, persisted, frame, clock, scale, enabled);
                }
            };
            run(&mut legacy, &mut persisted, 60, 0.05, 1., &all);
            assert!(!persisted.steam.sim.puffs.is_empty());
            // A repeated age, and a scale change, reuse the births.
            run(&mut legacy, &mut persisted, 3, 0., 1., &all);
            run(&mut legacy, &mut persisted, 5, 0.05, 1.7, &all);
            // A gate that closes on some birth times, then opens: those births forget their poses.
            run(&mut legacy, &mut persisted, 5, 0.05, 1., &|name, at, default| name != "e1" && (default || at > 2.));
            run(&mut legacy, &mut persisted, 5, 0.05, 1., &all);
            // Spec edits keep the pose and change what is derived from it.
            legacy.resize(0.5); persisted.resize(0.5);
            run(&mut legacy, &mut persisted, 2, 0.05, 1., &all);
            legacy.orient_velocity("e1"); persisted.orient_velocity("e1");
            run(&mut legacy, &mut persisted, 2, 0.05, 1., &all);
            legacy.attenuate(0.5); persisted.attenuate(0.5);
            run(&mut legacy, &mut persisted, 2, 0.05, 1., &all);
            // A long jump leaves every window.
            run(&mut legacy, &mut persisted, 1, 40., 1., &all);
            run(&mut legacy, &mut persisted, 3, 0.05, 1., &all);
            // Invalid input empties the set and forgets, without moving the clock.
            persist_step(&mut legacy, &mut persisted, 1000, f32::NAN, 1., &all);
            persist_step(&mut legacy, &mut persisted, 1001, 1.2, -1., &all);
            assert!(persisted.steam.sim.puffs.is_empty());
            run(&mut legacy, &mut persisted, 2, 0.05, 1., &all);
            // A small rewind: the window still overlaps, and the map forgets its poses anyway.
            for f in 0..30 { persist_step(&mut legacy, &mut persisted, 1100 + f, 50. + f as f32 * 0.05, 1., &all); }
            persist_step(&mut legacy, &mut persisted, 1131, 51., 1., &all);
            for f in 0..10 { persist_step(&mut legacy, &mut persisted, 1132 + f, 51. + f as f32 * 0.05, 1., &all); }
            // A rewind too small to move the window: the map still forgets every pose.
            persist_step(&mut legacy, &mut persisted, 1150, 51.4495, 1., &all);
            persist_step(&mut legacy, &mut persisted, 1151, 51.5, 1., &all);
            // A far one.
            persist_step(&mut legacy, &mut persisted, 1002, 0.5, 1., &all);
            for f in 0..30 { persist_step(&mut legacy, &mut persisted, 1003 + f, 0.5 + f as f32 * 0.05, 1., &all); }
        }
    }
    #[test]
    fn persisted_sample_matches_the_map_at_the_puff_cap() {
        let (mut legacy, mut persisted) = (attached_fixture(16), attached_fixture(16));
        let all = |_: &str, _: f32, _: bool| true;
        persist_step(&mut legacy, &mut persisted, 1, 10., 1., &all);
        assert_eq!(persisted.steam.sim.puffs.len(), 4096);
        // The births past the cap forgot their poses, and the cap moves as the first emitters die.
        for frame in 2..40 { persist_step(&mut legacy, &mut persisted, frame, 10. + frame as f32 * 0.5, 1., &all); }
        persist_step(&mut legacy, &mut persisted, 50, 31., 1., &all);
        // A gate moves the cap's position in the window: births past it forget their poses, so
        // they ask again when room returns.
        let (mut legacy, mut persisted) = (attached_fixture(5), attached_fixture(5));
        for e in legacy.steam.sim.emitters.iter_mut().chain(persisted.steam.sim.emitters.iter_mut()) { e.spec.life = Range::fixed(3.); e.spec.rate = 300.; }
        for (i, e) in legacy.steam.sim.emitters.iter_mut().chain(persisted.steam.sim.emitters.iter_mut()).enumerate() { e.spec.name = format!("e{}", i % 5); }
        let closed = |name: &str, _: f32, _: bool| name != "e0";
        persist_step(&mut legacy, &mut persisted, 100, 3., 1., &closed);
        persist_step(&mut legacy, &mut persisted, 101, 3.01, 1., &all);
        assert_eq!(persisted.steam.sim.puffs.len(), 4096);
        persist_step(&mut legacy, &mut persisted, 102, 3.02, 1., &closed);
        persist_step(&mut legacy, &mut persisted, 103, 3.03, 1., &closed);
    }
    #[test]
    fn persisted_sample_holds_one_window_and_warm_frames_do_not_allocate() {
        let mut a = attached_fixture(1);
        a.steam.sim.emitters[0].spec.rate = 110.;
        a.steam.sim.emitters[0].spec.life = Range { low: 7., high: 11. };
        let pose = |_: f32, _: Option<&str>| crate::skeletal::Transform { translation: Vec3::ZERO, rotation: Quat::IDENTITY };
        crate::sim_maps::with_fx_attached_persist(true, || {
            for f in 0..200 { a.sample(f as f32 * 0.1, 1., pose, |_, _, _| true); }
            // 11 s of births at 110/s, bounded by the sampler's own 1025.
            assert!(a.persisted[0].slots.len() <= 1025);
            assert!(a.steam.sim.puffs.len() > 500);
            a.steam.sim.puffs.reserve(4096);
            a.persisted[0].slots.reserve(1100);
            let scope = crate::kernel_alloc::Scope::begin();
            for f in 200..240 { a.sample(f as f32 * 0.1, 1., pose, |_, _, _| true); }
            assert_eq!(scope.finish(), 0);
        });
    }
    #[test]
    fn puff_sweep_cadence_covers_accumulated_segment_and_pause() {
        use crate::vita_sim::{Config, PuffCollide};
        let mut sweep = PuffSweep::default();
        let config = Config { puff_sweep_every: 3, ..Config::default() };
        assert!(sweep.segment(&config, Vec3::ZERO, Vec3::X, Vec3::X).is_none());
        assert!(sweep.segment(&config, Vec3::X, Vec3::X * 2., Vec3::X).is_none());
        assert_eq!(sweep.segment(&config, Vec3::X * 2., Vec3::X * 3., Vec3::X), Some((Vec3::ZERO, Vec3::X * 3.)));
        let config = Config { puff_collide: PuffCollide::Moving, ..Config::default() };
        assert!(sweep.segment(&config, Vec3::ZERO, Vec3::ZERO, Vec3::ZERO).is_none());
        assert!(sweep.segment(&config, Vec3::ZERO, Vec3::ZERO, Vec3::X * 0.0001).is_none());
        assert_eq!(sweep.segment(&config, Vec3::ZERO, Vec3::X, Vec3::X), Some((Vec3::ZERO, Vec3::X)));
        let config = Config { puff_collide: PuffCollide::None, ..Config::default() };
        assert!(sweep.segment(&config, Vec3::ZERO, Vec3::X, Vec3::X).is_none());
        assert_eq!(sweep.start, None);
        assert_eq!(sweep.ticks, 0);
    }
    #[test]
    fn delayed_puff_sweep_catches_wall_crossed_on_skipped_tick() {
        use crate::vita_sim::Config;
        let world = World::fixture(&[(vec3(1.4,-10.,-10.),vec3(1.6,10.,10.))]);
        let mut fixture = attached_fixture(1);
        let steam = &mut fixture.steam;
        steam.sim.emitters[0].spec.rate = 0.;
        steam.sim.emitters[0].spec.constrain = None;
        steam.sim.puffs.push(Puff { sweep: PuffSweep::default(), position: Vec3::ZERO,
            constraint_center: Vec3::ZERO, velocity: Vec3::X * 20., accel: Vec3::ZERO,
            age: 0., life: 5., size: 1., roll: 0., emitter: 0 });
        let config = Config { puff_sweep_every: 3, ..Config::default() };
        steam.sim.update_clocks_config(0.05,0.05,Vec3::ZERO,&world,&config);
        steam.sim.update_clocks_config(0.05,0.05,Vec3::ZERO,&world,&config);
        assert_eq!(steam.sim.puffs[0].position, Vec3::X * 2.);
        steam.sim.update_clocks_config(0.05,0.,Vec3::ZERO,&world,&config);
        assert_eq!(steam.sim.puffs[0].sweep.ticks, 2);
        steam.sim.update_clocks_config(0.05,0.05,Vec3::ZERO,&world,&config);
        assert!(steam.sim.puffs.is_empty());
    }
    #[test]
    fn puff_cap_drops_oldest_births_preserving_rng_and_emitter_credit() {
        use crate::vita_sim::{Config, PuffCollide};
        let world = World::fixture(&[]);
        for cap in [0,1,8,128,4096] {
            let mut reference = attached_fixture(1);
            let mut capped = attached_fixture(1);
            let full = Config { puff_collide: PuffCollide::None, ..Config::default() };
            let config = Config { puff_cap: cap, ..full };
            for tick in 0..40 {
                reference.steam.sim.update_clocks_config(0.05,0.05,Vec3::ZERO,&world,&full);
                capped.steam.sim.update_clocks_config(0.05,0.05,Vec3::ZERO,&world,&config);
                assert!(capped.steam.sim.puffs.len() <= cap);
                assert_eq!(reference.steam.sim.seed, capped.steam.sim.seed);
                assert_eq!(reference.steam.sim.emitters[0].carry, capped.steam.sim.emitters[0].carry);
                let newest = reference.steam.sim.puffs.iter().rev().take(cap).collect::<Vec<_>>();
                for (a,b) in capped.steam.sim.puffs.iter().rev().zip(newest) {
                    assert_eq!(a.position,b.position,"tick {tick} cap {cap}");
                    assert_eq!(a.age,b.age);
                    assert_eq!(a.life,b.life);
                }
            }
        }
    }

    #[test]
    fn dynamic_broad_phase_preserves_puff_impacts_deaths_and_emissions() {
        let make = || {
            let mut fixture = attached_fixture(4);
            for (i, e) in fixture.steam.sim.emitters.iter_mut().enumerate() {
                e.spec.rate = 20.;
                e.spec.life = Range::fixed(1.);
                e.spec.constrain = (i == 1).then_some(Vec3::splat(3.5));
                e.spec.no_deadtime = i == 3;
                e.spec.image = if i == 2 { "bubble" } else { "smoke" }.into();
            }
            fixture.steam.sim.puffs = (0..626)
                .map(|i| {
                    let position = vec3(
                        (i % 31) as f32 * 10. - 150.,
                        (i / 31) as f32 * 4. - 40.,
                        (i % 5) as f32 * 20. + 1.,
                    );
                    Puff {
                        sweep: PuffSweep::default(),
                        position,
                        constraint_center: position,
                        velocity: if i % 7 == 0 {
                            Vec3::ZERO
                        } else {
                            -Vec3::Z * 80.
                        },
                        accel: -Vec3::Z * 10.,
                        age: if i % 5 == 0 { 0.99 } else { 0. },
                        life: 1.,
                        size: 2.,
                        roll: 0.,
                        emitter: i % 4,
                    }
                })
                .collect();
            fixture
        };
        let puff_bits = |p: &Puff| {
            p.position
                .to_array()
                .into_iter()
                .chain(p.constraint_center.to_array())
                .chain(p.velocity.to_array())
                .chain(p.accel.to_array())
                .chain([p.age, p.life, p.size, p.roll])
                .map(f32::to_bits)
                .chain([p.emitter as u32])
                .collect::<Vec<_>>()
        };
        let mut a = make();
        let mut b = make();
        let boxes = [(vec3(-500., -500., -20.), vec3(500., 500., 0.))];
        let mut optimized = World::fixture(&boxes);
        let mut reference = World::fixture(&boxes);
        reference.legacy_dynamic_sweeps = true;
        for world in [&mut optimized, &mut reference] {
            world.add_liquid(vec3(-200., -200., 0.), vec3(200., 200., 120.), 0x20, None);
        }
        let mut first_frame_deaths = 0;
        for frame in 0..120 {
            optimized.set_dynamic(
                (0..25)
                    .map(|i| {
                        let x = i as f32 * 30. - 350. + (frame as f32 * 0.2).sin() * 10.;
                        crate::collision::Collider::fixture(vec3(x, -20., 5.), vec3(x + 10., 20., 60.))
                    })
                    .collect(),
            );
            reference.copy_dynamic_from(&optimized);
            if frame == 60 {
                optimized.set_dynamic(Vec::new());
                reference.set_dynamic(Vec::new());
            }
            let world_dt = if (20..30).contains(&frame) { 0. } else { 0.05 };
            a.steam
                .update_clocks(0.05, world_dt, Vec3::ZERO, &optimized);
            b.steam.sim
                .update_clocks_sim8(0.05, world_dt, Vec3::ZERO, &reference);
            if frame == 0 {
                first_frame_deaths = 626 - a.steam.sim.puffs.len();
            }
            assert_eq!(a.steam.sim.seed, b.steam.sim.seed);
            assert_eq!(
                a.steam.sim.puffs.iter().map(&puff_bits).collect::<Vec<_>>(),
                b.steam.sim.puffs.iter().map(&puff_bits).collect::<Vec<_>>()
            );
            assert_eq!(
                a.steam
                    .sim.emitters
                    .iter()
                    .map(|e| e.carry.to_bits())
                    .collect::<Vec<_>>(),
                b.steam
                    .sim.emitters
                    .iter()
                    .map(|e| e.carry.to_bits())
                    .collect::<Vec<_>>()
            );
        }
        assert!(first_frame_deaths > 0);
    }
    #[test]
    fn constrained_pixie_moves_reverses_and_stays_inside_after_pause() {
        let mut fixture = attached_fixture(1);
        let steam = &mut fixture.steam;
        let e = &mut steam.sim.emitters[0];
        e.spec.rate = 0.1;
        e.spec.life = Range::fixed(10.);
        e.spec.min_velocity = Vec3::splat(5.);
        e.spec.velocity = [Range { low: -2., high: 2. }; 3];
        e.spec.constrain = Some(Vec3::splat(3.5));
        e.spec.offset[2] = Range::fixed(10.);
        e.scale = 0.7;
        e.carry = 1.;
        let world = World::fixture(&[(Vec3::splat(-20.), Vec3::splat(20.))]);
        assert!(world.sweep(Vec3::ZERO, Vec3::Z, Vec3::ZERO).start_solid);
        steam.update(0.05, Vec3::X * 5000., &world);
        assert!(steam.sim.puffs.is_empty());
        steam.update(0.05, Vec3::ZERO, &world);
        assert_eq!(steam.sim.puffs.len(), 1);
        assert!(steam.sim.puffs[0].velocity.cmpge(Vec3::splat(3.6)).all());
        let start = steam.sim.puffs[0].position;
        for _ in 0..10 {
            steam.update(0.05, Vec3::X * 5000., &world);
            steam.update(0.05, Vec3::ZERO, &world);
            assert_eq!(steam.sim.puffs.len(), 1, "Crossing the range boundary duplicated a pixie");
        }
        let mut reversed = false;
        for _ in 0..160 {
            steam.update(0.05, Vec3::ZERO, &world);
            let p = &steam.sim.puffs[0];
            assert!((p.position - p.constraint_center).abs().cmple(Vec3::splat(3.501)).all());
            reversed |= p.velocity.x < 0.;
        }
        assert!(reversed && steam.sim.puffs[0].position != start);
        let paused = (
            steam.sim.puffs[0].position,
            steam.sim.puffs[0].velocity,
            steam.sim.puffs[0].age,
        );
        for _ in 0..60 {
            steam.update_clocks(0.05, 0., Vec3::ZERO, &world);
        }
        assert_eq!(
            paused,
            (
                steam.sim.puffs[0].position,
                steam.sim.puffs[0].velocity,
                steam.sim.puffs[0].age
            )
        );
        steam.update(0.05, Vec3::ZERO, &world);
        assert_ne!(steam.sim.puffs[0].position, paused.0);
    }
    #[test]
    fn negative_constraint_axes_remain_free_and_disabled_emitters_stay_off() {
        let mut fixture = attached_fixture(1);
        let steam = &mut fixture.steam;
        let e = &mut steam.sim.emitters[0];
        e.spec.rate = 1.;
        e.spec.min_velocity = Vec3::splat(5.);
        e.spec.constrain = Some(vec3(0.5, 0.5, -1.));
        e.carry = 1.;
        let world = World::fixture(&[]);
        steam.update(0.05, Vec3::ZERO, &world);
        steam.sim.emitters[0].enabled = false;
        for _ in 0..40 { steam.update(0.05, Vec3::ZERO, &world); }
        assert_eq!(steam.sim.puffs.len(), 1);
        assert!(steam.sim.puffs[0].position.z > 9.);
        assert!(steam.sim.puffs[0].position.x.abs() <= 0.501);
    }
    #[test]
    fn pickup_emitters_follow_visit_visibility_and_collection() {
        let mut a = attached_fixture(2);
        a.steam.sim.emitters[0].pickup = true;
        a.steam.sim.emitters[0].id = 7;
        a.steam.sim.emitters[1].id = 8;
        let p = crate::inventory::Pickup {
            id: "fixture:7".into(), model: "p_m1".into(), origin: Vec3::ZERO,
            kind: crate::inventory::PickupKind::Will, amount: 15.,
        };
        let mut s = crate::inventory::Stats::default();
        a.steam.collected(std::slice::from_ref(&p), &s);
        assert!(a.steam.sim.emitters.iter().all(|e| e.enabled));
        a.steam.collected(&[], &s); // Reserved for another visit.
        assert!(!a.steam.sim.emitters[0].enabled);
        assert!(a.steam.sim.emitters[1].enabled);
        a.steam.sim.emitters[0].enabled = true; // Next event-world sync.
        s.collected.insert(p.id.clone());
        a.steam.collected(&[p], &s);
        assert!(!a.steam.sim.emitters[0].enabled);
        assert!(a.steam.sim.emitters[1].enabled);
    }
    pub(crate) fn attached_fixture(count: usize) -> Attached {
        Attached {
            persisted: Vec::new(),
            births: Default::default(),
            age: 0.,
            reuse_scratch: false,
            order: Vec::new(),
            live_births: Vec::new(),
            steam: Steam::new(SteamSim {
                animations: vec![],
                emitters: (0..count)
                    .map(|_| EmitterSim {
                        id: 0,
                        pickup: false,
                        origin: Vec3::ZERO,
                        rotation: Quat::IDENTITY,
                        local: crate::skeletal::Transform {
                            translation: Vec3::ZERO,
                            rotation: Quat::IDENTITY,
                        },
                        scale: 1.,
                        spec: EmissionSpec {
                            name: "smoke".into(),
                            tag: Some("tag_tip".into()),
                            rate: 240.,
                            life: Range::fixed(30.),
                            start_off: true,
                            ..EmissionSpec::default()
                        },
                        light_style: None,
                        light_color: None,

                        carry: 0.,
                        enabled: true,
                        animation_enabled: None,
                    })
                    .collect(),
                puffs: vec![],
                seed: 1,
            }, (0..count).map(|_| EmitterVisual::new(
                0, &Spec::default(), vec![], vec![], Default::default(), false,
            )).collect(), vec![]),
        }
    }
    pub(crate) fn pickup_fixture(burst: bool) -> Attached {
        let mut a = attached_fixture(if burst { 1 } else { 3 });
        for (i, (e, visual)) in a.steam.sim.emitters.iter_mut().zip(&mut a.steam.art.emitters).enumerate() {
            e.spec.name = ["idle", "go_up", "burst"][i].into();
            e.spec.rate = if i == 0 { 50. } else { 100. };
            e.spec.life = Range { low: 0.1, high: [0.25, 0.7, 0.3][i] };
            e.spec.start_off = i != 0;
            visual.spec.color = vec3(0.7, 0.2 + i as f32 * 0.1, 0.5);
            visual.spec.fade = true;
            e.spec.random_roll = true;
            e.spec.offset = [Range { low: -2., high: 3. }; 3];
            e.spec.velocity = [Range { low: -5., high: 5. }; 3];
            visual.spec.growth = 0.5;
            visual.additive = i != 1;
            if burst { e.spec.burst = Some((0.12, 30)); }
        }
        a
    }
    #[test]
    fn watch_particle_exceptions_preserve_frozen_births_and_partial_expiry() {
        let mut fixture = attached_fixture(2);
        let steam = &mut fixture.steam;
        steam.sim.emitters[0].spec.no_deadtime = false;
        steam.sim.emitters[1].spec.no_deadtime = true;
        let world = World::fixture(&[]);
        steam.update_clocks(0.05, 0.05, Vec3::ZERO, &world);
        let count = steam.sim.puffs.iter().filter(|p| p.emitter == 0).count();
        assert!(count > 0);
        steam.update_clocks(0.05, 0., Vec3::ZERO, &world);
        assert_eq!(steam.sim.puffs.iter().filter(|p| p.emitter == 0).count(), count);
        assert!(steam
            .sim.puffs
            .iter()
            .filter(|p| p.emitter == 0)
            .all(|p| p.age == 0.));
        assert!(steam.sim.puffs.iter().any(|p| p.emitter == 1 && p.age == 0.05));
        steam.update_clocks(0.05, 0.025, Vec3::ZERO, &world);
        assert_eq!(steam.sim.puffs[0].age, 0.025);
        assert!(
            parse("originemitter e\n(\nno_deadtime\nspawnrate 1\nmodel steam.spr\n)\n")[0]
                .emission.no_deadtime
        );
    }
    #[test]
    fn attached_bursts_keep_exact_birth_count_and_time_on_rebuild() {
        let mut emitter = attached_fixture(1);
        emitter.steam.sim.emitters[0].spec.burst = Some((0.5, 30));
        emitter.steam.sim.emitters[0].spec.life = Range::fixed(1.);
        let pose = |at: f32, _: Option<&str>| crate::skeletal::Transform {
            translation: Vec3::X * at * 100.,
            rotation: Quat::IDENTITY,
        };
        emitter.sample(0.49, 1., pose, |_, _, _| true);
        assert!(emitter.steam.sim.puffs.is_empty());
        emitter.sample(0.5, 1., pose, |_, _, _| true);
        assert_eq!(emitter.steam.sim.puffs.len(), 30);
        emitter.sample(0.75, 1., pose, |_, _, _| true);
        assert_eq!(emitter.steam.sim.puffs.len(), 30);
        assert!(emitter
            .steam
            .sim.puffs
            .iter()
            .all(|p| p.age == 0.25 && p.position.x == 50.));
        emitter.births.clear();
        emitter.sample(0.75, 1., pose, |_, _, _| true);
        assert_eq!(emitter.steam.sim.puffs.len(), 30);
        assert!(emitter.steam.sim.puffs.iter().all(|p| p.position.x == 50.));
        emitter.sample(1.5, 1., pose, |_, _, _| true);
        assert!(emitter.steam.sim.puffs.is_empty());
    }
    #[test]
    fn burst_clips_and_random_lifetimes_are_independent() {
        let text = "animations\n{\nattack a.ska\n{\nclient\n{\n4 tagspawn tag_tip\n(\ncount 7\nmodel smoke.spr\nlife 1 random 0.5\n)\n}\n}\ndeath d.ska\n{\nclient\n{\n87 tagspawn tag_head\n(\ncount 30\nmodel blood.spr\nlife random 1\n)\n}\n}\n}";
        let attack = burst_specs(text, Some("attack"), 0.05);
        let death = burst_specs(text, Some("death"), 0.05);
        assert_eq!(attack.len(), 1);
        assert_eq!(attack[0].emission.burst, Some((0.2, 7)));
        assert_eq!((attack[0].emission.life.low, attack[0].emission.life.high), (1., 1.5));
        assert_eq!(death.len(), 1);
        assert_eq!(death[0].emission.burst, Some((87. * 0.05, 30)));
        assert_eq!((death[0].emission.life.low, death[0].emission.life.high), (0., 1.));
        assert_eq!(death[0].emission.tag.as_deref(), Some("tag_head"));
        assert!(burst_specs(text, Some("idle"), 0.05).is_empty());
        assert_eq!(burst_specs(text, None, 0.05).len(), 2);
    }
    #[test]
    fn attached_events_movement_pause_restore_and_total_budget() {
        let events = crate::animation_events::Model::parse(
            "animations { act a.tan { client { 1 emitteron smoke 6 emitteroff smoke } } }",
        )
        .unwrap();
        let pose = |at: f32, tag: Option<&str>| {
            assert_eq!(tag, Some("tag_tip"));
            crate::skeletal::Transform {
                translation: Vec3::X * (at * 100.),
                rotation: Quat::IDENTITY,
            }
        };
        let enabled = |name: &str, at: f32, default: bool| {
            events
                .visual("act", at, 2., 0.1, false)
                .emitters
                .get(name)
                .copied()
                .unwrap_or(default)
        };
        let snapshot = |a: &Attached| {
            a.steam
                .sim.puffs
                .iter()
                .map(|p| (p.position, p.age, p.size))
                .collect::<Vec<_>>()
        };
        let mut a = attached_fixture(1);
        a.sample(0.05, 1., pose, enabled);
        assert!(a.steam.sim.puffs.is_empty());
        a.sample(0.4, 1., pose, enabled);
        let before = snapshot(&a);
        assert!(!before.is_empty());
        assert!(before.iter().all(|(p, _, _)| p.x >= 10. && p.x <= 40.));
        a.sample(0.4, 1., pose, enabled);
        assert_eq!(
            before,
            snapshot(&a),
            "pause must freeze exact birth samples"
        );
        let clock: f32 = serde_json::from_str(&serde_json::to_string(&0.4_f32).unwrap()).unwrap();
        let mut restored = attached_fixture(1);
        restored.sample(clock, 1., pose, enabled);
        assert_eq!(
            before,
            snapshot(&restored),
            "restore must reproduce the same particles"
        );
        restored.sample(
            0.5,
            1.,
            |at, tag| {
                let mut p = pose(at, tag);
                p.translation.x += 500.;
                p
            },
            enabled,
        );
        assert!(
            restored
                .steam
                .sim.puffs
                .iter()
                .filter(|p| p.age >= 0.1)
                .all(|p| p.position.x <= 40.),
            "moving parent dragged previously emitted particles"
        );
        assert!(
            restored.steam.sim.puffs.iter().any(|p| p.position.x > 540.),
            "moving parent failed to carry new births"
        );
        a.sample(0.9, 1., pose, enabled);
        assert!(
            a.steam.sim.puffs.iter().all(|p| p.position.x < 60.),
            "disabled emitter created new births"
        );
        a.sample(31., 1., pose, enabled);
        assert!(a.steam.sim.puffs.is_empty());
        let mut many = attached_fixture(16);
        many.sample(10., 1., pose, |_, _, _| true);
        assert_eq!(many.steam.sim.puffs.len(), 4096);
        if !crate::sim_maps::fx_attached_persist() { assert_eq!(many.births.len(), 4096); }
        many.sample(f32::NAN, 1., pose, |_, _, _| true);
        assert!(many.steam.sim.puffs.is_empty());
    }
    #[test]
    fn light_only_and_invalid_combined_emitters_are_bounded() {
        let p = parse("originemitter flare\n(\ndlight .5 .5 1 1 1\n)\n");
        assert_eq!(p.len(), 1);
        assert!(p[0].emission.light.unwrap().3);
        assert!(parse(
            "originemitter invalid\n(\ndlight .5 .5 1 100\nmodel fire.spr\nspawnrate 999999\n)\n"
        )
        .is_empty());
    }
    #[test]
    fn moving_attachment_only_moves_new_puffs_and_preserves_disabled_state() {
        use crate::{entity::Id, skeletal::Transform};
        let mut steam = SteamSim {
            animations: vec![],
            emitters: vec![EmitterSim {
                pickup: false,
                id: 7,
                origin: Vec3::ZERO,
                rotation: Quat::IDENTITY,
                local: Transform {
                    translation: Vec3::X * 3.,
                    rotation: Quat::IDENTITY,
                },
                scale: 1.,
                spec: EmissionSpec {
                    rate: 60.,
                    life: Range::fixed(2.),
                    forward: 10.,
                    ..EmissionSpec::default()
                },
                light_style: None,
                light_color: None,

                carry: 0.,
                enabled: true,
                animation_enabled: None,
            }],
            puffs: vec![],
            seed: 12345,
        };
        let pose = Transform {
            translation: vec3(20., 30., 40.),
            rotation: Quat::from_rotation_z(std::f32::consts::FRAC_PI_2),
        };
        steam.place(&[(Id(7), pose)]);
        let world = World::fixture(&[]);
        steam.update(1. / 60., pose.translation, &world);
        assert_eq!(steam.puffs.len(), 1);
        let first = steam.puffs[0].position;
        assert!(first.distance(vec3(20., 33., 40.)) < 0.0001);
        assert!(steam.puffs[0].velocity.distance(Vec3::Y * 10.) < 0.0001);
        steam.place(&[(
            Id(7),
            Transform {
                translation: pose.translation + Vec3::X * 100.,
                ..pose
            },
        )]);
        steam.update(0., pose.translation, &world);
        assert_eq!(steam.puffs[0].position, first);
        steam.update(1. / 60., pose.translation, &world);
        assert_eq!(steam.puffs.len(), 2);
        assert!(steam.puffs[1].position.distance(first + Vec3::X * 100.) < 0.0001);
        steam.gate(&[(Id(7), false)]);
        steam.gate(&[(Id(7), true)]);
        assert!(!steam.emitters[0].enabled);
        steam.update(0.05, pose.translation, &world);
        assert_eq!(steam.puffs.len(), 2);
        assert!(steam.puffs[0].age > 0.);
    }
    #[test]
    fn bounded_multi_emitter_definitions_preserve_ranges_and_acceleration() {
        let s="originemitter steam\n(\nspawnrate 60\nmodel steam.spr\nlife 1 random .35\nrandvel crandom 25 crandom 25 200\n)\ntagemitter tag_base spray\n(\nspawnrate 20\nmodel water.spr\naccel 0 0 -250\nlife 2\nfade\n)\nspawnrate 99999";
        let p = parse(s);
        assert_eq!(p.len(), 2);
        assert_eq!(p[0].emission.rate, 60.);
        assert_eq!(p[0].emission.velocity[2].low, 200.);
        assert_eq!(p[0].emission.velocity[0].low, -25.);
        assert_eq!(p[0].emission.life.high, 1.35);
        assert_eq!(p[1].emission.tag.as_deref(), Some("tag_base"));
        assert_eq!(p[1].emission.accel[2].high, -250.);
        assert_eq!(parse(&s.replace("spawnrate 60", "spawnrate 280"))[0].emission.rate, 280.);
        assert_eq!(parse(&s.replace("spawnrate 60", "spawnrate 1025")).len(), 1);
    }
    #[test]
    fn authored_offset_modes_do_not_turn_constants_into_random_offsets() {
        let p =
            parse("originemitter a\n(\nspawnrate 1\nmodel a.spr\noffset crandom 2 random 3 4\n)\n");
        let p = &p[0];
        assert_eq!((p.emission.offset[0].low, p.emission.offset[0].high), (-2., 2.));
        assert_eq!((p.emission.offset[1].low, p.emission.offset[1].high), (0., 3.));
        assert_eq!((p.emission.offset[2].low, p.emission.offset[2].high), (4., 4.));
    }
    #[test]
    fn model_bursts_follow_saved_clock_and_expire_without_rebirth() {
        let source = "animations\n{\nidle timer.tan\n{\nclient\n{\n2 originspawn\n(\ncount 2\nmodel models/test_ring.tik\nlife 1\nscale 1\nscalerate 2\noffset crandom 4 0 0\nfade\n)\n}\n}\n}";
        let bursts = ModelBurst::parse(source, 0.1);
        assert_eq!(bursts.len(), 1);
        let b = &bursts[0];
        assert!(b.sample(0.19, 123).is_empty());
        let p = b.sample(0.45, 123);
        assert_eq!(p.len(), 2);
        assert_eq!(p[0].model, "test_ring");
        assert!((p[0].scale - 1.5).abs() < 0.0001);
        assert!((p[0].alpha - 0.75).abs() < 0.0001);
        assert_eq!(p[0].offset, b.sample(0.45, 123)[0].offset);
        assert!(b.sample(1.21, 123).is_empty());
    }
    /// F1: an attached set with an animated stage takes the raw path only with the lever on, and
    /// nothing compared which image each quad got. A frame pinned to the first image draws every
    /// puff with image 0, whatever its age.
    #[cfg(feature="vita_renderer")]
    #[test]
    fn raw_attached_set_uses_each_puffs_animated_stage_frame() {
        let mut a = attached_fixture(1);
        let first = crate::render_fx::test_support::texture(91);
        let second = crate::render_fx::test_support::texture(92);
        {
            let e = &mut a.steam.art.emitters[0];
            e.textures = vec![first.clone(), second.clone()];
            e.stage.images = vec!["first".into(), "second".into()];
            e.stage.fps = 7.;
            let emission = &mut a.steam.sim.emitters[0].spec;
            emission.rate = 30.;
            emission.life = Range { low: 0.7, high: 1.1 };
            emission.start_off = false;
        }
        let pose = |_: f32, _: Option<&str>| crate::skeletal::Transform {
            translation: Vec3::ZERO, rotation: Quat::IDENTITY };
        crate::sim_maps::with_fx_attached_persist(true, || {
            a.sample(2., 1., pose, |_, _, _| true);
        });
        let camera = vec3(17., -23., 8.);
        let order: Vec<_> = a.steam.sim.puffs.iter().enumerate()
            .map(|(i, p)| (i, p.position.distance_squared(camera))).collect();
        let frames: std::collections::BTreeSet<_> = order.iter()
            .map(|&(i, _)| a.steam.art.emitters[a.steam.sim.puffs[i].emitter].stage.frame(a.steam.sim.puffs[i].age))
            .collect();
        assert_eq!(frames.into_iter().collect::<Vec<_>>(), vec![0, 1],
            "the fixture must hold puffs on both stage frames");
        crate::render_fx::test_support::begin_direct(camera, Vec3::Z, 0., crate::render_fx::Appearance::default());
        crate::sim_maps::with_fx_attached_persist(true, || {
            a.steam.prepare_quads(camera, None);
            assert!(a.steam.live_draw().register_raw(&order), "the raw path refused the set");
        });
        let registered = crate::render_fx::test_support::snapshot().puffs;
        assert_eq!(registered.len(), order.len());
        for (slot, &(i, _)) in registered.iter().zip(&order) {
            let p = &a.steam.sim.puffs[i];
            let e = &a.steam.art.emitters[p.emitter];
            let frame = e.stage.frame(p.age);
            assert_eq!(slot.2.as_ref(), e.textures.get(frame),
                "puff {i} age {} asked for stage frame {frame}", p.age);
            assert_eq!(slot.2.as_ref(), a.steam.live_draw().puff_mesh(i, camera, (p.position - camera).normalize_or_zero()).texture.as_ref(),
                "puff {i} age {} differs from the one-quad path", p.age);
        }
        crate::render_fx::test_support::end_frame();
    }
    /// F2: once one emitter fills the 4096 cap the sampler forgets every later emitter's births,
    /// as the map does. A slot kept across the cap replays a pose from an older frame when space
    /// frees up, and a moving emitter asks for that pose again.
    #[test]
    fn a_later_emitter_relearns_poses_the_cap_hid() {
        let (mut legacy, mut persisted) = (attached_fixture(5), attached_fixture(5));
        for a in [&mut legacy, &mut persisted] {
            a.reuse_scratch = false; // the old path's non-reuse mode forgets births it never reached
            // Four capped emitters each hold 1,025 slots, so they reach 4,096 together. One
            // emitter alone cannot: its window stops at 1,025 births.
            for e in &mut a.steam.sim.emitters[..4] {
                e.spec.name = "cap".into();
                e.spec.rate = 5000.;
                e.spec.life = Range::fixed(10.);
                e.spec.start_off = false;
            }
            a.steam.sim.emitters[4].spec.name = "later".into();
            a.steam.sim.emitters[4].spec.rate = 2.;
            a.steam.sim.emitters[4].spec.life = Range::fixed(20.);
            a.steam.sim.emitters[4].spec.start_off = false;
        }
        for (frame, age) in [(1u32, 0.1f32), (2, 1.0), (3, 1.05)] {
            // The capped emitters fill 4,096 at 1.0 s, and the gate closes them at 1.05 s.
            let enabled = move |name: &str, _: f32, _: bool| name != "cap" || frame < 3;
            let pose = move |at: f32, _: Option<&str>| crate::skeletal::Transform {
                translation: vec3(at * 3., frame as f32, 2.),
                rotation: Quat::from_rotation_z(frame as f32 * 0.1 + at),
            };
            crate::sim_maps::with_fx_attached_persist(false, || legacy.sample(age, 1., pose, enabled));
            crate::sim_maps::with_fx_attached_persist(true, || persisted.sample(age, 1., pose, enabled));
            assert_eq!(puff_bits(&legacy), puff_bits(&persisted), "frame {frame} age {age}");
        }
        assert!(persisted.steam.sim.puffs.iter().any(|p| p.emitter == 4),
            "the later emitter never produced a puff");
    }
    /// F2: the random scripts the review found this with. Emitters, gates, caps, rewinds, jumps,
    /// spec edits, both scratch modes of the old path, and every input the sampler sees.
    #[test]
    fn persisted_sample_fuzz_matches_the_map_through_random_scripts() {
        let (mut puffs_seen, mut caps, mut rewinds, mut steps, mut pose_calls) = (0u64, 0u64, 0u64, 0u64, 0u64);
        for case in 0..600u64 {
            let mut r = FuzzRng(case.wrapping_mul(0x9e3779b97f4a7c15) ^ 0x5eed);
            let seed = r.0;
            let mut legacy = fuzz_set(&mut r);
            let mut r2 = FuzzRng(seed);
            let mut persisted = fuzz_set(&mut r2);
            r = r2;
            legacy.reuse_scratch = r.next() % 2 == 0;
            persisted.reuse_scratch = r.next() % 2 == 0;
            let mut clock = 0f32;
            let mut gate_mode = 0u32;
            for frame in 0..90u32 {
                steps += 1;
                let (mut age, mut scale) = (clock, 1f32);
                match r.next() % 40 {
                    0 => age = f32::NAN, 1 => age = f32::INFINITY, 2 => age = -1.,
                    3 => scale = -1., 4 => scale = f32::NAN, 5 => scale = 0.,
                    6 => { clock = (clock - r.pick(&[1e-4, 0.001, 0.05, 0.5, 3.])).max(0.); age = clock; rewinds += 1; }
                    7 => { clock = 0.; age = 0.; rewinds += 1; }
                    8 => { clock += r.pick(&[11., 40., 1e4, 1e7]); age = clock; }
                    9 => scale = r.f(0.1, 3.),
                    10 => { legacy.resize(0.5); persisted.resize(0.5); }
                    11 => { legacy.attenuate(0.5); persisted.attenuate(0.5); }
                    12 => { legacy.orient_velocity("e1"); persisted.orient_velocity("e1"); }
                    13 => { legacy.use_sprite_dimensions(); persisted.use_sprite_dimensions(); }
                    14 | 15 | 16 => gate_mode = r.next() % 6,
                    _ => { clock += r.pick(&[0., 1. / 120., 0.0167, 0.05, 0.065, 0.25, 2.]); age = clock; }
                }
                let mode = gate_mode;
                let enabled_calls = [std::cell::RefCell::new(Vec::new()), std::cell::RefCell::new(Vec::new())];
                let pose_log = [std::cell::RefCell::new(Vec::new()), std::cell::RefCell::new(Vec::new())];
                let enabled = |arm: usize| { let log = &enabled_calls[arm]; move |name: &str, at: f32, default: bool| {
                    log.borrow_mut().push((name.to_owned(), at.to_bits(), default));
                    match mode { 0 => true, 1 => default, 2 => name != "e1", 3 => default && at.to_bits() % 3 != 0,
                        4 => (at.to_bits() ^ frame) % 5 != 0, _ => frame % 2 == 0 || name == "e0" }
                }};
                let pose = |arm: usize| { let log = &pose_log[arm]; move |at: f32, tag: Option<&str>| {
                    log.borrow_mut().push((at.to_bits(), tag.map(str::to_owned)));
                    let f = frame as f32;
                    crate::skeletal::Transform { translation: vec3(at * 3. + f, -f * 0.5, 2.), rotation: Quat::from_rotation_z(f * 0.1 + at) }
                }};
                crate::sim_maps::with_fx_attached_persist(false, || legacy.sample(age, scale, pose(0), enabled(0)));
                crate::sim_maps::with_fx_attached_persist(true, || persisted.sample(age, scale, pose(1), enabled(1)));
                assert_eq!(puff_bits(&legacy), puff_bits(&persisted), "case {case} frame {frame} age {age} scale {scale}");
                assert_eq!(*pose_log[0].borrow(), *pose_log[1].borrow(), "pose calls case {case} frame {frame}");
                assert_eq!(*enabled_calls[0].borrow(), *enabled_calls[1].borrow(), "gate calls case {case} frame {frame}");
                for e in &persisted.persisted { assert!(e.slots.len() <= 1025); }
                puffs_seen += persisted.steam.sim.puffs.len() as u64;
                caps += u64::from(persisted.steam.sim.puffs.len() == 4096);
                pose_calls += pose_log[1].borrow().len() as u64;
            }
        }
        println!("PERSIST fuzz: steps {steps} puffs {puffs_seen} frames at the cap {caps} rewinds {rewinds} pose calls {pose_calls}");
        assert!(caps > 50 && rewinds > 100 && pose_calls > 1000);
    }
    struct FuzzRng(u64);
    impl FuzzRng {
        fn next(&mut self) -> u32 { self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407); (self.0 >> 33) as u32 }
        fn pick<T: Copy>(&mut self, v: &[T]) -> T { v[self.next() as usize % v.len()] }
        fn f(&mut self, low: f32, high: f32) -> f32 { low + (high - low) * ((self.next() % 65536) as f32 / 65536.) }
    }
    fn fuzz_set(r: &mut FuzzRng) -> Attached {
        let n = 1 + r.next() as usize % 6;
        let mut a = attached_fixture(n);
        for (i, e) in a.steam.sim.emitters.iter_mut().enumerate() {
            let range = |r: &mut FuzzRng, a: f32, b: f32| { let (x, y) = (r.f(a, b), r.f(a, b)); Range { low: x, high: y } };
            e.spec.name = format!("e{}", r.next() % 4);
            e.spec.tag = if r.next() % 3 == 0 { None } else { Some(format!("tag{i}")) };
            e.spec.rate = r.pick(&[0.5, 7., 40., 110., 300., 1000., 5000., 40000.]);
            e.spec.life = match r.next() % 4 { 0 => Range::fixed(r.f(0.05, 12.)), 1 => range(r, 0.01, 2.), 2 => Range { low: 7., high: 11. }, _ => range(r, 0.5, 30.) };
            e.spec.size = range(r, 0.1, 2.);
            e.spec.offset = [range(r, -5., 5.), range(r, -5., 5.), range(r, -5., 5.)];
            e.spec.axis_offset = [range(r, -5., 5.), range(r, -5., 5.), range(r, -5., 5.)];
            e.spec.velocity = [range(r, -5., 5.), range(r, -5., 5.), range(r, -5., 5.)];
            e.spec.accel = [range(r, -5., 5.), range(r, -5., 5.), range(r, -5., 5.)];
            e.spec.min_velocity = vec3(r.f(-1., 1.), r.f(-1., 1.), r.f(-1., 1.));
            e.spec.forward = r.f(-3., 3.);
            e.spec.tag_velocity = r.next() % 2 == 0;
            e.spec.random_roll = r.next() % 2 == 0;
            e.spec.start_off = r.next() % 3 == 0;
            e.spec.burst = match r.next() % 5 { 0 => Some((r.f(0., 3.), r.pick(&[0, 1, 40, 1024, 1025, 1026, 3000]))), _ => None };
        }
        a
    }
    /// F7: retained windows are bounded by the sampler's own 1,025 slots, and `VecDeque` owns the
    /// next power of two. One slot is 88 bytes with align 4 on a scalar `Vec3` build.
    #[test]
    fn retained_windows_stay_bounded_after_a_full_bubble_set() {
        assert_eq!(std::mem::size_of::<PersistedBirth>(), 88);
        assert_eq!(std::mem::align_of::<PersistedBirth>(), 4);
        let pose = |_: f32, _: Option<&str>| crate::skeletal::Transform { translation: Vec3::ZERO, rotation: Quat::IDENTITY };
        let mut a = attached_fixture(1);
        a.steam.sim.emitters[0].spec.rate = 110.;
        a.steam.sim.emitters[0].spec.life = Range { low: 7., high: 11. };
        let mut steps = Vec::new();
        let mut capacity = 0;
        crate::sim_maps::with_fx_attached_persist(true, || {
            for f in 0..480 {
                a.sample(f as f32 * 0.05, 1., pose, |_, _, _| true);
                let now = a.persisted[0].slots.capacity();
                if now != capacity { steps.push((f, now)); capacity = now; }
            }
        });
        let e = &a.persisted[0];
        println!("F7 window: slot {} B align {}, len {}, capacity {}, bytes {}",
            std::mem::size_of::<PersistedBirth>(), std::mem::align_of::<PersistedBirth>(),
            e.slots.len(), e.slots.capacity(), e.slots.capacity() * std::mem::size_of::<PersistedBirth>());
        println!("F7 capacity steps {steps:?}");
        assert_eq!(e.slots.len(), 1025);
        assert_eq!(e.slots.capacity(), 2048);
        assert_eq!(e.slots.capacity() * std::mem::size_of::<PersistedBirth>(), 180_224);
        // A gated emitter that never draws keeps its window until the instance drops.
        let mut g = attached_fixture(1);
        g.steam.sim.emitters[0].spec.rate = 110.;
        g.steam.sim.emitters[0].spec.life = Range::fixed(10.);
        crate::sim_maps::with_fx_attached_persist(true, || {
            for f in 0..480 { g.sample(f as f32 * 0.05, 1., pose, |_, _, _| false); }
        });
        println!("F7 gated window: puffs {}, len {}, capacity {}", g.steam.sim.puffs.len(), g.persisted[0].slots.len(), g.persisted[0].slots.capacity());
        assert_eq!(g.steam.sim.puffs.len(), 0);
        assert_eq!(g.persisted[0].slots.len(), 1025);
        assert_eq!(g.persisted[0].slots.capacity(), 2048);
        // A burst long dead holds only the burst's own births.
        let mut b = attached_fixture(1);
        b.steam.sim.emitters[0].spec.burst = Some((0., 1000));
        b.steam.sim.emitters[0].spec.life = Range::fixed(1.);
        crate::sim_maps::with_fx_attached_persist(true, || {
            for f in 0..480 { b.sample(f as f32 * 0.05, 1., pose, |_, _, _| true); }
        });
        println!("F7 dead burst: puffs {}, len {}, capacity {}", b.steam.sim.puffs.len(), b.persisted[0].slots.len(), b.persisted[0].slots.capacity());
        assert_eq!(b.steam.sim.puffs.len(), 0);
        assert_eq!(b.persisted[0].slots.len(), 1000);
        assert_eq!(b.persisted[0].slots.capacity(), 1024);
    }
}

#[cfg(test)]
mod snapshot_render_tests {
    use super::*;
    #[test]
    fn steam_parts_round_trip_keeps_live_and_published_puffs() {
        let mut steam = super::tests::world_test_steam();
        steam.update(0.05, Vec3::ZERO, &World::fixture(&[]));
        let visit = crate::render_snapshot::RenderVisit::default().id();
        assert!(!steam.extract_render_input(visit).1);
        let signature = |draw: SteamDraw<'_>| {
            draw.puffs.iter().map(|p| (p.emitter, p.position, p.age, p.life, p.size, p.roll))
                .collect::<Vec<_>>()
        };
        let live = signature(steam.live_draw());
        let published = signature(steam.draw_input(visit).unwrap());
        assert!(!live.is_empty());
        let runtime = steam.runtime();
        let packet = steam.art.input.as_ref().unwrap().puffs.as_ptr();
        let (sim, art) = steam.into_parts();
        let steam = Steam::from_parts(sim, art);
        assert_eq!(signature(steam.live_draw()), live);
        assert_eq!(signature(steam.draw_input(visit).unwrap()), published);
        assert_eq!(steam.art.input.as_ref().unwrap().puffs.as_ptr(), packet);
        assert_eq!(steam.runtime().seed, runtime.seed);
        assert_eq!(steam.runtime().emitters.len(), runtime.emitters.len());
        for (a, b) in steam.runtime().emitters.into_iter().zip(runtime.emitters) {
            assert_eq!((a.0, a.1, a.2.translation, a.2.rotation, a.3, a.4, a.5, a.6),
                (b.0, b.1, b.2.translation, b.2.rotation, b.3, b.4, b.5, b.6));
        }
    }
    #[test]
    fn captured_world_puffs_survive_live_mutation_and_retire_on_restore() {
        let mut steam=super::tests::world_test_steam();
        steam.sim.puffs.push(Puff{sweep:Default::default(),position:Vec3::X,constraint_center:Vec3::ZERO,velocity:Vec3::Y,accel:Vec3::ZERO,age:0.2,life:1.,size:2.,roll:0.3,emitter:0});
        let visit = crate::render_snapshot::RenderVisit::default().id();
        assert!(!steam.extract_render_input(visit).1);
        steam.sim.puffs[0].position = Vec3::Z;
        assert_eq!(steam.draw_input(visit).unwrap().puffs.at(0).position, Vec3::X);
        assert!(steam.draw_input(visit + 1).is_none());
        let runtime = steam.runtime();
        steam.restore_runtime(runtime);
        assert!(steam.draw_input(visit).is_none());
        assert_eq!(steam.live_draw().puffs.at(0).position, Vec3::Z);
        steam.sim.puffs[0].emitter = steam.sim.emitters.len();
        assert!(steam.extract_render_input(visit).1);
        assert!(steam.draw_input(visit).is_none());
    }
}
