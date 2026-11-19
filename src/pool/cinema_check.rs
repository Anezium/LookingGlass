//! Real-asset scene contacts, save continuation, skip and native staging probes.
use super::*;
use crate::{assets::Assets, interaction::Interactions, story::Story};

pub(super) fn trigger(map: &Bsp, name: &str) -> Vec3 {
    let e = map
        .entities
        .iter()
        .find(|e| e.get("thread").is_some_and(|n| n == name))
        .unwrap();
    let model: usize = e["model"].trim_start_matches('*').parse().unwrap();
    vector(&e["origin"]).unwrap() + (map.models[model].min + map.models[model].max) * 0.5
        - PLAYER_CENTER
}
pub(super) fn setup(
    assets: &mut Assets,
    map: &Bsp,
    name: &str,
) -> Result<(Interactions, World, Player, Story)> {
    let mut world = World::from_bsp(map)?;
    let mut i = Interactions::load(map)?;
    i.set_entry(assets, map, "potears1", None)?;
    i.sync(&mut world);
    let mut story = Story::load(assets, "potears1");
    let mut player = Player::spawn(&world, crate::interaction::spawn(map, None).0)
        .context("Pool start obstructed")?;
    if name == cinema::ENTRY {
        i.entry_story(&mut story);
    } else {
        // Later scene fixtures inherit the real arrival's world commitment.
        i.entry_story(&mut story);
        i.skip_cinematic(map, &mut world, &mut player, &mut story)?;
        let feet = trigger(map, name);
        player = Player::spawn(&world, feet).unwrap_or_else(|| Player::new(feet));
        let e = i.triggers(0.01, feet, feet);
        for id in e.story {
            story.trigger(&id);
        }
        ensure!(
            i.pool.as_ref().unwrap().scene_id() == Some(name),
            "Pool contact did not start {name}"
        );
    }
    Ok((i, world, player, story))
}
pub(super) fn tick(
    i: &mut Interactions,
    map: &Bsp,
    world: &mut World,
    player: &mut Player,
    story: &mut Story,
    dt: f32,
) -> Result<usize> {
    i.advance_school(dt, map, world, player)?;
    let e = i.update(dt, map, world, player, Vec3::X, false)?;
    if i.prepare_story(story) {
        story.tick(dt, false);
    }
    i.sync_cinematic_story(story);
    for id in story.take_completed() {
        i.completed_dialogue(&id);
    }
    if let Some(exit) = e.transition {
        ensure!(
            exit == ("potears2".into(), Some("potears2_start1".into())),
            "Incorrect Pool exit"
        );
        Ok(1)
    } else {
        Ok(0)
    }
}
pub fn check(assets: &mut Assets, map: &Bsp) -> Result<()> {
    for name in [
        cinema::ENTRY,
        "Tears1_Boulder1",
        "Tears1_Boulder2",
        TALK,
        cinema::EXIT,
    ] {
        for skip_at in [None, Some(0.), Some(2.)] {
            let (mut i, mut world, mut player, mut story) = setup(assets, map, name)?;
            let hints = crate::cheshire::Hints::load(assets, map, "potears1")?;
            let paused = serde_json::to_value(i.snapshot())?;
            i.advance_school(0., map, &mut world, &mut player)?;
            ensure!(
                paused == serde_json::to_value(i.snapshot())?,
                "Pause advanced Pool scene"
            );
            let mut exits = 0;
            let mut skipped = false;
            let mut restored = false;
            for frame in 0..36_000 {
                let dt = 1. / 60.;
                if skip_at.is_some_and(|at| frame as f32 * dt >= at) && !skipped {
                    ensure!(
                        i.skip_cinematic(map, &mut world, &mut player, &mut story)?,
                        "Skip rejected {name}"
                    );
                    skipped = true;
                }
                exits += tick(&mut i, map, &mut world, &mut player, &mut story, dt)?;
                let p = i.pool.as_ref().unwrap();
                if let Some(camera) = p.camera() {
                    ensure!(
                        camera.eye.is_finite()
                            && camera.target.is_finite()
                            && camera.up.is_finite()
                            && camera.eye.distance(camera.target) > 1.,
                        "Invalid Pool camera"
                    );
                }
                if frame == 85 {
                    let before = p.camera();
                    let saved = serde_json::from_value(serde_json::to_value(i.snapshot())?)?;
                    i.restore(&saved, map)?;
                    i.sync(&mut world);
                    player = serde_json::from_value(serde_json::to_value(&player)?)?;
                    player.validate_world(&world)?;
                    story.restore(
                        &serde_json::from_value(serde_json::to_value(story.snapshot())?)?,
                        &hints,
                    )?;
                    if let Some(a) = before {
                        let b = i
                            .pool
                            .as_ref()
                            .unwrap()
                            .camera()
                            .context("Saved camera lost")?;
                        ensure!(
                            a.eye.distance(b.eye) < 0.001 && a.target.distance(b.target) < 0.001,
                            "Saved camera changed"
                        );
                    }
                    restored = true;
                }
                if (name == cinema::EXIT && exits > 0) || !i.scripted() {
                    break;
                }
            }
            ensure!(
                (name == cinema::EXIT && exits == 1)
                    || (name != cinema::EXIT && !i.scripted() && world.body_clear(player.feet)),
                "Pool scene failed to hand off {name}"
            );
            ensure!(
                skip_at == Some(0.) || restored,
                "Save checkpoint not reached"
            );
            if name == TALK {
                ensure!(
                    story.completed == 1 && !story.busy(),
                    "Turtle dialogue repeated or unfinished"
                );
                ensure!(
                    i.pool.as_ref().unwrap().state.talked,
                    "Turtle commitment missing"
                );
                for _ in 0..600 {
                    tick(&mut i, map, &mut world, &mut player, &mut story, 1. / 60.)?;
                }
                ensure!(
                    i.pool.as_ref().unwrap().ready(0),
                    "Turtle did not release first leaf"
                );
            }
            for _ in 0..5 {
                exits += tick(&mut i, map, &mut world, &mut player, &mut story, 1. / 60.)?;
            }
            ensure!(
                exits == usize::from(name == cinema::EXIT),
                "Duplicate scene exit"
            );
            ensure!(
                !i.skip_cinematic(map, &mut world, &mut player, &mut story)?,
                "Completed scene skipped again"
            );
            println!("PASS Pool scene {name}, skip={skip_at:?}: real contact, pause, saved camera/player/dialogue, landing, one-shot commitment");
        }
    }
    // A pre-scene v12 save keeps the current position/transport; old pending scene
    // contacts become usable, while arrival and completed dialogue do not replay.
    let (mut i, mut world, mut player, mut story) = setup(assets, map, "Tears1_Boulder1")?;
    let mut saved = serde_json::to_value(i.snapshot())?;
    saved["pool"].as_object_mut().unwrap().remove("cinema");
    i.restore(&serde_json::from_value(saved)?, map)?;
    i.sync(&mut world);
    let before = player.feet;
    i.entry_story(&mut story);
    ensure!(
        !i.scripted() && player.feet == before,
        "Old Pool save replayed arrival"
    );
    let at = trigger(map, "Tears1_Boulder1");
    i.triggers(0.01, at, at);
    ensure!(i.scripted(), "Old consumed Pool contact not rearmed");
    i.skip_cinematic(map, &mut world, &mut player, &mut story)?;
    let mut invalid = i.pool.as_ref().unwrap().snapshot();
    invalid.cinema.time = -1.;
    ensure!(
        i.pool.as_mut().unwrap().restore(&invalid, map).is_err(),
        "Bad Pool scene timer accepted"
    );
    let mut clean = Interactions::load(map)?;
    clean.set_entry(assets, map, "potears1", None)?;
    let exit = map
        .entities
        .iter()
        .find(|e| e.get("targetname").is_some_and(|s| s == "end_of_level"))
        .unwrap();
    let id: usize = exit["model"].trim_start_matches('*').parse()?;
    let at = vector(&exit["origin"]).unwrap() + (map.models[id].min + map.models[id].max) * 0.5
        - PLAYER_CENTER;
    ensure!(
        clean.triggers(0.01, at, at).transition.is_none(),
        "Exit volume bypassed jump scene"
    );
    println!("PASS Pool legacy consumed contacts, unchanged player, no arrival replay, bad-save rejection and exit volume gate");
    Ok(())
}

pub async fn render(assets: &mut Assets) -> Result<()> {
    let crate::render::Loaded { mut scene, world: _world } = crate::render::Scene::load(assets, "potears1")?;
    let pool = Pool::load(assets, &scene.map)?;
    let mut art = Art::load(assets, &pool)?;
    for (name, mut seconds) in [
        (cinema::ENTRY, 0.),
        (cinema::ENTRY, 4.5),
        (cinema::ENTRY, 6.),
        (cinema::ENTRY, 9.),
        (cinema::ENTRY, 11.5),
        ("Tears1_Boulder1", 1.5),
        ("Tears1_Boulder2", 1.5),
        (TALK, 4.),
        (TALK, 35.),
        (TALK, 75.),
        ("turtle_departure", 0.),
        (cinema::EXIT, 1.5),
        (cinema::EXIT, 0.25),
        (cinema::EXIT, 2.75),
        (cinema::EXIT, 4.75),
    ] {
        let (mut i, mut world, mut player, mut story) = setup(
            assets,
            &scene.map,
            if name == "turtle_departure" {
                TALK
            } else {
                name
            },
        )?;
        if name == "turtle_departure" {
            story.finish_sequence(TALK);
            i.completed_dialogue(TALK);
            seconds = i.pool.as_ref().unwrap().depart_ride() + 2.;
        }
        for _ in 0..(seconds * 60.) as usize {
            tick(
                &mut i,
                &scene.map,
                &mut world,
                &mut player,
                &mut story,
                1. / 60.,
            )?;
        }
        art.cinema.story_pose(&story);
        let camera = i
            .pool
            .as_ref()
            .unwrap()
            .camera()
            .context("Missing Pool render camera")?;
        for frame in 0..3 {
            clear_background(BLACK);
            set_camera(&Camera3D {
                position: camera.eye,
                target: camera.target,
                up: camera.up,
                fovy: 75_f32.to_radians(),
                z_near: 2.,
                z_far: 20000.,
                ..Default::default()
            });
            let poses = i.transforms();
            scene.draw(camera.eye, seconds, false, false, &poses);
            art.draw(
                i.pool.as_ref().unwrap(),
                false,
                &scene.atmosphere,
                camera.eye,
            );
            crate::render::depth_read_only(|| scene.draw(camera.eye, seconds, false, true, &poses));
            set_default_camera();
            if frame == 2 {
                crate::viewer::save_capture(std::path::Path::new(&format!(
                    "private/pool-scene-{name}-{seconds}.png"
                )))?;
            }
            next_frame().await;
        }
        println!("PASS Pool rendered {name} at {seconds}");
    }
    Ok(())
}

/// Real scene/controller/asset control for the owned render return transaction.
/// This intentionally samples only selected rendered frames, so a scene ending
/// between samples must resume from the last draw rather than its current clock.
pub async fn snapshot_handoff_check(assets: &mut Assets) -> Result<()> {
    use crate::render_snapshot::{RenderVisit, HandoffReturn, HandoffCompletion,
        handoff_render_begin, handoff_render_end};
    let crate::render::Loaded { scene, world: _world } = crate::render::Scene::load(assets, "potears1")?;
    let mut control_character = crate::character::Character::load(assets)?;
    let mut extracted_character = crate::character::Character::load(assets)?;
    for skip_at in [None, Some(0usize), Some(120usize)] {
        for restore_at in [None, Some(90usize)] {
            let (mut i, mut world, mut player, mut story) = setup(assets, &scene.map, cinema::ENTRY)?;
            let mut art = super::Art::load(assets, i.pool.as_ref().unwrap())?;
            let mut visit = RenderVisit::default();
            let mut returns = HandoffReturn::default();
            let mut last_control: Option<crate::cinematic::ActorPose> = None;
            let mut finished = false;
            let mut draws = 0;
            for frame in 0..1800usize {
                ensure!(returns.consume(&visit), "Missing render acknowledgement");
                if restore_at == Some(frame) {
                    let saved = i.snapshot();
                    let mut restored = Interactions::load(&scene.map)?;
                    restored.set_entry(assets, &scene.map, "potears1", None)?;
                    restored.restore(&saved, &scene.map)?;
                    i = restored; i.sync(&mut world);
                    art = super::Art::load(assets, i.pool.as_ref().unwrap())?;
                    visit = RenderVisit::default();
                    ensure!(returns.consume(&visit) && returns.pose(&visit).is_none(), "Restore retained old rendered pose");
                    last_control = None;
                }
                if skip_at == Some(frame) { ensure!(i.skip_cinematic(&scene.map, &mut world, &mut player, &mut story)?, "Skip rejected"); }
                tick(&mut i, &scene.map, &mut world, &mut player, &mut story, 1./60.)?;
                if !i.scripted() {
                    let yaw = player.script_facing;
                    control_character.resume_scene(&player, yaw, last_control.as_ref());
                    extracted_character.resume_scene(&player, yaw, returns.pose(&visit));
                    ensure!(serde_json::to_value(control_character.snapshot())? == serde_json::to_value(extracted_character.snapshot())?, "Actual Character handoff changed");
                    finished = true; break;
                }
                // Model menu iterations with no renderer submission, and paused
                // iterations with dt=0. Neither may replace the last local pose.
                if frame == 75 {
                    let before = serde_json::to_value(i.snapshot())?;
                    for _ in 0..3 {
                        ensure!(returns.consume(&visit), "Menu lost completion");
                        i.advance_school(0., &scene.map, &mut world, &mut player)?;
                    }
                    ensure!(before == serde_json::to_value(i.snapshot())?, "Paused controller advanced");
                }
                if frame % 30 != 0 { continue; }
                let camera = i.pool.as_ref().unwrap().camera().unwrap();
                let view = Camera3D { position: camera.eye, target: camera.target, up: camera.up, ..Default::default() };
                set_camera(&view);
                art.cinema.story_pose(&story);
                art.clear_input();
                art.draw(i.pool.as_ref().unwrap(), false, &scene.atmosphere, camera.eye);
                last_control = art.cinema.alice.handoff_pose().cloned();
                let (_, overflow) = art.extract(i.pool.as_ref().unwrap());
                ensure!(!overflow, "Control exceeded Pool input limits");
                // Rejection is whole-batch and happens before any GPU publication.
                let generation = art.snapshot_input.as_ref().unwrap().generation;
                art.snapshot_input.as_mut().unwrap().generation = 0;
                ensure!(!art.publish_input(false, &scene.atmosphere, camera.eye), "Stale Pool registry accepted");
                art.snapshot_input.as_mut().unwrap().generation = generation;
                if let Some(slot) = art.snapshot_input.as_ref().unwrap().props.first().map(|p| p.slot) {
                    art.snapshot_input.as_mut().unwrap().props[0].slot = usize::MAX;
                    ensure!(!art.publish_input(false, &scene.atmosphere, camera.eye), "Invalid prop slot accepted");
                    art.snapshot_input.as_mut().unwrap().props[0].slot = slot;
                }
                let command_slot = art.snapshot_input.as_ref().unwrap().cinema.commands.iter().position(|c|
                    matches!(c, cinema::Command::Puppet(_, _)));
                if let Some(index) = command_slot {
                    let cinema::Command::Puppet(slot, _) = &mut art.snapshot_input.as_mut().unwrap().cinema.commands[index] else { unreachable!() };
                    let original = *slot; *slot = usize::MAX;
                    ensure!(!art.publish_input(false, &scene.atmosphere, camera.eye), "Invalid Puppet slot accepted");
                    let cinema::Command::Puppet(slot, _) = &mut art.snapshot_input.as_mut().unwrap().cinema.commands[index] else { unreachable!() };
                    *slot = original;
                }
                returns.expect(&visit, 4, frame as u64);
                handoff_render_begin(&visit, frame as u64);
                art.draw(i.pool.as_ref().unwrap(), false, &scene.atmosphere, camera.eye);
                handoff_render_end();
                let completion = HandoffCompletion::rendered(&visit, 4, frame as u64, art.cinema.alice.handoff_pose());
                ensure!(returns.publish(completion) && returns.consume(&visit), "Rendered return rejected");
                match (&last_control, returns.pose(&visit)) {
                    (None, None) => {},
                    (Some(a), Some(b)) => {
                        ensure!(a.local.len()==b.local.len() && a.scale.to_bits()==b.scale.to_bits(), "Local pose shape changed");
                        for (a,b) in a.local.iter().zip(&b.local).chain(std::iter::once((&a.transform,&b.transform))) {
                            ensure!(a.translation.to_array().map(f32::to_bits)==b.translation.to_array().map(f32::to_bits)
                                && a.rotation.to_array().map(f32::to_bits)==b.rotation.to_array().map(f32::to_bits), "Local pose bits changed");
                        }
                    }
                    _ => anyhow::bail!("No-pose semantics changed"),
                }
                draws += 1;
                next_frame().await;
            }
            ensure!(finished, "Arrival did not end");
            println!("PASS snapshot handoff skip={skip_at:?} restore={restore_at:?} draws={draws}: local bits, pause/menu, last draw, Character resume");
        }
    }
    Ok(())
}
