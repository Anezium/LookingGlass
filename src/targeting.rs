//! Cosmetic world-space aim marker using the supplied target emitter.
use crate::{assets::Assets, combat, environment::Atmosphere, particles::Attached};
use anyhow::{ensure, Context, Result};
use macroquad::prelude::*;
use std::sync::Arc;

#[derive(Clone, Copy)]
#[cfg_attr(test, derive(Debug, PartialEq))]
pub(crate) struct Rule {
    kind: u8,
    range: f32,
}
const _: fn() = || {
    fn send<T: Send>() {}
    send::<Vec<Rule>>();
};

impl Rule {
    fn read(text: &str) -> Result<Self> {
        let mut rule = Self {
            kind: 1,
            range: 1000.,
        };
        for line in crate::materials::lines(text) {
            match line[0].as_str() {
                "reticle" => rule.kind = line.get(1).context("Missing reticle kind")?.parse()?,
                "maxrange" => rule.range = line.get(1).context("Missing aim range")?.parse()?,
                _ => {}
            }
        }
        ensure!(
            rule.kind <= 4 && rule.range.is_finite() && (1. ..=30000.).contains(&rule.range),
            "Invalid aim marker rule"
        );
        Ok(rule)
    }
}

fn endpoint(ctx: &combat::Context<'_>, eye: Vec3, direction: Vec3, rule: Rule) -> Option<Vec3> {
    if rule.kind == 0 || !eye.is_finite() {
        return None;
    }
    let direction = direction.try_normalize()?;
    let end = eye + direction * rule.range;
    // Start at the player's sight origin so a nearby wall cannot be skipped by
    // an authored minimum range. The view camera may be behind or beside Alice.
    let wall = ctx.world.sweep(eye, end, Vec3::ZERO);
    if wall.start_solid || wall.all_solid {
        return None;
    }
    let fraction = combat::contact(ctx, eye, end, 0.).map_or(wall.fraction, |(_, f)| f);
    // Native endpoint markers float in front of the contact rather than z-fight
    // with its surface. A ray without contact still has a bounded marker.
    Some(eye.lerp(end, fraction * 0.9))
}

pub struct Sight {
    pub eye: Vec3,
    pub direction: Vec3,
    /// None suppresses the marker for menus, performances and unavailable weapons.
    pub weapon: Option<usize>,
}
#[cfg_attr(test, derive(Debug, PartialEq))]
pub(crate) struct Input { generation:u64, scene_visit:u64, point:Option<(usize,Rule,Vec3)> }

impl Input {
    pub(crate) fn capture(
        rules: &[Rule],
        ctx: &combat::Context<'_>,
        sight: Sight,
        generation: u64,
        scene_visit: u64,
    ) -> Self {
        let point = sight.weapon.and_then(|weapon| {
            let rule = *rules.get(weapon)?;
            Some((weapon, rule, endpoint(ctx, sight.eye, sight.direction, rule)?))
        });
        Self { generation, scene_visit, point }
    }
}
pub struct Pointer {
    visit:crate::render_snapshot::RenderVisit,
    effect: Attached,
    rules: Arc<[Rule]>,
    age: f32,
    previous: Option<(usize, Vec3)>,
}
impl Pointer {
    pub fn load(assets: &mut Assets) -> Result<Self> {
        let mut rules = Vec::new();
        for (name, _) in crate::inventory::WEAPONS {
            rules.push(Rule::read(&String::from_utf8_lossy(
                &assets.read(&format!("models/w_{name}.tik"))?,
            ))?);
        }
        let materials = crate::texture::read_materials(assets)?;
        let mut effect = Attached::load(assets, "fx_emitter_target", &materials)?
            .context("Missing aim marker emitters")?;
        // Overlapping additive births otherwise turn this sight into a large,
        // saturated flare. Keep the local artwork as a subdued aiming point.
        effect.attenuate(0.1);
        Ok(Self {
            visit:Default::default(),
            effect,
            rules: rules.into(),
            age: 0.,
            previous: None,
        })
    }
    pub fn draw(
        &mut self,
        ctx: &combat::Context<'_>,
        sight: Sight,
        dt: f32,
        camera: Vec3,
        atmosphere: &Atmosphere,
    ) {
        let input=self.resolve(ctx,sight,self.visit.id());
        self.draw_input(&input,self.visit.id(),dt,camera,atmosphere);
    }
    /// Copy the CPU inputs once while the pointer is on the owner.
    pub(crate) fn capture_rules(&self) -> (u64, Arc<[Rule]>) {
        (self.visit.id(), Arc::clone(&self.rules))
    }

    pub(crate) fn resolve(&self,ctx:&combat::Context<'_>,sight:Sight,scene_visit:u64)->Input {
        Input::capture(&self.rules, ctx, sight, self.visit.id(), scene_visit)
    }
    pub(crate) fn valid_input(&self,input:&Input,scene_visit:u64)->bool {
        input.generation==self.visit.id() && input.scene_visit==scene_visit && input.point.as_ref().is_none_or(|(slot,_,_)|*slot<self.rules.len())
    }
    pub(crate) fn draw_input(&mut self,input:&Input,scene_visit:u64,dt:f32,camera:Vec3,atmosphere:&Atmosphere) {
        assert!(self.valid_input(input,scene_visit));
        let point=input.point;
        let Some((weapon, rule, position)) = point else {
            self.previous = None;
            self.age = 0.;
            return;
        };
        // Do not leave a trail at the previous map, after a teleport, or when
        // swapping to another marker type. Ordinary small aim motion keeps it.
        if self
            .previous
            .is_none_or(|(w, p)| w != weapon || p.distance(position) > 256.)
        {
            self.effect = self.effect.fork();
            self.age = 0.;
        }
        self.previous = Some((weapon, position));
        self.age += dt.clamp(0., 0.1);
        let color = if rule.kind == 4 { "red" } else { "blue" };
        self.effect.draw(
            self.age,
            0.2,
            |_, _| crate::skeletal::Transform {
                translation: position,
                rotation: Quat::IDENTITY,
            },
            |name, _, _| name == color,
            camera,
            atmosphere,
        );
    }
}

pub async fn render_check(assets: &mut Assets) -> Result<()> {
    use crate::{collision::World, environment::Atmosphere};
    std::fs::create_dir_all("private/aim-pointer-captures")?;
    let world = World::fixture(&[(vec3(300., -300., -100.), vec3(310., 300., 300.))]);
    let ctx = combat::Context {
        world: &world,
        targets: &[],
    };
    let camera = Camera3D {
        position: vec3(-120., -100., 80.),
        target: vec3(270., 0., 48.),
        up: Vec3::Z,
        fovy: 60_f32.to_radians(),
        z_near: 1.,
        z_far: 10000.,
        ..Default::default()
    };
    let atmosphere = Atmosphere::default();
    let mut pointer = Pointer::load(assets)?;
    for (name, weapon, blocked, visible) in [
        ("blue", Some(0), false, true),
        ("red", Some(8), false, true),
        ("disabled-weapon", Some(6), false, false),
        ("hidden-context", None, false, false),
        ("occluded", Some(0), true, false),
    ] {
        let mut images = Vec::new();
        for draw_pointer in [false, true] {
            gl_use_default_material();
            clear_background(BLACK);
            crate::render::clear_view_depth();
            set_camera(&camera);
            crate::render_fx::begin_view(&camera, 1., &atmosphere, false);
            draw_cube(vec3(305., 0., 100.), vec3(10., 600., 400.), None, GRAY);
            if blocked {
                draw_cube(vec3(210., 0., 80.), vec3(10., 500., 400.), None, GRAY);
            }
            if draw_pointer {
                pointer.draw(
                    &ctx,
                    Sight {
                        eye: vec3(0., 0., 48.),
                        direction: Vec3::X,
                        weapon,
                    },
                    0.1,
                    camera.position,
                    &atmosphere,
                );
            }
            crate::render_fx::finish();
            images.push(crate::character::visibility_image());
            if draw_pointer {
                crate::viewer::save_capture(std::path::Path::new(&format!(
                    "private/aim-pointer-captures/{name}.png"
                )))?;
            }
            set_default_camera();
            next_frame().await;
        }
        let pixels = crate::character::visible_pixels(&images[0], &images[1]);
        ensure!(
            if visible { pixels > 20 } else { pixels == 0 },
            "Aim marker {name}: unexpected {pixels} pixels"
        );
        println!("PASS aim marker {name}: {pixels} pixels");
    }
    Ok(())
}

#[cfg(all(test, target_os = "linux"))]
pub(crate) fn test_pure_capture(assets: &mut Assets) {
    let pointer = Pointer::load(assets).unwrap();
    let (generation, rules) = pointer.capture_rules();
    assert!(Arc::ptr_eq(&rules, &pointer.capture_rules().1));
    let world = crate::collision::World::fixture(&[(vec3(100., -20., -20.), vec3(110., 20., 20.))]);
    let ctx = combat::Context { world: &world, targets: &[] };
    let visit = crate::render_snapshot::RenderVisit::default().id();
    for (eye, direction) in [(Vec3::ZERO, Vec3::X), (Vec3::ZERO, Vec3::ZERO), (Vec3::X * 105., Vec3::X)] {
        for weapon in (0..=rules.len()).map(Some).chain([None]) {
            let sight = || Sight { eye, direction, weapon };
            let captured = Input::capture(&rules, &ctx, sight(), generation, visit);
            assert_eq!(captured, pointer.resolve(&ctx, sight(), visit));
            assert!(pointer.valid_input(&captured, visit));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collision::World;
    #[test]
    fn marker_stops_at_the_first_world_or_actor_contact() {
        let world = World::fixture(&[(vec3(100., -100., -100.), vec3(110., 100., 100.))]);
        let mut ctx = combat::Context {
            world: &world,
            targets: &[],
        };
        let rule = Rule {
            kind: 1,
            range: 1000.,
        };
        let wall = endpoint(&ctx, Vec3::ZERO, Vec3::X, rule).unwrap();
        assert!((wall.x - 90.).abs() < 0.1);
        let actors = [combat::Target {
            id: 1,
            center: vec3(60., 0., 0.),
            half: Vec3::splat(10.),
        }];
        ctx.targets = &actors;
        assert!((endpoint(&ctx, Vec3::ZERO, Vec3::X, rule).unwrap().x - 45.).abs() < 0.1);
        let hidden = [combat::Target {
            center: vec3(200., 0., 0.),
            ..actors[0]
        }];
        ctx.targets = &hidden;
        assert_eq!(endpoint(&ctx, Vec3::ZERO, Vec3::X, rule), Some(wall));
        assert!(endpoint(&ctx, vec3(105., 0., 0.), Vec3::X, rule).is_none());
        assert!(endpoint(&ctx, Vec3::ZERO, Vec3::ZERO, rule).is_none());
        assert!(endpoint(&ctx, Vec3::ZERO, Vec3::X, Rule { kind: 0, ..rule }).is_none());
        assert_eq!(
            endpoint(&ctx, Vec3::ZERO, Vec3::Y, rule),
            Some(Vec3::Y * 900.)
        );
    }
    #[test]
    fn reads_primary_weapon_rules_without_using_alternate_or_comments() {
        let rule = Rule::read(
            "// reticle 0\nreticle 2\nalternate reticle 3\nmaxrange 3000\nalternate maxrange 6000",
        )
        .unwrap();
        assert_eq!(rule.kind, 2);
        assert_eq!(rule.range, 3000.);
        assert_eq!(Rule::read("").unwrap().kind, 1);
        assert!(Rule::read("maxrange NaN").is_err());
    }
}
