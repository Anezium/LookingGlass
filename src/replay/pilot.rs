//! Bounded input-only viewer driver. Never edits simulation or exports route ticks.
use super::input::Frame;
use macroquad::prelude::*;
use serde_json::Value;

/// Read-only snapshots gathered at 1 Hz by the viewer before `Session::begin_step`.
/// All of them are existing save-state views; nothing here writes simulation state.
pub struct Sample {
    /// The registered level controller whose id equals the map.
    pub level: Option<Value>,
    /// Legacy school/duchess owners that do not register a controller.
    pub legacy: Option<Value>,
    /// Placed NPC actors (spiders, snarks, clockworks) for legacy NPC-owned enemies.
    pub npcs: Option<Value>,
}

#[derive(Clone, Copy)]
struct Plan {
    /// Short approach borrowed from the production route, never exported route controls.
    goals: &'static [Vec3],
    /// Visible-target combat cap.
    fight_limit: f32,
    /// Farthest considered visible target.
    aim: f32,
    /// Vertical band around the eye; keeps multi-storey fights on one floor.
    vertical: f32,
    /// Holding a ranged weapon this close still approaches.
    reach: f32,
    /// Large single target: wider firing range, continuous strafe.
    boss: bool,
    /// May stop after an observed fight has both dealt and taken damage.
    clear: bool,
    /// Reach the last approach point before acquiring targets; avoids dormant balcony
    /// guards that are visible but not yet reachable.
    after_goals: bool,
    time_limit: f32,
}

const NONE: &[Vec3] = &[];
const HATTER2: &[Vec3] = &[vec3(5664., 160., 0.)];

/// Centipede1's production Flora approach (src/levels/centipede1/check.rs::drive).
const FLORA: &[Vec3] = &[
    vec3(1320., -1976., -496.),
    vec3(857., -1653., -491.),
    vec3(1136., -1200., -500.),
    vec3(1136., -640., -560.),
    vec3(576., -256., -600.),
    vec3(-476., 84., -608.),
    vec3(-468., 1024., -560.),
    vec3(1244., 1588., -576.),
    vec3(640., 2816., -560.),
    vec3(578., 3294., -428.),
    vec3(-256., 4100., -680.),
    vec3(-280., 4820., -704.),
];

/// School first visit: the first-monster trigger sits just ahead of the spawn and
/// wakes group t188; the library balcony guards follow (src/school_route.rs).
const SCHOOL: &[Vec3] = &[
    vec3(-1888., 2290., -512.),
    vec3(-2340., 2368., -480.),
    vec3(-2256., 2544., -480.),
    vec3(-2304., 2624., -342.),
    vec3(-2304., 2800., -256.),
    vec3(-2304., 3248., -256.),
    vec3(-1664., 3248., -256.),
];

/// Grounds1: the intro hands Alice to `gdeath` facing the Jabberwock arena.
const GROUNDS_ARENA: &[Vec3] = &[vec3(-700., 288., 32.)];

/// Grounds2: production pathnodes 267/273/275 to the first Spawner01/02 trigger.
const ROYALE: &[Vec3] = &[
    vec3(-2072., -32., -672.),
    vec3(-2184., 720., -648.),
    vec3(-2056., 336., -648.),
    vec3(-2432., 880., -488.),
];

/// Jabberwock lair: the intro hands Alice to `alice_pos1`; the boss lands ahead.
const LAIR: &[Vec3] = &[vec3(-688., 600., 96.)];

/// Duchess: src/duchess_check.rs approach to the Jackbomb pickup that starts her.
const DUCHESS: &[Vec3] = &[
    vec3(-832., -1000., 16.),
    vec3(-536., -520., 16.),
    vec3(-280., -264., 16.),
    vec3(-114., 66., 16.),
];

/// Queen's lair: the corridor contact that starts Queen1 (src/levels/qlair/check.rs).
const QUEEN: &[Vec3] = &[
    vec3(0., -792., 48.),
    vec3(0., -520., 0.),
    vec3(0., -96., 64.),
    vec3(0., 184., 64.),
    vec3(16., 400., 64.),
];

/// Clockwork: src/levels/hatter1/route_steps.json through the first spider rooms.
const CLOCKWORK: &[Vec3] = &[
    vec3(-2720., 3296., 320.),
    vec3(-3296., 2960., 192.),
    vec3(-3296., 2750., 192.),
    vec3(-3312., 2696., 192.),
    vec3(-3552., 2696., 192.),
    vec3(-3904., 2750., 208.),
    vec3(-4512., 2625., 208.),
];

/// Fortress of Doors first visit: src/fortress_route.rs through the portal to the Boojums.
const FORTRESS: &[Vec3] = &[
    vec3(-3500., 2140., 0.),
    vec3(-3600., 2430., 0.),
    vec3(-3500., 2550., 48.),
    vec3(-3450., 2600., 48.),
    vec3(-3380., 2700., 0.),
    vec3(-3230., 2730., 0.),
    vec3(-3230., 3050., 0.),
    vec3(-360., -2410., -192.),
    vec3(-400., -2240., -192.),
    vec3(-448., -1810., 32.),
];

/// Temple: the underwater guide path starts at the turtle shell.
const TEMPLE: &[Vec3] = &[vec3(-88., -5980., 1330.)];

/// Castling: the first stretch of the production course toward the red court.
const CASTLING: &[Vec3] = &[
    vec3(160., -4896., 0.),
    vec3(-128., -4896., 0.),
    vec3(-320., -4896., -64.),
    vec3(-384., -4352., -64.),
    vec3(-832., -4160., -64.),
    vec3(-1024., -3904., -64.),
    vec3(-1024., -3456., 0.),
    vec3(-1024., -2368., 0.),
    vec3(-608., -1632., 192.),
];

fn plan(map: &str) -> Option<Plan> {
    let p = match map {
        "hatter2" => Plan {
            after_goals: false,
            goals: HATTER2,
            fight_limit: 45.,
            aim: 3000.,
            vertical: 1e9,
            reach: 420.,
            boss: true,
            clear: false,
            time_limit: 180.,
        },
        "centipede1" => Plan {
            after_goals: false,
            goals: FLORA,
            fight_limit: 60.,
            aim: 1200.,
            vertical: 320.,
            reach: 280.,
            boss: false,
            clear: true,
            time_limit: 180.,
        },
        "skool1" => Plan {
            after_goals: true,
            goals: SCHOOL,
            fight_limit: 75.,
            aim: 1200.,
            vertical: 320.,
            reach: 300.,
            boss: false,
            clear: false,
            time_limit: 180.,
        },
        "grounds1" => Plan {
            after_goals: false,
            goals: GROUNDS_ARENA,
            fight_limit: 60.,
            aim: 2000.,
            vertical: 600.,
            reach: 420.,
            boss: true,
            clear: false,
            time_limit: 180.,
        },
        "grounds2" => Plan {
            after_goals: true,
            goals: ROYALE,
            fight_limit: 75.,
            aim: 1200.,
            vertical: 320.,
            reach: 300.,
            boss: false,
            clear: false,
            time_limit: 180.,
        },
        "jlair2" => Plan {
            after_goals: false,
            goals: LAIR,
            fight_limit: 60.,
            aim: 2000.,
            vertical: 400.,
            reach: 420.,
            boss: true,
            clear: false,
            time_limit: 180.,
        },
        "potears3" => Plan {
            after_goals: true,
            goals: DUCHESS,
            fight_limit: 60.,
            aim: 1600.,
            vertical: 400.,
            reach: 420.,
            boss: true,
            clear: false,
            time_limit: 180.,
        },
        "qlair" => Plan {
            after_goals: true,
            goals: QUEEN,
            fight_limit: 60.,
            aim: 2000.,
            vertical: 500.,
            reach: 420.,
            boss: true,
            clear: false,
            time_limit: 180.,
        },
        "hatter1" => Plan {
            after_goals: true,
            goals: CLOCKWORK,
            fight_limit: 75.,
            aim: 1400.,
            vertical: 500.,
            reach: 300.,
            boss: false,
            clear: false,
            time_limit: 210.,
        },
        "fortress1" => Plan {
            after_goals: true,
            goals: FORTRESS,
            fight_limit: 60.,
            aim: 1200.,
            vertical: 400.,
            reach: 300.,
            boss: false,
            clear: false,
            time_limit: 210.,
        },
        "utemple" => Plan {
            after_goals: true,
            goals: TEMPLE,
            fight_limit: 60.,
            aim: 1200.,
            vertical: 400.,
            reach: 300.,
            boss: false,
            clear: false,
            time_limit: 180.,
        },
        "wchess2" => Plan {
            after_goals: true,
            goals: CASTLING,
            fight_limit: 60.,
            aim: 1200.,
            vertical: 320.,
            reach: 300.,
            boss: false,
            clear: false,
            time_limit: 180.,
        },
        _ => return None,
    };
    Some(p)
}

/// Which extra read-only snapshot the viewer must gather for this map.
pub fn needs_legacy(map: &str) -> bool {
    matches!(map, "skool1" | "potears3")
}
pub fn needs_npcs(map: &str) -> bool {
    matches!(map, "hatter1" | "utemple")
}

pub struct Pilot {
    plan: Plan,
    seconds: f32,
    fight: f32,
    still: f32,
    anchor: Vec3,
    previous: Frame,
    deaths: u32,
    was_alive: bool,
    next_sample: f32,
    last_sanity: f32,
    waypoint: usize,
    health: std::collections::BTreeMap<String, f32>,
    last_threat: f32,
    dealt: f32,
    taken: f32,
    neutral_until: f32,
    last_seek_log: f32,
    last_feet: Vec3,
    waiting: bool,
}

impl Pilot {
    pub fn open(recording: bool) -> anyhow::Result<Option<Self>> {
        #[cfg(feature = "vita_renderer")]
        let enabled = crate::render::vita_renderer::config().replay_fight_pilot;
        #[cfg(not(feature = "vita_renderer"))]
        let enabled = false;
        if !enabled || !recording {
            return Ok(None);
        }
        println!("[lg/pilot] enabled input-only; time_limit=210 retries=2");
        Ok(Some(Self {
            plan: plan("hatter2").unwrap(),
            seconds: 0.,
            fight: 0.,
            still: 0.,
            anchor: Vec3::ZERO,
            previous: Frame::default(),
            deaths: 0,
            was_alive: true,
            next_sample: 0.,
            last_sanity: 100.,
            waypoint: 0,
            health: Default::default(),
            last_threat: 0.,
            dealt: 0.,
            taken: 0.,
            neutral_until: 0.,
            last_seek_log: 0.,
            last_feet: Vec3::ZERO,
            waiting: false,
        }))
    }

    pub fn sample_due(&self) -> bool {
        self.seconds >= self.next_sample
    }

    /// Consume the 1 Hz read-only snapshots. Health loss is sampled deltas, not contact sums.
    pub fn sample(&mut self, map: &str, s: Sample) {
        self.plan = plan(map).unwrap_or(self.plan);
        self.next_sample = self.seconds + 1.;
        let mut rows: Vec<(String, f32)> = Vec::new();
        match map {
            "hatter2" | "grounds1" | "jlair2" => boss_health(s.level.as_ref(), &mut rows),
            "centipede1" => {
                if let Some(ants) = s.level.as_ref().and_then(|v| v["ants"].as_array()) {
                    for ant in ants {
                        let a = &ant["ant"];
                        if let (Some(id), Some(hp)) = (a["id"].as_u64(), a["health"].as_f64()) {
                            rows.push((format!("ant{id}"), hp as f32));
                        }
                    }
                }
            }
            "grounds2" => {
                if let Some(actors) = s.level.as_ref().and_then(|v| v["actors"].as_array()) {
                    for (i, a) in actors.iter().enumerate() {
                        if a["spawned"].as_bool() == Some(true) {
                            if let Some(hp) = a["body"]["health"].as_f64() {
                                rows.push((format!("company{i}"), hp as f32));
                            }
                        }
                    }
                }
            }
            "qlair" => {
                if let Some(v) = s.level.as_ref() {
                    for key in ["queen1", "queen2"] {
                        if let Some(hp) = v[key].as_f64() {
                            rows.push((key.into(), hp as f32));
                        }
                    }
                }
            }
            "wchess2" => {
                if let Some(cast) = s.level.as_ref().and_then(|v| v["cast"].as_array()) {
                    for a in cast {
                        if let Some(hp) = a["piece"]["health"].as_f64() {
                            rows.push((
                                format!("cast{}:{}", a["entity"], a["name"].as_str().unwrap_or("")),
                                hp as f32,
                            ));
                        }
                    }
                }
            }
            "skool1" => {
                if let Some(enc) = s.legacy.as_ref().and_then(|v| v["encounters"].as_object()) {
                    if let Some(actors) = enc.get("actors").and_then(|v| v.as_array()) {
                        for (i, a) in actors.iter().enumerate() {
                            if let Some(hp) = a["enemy"]
                                .as_object()
                                .and_then(|e| e.values().next())
                                .and_then(|body| body["health"].as_f64())
                            {
                                rows.push((
                                    format!("school{i}:{}", a["name"].as_str().unwrap_or("")),
                                    hp as f32,
                                ));
                            }
                        }
                    }
                }
            }
            "potears3" => {
                if let Some(hp) = s
                    .legacy
                    .as_ref()
                    .and_then(|v| v["duchess"]["health"].as_f64())
                {
                    rows.push(("duchess".into(), hp as f32));
                }
            }
            "hatter1" | "utemple" => {
                if let Some(actors) = s.npcs.as_ref().and_then(|v| v["actors"].as_array()) {
                    for (i, a) in actors.iter().enumerate() {
                        let mut found = Vec::new();
                        collect_health(a, &format!("npc{i}"), &mut found);
                        rows.extend(found);
                    }
                }
            }
            _ => {}
        }
        let mut loss = 0.;
        for (key, hp) in rows {
            if let Some(before) = self.health.insert(key, hp) {
                loss += (before - hp).max(0.);
            }
        }
        self.dealt += loss;
        if loss > 0. {
            println!("[lg/pilot] health_loss={loss} seconds={:.3}", self.seconds);
        }
        if let Some(level) = s.level.as_ref().filter(|v| !v.is_null()) {
            println!(
                "[lg/pilot] state seconds={:.3} phase={} boss_health={} feet={:?}",
                self.seconds, level["phase"], level["boss"]["health"], self.last_feet
            );
        }
    }

    pub fn frame(
        &mut self,
        map: &str,
        player: &crate::movement::Player,
        yaw: f32,
        pitch: f32,
        alive: bool,
        sanity: f32,
        busy: bool,
        targets: &[crate::combat::Target],
        world: &crate::collision::World,
        ready: bool,
        selected: usize,
        will: f32,
        preferences: &crate::preferences::Preferences,
    ) -> Frame {
        let dt = macroquad::time::get_frame_time();
        let elapsed = dt.clamp(0., 0.1);
        self.seconds += elapsed;
        self.plan = plan(map).unwrap_or(self.plan);
        let teleported = player.feet.distance(self.last_feet) > 300.;
        self.last_feet = player.feet;
        if self.was_alive && !alive {
            self.deaths += 1;
        }
        if !self.was_alive && alive {
            self.waypoint = 0;
            self.health.clear();
            // Input::suppress requires both sticks to be neutral after checkpoint restore.
            self.neutral_until = self.seconds + 0.3;
        }
        self.was_alive = alive;
        if sanity < self.last_sanity {
            self.taken += self.last_sanity - sanity;
            println!(
                "[lg/pilot] damage_taken={} seconds={:.3}",
                self.last_sanity - sanity,
                self.seconds
            );
        }
        self.last_sanity = sanity;
        let mut frame = Frame {
            dt,
            pad: Some([128; 4]),
            ..Default::default()
        };
        let mut press = |key| super::input::hold_key(&mut frame, key);
        if player.feet.distance(self.anchor) > 24. {
            self.anchor = player.feet;
            self.still = 0.;
        } else {
            self.still += elapsed;
        }
        if busy {
            self.still = 0.;
        }
        let p = self.plan;
        let clear = p.clear
            && self.fight >= 12.
            && self.dealt > 0.
            && self.taken > 0.
            && self.seconds - self.last_threat > 8.;
        let stop = macroquad::input::is_key_pressed(KeyCode::Escape)
            || self.seconds > p.time_limit
            || self.deaths > 2
            || (self.still > if self.waiting { 30. } else { 18. } && !busy && self.fight == 0.)
            || self.fight > p.fight_limit
            || clear;
        if stop {
            println!(
                "[lg/pilot] stop map={map} seconds={:.3} fight={:.3} deaths={} feet={:?}",
                self.seconds, self.fight, self.deaths, player.feet
            );
            press(KeyCode::Escape); // Ordinary Start/menu stop flushes the native tape.
        } else if !alive {
            if (self.seconds * 2.) as u32 % 2 == 0 {
                press(KeyCode::E);
            }
        } else if self.seconds < 1. || self.seconds < self.neutral_until {
            // Neutral unlocks both stick latches after load; click resumes a loaded chapter.
            if self.seconds > 0.4 && self.seconds < 0.6 {
                press(KeyCode::LeftControl);
            }
        } else if !busy {
            // D-pad weapon cycle, translated by the backend to Right. Chapter loadout
            // starts with the knife; Cards are the first ranged weapon.
            let desired = if map == "hatter2" {
                1
            } else if will < 6. {
                0
            } else {
                1
            };
            if selected != desired && (self.seconds * 4.) as u32 % 2 == 0 {
                press(KeyCode::Right);
            }
            let at_post = !p.after_goals || self.waypoint + 1 >= p.goals.len();
            let target = targets
                .iter()
                .filter(|_| at_post)
                .filter(|t| t.center.distance(player.eye()) < p.aim)
                .filter(|t| (t.center.z - player.eye().z).abs() < p.vertical)
                .filter(|t| {
                    let trace = world.sight_sweep(player.eye(), t.center, Vec3::ZERO);
                    !trace.start_solid && trace.fraction > 0.98
                })
                .min_by(|a, b| {
                    a.center
                        .distance_squared(player.eye())
                        .total_cmp(&b.center.distance_squared(player.eye()))
                });
            let goal = if let Some(t) = target {
                self.waiting = false;
                if self.fight == 0. {
                    println!(
                        "[lg/pilot] fight_start seconds={:.3} id={} feet={:?} target={:?}",
                        self.seconds, t.id, player.feet, t.center
                    );
                }
                self.fight += elapsed;
                self.last_threat = self.seconds;
                t.center
            } else {
                if !p.goals.is_empty() {
                    // A teleport (portal) can carry Alice past several route points.
                    if teleported {
                        while self.waypoint + 1 < p.goals.len()
                            && p.goals[self.waypoint].distance(player.feet)
                                > p.goals[self.waypoint + 1].distance(player.feet) + 64.
                        {
                            self.waypoint += 1;
                        }
                    }
                    let goal = p.goals[self.waypoint];
                    if player.feet.truncate().distance(goal.truncate()) < 55.
                        && (player.feet.z - goal.z).abs() < 110.
                        && self.waypoint + 1 < p.goals.len()
                    {
                        self.waypoint += 1;
                        self.still = 0.;
                        println!(
                            "[lg/pilot] waypoint={} seconds={:.3}",
                            self.waypoint, self.seconds
                        );
                    }
                    if self.seconds - self.last_seek_log > 5. {
                        self.last_seek_log = self.seconds;
                        println!(
                            "[lg/pilot] seek map={map} waypoint={} seconds={:.3} feet={:?} goal={:?}",
                            self.waypoint, self.seconds, player.feet, p.goals[self.waypoint]
                        );
                    }
                    let mut goal = p.goals[self.waypoint];
                    // A lift or mover is the only way up: hold position and let it carry
                    // Alice, instead of sliding off the platform.
                    self.waiting = player.feet.truncate().distance(goal.truncate()) < 75.
                        && (player.feet.z - goal.z).abs() > 110.;
                    if self.waiting {
                        goal = player.feet;
                    }
                    if self.still > 2. && !self.waiting {
                        // Slide along a blocking lip or doorway instead of pushing into it,
                        // alternating sides with a growing offset. Bounded by the stall stop.
                        let d = (goal - player.feet).truncate().normalize_or_zero();
                        let side = vec2(-d.y, d.x);
                        let phase = ((self.still - 2.) / 2.5) as u32;
                        let sign = if phase % 2 == 0 { 1. } else { -1. };
                        let amount = 120. * (1. + (phase / 2) as f32);
                        goal.x += side.x * sign * amount;
                        goal.y += side.y * sign * amount;
                    }
                    vec3(goal.x, goal.y, player.eye().z)
                } else {
                    let f = player.eye() + vec3(yaw.cos() * 32., yaw.sin() * 32., 0.);
                    vec3(f.x, f.y, player.eye().z)
                }
            };
            let d = goal - player.eye();
            let turn = (d.y.atan2(d.x) - yaw + std::f32::consts::PI)
                .rem_euclid(std::f32::consts::TAU)
                - std::f32::consts::PI;
            let elevation = if target.is_some() {
                d.z.atan2(d.truncate().length())
            } else {
                0.
            };
            // Proportional bounded sticks. The real deadzone/curve/sensitivity still apply.
            let axis = |error: f32, rate: f32| {
                if error.abs() < 0.015 {
                    0.
                } else {
                    let magnitude = (error.abs() / (rate * elapsed.max(0.005)))
                        .min(1.)
                        .powf(1. / 1.7);
                    error.signum()
                        * (preferences.pad_deadzone + (1. - preferences.pad_deadzone) * magnitude)
                }
            };
            let rates = crate::look::stick::rates(preferences.pad_sensitivity)
                * (std::f32::consts::PI / 180.);
            let rx = -axis(turn, rates.x);
            let ry =
                -axis(elevation - pitch, rates.y) * if preferences.invert_pad { -1. } else { 1. };
            let distance = d.truncate().length();
            let forward = if target.is_some() {
                if distance > if desired == 0 { 85. } else { p.reach } {
                    0.75
                } else {
                    0.
                }
            } else if distance > 20. && turn.abs() < 0.65 {
                1.
            } else {
                0.
            };
            let strafe = if target.is_some()
                && distance < 800.
                && (map == "hatter2"
                    || map == "centipede1" && self.fight % 6. > 3.
                    || p.boss && map != "hatter2")
            {
                if (self.fight / 3.) as u32 % 2 == 0 {
                    0.55
                } else {
                    -0.55
                }
            } else {
                0.
            };
            let byte = |x: f32| (128. + x.clamp(-1., 1.) * 127.).round() as u8;
            frame.pad = Some([byte(strafe), byte(-forward), byte(rx), byte(ry)]);
            if target.is_some()
                && ready
                && selected == desired
                && turn.abs() < 0.15
                && (elevation - pitch).abs() < 0.15
            {
                super::input::hold_key(&mut frame, KeyCode::LeftControl);
            }
            if !self.waiting && self.still > 1.6 && (self.seconds * 3.) as u32 % 2 == 0 {
                // Real jump input clears small ledges and doorway lips.
                super::input::hold_key(&mut frame, KeyCode::Space);
            }
            if self.still > 3. && (self.seconds * 2.) as u32 % 3 == 0 {
                super::input::hold_key(&mut frame, KeyCode::Enter);
            }
        }
        frame.keys_pressed = frame.keys_down & !self.previous.keys_down;
        frame.keys_released = self.previous.keys_down & !frame.keys_down;
        self.previous = frame.clone();
        frame
    }
}

fn boss_health(level: Option<&Value>, rows: &mut Vec<(String, f32)>) {
    if let Some(hp) = level.and_then(|v| v["boss"]["health"].as_f64()) {
        rows.push(("boss".into(), hp as f32));
    }
}

/// Every numeric `health` under an NPC actor, keyed by its JSON path. Save actors keep
/// their array index, and health increases (respawn/heal) are ignored by the caller.
fn collect_health(value: &Value, prefix: &str, out: &mut Vec<(String, f32)>) {
    match value {
        Value::Object(map) => {
            for (k, v) in map {
                let path = format!("{prefix}.{k}");
                if k == "health" {
                    if let Some(hp) = v.as_f64() {
                        out.push((path, hp as f32));
                    }
                } else {
                    collect_health(v, &path, out);
                }
            }
        }
        Value::Array(items) => {
            for (i, v) in items.iter().enumerate() {
                collect_health(v, &format!("{prefix}[{i}]"), out);
            }
        }
        _ => {}
    }
}
