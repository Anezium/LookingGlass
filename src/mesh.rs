//! Geometry and its material recipe have independent ownership from GPU images.
use macroquad::prelude::{Texture2D, Vertex};
use std::rc::Rc;
pub(crate) struct Mesh {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u16>,
    pub texture: Option<Texture2D>,
    pub surface: Option<Rc<crate::render_fx::Surface>>,
}
/// Same submission as macroquad::models::draw_mesh, without copying geometry.
pub(crate) fn draw_mesh(mesh: &Mesh) {
    let gl = unsafe { macroquad::window::get_internal_gl() };
    gl.quad_gl.texture(mesh.texture.as_ref());
    gl.quad_gl.draw_mode(macroquad::prelude::DrawMode::Triangles);
    gl.quad_gl.geometry(&mesh.vertices, &mesh.indices);
}
