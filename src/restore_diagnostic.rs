//! One-shot load/restore observation. Queries never flush, bind or repair GL state.
use macroquad::prelude::*;
use std::time::Instant;

pub struct Probe {
    reason: &'static str,
    start: Instant,
    world_draws: usize,
    triangles: usize,
    retained: bool,
}
impl Probe {
    pub fn new(reason: &'static str) -> Self {
        eprintln!("[lg/load-diagnostic] reason={reason} stage=armed");
        Self { reason, start: Instant::now(), world_draws: 0, triangles: 0, retained: false }
    }
    pub fn stage(&self, stage: &str) {
        eprintln!("[lg/load-diagnostic] reason={} stage={stage} wall_ms={:.3}", self.reason, self.start.elapsed().as_secs_f64()*1000.);
    }
    pub fn camera(&self, camera: &Camera3D, feet: Vec3, paused: bool, scene: Option<&str>) {
        eprintln!("[lg/load-diagnostic] reason={} camera_eye={:?} camera_target={:?} camera_up={:?} feet={feet:?} fovy={} paused={paused} cinema_owner=engine scene={scene:?}",
            self.reason, camera.position, camera.target, camera.up, camera.fovy);
    }
    pub fn world(&mut self, draws: usize, triangles: usize, retained: bool) {
        self.world_draws = draws; self.triangles = triangles; self.retained = retained;
        self.gl("world-returned");
    }
    pub fn gl(&self, stage: &str) {
        use macroquad::miniquad::gl::*;
        // All queries describe the current target; no state reset/readback is hidden here.
        let (error_before, framebuffer, viewport, status, error_after) = unsafe {
            let error_before = glGetError();
            let mut framebuffer = 0;
            let mut viewport = [0; 4];
            glGetIntegerv(0x8CA6, &mut framebuffer);
            glGetIntegerv(0x0BA2, viewport.as_mut_ptr());
            let status = glCheckFramebufferStatus(0x8D40);
            (error_before, framebuffer, viewport, status, glGetError())
        };
        eprintln!("[lg/load-diagnostic] reason={} stage={stage} framebuffer_binding={framebuffer} viewport={viewport:?} framebuffer_status={status:#x} gl_error_before={error_before:#x} gl_error_after={error_after:#x} scale_target={:?} render_scale={}",
            self.reason, crate::render::vita_renderer::diagnostic_target(), crate::render::vita_renderer::config().render_scale);
    }
    pub fn finish(&self, map: &str, hud_submitted: bool) {
        self.gl("hud-queued");
        eprintln!("[lg/load-diagnostic] reason={} map={map} end=frame-queued retained={} world_draws={} triangles={} world_submitted={} hud_submitted={hud_submitted} flush_forced=false wall_ms={:.3}",
            self.reason, self.retained, self.world_draws, self.triangles, self.world_draws > 0, self.start.elapsed().as_secs_f64()*1000.);
    }
}
