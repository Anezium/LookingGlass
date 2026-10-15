//! Save-free, deterministic world comparison at each chapter's initial gameplay pose.
//! Does not replay opening cinematics or claim full gameplay/actor parity.
use super::*;

pub(crate) async fn entry(mut assets: crate::assets::Assets, options: crate::Options) -> Result<()> {
    macroquad::rand::srand(0x416c696365);
    let loaded = crate::campaign::load_visit(&mut assets, &options.map, options.entry.as_deref(), None, options.difficulty, false)?;
    let crate::campaign::Loaded { map, world, mut interactions, .. } = loaded;
    let mut scene = Scene::from_parts(&mut assets, &options.map, map, world)?;
    interactions.sync(&mut scene.world);
    let (spawn, yaw) = crate::interaction::spawn(&scene.map, options.entry.as_deref());
    let player = crate::movement::Player::spawn(&scene.world, spawn);
    let direction = vec3(yaw.cos(), yaw.sin(), 0.);
    let (eye, target, _) = if let Some(player) = &player {
        crate::camera::Follow::default().update(&scene.world, player.feet, direction, 132., 1. / 60.)
    } else { (spawn, spawn + direction, false) };
    let camera = Camera3D { position: eye, target, up: Vec3::Z, fovy: 75_f32.to_radians(), z_near: 2., z_far: 30000., ..Default::default() };
    record_view(&camera);
    let mut transforms = interactions.transforms();
    if let Some(player) = &player { transforms.extend(scene.world.traversal.transforms(player)); }
    let specs = crate::texture::read_materials(&mut assets)?;
    let portal_surfaces = scene.map.surfaces.iter().filter(|s| specs.get(&scene.map.shaders[s.shader].name.to_ascii_lowercase()).is_some_and(|s| s.camera_portal)).count();
    let sky_surfaces = scene.map.surfaces.iter().filter(|s| specs.get(&scene.map.shaders[s.shader].name.to_ascii_lowercase()).is_some_and(|s| s.sky)).count();
    // A fixed material clock makes frozen deformations/ambient poses visible without
    // letting gameplay, input, loading duration or wall time choose the scene state.
    let time = 1.0;
    let frames = options.frames.unwrap_or(3).max(1);
    for frame in 0..frames {
        clear_background(scene.atmosphere.background());
        prepare_3d(scene.atmosphere.background());
        set_3d_camera(&camera);
        scene.prepare_camera_portals(&camera, time, false, &transforms);
        set_3d_camera(&camera);
        scene.begin_fx_view(&camera, time, false, true);
        scene.draw(eye, time, false, false, &transforms);
        if scene.retained_active() {
            scene.draw(eye, time, false, true, &transforms);
        } else {
            crate::render::depth_read_only(|| scene.draw(eye, time, false, true, &transforms));
        }
        scene.finish_fx(true);
        present_3d();
        set_default_camera();
        if frame + 1 == frames {
            let result = serde_json::json!({"map":options.map,"entry":options.entry,"renderer":if scene.retained_active(){"vita"}else{"legacy"},
                "seed":0x416c696365u64,"material_time":time,"frames":frames,"scope":"world + ambient props at initial chapter gameplay spawn; no cast/HUD/cinematic replay",
                "spawn_eye":spawn.to_array(),"player_feet":player.as_ref().map(|p|p.feet.to_array()),"yaw":yaw,"transforms":transforms,
                "portal_candidate_surfaces":portal_surfaces,"authored_sky_surfaces":sky_surfaces,"rope_strands":scene.world.traversal.ropes.len(),
                "legacy_portals_loaded":scene.camera_portals.len(),"missing_images":scene.missing,"stats":scene.stats,"report":report()});
            std::fs::create_dir_all("private")?;
            std::fs::write("private/allmaps-entry.json", serde_json::to_vec_pretty(&result)?)?;
            if let Some(path) = &options.capture { crate::viewer::save_capture(path)?; }
            println!("[lg/allmaps] entry {result}");
        }
        next_frame().await;
    }
    Ok(())
}
