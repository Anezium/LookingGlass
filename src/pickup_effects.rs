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

/// Real authored 75-sprite fixture through the retained publication/merge/upload path.
#[cfg(all(feature="vita_renderer", not(target_os="vita")))]
pub(crate) async fn check(assets: &mut Assets) -> Result<()> {
    use crate::render_fx::retained;
    let specs = texture::read_materials(assets)?;
    let atmosphere = Atmosphere::default();
    let eye = vec3(0., -180., 50.);
    let camera = Camera3D { position: eye, target: vec3(0.,0.,12.), up: Vec3::Z,
        fovy: 70_f32.to_radians(), z_near: 1., z_far: 1000., ..Default::default() };
    let mut gpu = retained::Gpu::new()?;
    std::fs::create_dir_all("private/pickup-fixture")?;
    let mut report = Vec::new();
    for model in ["p_h1","p_h2","p_m1","p_m2","w_me_small","w_me_medium","w_me_large","w_me_super"] {
        let effect = Effect::load(assets, model, "pickup", &specs)?;
        let mut samples = Vec::new();
        let mut finish_samples = Vec::new();
        let mut counters = serde_json::Value::Null;
        let iterations = if std::env::var_os("LOOKING_GLASS_PICKUP_TIMING").is_some() && model == "p_h1" { 360 } else { 12 };
        for frame in 0..iterations {
            clear_background(BLACK);
            crate::render::vita_renderer::prepare_3d(BLACK);
            crate::render::vita_renderer::set_3d_camera(&camera);
            let age = if model == "p_h1" { 0.15 + (frame % 60) as f32 / 30. } else { 0.15 };
            retained::begin_view(&camera, age, &atmosphere, false, true);
            let started = std::time::Instant::now();
            effect.draw(Some("fixture"), age, Vec3::ZERO, 0., eye, &atmosphere);
            let producer_us = started.elapsed().as_secs_f64() * 1e6;
            let started = std::time::Instant::now();
            let (_, dropped) = retained::finish(&mut gpu, &[], true, |_| unreachable!());
            let finish_us = started.elapsed().as_secs_f64() * 1e6;
            crate::render::vita_renderer::present_3d();
            anyhow::ensure!(dropped == 0, "dropped fixture quads: {model}");
            if frame >= 10 { samples.push(producer_us); finish_samples.push(finish_us); }
            counters = retained::host_report();
            if frame == 10 {
                let count: usize = effect.instances.borrow().values().flat_map(|i| &i.particles).map(|p| p.live_puffs()).sum();
                if model.starts_with("p_") { anyhow::ensure!(count == 75, "expected 75 authored sprites, {model}: {count}"); }
                let image = get_screen_data();
                anyhow::ensure!(image.bytes.chunks_exact(4).any(|p| p[..3] != [0,0,0]), "empty pickup: {model}");
                image.export_png(&format!("private/pickup-fixture/{model}.png"));
            }
            set_default_camera(); next_frame().await;
        }
        let stats = |mut v: Vec<f64>| { v.sort_by(f64::total_cmp); serde_json::json!({"n":v.len(),"median_us":v[v.len()/2],"p95_us":v[v.len()*95/100],"max_us":v[v.len()-1]}) };
        report.push(serde_json::json!({"model":model,"age_min":0.15,"age_max":if model == "p_h1" { 0.15 + 59./30. } else { 0.15 },"producer":stats(samples),"fx_finish_cpu":stats(finish_samples),"last_frame_counters":counters}));
    }
    std::fs::write("private/pickup-fixture/report.json", serde_json::to_vec_pretty(&report)?)?;
    println!("PASS real pickup retained fixture, 75-sprite shards, images and CPU samples");
    Ok(())
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
