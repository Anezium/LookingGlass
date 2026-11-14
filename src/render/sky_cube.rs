//! `sky_cube`: the authored sky drawn once by the legacy path into six faces of one render target.
//!
//! Two sets of six faces can share the target, one 3 by 2 block each. `Room`: the sky room seen
//! from the sky camera, for the portal faces (whose own layers are invisible). `Layers`: the sky
//! room, then the sky shader of the other sky faces drawn on a closed cube around the eye, for
//! those faces. A direction can show the room through one face and a sky shader through another,
//! so one set cannot serve both.
//!
//! The legacy authored sky (`authored_sky`) redraws the sky room and the sky shader layers through
//! Macroquad every frame, with a CPU transform of every visible vertex. Seen from the sky camera,
//! that picture depends only on the view direction (the portal pose maps the eye to the sky origin
//! and only rotates), so one capture per sky camera holds it. The retained renderer then samples the
//! capture by direction on the map's sky surfaces (`vita_renderer::sky_surfaces`).
use macroquad::prelude::*;

/// (forward, up) of each face, in the atlas order +X -X +Y / -Y +Z -Z. Screen right is
/// forward x up, as `Mat4::look_at_rh` builds it.
pub(crate) const FACES: [(Vec3, Vec3); 6] = [
    (Vec3::X, Vec3::Z), (Vec3::NEG_X, Vec3::Z), (Vec3::Y, Vec3::Z),
    (Vec3::NEG_Y, Vec3::Z), (Vec3::Z, Vec3::X), (Vec3::NEG_Z, Vec3::X),
];
/// Maps whose level script moves the sky camera after load. Their legacy sky geometry stays so a
/// new sky camera can be captured again.
const MOVING_SKY_CAMERA: [&str; 6] = ["funhouse", "grounds1", "hedge1", "hedge3", "jlair1", "keep"];
/// A sky camera that moved less than this keeps its capture.
const ORIGIN_EPSILON: f32 = 0.5;
/// The fewest frames between two full captures after a sky camera move.
const RECAPTURE_FRAMES: u64 = 30;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Plan { Nothing, All, Face(usize) }
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Set { Room, Layers }

pub(crate) struct SkyCube {
    pub(crate) target: RenderTarget,
    #[cfg(not(target_os = "vita"))]
    depth: Option<macroquad::miniquad::TextureId>,
    pub(crate) size: u32,
    /// Row of faces where each set starts; `None` when the map has no such faces.
    room_row: Option<u32>,
    layers_row: Option<u32>,
    /// The sky camera of the last full capture; `None` before the first one.
    captured: Option<Option<Vec3>>,
    last_full: u64,
    frame: u64,
    next_face: usize,
    pub(crate) keep_geometry: bool,
    pub(crate) captures: u32,
    pub(crate) face_draws: u32,
    pub(crate) ignored_moves: u32,
    pub(crate) map: String,
    /// Index in `Scene::batches` of the sky shader drawn on the dome of `Set::Layers`.
    pub(crate) layer_batch: Option<usize>,
}
pub(crate) fn keeps_geometry(map: &str, refresh: u32) -> bool {
    refresh > 0 || MOVING_SKY_CAMERA.contains(&map)
}
impl SkyCube {
    pub(crate) fn new(map: &str, size: u32, keep_geometry: bool, room: bool, layers: bool) -> Self {
        let room_row = room.then_some(0);
        let layers_row = layers.then_some(if room { 2 } else { 0 });
        let rows = 2 * (u32::from(room) + u32::from(layers));
        // As the scaled 3D target (`scale.rs`): no Macroquad depth attachment, whose 16 bits make
        // nested sky room surfaces fight; the host attaches its own Depth32 image.
        let target = render_target_ex(3 * size, rows * size, RenderTargetParams { sample_count: 0, depth: false });
        target.texture.set_filter(FilterMode::Linear);
        #[cfg(not(target_os = "vita"))]
        let depth = Some(host_depth(&target, 3 * size, rows * size));
        Self { target,
            #[cfg(not(target_os = "vita"))]
            depth,
            size, room_row, layers_row, captured: None, last_full: 0, frame: 0, next_face: 0, keep_geometry,
            captures: 0, face_draws: 0, ignored_moves: 0, map: map.into(), layer_batch: None }
    }
    pub(crate) fn ready(&self) -> bool { self.captured.is_some() }
    /// What to draw this frame. Call once per frame, before the world.
    pub(crate) fn plan(&mut self, origin: Option<Vec3>, refresh: u32) -> Plan {
        self.frame += 1;
        let plan = plan(self.captured, origin, self.keep_geometry, refresh, self.frame, self.last_full, self.next_face);
        match plan {
            Plan::All => { self.captured = Some(origin); self.last_full = self.frame; self.captures += 1; }
            Plan::Face(face) => { self.next_face = (face + 1) % 6; self.face_draws += 1; }
            Plan::Nothing => {
                if self.captured.is_some_and(|c| moved(c, origin)) && !self.keep_geometry { self.ignored_moves += 1; }
            }
        }
        plan
    }
    pub(crate) fn sets(&self) -> impl Iterator<Item = Set> {
        self.room_row.map(|_| Set::Room).into_iter().chain(self.layers_row.map(|_| Set::Layers))
    }
    pub(crate) fn rows(&self) -> u32 { 2 * self.sets().count() as u32 }
    pub(crate) fn row(&self, set: Set) -> Option<u32> {
        match set { Set::Room => self.room_row, Set::Layers => self.layers_row }
    }
    pub(crate) fn viewport(&self, set: Set, face: usize) -> (i32, i32, i32, i32) {
        let s = self.size as i32;
        let row = self.row(set).unwrap_or(0) as i32 + (face / 3) as i32;
        ((face % 3) as i32 * s, row * s, s, s)
    }
    pub(crate) fn camera(&self, eye: Vec3, set: Set, face: usize, z_far: f32) -> Camera3D {
        let (forward, up) = FACES[face];
        Camera3D {
            position: eye, target: eye + forward, up,
            fovy: std::f32::consts::FRAC_PI_2, aspect: Some(1.),
            projection: Projection::Perspective,
            render_target: Some(self.target.clone()),
            viewport: Some(self.viewport(set, face)),
            z_near: 4., z_far,
        }
    }
}
#[cfg(not(target_os = "vita"))]
impl Drop for SkyCube {
    fn drop(&mut self) {
        if let Some(depth) = self.depth.take() { unsafe { get_internal_gl() }.quad_context.delete_texture(depth); }
    }
}
/// The host's Depth32 attachment, as `scale.rs` builds it for the scaled 3D target.
#[cfg(not(target_os = "vita"))]
fn host_depth(target: &RenderTarget, width: u32, height: u32) -> macroquad::miniquad::TextureId {
    use macroquad::miniquad::{gl, PassAction, RawId, TextureAccess, TextureFormat, TextureParams, TextureSource};
    let ctx = unsafe { get_internal_gl() }.quad_context;
    let depth = ctx.new_texture(TextureAccess::RenderTarget, TextureSource::Empty, TextureParams {
        width, height, format: TextureFormat::Depth32, sample_count: 1, ..Default::default()
    });
    ctx.begin_pass(Some(target.render_pass.raw_miniquad_id()), PassAction::Nothing);
    if let RawId::OpenGl(id) = unsafe { ctx.texture_raw_id(depth) } {
        unsafe {
            gl::glFramebufferTexture2D(0x8D40, 0x8D00, 0x0DE1, id, 0);
            assert_eq!(gl::glCheckFramebufferStatus(0x8D40), 0x8CD5, "incomplete sky cube depth target");
        }
    }
    ctx.end_render_pass();
    depth
}
/// A closed cube around `eye`, `n` by `n` quads a side, inward. The sky shader's projection is
/// computed per vertex, so a finer grid follows its curve more closely.
pub(crate) fn dome(eye: Vec3, half: f32, n: usize) -> (Vec<Vertex>, Vec<u16>) {
    let (mut vertices, mut indices) = (Vec::new(), Vec::new());
    for (forward, up) in FACES {
        let right = forward.cross(up);
        let base = vertices.len() as u16;
        for j in 0..=n {
            for i in 0..=n {
                let (u, v) = (i as f32 / n as f32 * 2. - 1., j as f32 / n as f32 * 2. - 1.);
                let p = eye + (forward + right * u + up * v) * half;
                vertices.push(Vertex { position: p, uv: vec2(0., 0.), color: [255; 4], normal: Vec4::ZERO });
            }
        }
        let row = (n + 1) as u16;
        for j in 0..n as u16 {
            for i in 0..n as u16 {
                let a = base + j * row + i;
                indices.extend([a, a + 1, a + row + 1, a, a + row + 1, a + row]);
            }
        }
    }
    (vertices, indices)
}
fn moved(captured: Option<Vec3>, origin: Option<Vec3>) -> bool {
    match (captured, origin) {
        (Some(a), Some(b)) => a.distance(b) > ORIGIN_EPSILON,
        (a, b) => a.is_some() != b.is_some(),
    }
}
fn plan(captured: Option<Option<Vec3>>, origin: Option<Vec3>, keep: bool, refresh: u32, frame: u64, last_full: u64,
    next_face: usize) -> Plan {
    let Some(captured) = captured else { return Plan::All };
    if keep && moved(captured, origin) && frame >= last_full + RECAPTURE_FRAMES { return Plan::All; }
    if refresh > 0 && frame.is_multiple_of(refresh as u64) { return Plan::Face(next_face); }
    Plan::Nothing
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sky_cube_plan_captures_once_then_follows_moves_and_refresh() {
        let o = Some(vec3(10., 20., 30.));
        assert_eq!(plan(None, o, false, 0, 1, 0, 0), Plan::All, "first frame captures");
        assert_eq!(plan(Some(o), o, false, 0, 2, 1, 0), Plan::Nothing);
        let moved_to = Some(vec3(500., 20., 30.));
        assert_eq!(plan(Some(o), moved_to, false, 0, 90, 1, 0), Plan::Nothing, "released geometry cannot recapture");
        assert_eq!(plan(Some(o), moved_to, true, 0, 20, 1, 0), Plan::Nothing, "at most one full capture per 30 frames");
        assert_eq!(plan(Some(o), moved_to, true, 0, 31, 1, 0), Plan::All);
        assert_eq!(plan(Some(o), Some(vec3(10.2, 20., 30.)), true, 0, 90, 1, 0), Plan::Nothing, "sub-unit jitter keeps the capture");
        assert_eq!(plan(Some(o), o, true, 4, 8, 1, 3), Plan::Face(3), "one face every refresh frames");
        assert_eq!(plan(Some(o), o, true, 4, 9, 1, 3), Plan::Nothing);
        assert_eq!(plan(Some(None), None, false, 0, 5, 1, 0), Plan::Nothing, "a map without sky camera captures once too");
        assert!(keeps_geometry("hedge1", 0) && keeps_geometry("hatter1", 2) && !keeps_geometry("hatter1", 0));
    }
    #[test]
    fn sky_dome_closes_every_direction_around_the_eye() {
        let eye = vec3(100., -50., 20.);
        let (v, i) = dome(eye, 1000., 4);
        assert_eq!(v.len(), 6 * 25);
        assert_eq!(i.len(), 6 * 16 * 6);
        assert!(v.iter().all(|v| (v.position - eye).abs().max_element() > 999.9), "every vertex on the cube");
        for (forward, _) in FACES {
            assert!(v.iter().any(|v| (v.position - eye).normalize().dot(forward) > 0.999), "face centre {forward}");
        }
    }
}
