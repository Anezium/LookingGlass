//! Owned numeric world preparation. This module never reads renderer TLS or calls GL.
//! Both switch arms execute this kernel with the same scratch and arithmetic order.
use super::{NumericRecipe, Parameter, Renderer, Scene, Lists, shader, model_matrix,
    parameter_view_key, uniforms, config, count};
use macroquad::prelude::*;
use std::sync::Arc;

pub(crate) type Job = crate::character::skinning::overlap::CpuJob<State>;
pub(super) struct Group { recipe: Arc<NumericRecipe>, parameter: usize, min: Vec3, max: Vec3, center: Vec3 }
pub(super) struct Prepared { pub depth: f32, pub group: usize, pub texture_frame: usize,
    pub uniforms: Option<shader::Uniforms> }
struct Input {
    projection: Mat4, eye: Vec3, depth: [u32; 6], retained: bool, time: f32,
    fullbright: bool, alpha: bool, atlas: Vec2, atmosphere: crate::environment::Atmosphere,
    transforms: Vec<(usize, Vec3, Quat)>, lists: Arc<Lists>, reuse: bool,
    parameter_reuse: bool, split: bool, world_alpha: bool, front_to_back: bool, sort_alpha: bool, fast_frustum: bool,
}
#[derive(Default)]
struct Metrics { tests: u64, hits: u64, builds: u64, time_updates: u64, visibility_ns: u64, parameter_ns: u64 }
pub(crate) struct State {
    groups: Vec<Group>, pub(super) parameters: Vec<Parameter>, input: Input,
    parameter_key: Option<[u32; 51]>, parameter_generation: u64,
    visibility_key: Option<[u32; 26]>, visibility_generation: u64,
    visibility: Vec<(u64, Option<(f32, Mat4)>)>, models: Vec<Option<Mat4>>,
    order: Vec<(f32, usize)>, pub(super) prepared: Vec<Prepared>, metrics: Metrics,
}
impl State {
    pub(super) fn new(groups: &[super::Group], parameters: Vec<Parameter>) -> Self {
        fn cpu_only<T: Send + 'static>() {} cpu_only::<Self>();
        let n = groups.len();
        Self {
            groups: groups.iter().map(|g| Group { recipe: g.recipe.numeric.clone(), parameter: g.parameter,
                min: g.min, max: g.max, center: g.center }).collect(), parameters,
            input: Input { projection: Mat4::IDENTITY, eye: Vec3::ZERO, depth: [0;6], retained: false,
                time: 0., fullbright: false, alpha: false, atlas: Vec2::ONE, atmosphere: Default::default(),
                transforms: Vec::new(), lists: Arc::new(Lists::default()), reuse: false,
                parameter_reuse: false, split: false, world_alpha: true, front_to_back: false, sort_alpha: false, fast_frustum: false },
            parameter_key: None, parameter_generation: 0, visibility_key: None, visibility_generation: 0,
            visibility: vec![(0,None); n], models: vec![None;n], order: Vec::with_capacity(n),
            prepared: Vec::with_capacity(n), metrics: Default::default(),
        }
    }
    /// Game-thread-only snapshot. No managed handle is copied into the state.
    pub(super) fn snapshot(&mut self, renderer: &Renderer, scene: &Scene, eye: Vec3, time: f32,
        fullbright: bool, alpha: bool, transforms: &[(usize, Vec3, Quat)]) {
        crate::game_thread::assert_current();
        let i = &mut self.input;
        i.projection = crate::render_fx::projection().expect("world camera projection");
        i.depth = crate::render_fx::depth_view_bits();
        i.retained = crate::render_fx::retained_active();
        let cluster = scene.map.visibility.cluster(eye, &scene.map.planes);
        i.lists = renderer.pvs.clusters.get(cluster.unwrap_or(usize::MAX))
            .unwrap_or(&renderer.pvs.unknown).clone();
        i.eye = eye; i.time = time; i.fullbright = fullbright; i.alpha = alpha; i.atlas = scene.atlas_grid;
        i.atmosphere.distance = scene.atmosphere.distance; i.atmosphere.liquid = scene.atmosphere.liquid;
        // Only the first two authored volumes contribute to the shader payload/key.
        i.atmosphere.volumes.clear(); i.atmosphere.volumes.extend(scene.atmosphere.volumes.iter().take(2).copied());
        i.transforms.clear(); i.transforms.extend_from_slice(transforms);
        let c = config();
        i.reuse = c.world_view_reuse; i.parameter_reuse = c.world_parameter_reuse;
        i.split = c.world_parameter_time_split; i.world_alpha = c.world_alpha;
        i.front_to_back = c.opaque_order == "front_to_back"; i.sort_alpha = c.sort_alpha; i.fast_frustum = c.fast_frustum;
    }
    pub(super) fn run(&mut self) {
        let start = std::time::Instant::now();
        self.metrics = Metrics::default();
        let i = &self.input;
        let ids = if i.alpha { &i.lists.alpha } else { &i.lists.opaque };
        self.order.clear();
        let mut key = [0;26];
        key[..16].copy_from_slice(&i.projection.to_cols_array().map(f32::to_bits));
        key[16..19].copy_from_slice(&i.eye.to_array().map(f32::to_bits));
        key[19..25].copy_from_slice(&i.depth); key[25] = u32::from(i.retained);
        if self.visibility_key != Some(key) {
            self.visibility_key = Some(key); self.visibility_generation = self.visibility_generation.wrapping_add(1);
            if self.visibility_generation == 0 { self.visibility.fill((0,None)); self.visibility_generation = 1; }
        }
        let camera = Vec3::new(f32::from_bits(i.depth[0]), f32::from_bits(i.depth[1]), f32::from_bits(i.depth[2]));
        let direction = Vec3::new(f32::from_bits(i.depth[3]), f32::from_bits(i.depth[4]), f32::from_bits(i.depth[5]));
        for &id in ids {
            let g = &self.groups[id];
            let static_group = g.recipe.model.is_none() && g.recipe.motion.is_none();
            let result = if i.reuse && static_group && self.visibility[id].0 == self.visibility_generation {
                self.metrics.hits += 1; self.visibility[id].1
            } else {
                let result = model_matrix(&g.recipe, i.time, &i.transforms).and_then(|model| {
                    self.metrics.tests += 1;
                    let outside = if i.fast_frustum { crate::render::aabb_outside_matrix(i.projection * model,g.min,g.max) }
                        else { crate::render::corner_outside_matrix(i.projection * model,g.min,g.max) };
                    (!outside).then(|| {
                        let depth = if i.retained { (model.transform_point3(g.center) - camera).dot(direction) }
                            else { (model.transform_point3(g.center) - i.eye).length_squared() };
                        (depth,model)
                    })
                });
                if i.reuse && static_group { self.visibility[id] = (self.visibility_generation,result); }
                result
            };
            self.models[id] = result.map(|(_,model)| model);
            if let Some((depth,_)) = result { self.order.push((depth,id)); }
        }
        if i.alpha && !i.world_alpha { self.order.clear(); }
        if !i.alpha && i.front_to_back { self.order.sort_unstable_by(|a,b| a.0.total_cmp(&b.0).then_with(|| a.1.cmp(&b.1))); }
        if i.alpha && i.sort_alpha && !i.retained { self.order.sort_unstable_by(|a,b| b.0.total_cmp(&a.0).then_with(|| a.1.cmp(&b.1))); }
        self.metrics.visibility_ns = start.elapsed().as_nanos() as u64;
        let start = std::time::Instant::now();
        self.prepared.clear();
        let key = parameter_view_key(i.projection, i.eye, i.fullbright, i.atlas, if i.split {0.} else {i.time}, &i.atmosphere);
        if self.parameter_key != Some(key) {
            self.parameter_key = Some(key); self.parameter_generation = self.parameter_generation.wrapping_add(1);
            if self.parameter_generation == 0 { for p in &mut self.parameters {p.generation = 0;} self.parameter_generation = 1; }
        }
        for &(depth,id) in &self.order {
            let g = &self.groups[id];
            let model = if i.reuse { self.models[id].unwrap() } else { model_matrix(&g.recipe,i.time,&i.transforms).unwrap() };
            let u = if i.parameter_reuse {
                let p = &mut self.parameters[g.parameter];
                let model_bits = model.to_cols_array().map(f32::to_bits);
                if p.generation != self.parameter_generation || p.model != model_bits {
                    p.uniforms = uniforms(&p.recipe,i.time,i.eye,model,i.projection,i.fullbright,i.atlas);
                    p.uniforms.atmosphere(&i.atmosphere,i.eye); p.bits = p.uniforms.bits();
                    p.generation = self.parameter_generation; p.model = model_bits; p.time = i.time.to_bits(); self.metrics.builds += 1;
                } else if i.split && p.time != i.time.to_bits() { p.update_time(i.time); self.metrics.time_updates += 1; }
                None
            } else {
                let mut u = uniforms(&g.recipe,i.time,i.eye,model,i.projection,i.fullbright,i.atlas);
                u.atmosphere(&i.atmosphere,i.eye); self.metrics.builds += 1; Some(u)
            };
            self.prepared.push(Prepared {depth,group:id,texture_frame:g.recipe.stage.frame(i.time),uniforms:u});
        }
        self.metrics.parameter_ns = start.elapsed().as_nanos() as u64;
    }
    /// Credit main-thread intervals at their actual owning scope, including fallback.
    pub(super) fn credit_inline(&self) {
        use crate::light_profile::{self as lp, Probe};
        lp::probe_child(Probe::WorldVisibility,self.metrics.visibility_ns);
        lp::probe_child(Probe::WorldParameters,self.metrics.parameter_ns);
    }
    pub(super) fn publish(&self, phase: crate::light_profile::RenderPhase, opaque: bool) {
        use crate::light_profile::{self as lp, RenderMetrics};
        let m = &self.metrics;
        lp::render_add(phase,RenderMetrics {visibility_tests:m.tests, visibility_cpu_ns:m.visibility_ns,
            parameter_cpu_ns:m.parameter_ns,..Default::default()});
        // Worker service is a parallel axis, never subtracted from a main-thread scope.
        count("vita_world_visibility_cache_hits",m.hits);
        count("vita_world_parameter_builds",m.builds);
        count("vita_world_parameter_time_updates",m.time_updates);
        count("vita_world_parameter_reuses",self.prepared.len() as u64-m.builds);
        #[cfg(not(target_os="vita"))]
        if std::env::var_os("LOOKING_GLASS_WORLD_HASHES").is_some() {
            use sha2::{Digest,Sha256};
            let mut visibility = Sha256::new();
            let mut ids = self.prepared.iter().map(|p| p.group).collect::<Vec<_>>();
            ids.sort_unstable();
            for id in ids { visibility.update((id as u64).to_le_bytes()); }
            let mut h = Sha256::new();
            for p in &self.prepared {
                h.update((p.group as u64).to_le_bytes()); h.update(p.depth.to_bits().to_le_bytes());
                h.update((p.texture_frame as u64).to_le_bytes());
                let g = &self.groups[p.group];
                let bits = p.uniforms.as_ref().unwrap_or(&self.parameters[g.parameter].uniforms).bits();
                for v in bits { h.update(v.to_le_bytes()); }
            }
            println!("WORLD_PREPARE_HASH {}",serde_json::json!({"alpha":!opaque,"groups":self.prepared.len(),
                "tests":m.tests,"hits":m.hits,"visibility_sha256":format!("{:x}",visibility.finalize()),"sha256":format!("{:x}",h.finalize())}));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn switch_is_strict_and_default_off() {
        let c: super::super::Config = serde_json::from_str("{}").unwrap();
        assert!(!c.world_prepare_worker);
        assert!(serde_json::from_str::<super::super::Config>(r#"{"world_prepare_worker":true}"#).unwrap().world_prepare_worker);
        assert!(serde_json::from_str::<super::super::Config>(r#"{"world_prepare_worker":"on"}"#).is_err());
    }
}
