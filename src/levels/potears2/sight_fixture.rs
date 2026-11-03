//! Staged actual-map fixture for the two Hard door Corporals (BSP entities 5 and 554).
//!
//! Alice is placed on a body-valid floor near the house; this says nothing about reaching it
//! by the lily pads. Damage stays real and death fails the fixture. Alice never attacks.
//! One process is one lever arm: `tools/host_fixtures_r22.py` compares ON and OFF.
//!
//! There are two cadences. `check` runs `Route::tick` at 120 Hz, where each guard update is
//! one substep, so no sight key repeats inside a reuse scope and the memo never hits.
//! `check_30hz` passes a whole frame's time to the guards as the viewer does (`viewer.rs`,
//! `levels_step`). Their four substeps repeat the same key, so that arm tests a reused trace.
use super::*;
use crate::host_fixture::{variant, Recorder};
use crate::movement::{Controls, Player, FIXED_DT};
use crate::route::Route;
use serde_json::{json, Value};

const IDS: [usize; 2] = [5, 554];
const MEMORY: f32 = 3.;
/// No target has this id, so `Route::tick` never fires for Alice.
const NO_TARGET: usize = usize::MAX;

struct Course {
    exposed: Vec3,
    wish: Vec2,
    expose: usize,
    move_ticks: usize,
    cover: usize,
    return_ticks: usize,
    reexpose: usize,
}
fn course() -> Course {
    // Against the wall north-east of the open floor, out of both guards' sight. The times are
    // ticks at 120 Hz. The first exposure lasts until guard 554 has a grenade in the air.
    let mut c = Course {
        exposed: vec3(-124., 0., 128.),
        wish: vec2(0.7071, 0.7071),
        expose: 280,
        move_ticks: 70,
        cover: 840,
        return_ticks: 50,
        reexpose: 500,
    };
    match variant().as_str() {
        // Cover removed: the same clock, but Alice never leaves the exposed floor.
        "no-cover" => c.move_ticks = 0,
        // Back in the open before the 3 s memory can expire.
        "short-cover" => c.cover = 120,
        // Never comes back into view.
        "no-reexpose" => c.return_ticks = 0,
        // Every assertion should hold; only ON/OFF style comparison can catch this.
        "timing-shift" => c.cover += 1,
        _ => {}
    }
    c
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Stage { Exposed, Moving, Cover, Returning, Reexposed }

struct Guard {
    id: usize,
    visible: bool,
    acquired_tick: Option<usize>,
    last_visible_tick: Option<usize>,
    lost_tick: Option<usize>,
    expired_tick: Option<usize>,
    expired_after: Option<f32>,
    frozen_last_seen: Option<Value>,
    occluded_ticks: usize,
    memory_ticks: usize,
    reacquired_tick: Option<usize>,
    grenades: usize,
    attacks: usize,
    last_phase: String,
    memory: f32,
}

fn guards_json(r: &mut Route) -> Result<Vec<Value>> {
    owner(&mut r.interactions)?.saved.guards.iter()
        .map(|g| Ok(serde_json::to_value(g)?)).collect()
}
fn center(r: &mut Route, id: usize) -> Result<Vec3> {
    let g = owner(&mut r.interactions)?.saved.guards.iter().find(|g| g.id == id)
        .context("Named guard missing")?;
    Ok(g.target(0).center)
}

pub(super) fn check(a: &mut Assets) -> Result<()> {
    fixture(a, "potears2-sight", 1)
}
pub(super) fn check_30hz(a: &mut Assets) -> Result<()> {
    fixture(a, "potears2-sight-30hz", 4)
}

/// One rendered frame of `step` physics ticks in the viewer's order, without Alice's weapons.
fn viewer_frame(r: &mut Route, wish: Vec2, step: usize) -> Result<()> {
    let dt = step as f32 * FIXED_DT;
    r.interactions.prepare_player(&mut r.stats, &mut r.player);
    let updates = r.interactions.update(dt, &r.map, &mut r.world, &r.player, Vec3::ZERO, false)?;
    r.interactions.companions_for_movers(&[]);
    r.interactions.actors_for_movers(&[]);
    r.interactions.advance_school(dt, &r.map, &mut r.world, &mut r.player)?;
    let before = r.player.feet;
    let landings = r.player.landings;
    r.stats.prepare_player(&mut r.player);
    for _ in 0..step {
        r.player.tick(&r.world, Controls { wish, ..Default::default() });
    }
    r.stats.damage(std::mem::take(&mut r.player.liquid_damage));
    if r.player.landings > landings {
        r.stats.fall(r.player.landing_speed);
    }
    let mut e = r.interactions.triggers(dt, before, r.player.feet);
    e.merge(updates);
    r.stats.damage(e.damage);
    let notarget = r.stats.ignores_alice();
    let f = r.interactions.levels_step(&mut crate::level::Combat {
        dt,
        world: &r.world,
        player: &mut r.player,
        stats: &mut r.stats,
        story: &mut r.story,
        notarget,
        summon: None,
        threatens: &|_| false,
    });
    r.stats.damage(f.damage);
    r.player.knockback(f.impulse);
    r.damage += f.damage;
    r.stats.update(dt);
    r.ticks += step;
    ensure!(r.world.body_clear(r.player.feet), "Route body overlap at {:?}", r.player.feet);
    ensure!(r.stats.alive(), "Route died at {:?}", r.player.feet);
    Ok(())
}

fn fixture(a: &mut Assets, label: &'static str, step: usize) -> Result<()> {
    let name = "potears2";
    let mut r = Route::enter(a, name, None, crate::inventory::Stats::for_level(name, None),
        crate::powerups::Difficulty::Hard)?;
    let mut rec = Recorder::new(label);
    let outcome = run(&mut r, &mut rec, step);
    let summary = json!({ "feet": r.player.feet.to_array(), "sanity": r.stats.sanity(),
        "combat_damage": r.damage, "difficulty": format!("{:?}", r.difficulty) });
    let state = r.checkpoint();
    rec.finish(&state, summary, outcome)?;
    println!("PASS {label} ({})", variant());
    Ok(())
}

fn run(r: &mut Route, rec: &mut Recorder, step: usize) -> Result<()> {
    ensure!(r.difficulty == crate::powerups::Difficulty::Hard, "Fixture is not on Hard");
    let c = course();
    let mut guards = Vec::new();
    for (g, id) in guards_json(r)?.iter().zip(IDS) {
        ensure!(g["id"] == id && g["corporal"] == true && g["health"] == 160.0,
            "Expected Corporal entity {id}, found {g}");
        ensure!(r.map.entities[id].get("targetname").is_some_and(|n| n == "antguards"),
            "BSP entity {id} is not a door guard");
        ensure!(g["memory"] == 0.0 && g["last_seen"].is_null() && g["phase"] == "Idle",
            "Guard {id} did not start unaware: {g}");
        guards.push(Guard { id, visible: false, acquired_tick: None, last_visible_tick: None,
            lost_tick: None, expired_tick: None, expired_after: None, frozen_last_seen: None,
            occluded_ticks: 0, memory_ticks: 0, reacquired_tick: None, grenades: 0, attacks: 0,
            last_phase: "Idle".into(), memory: 0. });
    }
    ensure!(guards.len() == 2 && owner(&mut r.interactions)?.saved.guards.len() == 2, "Two house guards required");
    let feet = r.world.actor_footing(c.exposed + Vec3::Z * 32., crate::collision::PLAYER_CENTER,
        crate::collision::PLAYER_HALF, 120.).context("Exposed floor has no footing")?;
    ensure!(r.world.body_clear(feet), "Exposed floor is not body-valid");
    r.player = Player::new(feet);
    r.player.grounded = true;
    r.aim_at = Some(NO_TARGET);

    let total = c.expose + c.move_ticks + c.cover + c.return_ticks + c.reexpose;
    let cover_end = c.expose + c.move_ticks + c.cover;
    let mut damage_exposed = 0f32;
    let mut damage_flight = 0f32;
    let mut damage_blind = 0f32;
    let static_world = World::from_bsp(&r.map)?;
    let dt = step as f32 * FIXED_DT;
    let mut cover_frames = 0;
    for t in (0..total).step_by(step) {
        let (stage, wish) = if t < c.expose { (Stage::Exposed, Vec2::ZERO) }
            else if t < c.expose + c.move_ticks { (Stage::Moving, c.wish) }
            else if t < cover_end { (Stage::Cover, Vec2::ZERO) }
            else if t < cover_end + c.return_ticks { (Stage::Returning, -c.wish) }
            else { (Stage::Reexposed, Vec2::ZERO) };
        ensure!(!r.interactions.scripted() && !r.story.busy(), "A scene interrupted the fixture at tick {t}");
        let (sanity, damage) = (r.stats.sanity(), r.damage);
        let flying = guards_json(r)?.iter().any(|g|
            !g["shots"].as_array().unwrap().is_empty() || !g["blasts"].as_array().unwrap().is_empty());
        if stage == Stage::Cover { cover_frames += 1; }
        if step == 1 {
            r.tick(Controls { wish, ..Default::default() })
        } else {
            viewer_frame(r, wish, step)
        }.with_context(|| format!("Tick {t}, {stage:?}"))?;
        rec.tick(&r.checkpoint())?;
        let eye = r.player.eye();
        let states = guards_json(r)?;
        let any_visible = states.iter().any(|g| g["memory"].as_f64() == Some(MEMORY as f64));
        for (g, s) in guards.iter_mut().zip(&states) {
            let memory = s["memory"].as_f64().context("memory")? as f32;
            let phase = s["phase"].as_str().context("phase")?.to_string();
            // The controller sets memory to exactly 3 on the ticks its own sight test passes.
            let visible = memory == MEMORY;
            if visible && !g.visible {
                g.acquired_tick.get_or_insert(t);
                if g.lost_tick.is_some() && g.expired_tick.is_some() && g.reacquired_tick.is_none() {
                    g.reacquired_tick = Some(t);
                }
                rec.event(json!({"tick":t,"stage":format!("{stage:?}"),"guard":g.id,"event":"sight","eye":eye.to_array(),"at":s["feet"]}));
            }
            if !visible && g.visible {
                if g.expired_tick.is_none() { g.lost_tick = Some(t); }
                g.frozen_last_seen = Some(s["last_seen"].clone());
                rec.event(json!({"tick":t,"stage":format!("{stage:?}"),"guard":g.id,"event":"lost","eye":eye.to_array(),"at":s["feet"],"last_seen":s["last_seen"]}));
            }
            if visible {
                g.last_visible_tick = Some(t);
                let seen: Vec3 = serde_json::from_value(s["last_seen"].clone())?;
                ensure!(seen.distance(eye) < 0.001, "Guard {} last_seen {seen:?} is not the visible eye {eye:?}", g.id);
                ensure!(stage != Stage::Cover, "Guard {} still sees Alice in cover at tick {t}", g.id);
            } else if memory > 0. {
                g.memory_ticks += 1;
                ensure!(g.frozen_last_seen.as_ref() == Some(&s["last_seen"]),
                    "Guard {} last_seen moved without sight at tick {t}", g.id);
                // A frame that lost sight partway through ages memory by less than dt.
                let stepped = g.memory - memory;
                ensure!(stepped > 0. && stepped < dt + 0.0002 && (g.visible || (stepped - dt).abs() < 0.0002),
                    "Guard {} memory stepped {} to {memory} at tick {t}", g.id, g.memory);
            } else if g.memory > 0. && g.expired_tick.is_none() {
                g.expired_tick = Some(t);
                g.expired_after = g.lost_tick.map(|l| (t - l + step) as f32 * FIXED_DT);
                rec.event(json!({"tick":t,"stage":format!("{stage:?}"),"guard":g.id,"event":"memory-expired","after_seconds":g.expired_after,"at":s["feet"]}));
            }
            if !visible && matches!(stage, Stage::Cover) {
                let from = center(r, g.id)?;
                let tr = r.world.sweep(from, eye, Vec3::splat(0.5));
                let fixed = static_world.sweep(from, eye, Vec3::splat(0.5));
                ensure!(from.distance(eye) < 1200. && !tr.start_solid && tr.fraction < 1.
                    && !fixed.start_solid && fixed.fraction < 1.,
                    "Guard {} lost Alice at tick {t} without static geometry between them", g.id);
                g.occluded_ticks += 1;
            }
            if phase != g.last_phase {
                rec.event(json!({"tick":t,"guard":g.id,"event":"phase","from":g.last_phase,"to":phase,"visible":visible,"memory":memory}));
                if matches!(phase.as_str(), "Fire" | "Melee") { g.attacks += 1; }
                g.last_phase = phase;
            }
            let fresh = s["shots"].as_array().unwrap().iter()
                .filter(|b| b["age"].as_f64().is_some_and(|a| a < f64::from(dt) - f64::from(FIXED_DT) * 0.5)).count();
            if fresh > 0 {
                g.grenades += fresh;
                ensure!(visible, "Guard {} released a grenade without sight at tick {t}", g.id);
                rec.event(json!({"tick":t,"guard":g.id,"event":"grenade","at":s["feet"]}));
            }
            if stage == Stage::Cover && t + step >= cover_end {
                ensure!(g.expired_tick.is_some(), "Guard {} memory did not expire behind cover", g.id);
            }
            g.memory = memory;
            g.visible = visible;
        }
        let lost = r.damage - damage;
        if lost > 0. || r.stats.sanity() != sanity {
            let kind = if any_visible { "visible" } else if flying { "in-flight" } else { "blind" };
            match kind { "visible" => damage_exposed += lost, "in-flight" => damage_flight += lost, _ => damage_blind += lost }
            rec.event(json!({"tick":t,"stage":format!("{stage:?}"),"event":"damage","combat":lost,"sanity":[sanity,r.stats.sanity()],"kind":kind,"feet":r.player.feet.to_array()}));
        }
    }
    let table: Vec<Value> = guards.iter().map(|g| json!({"id":g.id,"acquired":g.acquired_tick,"lost":g.lost_tick,
        "expired":g.expired_tick,"expired_after":g.expired_after,"occluded_ticks":g.occluded_ticks,"memory_ticks":g.memory_ticks,
        "reacquired":g.reacquired_tick,"grenades":g.grenades,"attacks":g.attacks})).collect();
    rec.event(json!({"event":"table","guards":table,
        "damage":{"visible":damage_exposed,"in_flight":damage_flight,"blind":damage_blind}}));
    for g in &guards {
        ensure!(g.acquired_tick.is_some_and(|t| t < c.expose), "Guard {} never saw Alice in the first exposure", g.id);
        ensure!(g.lost_tick.is_some_and(|t| t >= c.expose), "Guard {} never lost sight of Alice in cover", g.id);
        ensure!(g.occluded_ticks == cover_frames, "Guard {} occluded {} of {cover_frames} cover frames", g.id, g.occluded_ticks);
        ensure!(g.expired_after.is_some_and(|s| (s - MEMORY).abs() <= dt + 0.02),
            "Guard {} memory expired after {:?} s, not the 3 s timer", g.id, g.expired_after);
        ensure!(g.reacquired_tick.is_some_and(|t| t >= cover_end), "Guard {} did not reacquire Alice", g.id);
        ensure!(g.grenades > 0, "Guard {} threw no grenade", g.id);
    }
    ensure!(damage_blind == 0., "Damage without sight or a projectile in flight: {damage_blind}");
    ensure!(damage_flight > 0., "No grenade already in flight hit Alice behind cover");
    ensure!(damage_exposed > 0., "No attack hit Alice after she was reacquired");
    ensure!(r.transition.is_none() && r.teleports == 0, "Fixture left the staged area");
    let now = guards_json(r)?;
    ensure!(now.iter().all(|g| g["health"] == 160.0), "Alice must not have damaged a guard");
    Ok(())
}
