//! Fixed Hollow Hideaway water views through whichever world renderer the build and config select.
//! `--water-view-capture DIR` writes one PNG per view and time and prints how many pixels change
//! between the two times. The views are the original `--water-render-check` views, so the same
//! spots can be compared with the desktop renderer.
use super::*;

pub(crate) const VIEWS: [(&str, Vec3, Vec3); 2] = [
    ("hollow-bank", vec3(-4600., 1800., 300.), vec3(-4250., 2400., 180.)),
    ("hollow-falls", vec3(-3970., 1750., 290.), vec3(-4460., 2480., 300.)),
];
pub(crate) const TIMES: [(&str, f32); 2] = [("t0.75", 0.75), ("t2.00", 2.)];

fn draw(scene: &mut Scene, camera: &Camera3D, time: f32) {
    clear_background(scene.atmosphere.background());
    set_camera(camera);
    crate::lighting::select(vec![], camera.position, &scene.world);
    crate::render_fx::begin_view(camera, time, &scene.atmosphere, false);
    #[cfg(feature = "vita_renderer")]
    crate::render_fx::retained::host_reset();
    scene.draw(camera.position, time, false, false, &[]);
    if scene.retained_active() {
        scene.draw(camera.position, time, false, true, &[]);
    } else {
        depth_read_only(|| scene.draw(camera.position, time, false, true, &[]));
    }
    crate::render_fx::finish();
    set_default_camera();
}

pub async fn capture(assets: &mut Assets, out: &std::path::Path) -> Result<()> {
    let mut scene = Scene::load(assets, "potears2")?;
    println!("[lg/water-view] renderer={}", if scene.retained_active() { "vita" } else { "desktop" });
    for (name, eye, target) in VIEWS {
        let camera = Camera3D { position: eye, target, up: Vec3::Z,
            fovy: 75_f32.to_radians(), z_near: 2., z_far: 30000., ..Default::default() };
        let mut images = Vec::new();
        for (label, time) in TIMES {
            // Warm the pipeline and render target before the frame that is kept.
            draw(&mut scene, &camera, time);
            next_frame().await;
            draw(&mut scene, &camera, time);
            images.push(get_screen_data());
            crate::viewer::save_capture(&out.join(format!("{name}-{label}.png")))?;
            next_frame().await;
        }
        let changed = images[0].bytes.chunks_exact(4).zip(images[1].bytes.chunks_exact(4))
            .filter(|(a, b)| a[..3].iter().zip(&b[..3]).any(|(x, y)| x.abs_diff(*y) >= 3)).count();
        println!("[lg/water-view] view={name} pixels={} changed_between_times={changed} draws={} triangles={}",
            images[0].bytes.len() / 4, scene.stats.batches, scene.stats.triangles);
    }
    Ok(())
}
