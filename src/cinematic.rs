//! Declarative camera paths from local assets. No script commands are executed.
use crate::{assets::Assets, interaction::vector};
use anyhow::{ensure, Context, Result};
use macroquad::prelude::*;

/// Resolve only a small vertical support adjustment at the authored final pose.
/// Never use the pre-scene location or search sideways through walls.
pub fn land_player(
    player: &mut crate::movement::Player,
    world: &crate::collision::World,
    at: crate::skeletal::Transform,
) -> Result<()> {
    let feet = at.translation;
    ensure!(
        world.body_clear(feet),
        "Cinematic endpoint obstructed: {feet:?}"
    );
    let trace = world.body_trace(feet, feet - Vec3::Z * 8.);
    player.feet = if !trace.start_solid && trace.fraction < 1. && trace.normal.z > 0.65 {
        player.grounded = true;
        player.ground_normal = trace.normal;
        feet - Vec3::Z * (8. * trace.fraction)
    } else {
        player.grounded = false;
        player.ground_normal = Vec3::Z;
        feet
    };
    player.velocity = Vec3::ZERO;
    player.script_motion = 0;
    player.script_facing = at.rotation.to_euler(EulerRot::ZYX).0;
    player.cancel_climb();
    player.release_rope();
    player.immersion = crate::water::Immersion::sample(world, player.feet);
    player.swimming = player.immersion.level >= 2;
    Ok(())
}

/// Last rendered local pose, retained only by Alice puppets for gameplay blending.
#[derive(Clone)]
pub struct ActorPose {
    pub local: Vec<crate::skeletal::Transform>,
    pub transform: crate::skeletal::Transform,
    pub scale: f32,
}

#[derive(Clone, Copy)]
pub struct Camera {
    pub eye: Vec3,
    pub target: Vec3,
    pub up: Vec3,
}
impl Camera {
    pub fn look(eye: Vec3, target: Vec3) -> Self {
        Self {
            eye,
            target,
            up: Vec3::Z,
        }
    }
}
struct Node {
    position: Vec3,
    rotation: Quat,
    time: f32,
    speed: f32,
}
pub struct Track(Vec<Node>);
impl Track {
    /// Validated control points, for scene owners opting into spline playback.
    pub fn controls(&self) -> impl Iterator<Item = (Vec3, Quat, f32)> + '_ {
        self.0.iter().map(|n| (n.position, n.rotation, n.speed))
    }
    pub fn load(assets: &mut Assets, name: &str) -> Result<Self> {
        Self::parse(&String::from_utf8(
            assets.read(&format!("cams/{name}.cam"))?,
        )?)
    }
    fn parse(text: &str) -> Result<Self> {
        ensure!(text.len() < 100_000, "Camera path too large");
        let mut nodes: Vec<Node> = Vec::new();
        let mut time = 0.;
        let mut expected = None;
        for row in crate::materials::lines(text.trim_end_matches('\0')) {
            if row[0] == "end" {
                break;
            }
            ensure!(
                row.len() >= 8 && row[0] == "spawn" && row[1] == "SplinePath" && row.len() % 2 == 0,
                "Unsupported camera declaration"
            );
            let fields = row[2..]
                .chunks_exact(2)
                .map(|v| (v[0].as_str(), v[1].as_str()))
                .collect::<std::collections::BTreeMap<_, _>>();
            let name = *fields.get("targetname").context("Unnamed camera node")?;
            ensure!(
                expected.is_none_or(|n| n == name),
                "Broken camera path links"
            );
            let position = fields
                .get("origin")
                .and_then(|s| vector(s))
                .context("Invalid camera origin")?;
            let angles = fields
                .get("angles")
                .and_then(|s| vector(s))
                .context("Invalid camera angles")?;
            let speed: f32 = fields
                .get("speed")
                .context("Missing camera speed")?
                .parse()?;
            ensure!(
                position.is_finite()
                    && position.abs().max_element() < 100_000.
                    && angles.is_finite()
                    && (0.01..=100.).contains(&speed)
                    && nodes.len() < 256,
                "Invalid camera node"
            );
            // Original Euler pitch is positive down; shortest quaternion interpolation
            // avoids a full revolution when an authored angle crosses 360 degrees.
            let rotation = Quat::from_rotation_z(angles.y.to_radians())
                * Quat::from_rotation_y(angles.x.to_radians())
                * Quat::from_rotation_x(angles.z.to_radians());
            nodes.push(Node {
                position,
                rotation,
                time,
                speed,
            });
            time += 1. / speed;
            expected = fields.get("target").map(|s| s.to_string());
        }
        ensure!(
            !nodes.is_empty() && expected.is_none(),
            "Incomplete camera path"
        );
        Ok(Self(nodes))
    }
    pub fn sample(&self, time: f32) -> Camera {
        if self.0.len() == 1 {
            let n = &self.0[0];
            return Camera {
                eye: n.position,
                target: n.position + n.rotation * Vec3::X * 100.,
                up: n.rotation * Vec3::Z,
            };
        }
        let end = self.0.len() - 1;
        let i = self
            .0
            .iter()
            .position(|n| n.time >= time.max(0.))
            .unwrap_or(end)
            .max(1);
        let a = &self.0[i - 1];
        let b = &self.0[i];
        let f = ((time - a.time) / (b.time - a.time)).clamp(0., 1.);
        let eye = a.position.lerp(b.position, f);
        let q = a.rotation.slerp(b.rotation, f);
        Camera {
            eye,
            target: eye + q * Vec3::X * 100.,
            up: q * Vec3::Z,
        }
    }
}

/// One transaction covers setup, skip acceptance and the final owner handoff.
pub struct FilmTransaction {
    id: u64,
    film: String,
    owner: &'static str,
    outcome: &'static str,
}
impl FilmTransaction {
    fn new(film: &str, owner: &'static str, binding: &str) -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        let t = Self { id: NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            film: film.into(), owner, outcome: "aborted-or-error" };
        t.event(&format!("begin binding={binding:?}"));
        t
    }
    fn event(&self, event: &str) {
        eprintln!("[lg/film-transaction] tx={} film={} owner={} {event}", self.id, self.film, self.owner);
    }
}
impl Drop for FilmTransaction {
    fn drop(&mut self) { self.event(&format!("terminal end={}", self.outcome)); }
}
/// A fresh press is timed with a monotonic clock, independently of simulation dt.
#[derive(Default)]
pub struct Skip {
    scene: Option<&'static str>,
    armed: bool,
    held: f32,
    since: Option<std::time::Instant>,
    down: bool,
    binding: String,
    film: bool,
    transaction: Option<FilmTransaction>,
    input_sample: Option<(bool, bool, bool)>,
}
impl Skip {
    pub fn film(name: &str, binding: String) -> Self {
        Self { transaction: Some(FilmTransaction::new(name, "roq", &binding)),
            binding, film: true, ..Self::default() }
    }
    pub fn binding(&mut self, binding: String) { self.binding = binding; }
    pub fn event(&self, event: &str) {
        if let Some(t) = &self.transaction { t.event(event); }
    }
    pub fn note_input(&mut self, down: bool, cross: bool, triangle: bool) {
        let sample = (down, cross, triangle);
        if self.input_sample != Some(sample) {
            self.event(&format!("input effective_down={down} raw_cross_enter={cross} raw_triangle_e={triangle}"));
            self.input_sample = Some(sample);
        }
    }
    pub fn handoff(&mut self, outcome: &'static str) {
        if let Some(t) = &mut self.transaction {
            t.event(&format!("handoff outcome={outcome}"));
            t.outcome = outcome;
        }
    }
    pub fn update(&mut self, scene: Option<&'static str>, active: bool, down: bool) -> bool {
        self.update_at(scene, active, down, std::time::Instant::now())
    }
    fn update_at(&mut self, scene: Option<&'static str>, active: bool, down: bool,
        now: std::time::Instant) -> bool {
        if scene != self.scene {
            if !self.film {
                if let Some(t) = &mut self.transaction {
                    if t.outcome == "aborted-or-error" { t.outcome = "complete"; }
                }
                self.transaction = scene.map(|name| FilmTransaction::new(name, "engine", &self.binding));
            }
            self.scene = scene;
            self.since = None;
            self.held = 0.;
            self.armed = !down && active;
            self.down = down;
            self.event(&format!("state active={active} down={down} armed={} held_s=0", self.armed));
            return false;
        }
        let was_armed = self.armed;
        if !active || scene.is_none() {
            self.since = None;
            self.held = 0.;
            self.armed = false;
        } else if !down {
            self.armed = true;
            self.since = None;
            self.held = 0.;
        } else if self.armed {
            let since = *self.since.get_or_insert(now);
            self.held = now.saturating_duration_since(since).as_secs_f32();
        }
        if self.down != down || was_armed != self.armed {
            self.event(&format!("state active={active} down={down} armed={} held_s={:.3}", self.armed, self.held));
        }
        self.down = down;
        if self.held >= 0.65 {
            self.event(&format!("accepted held_s={:.3} threshold_s=0.650 binding={:?}", self.held, self.binding));
            self.since = None;
            self.held = 0.;
            self.armed = false;
            return true;
        }
        false
    }
    pub fn draw(&self, ui: &crate::ui::Ui, key: &str) {
        if self.scene.is_none() {
            return;
        }
        let label = format!("Hold {key} to skip scene");
        let w = ui.font.width(&label, 18.);
        let x = screen_width() - w - 34.;
        ui.dialog(Rect::new(x - 18., 18., w + 36., 36.));
        ui.label(&label, x, 41., 18., WHITE);
        draw_rectangle(x, 48., w * (self.held / 0.65).min(1.), 2., WHITE);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn handoff_lands_at_final_feet_without_searching_through_walls() {
        use crate::{collision::World, movement::Player, skeletal::Transform};
        let world = World::fixture(&[
            (vec3(-100., -100., -20.), vec3(100., 100., 0.)),
            (vec3(40., -100., 0.), vec3(60., 100., 100.)),
        ]);
        let mut p = Player::new(vec3(-50., 0., 0.));
        p.velocity = Vec3::splat(100.);
        let at = Transform {
            translation: vec3(0., 0., 2.),
            rotation: Quat::from_rotation_z(1.2),
        };
        land_player(&mut p, &world, at).unwrap();
        assert!(p.feet.truncate().length() < 0.001 && p.feet.z.abs() < 0.1);
        assert!(p.grounded && p.velocity == Vec3::ZERO);
        assert!((p.script_facing - 1.2).abs() < 0.001);
        let before = p.feet;
        assert!(land_player(
            &mut p,
            &world,
            Transform {
                translation: vec3(50., 0., 0.),
                ..at
            }
        )
        .is_err());
        assert_eq!(p.feet, before);
    }
    #[test]
    fn single_node_authored_camera_is_a_valid_static_shot() {
        let t = Track::parse(
            "spawn SplinePath targetname still origin \"1 2 3\" angles \"0 90 0\" speed 1\nend\0",
        )
        .unwrap();
        assert_eq!(t.sample(0.).eye, t.sample(500.).eye);
        assert_eq!(t.sample(0.).eye, vec3(1., 2., 3.));
        assert!(
            (t.sample(0.).target - t.sample(0.).eye)
                .normalize()
                .dot(Vec3::Y)
                > 0.999
        );
    }
    #[test]
    fn camera_angles_wrap_and_endpoints_hold() {
        let t = Track::parse("spawn SplinePath targetname a target b origin \"0 0 0\" angles \"0 350 0\" speed 1\nspawn SplinePath targetname b origin \"10 0 0\" angles \"0 10 0\" speed 1\nend\0").unwrap();
        let c = t.sample(0.5);
        assert_eq!(c.eye, vec3(5., 0., 0.));
        assert!((c.target - c.eye).normalize().dot(Vec3::X) > 0.999);
        assert_eq!(t.sample(50.).eye, vec3(10., 0., 0.));
        assert!(Track::parse("exec something").is_err());
    }
    #[test]
    fn skip_uses_wall_time_at_one_five_and_ten_fps_and_requires_release() {
        use std::time::{Duration, Instant};
        for fps in [1., 5., 10.] {
            let mut s = Skip::default();
            let mut now = Instant::now();
            assert!(!s.update_at(Some("a"), true, true, now));
            now += Duration::from_secs(10);
            assert!(!s.update_at(Some("a"), true, true, now));
            assert!(!s.update_at(Some("a"), true, false, now));
            assert!(!s.update_at(Some("a"), true, true, now));
            let started = now;
            loop {
                now += Duration::from_secs_f64(1. / fps);
                if s.update_at(Some("a"), true, true, now) { break; }
                assert!(now.duration_since(started).as_secs_f64() < 2.);
            }
            let held = now.duration_since(started).as_secs_f64();
            assert!(held >= 0.65 && held <= 0.65 + 1. / fps);
            assert!(!s.update_at(Some("a"), true, true, now + Duration::from_secs(5)));
            assert!(!s.update_at(Some("a"), true, false, now));
            assert!(!s.update_at(Some("a"), true, true, now));
            assert!(!s.update_at(Some("a"), false, true, now + Duration::from_secs(1)));
            assert!(!s.update_at(Some("a"), true, true, now + Duration::from_secs(2)));
            assert!(!s.update_at(Some("b"), true, true, now + Duration::from_secs(3)));
        }
    }
}
