//! Staged actual-map fixture for two Clockwork doors, through the viewer's use path:
//! `Interactions::prompt`, `use_owner`, `Interactions::update`, then the movers and Alice.
//!
//! `Route::tick` is not used here. It aims with the movement direction and opens any door
//! that prompts, so it cannot show a prompt without a press. The NPC cast is absent, so the
//! Talk owner cannot take the press. One process is one lever arm:
//! `tools/host_fixtures_r22.py` compares ON and OFF.
use super::super::*;
use crate::collision::{PLAYER_CENTER, PLAYER_HALF};
use crate::host_fixture::{variant, Recorder};
use crate::interaction::friendly::{use_owner, UseOwner};
use crate::movement::{Controls, FIXED_DT};
use crate::route::Route;
use serde_json::json;

const OPEN: &str = "E  open door";
const RANGE: f32 = 150.;

struct Approach {
    label: &'static str,
    /// BSP entity of the door the approach is aimed at.
    door: usize,
    /// Body-valid floor out of range, facing the door.
    start: Vec3,
    wish: Vec2,
    /// Body-valid floor in range and faced, with static geometry on the line to the door, and
    /// outside the zone where flag-64 doors open for a nearby body with no press at all.
    behind: Vec3,
}
fn approaches() -> [Approach; 2] {
    [
        Approach { label: "east-corridor", door: 119, start: vec3(-3540., 2728., 192.),
            wish: vec2(1., 0.), behind: vec3(-3408., 2840., 192.) },
        Approach { label: "north-corridor", door: 112, start: vec3(-4880., 3000., 208.),
            wish: vec2(0., -1.), behind: vec3(-5024., 2872., 320.) },
    ]
}

fn own(r: &Route) -> &Clockwork {
    r.interactions.levels.iter().find_map(|s| s.ctl.downcast_ref::<Clockwork>()).unwrap()
}

/// One 120 Hz viewer frame, in the viewer's order. `press` is the raw use edge; the owner
/// decides whether it reaches the world. Returns the prompt the player saw and that owner.
fn frame(r: &mut Route, rec: &mut Recorder, aim: Vec3, press: bool, wish: Vec2)
    -> Result<(Option<&'static str>, UseOwner)> {
    let eye = r.player.eye();
    let prompt = r.interactions.prompt(&r.world, eye, aim);
    let rope = r.player.rope_prompt(&r.world).is_some();
    let owner = use_owner(r.story.busy(), r.interactions.scripted(), rope, false, false, prompt.is_some());
    let updates = r.interactions.update(FIXED_DT, &r.map, &mut r.world, &r.player, aim,
        press && owner == UseOwner::World)?;
    r.interactions.companions_for_movers(&[]);
    r.interactions.actors_for_movers(&[]);
    r.interactions.advance_school(FIXED_DT, &r.map, &mut r.world, &mut r.player)?;
    let before = r.player.feet;
    r.stats.prepare_player(&mut r.player);
    r.player.tick(&r.world, Controls { wish, ..Default::default() });
    let mut e = r.interactions.triggers(FIXED_DT, before, r.player.feet);
    e.merge(updates);
    ensure!(e.damage == 0. && e.transition.is_none() && e.teleport.is_none(),
        "A trigger interfered with the door fixture at {:?}", r.player.feet);
    for event in e.story {
        rec.event(json!({"tick":r.ticks,"event":"story","story":event}));
        r.story.trigger(&event);
    }
    r.ticks += 1;
    ensure!(r.world.body_clear(r.player.feet), "Alice left a body-valid position at {:?}", r.player.feet);
    rec.tick(&r.checkpoint())?;
    Ok((prompt, owner))
}

fn stage(r: &mut Route, at: Vec3) -> Result<()> {
    let feet = r.world.actor_footing(at + Vec3::Z * 32., PLAYER_CENTER, PLAYER_HALF, 120.)
        .with_context(|| format!("No footing at {at:?}"))?;
    ensure!(r.world.body_clear(feet), "{at:?} is not body-valid");
    r.player = Player::new(feet);
    r.player.grounded = true;
    r.player.velocity = Vec3::ZERO;
    Ok(())
}

/// The authored proximity opening in `advance_motion`, for this door and its neighbours.
fn in_auto_open_zone(r: &Route) -> bool {
    let center = r.player.feet + PLAYER_CENTER;
    let o = own(r);
    o.objects.iter().any(|d| d.class.contains("door") && d.flags & 64 != 0 && d.flags & (8 | 128) == 0
        && center.cmpge(d.bounds.0 - vec3(60., 60., 8.) - PLAYER_HALF).all()
        && center.cmple(d.bounds.1 + vec3(60., 60., 8.) + PLAYER_HALF).all())
}

fn door_state(r: &Route, k: usize) -> (f32, f32, f32) {
    let o = own(r);
    let d = &o.objects[k];
    (o.saved.holds[k], o.saved.doors[k], d.pose.rotation.angle_between(d.base.rotation)
        + d.pose.translation.distance(d.base.translation))
}

pub(in crate::levels::hatter1) fn check(a: &mut Assets) -> Result<()> {
    let name = "hatter1";
    let entry = Some("hatter1_start1");
    let mut r = Route::enter(a, name, entry, crate::inventory::Stats::for_level(name, entry),
        crate::powerups::Difficulty::Hard)?;
    let mut rec = Recorder::new("hatter1-door");
    let outcome = run(&mut r, &mut rec);
    let summary = json!({ "feet": r.player.feet.to_array(), "sanity": r.stats.sanity(),
        "difficulty": format!("{:?}", r.difficulty) });
    let state = r.checkpoint();
    rec.finish(&state, summary, outcome)?;
    println!("PASS hatter1 door fixture ({})", variant());
    Ok(())
}

fn run(r: &mut Route, rec: &mut Recorder) -> Result<()> {
    ensure!(r.difficulty == crate::powerups::Difficulty::Hard, "Fixture is not on Hard");
    r.interactions.prepare_player(&mut r.stats, &mut r.player);
    for _ in 0..4 {
        r.interactions.skip_cinematic(&r.map, &mut r.world, &mut r.player, &mut r.story)?;
    }
    ensure!(!r.interactions.scripted() && !r.story.busy(), "An opening scene still owns Alice");
    let static_world = World::from_bsp(&r.map)?;
    let v = variant();
    for ap in approaches() {
        approach(r, rec, &ap, &static_world, &v).with_context(|| ap.label)?;
    }
    Ok(())
}

fn approach(r: &mut Route, rec: &mut Recorder, ap: &Approach, static_world: &World, v: &str) -> Result<()> {
    let k = own(r).objects.iter().position(|o| o.id == ap.door).context("Door entity missing")?;
    {
        let o = own(r);
        let d = &o.objects[k];
        ensure!(d.class.contains("door") && o.door_target(d).is_none() && d.flags & (8 | 128) == 0,
            "Entity {} is not a usable door", ap.door);
    }
    let center = { let d = &own(r).objects[k]; (d.bounds.0 + d.bounds.1) * 0.5 };
    let label = ap.label;
    ensure!(door_state(r, k).0 == 0. && door_state(r, k).1 == 0., "Door {} did not start closed", ap.door);

    // Walk in, facing the door, without pressing use.
    stage(r, ap.start)?;
    ensure!(center.distance(r.player.eye()) >= RANGE, "Approach start is already in range");
    let mut first_prompt = None;
    for _ in 0..480 {
        let eye = r.player.eye();
        let distance = center.distance(eye);
        if distance < RANGE - 16. { break; }
        let t = r.ticks;
        let (prompt, _) = frame(r, rec, (center - eye).normalize(), false, ap.wish)?;
        ensure!((prompt == Some(OPEN)) == (distance < RANGE),
            "Prompt {prompt:?} at distance {distance} on the approach");
        if prompt.is_some() && first_prompt.is_none() {
            first_prompt = Some(distance);
            rec.event(json!({"tick":t,"approach":label,"event":"prompt","distance":distance,"feet":r.player.feet.to_array()}));
        }
    }
    let first_prompt = first_prompt.context("The approach never reached a prompt")?;
    for _ in 0..60 { let aim = (center - r.player.eye()).normalize(); frame(r, rec, aim, false, Vec2::ZERO)?; }
    ensure!(door_state(r, k).0 == 0. && door_state(r, k).1 == 0., "A prompt alone opened door {}", ap.door);

    // The use edge reaches the world through the owner.
    ensure!(!in_auto_open_zone(r), "Prompt stand {:?} can open a door by proximity", r.player.feet);
    let aim = (center - r.player.eye()).normalize();
    let picked = {
        let eye = r.player.eye();
        let o = own(r);
        let eager = o.door_pick_mode(&r.world, eye, aim, false);
        ensure!(eager == o.door_pick_mode(&r.world, eye, aim, true), "Door modes disagree at the use edge");
        eager
    };
    ensure!(picked == Some(k), "Prompt selected object {picked:?}, not door object {k}");
    let tick = r.ticks;
    let (prompt, owner) = frame(r, rec, aim, v != "no-use", Vec2::ZERO)?;
    ensure!(prompt == Some(OPEN) && owner == UseOwner::World, "Use edge saw {prompt:?}, owner {owner:?}");
    let (hold, _, _) = door_state(r, k);
    ensure!(hold > 2.9, "Use edge did not hold door {} open: hold {hold}", ap.door);
    frame(r, rec, aim, false, Vec2::ZERO)?;
    ensure!(door_state(r, k).0 < hold, "Door hold did not count down");
    for _ in 0..120 { frame(r, rec, aim, false, Vec2::ZERO)?; }
    let (_, open, moved) = door_state(r, k);
    ensure!(open > 0. && moved > 0.01, "Door {} did not move: state {open}, pose {moved}", ap.door);
    rec.event(json!({"tick":tick,"approach":label,"event":"use","hold":hold,"open":open,"moved":moved}));

    // Turning away drops the prompt while still in range; stepping back keeps it dropped.
    if v != "no-turn" {
        let eye = r.player.eye();
        ensure!(center.distance(eye) < RANGE, "Alice is out of range before turning");
        let away = (eye - center).normalize();
        let (prompt, _) = frame(r, rec, away, false, Vec2::ZERO)?;
        ensure!(prompt.is_none(), "Turned away, in range: prompt {prompt:?}");
        rec.event(json!({"tick":r.ticks,"approach":label,"event":"turned-away","distance":center.distance(r.player.eye())}));
        for _ in 0..600 {
            if center.distance(r.player.eye()) >= RANGE + 8. { break; }
            let (prompt, _) = frame(r, rec, away, false, -ap.wish)?;
            ensure!(prompt.is_none(), "Stepping away: prompt {prompt:?}");
        }
    }
    let distance = center.distance(r.player.eye());
    let faced = (center - r.player.eye()).normalize();
    let (prompt, _) = frame(r, rec, faced, false, Vec2::ZERO)?;
    ensure!(prompt.is_none(), "Out of step-away sequence at distance {distance}: prompt {prompt:?}");
    rec.event(json!({"tick":r.ticks,"approach":label,"event":"out-of-range-faced","distance":distance}));

    // The door closes by itself once its hold runs out.
    let mut closed = false;
    for _ in 0..1200 {
        frame(r, rec, -faced, false, Vec2::ZERO)?;
        let (hold, open, moved) = door_state(r, k);
        if hold == 0. && open == 0. && moved < 0.01 { closed = true; break; }
    }
    ensure!(closed, "Door {} did not close again", ap.door);
    rec.event(json!({"tick":r.ticks,"approach":label,"event":"closed"}));
    if v == "timing-shift" { frame(r, rec, -faced, false, Vec2::ZERO)?; }

    // In range and faced, with real geometry on the line. `no-occluder` stands on the clear line.
    let spot = if v == "no-occluder" { ap.start + ap.wish.extend(0.) * 90. } else { ap.behind };
    stage(r, spot)?;
    let eye = r.player.eye();
    let d = center - eye;
    let aim = d.normalize();
    ensure!(d.length() < RANGE && d.normalize().dot(aim) > 0.3, "Occlusion spot is not in range and faced");
    let (world_trace, own_trace, fixed) = {
        let o = own(r);
        let door = &o.objects[k];
        (r.world.sweep(eye, center, Vec3::splat(0.5)), door.collider.trace(eye, center, Vec3::splat(0.5)),
            static_world.sweep(eye, center, Vec3::splat(0.5)))
    };
    rec.event(json!({"tick":r.ticks,"approach":label,"event":"occlusion-spot","feet":r.player.feet.to_array(),"distance":d.length(),
        "world_fraction":world_trace.fraction,"own_fraction":own_trace.fraction,"static_fraction":fixed.fraction}));
    for press in [false, true] {
        let hold = door_state(r, k).0;
        let (prompt, owner) = frame(r, rec, aim, press, Vec2::ZERO)?;
        ensure!(prompt.is_none(), "Behind geometry: prompt {prompt:?}");
        ensure!(owner == UseOwner::Traversal, "Behind geometry: owner {owner:?}");
        ensure!(door_state(r, k).0 <= hold && door_state(r, k).1 == 0., "Use behind geometry moved door {}", ap.door);
    }
    // The controller's own use path, with the press forced past the owner.
    let (eye, hold) = (r.player.eye(), door_state(r, k).0);
    r.interactions.update(FIXED_DT, &r.map, &mut r.world, &r.player, aim, true)?;
    ensure!(door_state(r, k).0 <= hold, "Forced use behind geometry held door {}", ap.door);
    {
        let o = own(r);
        ensure!(o.door_pick_mode(&r.world, eye, aim, false).is_none()
            && o.door_pick_mode(&r.world, eye, aim, true).is_none(), "Door modes selected through geometry");
    }
    ensure!(!in_auto_open_zone(r), "Occlusion spot {:?} can open a door by proximity", r.player.feet);
    ensure!(!world_trace.start_solid && world_trace.fraction + 0.02 < own_trace.fraction,
        "Door {} is not behind geometry from {:?}", ap.door, r.player.feet);
    ensure!(!fixed.start_solid && fixed.fraction + 0.02 < own_trace.fraction,
        "The occluder of door {} is not static map geometry", ap.door);
    rec.event(json!({"approach":label,"event":"approach-passed","first_prompt_distance":first_prompt,"door":ap.door}));
    Ok(())
}
