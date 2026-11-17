//! Host-only sky cost probe, `LOOKING_GLASS_SKY_STATS=1`. Off, nothing here runs.
//!
//! One line per frame for the sky part of the opaque pass: CPU time on the main thread, GPU time
//! from a `GL_TIME_ELAPSED` query read back at once (a stall the probe accepts), and the legacy
//! draw counters. The legacy path flushes queued Macroquad draws before the query starts so they
//! are not counted, and its own last flush lands inside the query; that first flush happens only
//! with the probe on.
use std::{cell::Cell, sync::OnceLock, time::Instant};

const TIME_ELAPSED: u32 = 0x88BF;
const QUERY_RESULT: u32 = 0x8866;
thread_local! {
    static QUERY: Cell<u32> = const { Cell::new(0) };
    static FRAME: Cell<u64> = const { Cell::new(0) };
}
pub(crate) fn enabled() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("LOOKING_GLASS_SKY_STATS").is_some())
}
pub(crate) struct Probe { started: Instant, query: u32 }
/// `flush`: send queued Macroquad draws first. Inside a raw retained pass there are none.
pub(crate) fn begin(flush: bool) -> Option<Probe> {
    if !enabled() { return None; }
    use macroquad::miniquad::gl;
    unsafe {
        if flush { macroquad::window::get_internal_gl().flush(); }
        let mut query = QUERY.with(Cell::get);
        if query == 0 { gl::glGenQueries(1, &mut query); QUERY.with(|q| q.set(query)); }
        gl::glBeginQuery(TIME_ELAPSED, query);
        Some(Probe { started: Instant::now(), query })
    }
}
/// `what`: the sky mode. `extra`: free-form counters appended to the line.
pub(crate) fn end(probe: Option<Probe>, what: &str, extra: &str) {
    let Some(probe) = probe else { return };
    use macroquad::miniquad::gl;
    let cpu = probe.started.elapsed().as_secs_f64() * 1000.;
    let mut nanos = 0u64;
    unsafe {
        gl::glEndQuery(TIME_ELAPSED);
        gl::glGetQueryObjectui64v(probe.query, QUERY_RESULT, &mut nanos);
    }
    let frame = FRAME.with(|f| { f.set(f.get() + 1); f.get() });
    println!("[lg/sky-stats] n={frame} mode={what} cpu_ms={cpu:.3} gpu_ms={:.3} {extra}", nanos as f64 / 1e6);
}

thread_local! {
    static PHASES: std::cell::RefCell<String> = const { std::cell::RefCell::new(String::new()) };
    static VERTICES: Cell<u64> = const { Cell::new(0) };
    /// Macroquad submissions of the legacy path: [draw calls, vertices uploaded, indices uploaded,
    /// triangles sorted on the CPU].
    static SUBMITTED: Cell<[u64; 4]> = const { Cell::new([0; 4]) };
}
/// One `draw_mesh` of the legacy path. Every layer sets uniforms first, which breaks Macroquad's
/// batching, so one submission is one draw call with its own vertex and index upload.
pub(crate) fn add_submission(vertices: usize, indices: usize) {
    SUBMITTED.with(|s| { let mut v = s.get(); v[0] += 1; v[1] += vertices as u64; v[2] += indices as u64; s.set(v); });
}
/// Triangles of a translucent batch sorted back to front on the CPU.
pub(crate) fn add_sorted(triangles: usize) {
    SUBMITTED.with(|s| { let mut v = s.get(); v[3] += triangles as u64; s.set(v); });
}
/// Legacy vertices rewritten on the CPU (position pass plus one pass per layer), counted by
/// `draw_batch` while the probe is on.
pub(crate) fn add_vertices(n: usize) { VERTICES.with(|v| v.set(v.get() + n as u64)); }
/// CPU split of one legacy sky draw. Each `mark` closes the phase since the previous one.
pub(crate) struct Phases { last: Instant, batches: usize, culled: usize, triangles: usize, vertices: u64, submitted: [u64; 4] }
impl Phases {
    pub(crate) fn new() -> Option<Self> {
        enabled().then(|| { PHASES.with(|p| p.borrow_mut().clear()); Self { last: Instant::now(), batches: 0, culled: 0, triangles: 0,
            vertices: VERTICES.with(Cell::get), submitted: SUBMITTED.with(Cell::get) } })
    }
    pub(crate) fn mark(&mut self, name: &str, stats: &super::DrawStats) {
        let now = Instant::now();
        let vertices = VERTICES.with(Cell::get);
        let submitted = SUBMITTED.with(Cell::get);
        let d: [u64; 4] = std::array::from_fn(|i| submitted[i] - self.submitted[i]);
        PHASES.with(|p| p.borrow_mut().push_str(&format!(" {name}=[ms={:.3},batches={},layer_triangles={},cpu_vertices={},drawn_batches={},draw_calls={},upload_vertices={},upload_indices={},sorted_triangles={}]",
            (now - self.last).as_secs_f64() * 1000., stats.batches - self.batches, stats.triangles - self.triangles, vertices - self.vertices,
            (stats.batches - self.batches) - (stats.culled - self.culled), d[0], d[1], d[2], d[3])));
        self.last = now; self.batches = stats.batches; self.culled = stats.culled; self.triangles = stats.triangles; self.vertices = vertices;
        self.submitted = submitted;
    }
}
/// The phases of the last legacy sky draw, for `end`'s line.
pub(crate) fn phases() -> String { PHASES.with(|p| p.borrow().clone()) }

/// `LOOKING_GLASS_SKY_CUBE_DUMP=DIR`: write each full `sky_cube` capture as `DIR/<map>-<n>.png`,
/// top row of faces +X -X +Y, bottom row -Y +Z -Z (rows as GL stores them, flipped to upright).
pub(crate) fn dump_cube(texture: &macroquad::texture::Texture2D, map: &str, n: u32) {
    let Some(dir) = std::env::var_os("LOOKING_GLASS_SKY_CUBE_DUMP") else { return };
    unsafe { macroquad::window::get_internal_gl().flush(); }
    let path = std::path::Path::new(&dir).join(format!("{map}-{n}.png"));
    texture.get_texture_data().export_png(&path.to_string_lossy());
}
