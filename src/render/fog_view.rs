//! Fixed views of maps with authored fog through whichever world renderer the build and config
//! select. `--fog-view-capture DIR` writes one PNG per view, so the same spots can be compared
//! between the desktop renderer, the Vita renderer in `quality: full` and in `quality: low`.
//! The potears2 eyes stand 80 units above player positions recorded on the console (tape
//! `a2-04`); the other maps use their first `info_player_start`. Each eye looks level at
//! the listed yaws.
use super::*;

const EYE_HEIGHT: f32 = 80.;
const TIMED_DRAWS: usize = 60;
/// Map, view name, player position, yaws in degrees (0 = +X, 90 = +Y).
pub(crate) const VIEWS: [(&str, &str, Vec3, &[f32]); 6] = [
    ("potears2", "start", vec3(-4592., 1792., 224.), &[0., 90., 180., 270.]),
    ("potears2", "bank", vec3(-3283.73, 583.52, -97.14), &[0., 90., 180., 270.]),
    ("potears2", "basin", vec3(-1987.39, 1398.15, -191.97), &[0., 90., 180., 270.]),
    ("garden2", "start", vec3(2424., -2576., -136.), &[60., 240.]),
    ("wforest", "start", vec3(4528., 460., 296.), &[45., 225.]),
    ("utemple", "start", vec3(-64., -5592., 1480.), &[270., 90.]),
];

fn draw(scene: &mut Scene, camera: &Camera3D) {
    clear_background(scene.atmosphere.background());
    set_camera(camera);
    crate::lighting::select(vec![], camera.position, &scene.world);
    crate::render_fx::begin_view(camera, 1., &scene.atmosphere, false);
    #[cfg(feature = "vita_renderer")]
    crate::render_fx::retained::host_reset();
    scene.draw(camera.position, 1., false, false, &[]);
    if scene.retained_active() {
        scene.draw(camera.position, 1., false, true, &[]);
    } else {
        depth_read_only(|| scene.draw(camera.position, 1., false, true, &[]));
    }
    crate::render_fx::finish();
    set_default_camera();
}

pub async fn capture(assets: &mut Assets, out: &std::path::Path) -> Result<()> {
    std::fs::create_dir_all(out)?;
    let mut loaded: Option<(&str, Scene)> = None;
    for (map, name, player, yaws) in VIEWS {
        if loaded.as_ref().map(|(m, _)| *m) != Some(map) {
            loaded = None;
            let scene = Scene::load(assets, map)?;
            let a = &scene.atmosphere;
            #[cfg(feature = "vita_renderer")]
            let quality = crate::render::vita_renderer::config().quality.as_str();
            #[cfg(not(feature = "vita_renderer"))]
            let quality = "none";
            println!("[lg/fog-view] map={map} renderer={} distance_fog={:?} volumes={} quality={quality}",
                if scene.retained_active() { "vita" } else { "desktop" }, a.distance.to_array(), a.volumes.len());
            loaded = Some((map, scene));
        }
        let scene = &mut loaded.as_mut().unwrap().1;
        let eye = player + vec3(0., 0., EYE_HEIGHT);
        for &yaw in yaws {
            let r = yaw.to_radians();
            let camera = Camera3D { position: eye, target: eye + vec3(r.cos(), r.sin(), 0.) * 1000., up: Vec3::Z,
                fovy: 75_f32.to_radians(), z_near: 2., z_far: 30000., ..Default::default() };
            // Warm the pipeline and render target before the frame that is kept.
            draw(scene, &camera);
            next_frame().await;
            draw(scene, &camera);
            crate::viewer::save_capture(&out.join(format!("{map}-{name}-y{yaw:03}.png")))?;
            next_frame().await;
            // Host wall time of the whole view with the GPU drained, as a rough A/B between configs.
            // A desktop GPU is not the Vita's; the console figure needs `gpu_timing` on the Vita.
            let mut samples: Vec<f64> = (0..TIMED_DRAWS).map(|_| {
                let start = std::time::Instant::now();
                draw(scene, &camera);
                unsafe { macroquad::miniquad::gl::glFinish() };
                start.elapsed().as_secs_f64() * 1000.
            }).collect();
            samples.sort_by(f64::total_cmp);
            next_frame().await;
            println!("[lg/fog-view] view={map}-{name}-y{yaw:03} draws={} triangles={} host_ms_median={:.3} host_ms_min={:.3}",
                scene.stats.batches, scene.stats.triangles, samples[samples.len() / 2], samples[0]);
        }
    }
    Ok(())
}
