//! Bounded input-only viewer driver. Never edits simulation or exports route ticks.
use macroquad::prelude::*;
use super::input::Frame;

pub struct Pilot {
    seconds: f32,
    fight: f32,
    still: f32,
    anchor: Vec3,
    previous: Frame,
    deaths: u32,
    was_alive: bool,
    waypoint: usize,
    last_sanity: f32,
}
impl Default for Pilot {
    fn default() -> Self {
        Self { seconds: 0., fight: 0., still: 0., anchor: Vec3::ZERO,
            previous: Frame::default(), deaths: 0, was_alive: true, waypoint: 0,
            last_sanity: 100. }
    }
}
impl Pilot {
    pub fn open(recording: bool) -> anyhow::Result<Option<Self>> {
        #[cfg(feature = "vita_renderer")]
        let enabled = crate::render::vita_renderer::config().replay_fight_pilot;
        #[cfg(not(feature = "vita_renderer"))]
        let enabled = false;
        if !enabled || !recording { return Ok(None); }
        println!("[lg/pilot] enabled input-only; time_limit=180 retries=2");
        Ok(Some(Self::default()))
    }

    pub fn frame(&mut self, map: &str, player: &crate::movement::Player,
        yaw: f32, pitch: f32, alive: bool, sanity: f32, busy: bool,
        targets: &[crate::combat::Target], world: &crate::collision::World,
        ready: bool, selected: usize, preferences: &crate::preferences::Preferences) -> Frame
    {
        let dt = macroquad::time::get_frame_time();
        let elapsed = dt.clamp(0., 0.1);
        self.seconds += elapsed;
        if self.was_alive && !alive { self.deaths += 1; self.waypoint = 0; }
        self.was_alive = alive;
        if sanity < self.last_sanity {
            println!("[lg/pilot] damage_taken={} seconds={:.3}", self.last_sanity-sanity, self.seconds);
        }
        self.last_sanity = sanity;
        let mut frame = Frame { dt, pad: Some([128; 4]), ..Default::default() };
        let mut press = |key| super::input::hold_key(&mut frame, key);
        if player.feet.distance(self.anchor) > 24. {
            self.anchor = player.feet;
            self.still = 0.;
        } else { self.still += elapsed; }
        let supported = map == "hatter2";
        let stop = !supported || self.seconds > 180. || self.deaths > 2
            || (self.still > 18. && !busy && self.fight == 0.) || self.fight > 45.;
        if stop {
            println!("[lg/pilot] stop map={map} seconds={:.3} fight={:.3} deaths={} feet={:?}",
                self.seconds, self.fight, self.deaths, player.feet);
            press(KeyCode::Escape); // Ordinary Start/menu stop flushes the native tape.
        } else if !alive {
            if (self.seconds * 2.) as u32 % 2 == 0 { press(KeyCode::E); }
        } else if self.seconds < 1. {
            // Neutral unlocks both stick latches after load; click resumes a loaded chapter.
            if self.seconds > 0.4 && self.seconds < 0.6 { press(KeyCode::LeftControl); }
        } else if !busy {
            // D-pad weapon cycle, translated by the backend to Right. Chapter loadout
            // starts with the knife; Cards are the first ranged weapon on this map.
            if selected != 1 && (self.seconds * 4.) as u32 % 2 == 0 {
                press(KeyCode::Right);
            }
            let target = targets.iter().filter(|t| t.center.distance(player.eye()) < 3000.)
                .filter(|t| {
                    let trace = world.sight_sweep(player.eye(), t.center, Vec3::ZERO);
                    !trace.start_solid && trace.fraction > 0.98
                }).min_by(|a,b| a.center.distance_squared(player.eye())
                    .total_cmp(&b.center.distance_squared(player.eye())));
            let goal = if let Some(t) = target {
                if self.fight == 0. { println!("[lg/pilot] fight_start seconds={:.3} id={} feet={:?}", self.seconds, t.id, player.feet); }
                self.fight += elapsed;
                t.center
            } else { vec3(5664., 160., player.eye().z) };
            let d = goal - player.eye();
            let turn = (d.y.atan2(d.x) - yaw + std::f32::consts::PI)
                .rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI;
            let elevation = if target.is_some() { d.z.atan2(d.truncate().length()) } else { 0. };
            // Proportional bounded sticks. The real deadzone/curve/sensitivity still apply.
            let axis = |error: f32, rate: f32| {
                if error.abs() < 0.015 { 0. } else {
                    let magnitude = (error.abs() / (rate * elapsed.max(0.005))).min(1.).powf(1./1.7);
                    error.signum() * (preferences.pad_deadzone + (1.-preferences.pad_deadzone)*magnitude)
                }
            };
            let rates = crate::look::stick::rates(preferences.pad_sensitivity) * (std::f32::consts::PI / 180.);
            let rx = -axis(turn, rates.x);
            let ry = -axis(elevation-pitch, rates.y) * if preferences.invert_pad { -1. } else { 1. };
            let distance = d.truncate().length();
            let forward = if target.is_some() { if distance > 420. { 0.75 } else { 0. } }
                else if distance > 55. && turn.abs() < 0.65 { 1. } else { 0. };
            let strafe = if target.is_some() && distance < 800. {
                if (self.fight / 3.) as u32 % 2 == 0 { 0.55 } else { -0.55 }
            } else { 0. };
            let byte = |x: f32| (128. + x.clamp(-1.,1.)*127.).round() as u8;
            frame.pad = Some([byte(strafe), byte(-forward), byte(rx), byte(ry)]);
            if target.is_some() && ready && selected == 1 && turn.abs() < 0.15 && (elevation-pitch).abs() < 0.15 {
                super::input::hold_key(&mut frame, KeyCode::LeftControl);
            }
            if self.still > 3. && (self.seconds*2.) as u32 % 3 == 0 {
                super::input::hold_key(&mut frame, KeyCode::Enter);
            }
        }
        frame.keys_pressed = frame.keys_down & !self.previous.keys_down;
        frame.keys_released = self.previous.keys_down & !frame.keys_down;
        self.previous = frame.clone();
        frame
    }
}
