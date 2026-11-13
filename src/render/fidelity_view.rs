//! Fixed views for comparing the desktop renderer with the Vita renderer on every shipped map.
//! `--fidelity-view-pick FILE` needs no window: it reads each map's BSP, builds the static
//! collision world and writes, per map, the player start and four or five views as JSON.
//! The captures themselves run the normal game loop with `LOOKING_GLASS_SIM_MAPS_PROBE`,
//! `--start-at` and `LOOKING_GLASS_FIXED_VIEWS`, so Alice, NPCs, effects and the HUD are in the
//! picture and both renderers see the same simulation.
//!
//! Views, in order:
//! - `start`: the follow camera behind Alice at the map's player start.
//! - `far`: the path node and heading with the longest clear sight lines, tilted up a little,
//!   for distance fog, the far world and the sky.
//! - `actor`: a character or enemy placed in the map, seen from about 200 units.
//! - `fx`: an authored emitter or torch (else a lamp), seen from about 220 units.
//! - `water`: the centre of the largest liquid surface over 20000 square units, if any.
//! - `far2`: on maps with fewer than four views so far, more open views 1500 units apart.
use crate::{assets::Assets, bsp::Bsp, collision::World, interaction::vector};
use anyhow::Result;
use macroquad::prelude::*;

pub(crate) const MAPS: [&str; 36] = [
    "centipede1", "centipede2", "facade", "fortress1", "fortress2", "funhouse", "garden1", "garden2",
    "garden3", "garden4", "grounds1", "grounds2", "gvillage", "hatter1", "hatter2", "hedge1", "hedge2",
    "hedge3", "jlair1", "jlair2", "keep", "pandemonium", "potears1", "potears2", "potears3", "qlair",
    "rchess1", "skool1", "skool2", "tower1", "tower2", "tower3", "utemple", "wchess1", "wchess2", "wforest",
];
const SIGHT_CAP: f32 = 8000.;
const SURF_SKY: i32 = 0x4;

fn sight(world: &World, from: Vec3, dir: Vec3, cap: f32) -> f32 {
    world.sweep(from, from + dir * cap, Vec3::ZERO).fraction * cap
}

fn heading(yaw: f32, pitch: f32) -> Vec3 {
    vec3(yaw.cos() * pitch.cos(), yaw.sin() * pitch.cos(), pitch.sin())
}

fn clear(world: &World, a: Vec3, b: Vec3) -> bool {
    let t = world.sweep(a, b, Vec3::splat(4.));
    !t.start_solid && t.fraction >= 0.999
}

/// A camera about `range` units from `subject`, on the clearest of 16 headings.
fn around(world: &World, subject: Vec3, range: f32, prefer: Vec3) -> Option<(Vec3, Vec3)> {
    let mut best: Option<(f32, Vec3)> = None;
    for i in 0..16 {
        let yaw = i as f32 * std::f32::consts::TAU / 16.;
        let eye = subject + vec3(yaw.cos() * range, yaw.sin() * range, range * 0.35);
        if !clear(world, subject, eye) {
            continue;
        }
        // Prefer headings whose view past the subject is open and that face away from walls.
        let back = sight(world, eye, (subject - eye).normalize(), 3000.);
        let toward = (eye - subject).normalize().dot((prefer - subject).normalize_or_zero());
        let score = back + toward * 200.;
        if best.map_or(true, |(s, _)| score > s) {
            best = Some((score, eye));
        }
    }
    best.map(|(_, eye)| (eye, subject))
}

fn origin(e: &std::collections::BTreeMap<String, String>) -> Option<Vec3> {
    e.get("origin").and_then(|s| vector(s))
}

fn pick_map(assets: &mut Assets, name: &str) -> Result<serde_json::Value> {
    let map = Bsp::parse(&assets.read(&format!("maps/{name}.bsp"))?)?;
    let world = World::footing(&map, None)?;
    let (spawn_eye, yaw) = crate::interaction::spawn(&map, None);
    let player = crate::movement::Player::spawn(&world, spawn_eye);
    let feet = player.as_ref().map_or(spawn_eye - Vec3::Z * crate::movement::EYE_HEIGHT, |p| p.feet);
    let direction = vec3(yaw.cos(), yaw.sin(), 0.);
    let mut views = Vec::new();
    fn push(views: &mut Vec<serde_json::Value>, name: &str, eye: Vec3, target: Vec3, note: String) {
        views.push(serde_json::json!({"name": name, "eye": eye.to_array(), "target": target.to_array(), "note": note}));
    }
    let (eye, target, _) = crate::camera::Follow::default().update(&world, feet, direction, 132., 1. / 60.);
    push(&mut views, "start", eye, target, format!("follow camera at the player start, yaw {:.0}", yaw.to_degrees()));

    // Far: path nodes and the start eye, 16 headings, a fan of five rays at a small upward tilt.
    let mut nodes: Vec<Vec3> = map.entities.iter()
        .filter(|e| e.get("classname").is_some_and(|c| c == "info_pathnode"))
        .filter_map(origin).map(|o| o + Vec3::Z * 64.).collect();
    let stride = (nodes.len() / 400).max(1);
    nodes = nodes.into_iter().step_by(stride).collect();
    nodes.push(spawn_eye);
    // Best heading per eye, kept for a second far view on maps with few other subjects.
    let mut fars: Vec<(f32, Vec3, f32)> = Vec::new();
    for &eye in &nodes {
        let mut far: Option<(f32, Vec3, f32)> = None;
        if world.sweep(eye, eye, Vec3::splat(4.)).start_solid {
            continue;
        }
        let up = sight(&world, eye, Vec3::Z, 4000.);
        for i in 0..16 {
            let yaw = i as f32 * std::f32::consts::TAU / 16.;
            let mut sum = 0.;
            for spread in [-0.42_f32, -0.21, 0., 0.21, 0.42] {
                sum += sight(&world, eye, heading(yaw + spread, 0.07), SIGHT_CAP);
                sum += sight(&world, eye, heading(yaw + spread, 0.30), SIGHT_CAP) * 0.5;
            }
            let score = sum / 7.5 + up.min(2000.) * 0.25;
            if far.map_or(true, |(s, _, _)| score > s) {
                far = Some((score, eye, yaw));
            }
        }
        fars.extend(far);
    }
    fars.sort_by(|a, b| b.0.total_cmp(&a.0));
    if let Some(&(score, eye, yaw)) = fars.first() {
        push(&mut views, "far", eye, eye + heading(yaw, 0.1) * 1000., format!("longest sight lines, mean score {score:.0} units"));
    }

    let class = |e: &std::collections::BTreeMap<String, String>| e.get("classname").cloned().unwrap_or_default();
    let nearest = |filter: &dyn Fn(&str) -> bool| -> Option<(String, Vec3)> {
        map.entities.iter()
            .filter(|e| filter(&class(e)))
            .filter_map(|e| origin(e).map(|o| (class(e), o, e.contains_key("targetname"))))
            // Placed actors without a spawn trigger are more likely to stand there at the start.
            .min_by(|a, b| (a.2, a.1.distance(feet)).partial_cmp(&(b.2, b.1.distance(feet))).unwrap())
            .map(|(c, o, _)| (c, o))
    };
    if let Some((c, o)) = nearest(&|c| c.starts_with("Enemies_") || c.starts_with("Characters_")) {
        if let Some((eye, target)) = around(&world, o + Vec3::Z * 40., 200., feet) {
            push(&mut views, "actor", eye, target, format!("{c} at {:.0} {:.0} {:.0}", o.x, o.y, o.z));
        }
    }
    let fx = |c: &str| (c.starts_with("emitter_") && !c.contains("LensFlare")) || c.contains("torch") || c.contains("Torch");
    // Maps without an emitter or torch get a lamp, then a lens flare.
    if let Some((c, o)) = nearest(&fx).or_else(|| nearest(&|c| c.starts_with("lamp_")))
        .or_else(|| nearest(&|c| c.starts_with("emitter_"))) {
        if let Some((eye, target)) = around(&world, o, 220., feet) {
            push(&mut views, "fx", eye, target, format!("{c} at {:.0} {:.0} {:.0}", o.x, o.y, o.z));
        }
    }
    // Water: the largest liquid surface by triangle area.
    let mut water: Option<(f32, Vec3, String)> = None;
    for s in &map.surfaces {
        let shader = &map.shaders[s.shader];
        if shader.contents & crate::collision::LIQUID_MASK == 0 || shader.flags & SURF_SKY != 0 {
            continue;
        }
        let (mut area, mut centre) = (0., Vec3::ZERO);
        for tri in map.indices[s.first_index..s.first_index + s.index_count].chunks_exact(3) {
            let p = |i: usize| map.vertices[s.first_vertex + i].position;
            let (a, b, c) = (p(tri[0]), p(tri[1]), p(tri[2]));
            let t = (b - a).cross(c - a).length() * 0.5;
            area += t;
            centre += (a + b + c) / 3. * t;
        }
        // 128 by 128 patches (16384 square units) are pools, sinks or hidden cauldron tops.
        if area > 20000. && water.as_ref().map_or(true, |(w, _, _)| area > *w) {
            water = Some((area, centre / area, shader.name.clone()));
        }
    }
    if let Some((area, centre, shader)) = water {
        if let Some((eye, target)) = around(&world, centre + Vec3::Z * 8., 300., feet) {
            push(&mut views, "water", eye, target, format!("{shader}, {area:.0} square units"));
        }
    }
    // At least four views: the next best far views at least 1500 units from the first.
    let mut taken: Vec<Vec3> = fars.first().map(|f| f.1).into_iter().collect();
    for &(score, eye, yaw) in fars.iter().skip(1) {
        if views.len() >= 4 {
            break;
        }
        if taken.iter().all(|t| t.distance(eye) > 1500.) {
            taken.push(eye);
            push(&mut views, "far2", eye, eye + heading(yaw, 0.1) * 1000., format!("another open view, mean score {score:.0} units"));
        }
    }
    let sky = map.surfaces.iter().filter(|s| map.shaders[s.shader].flags & SURF_SKY != 0).count();
    Ok(serde_json::json!({
        "map": name, "spawn_eye": spawn_eye.to_array(), "yaw_degrees": yaw.to_degrees(),
        "feet": feet.to_array(), "player_spawned": player.is_some(), "sky_surfaces": sky, "views": views,
    }))
}

pub fn pick(assets: &mut Assets, out: &std::path::Path) -> Result<()> {
    let mut maps = Vec::new();
    for name in MAPS {
        let picked = pick_map(assets, name)?;
        println!("[lg/fidelity-view] {picked}");
        maps.push(picked);
    }
    std::fs::write(out, serde_json::to_vec_pretty(&maps)?)?;
    Ok(())
}
