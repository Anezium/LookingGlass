//! Plain numeric layer output. No renderer handles or thread-local state.
use macroquad::prelude::Vertex;
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Revision {
    pub identity: u64,
    pub vertices: [u64; 2],
}
impl Revision {
    pub(crate) fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let identity = NEXT.fetch_add(1, Ordering::Relaxed);
        assert_ne!(identity, u64::MAX, "actor producer identity overflow");
        Self { identity, vertices: [1; 2] }
    }
    pub(crate) fn changed(&mut self, dirty: [bool; 2]) {
        for (version, changed) in self.vertices.iter_mut().zip(dirty) {
            if changed { *version = version.checked_add(1).expect("actor revision overflow"); }
        }
    }
}
/// What a worker writes into an owned publication. Opaque255 and VertexAlpha are the
/// two normalized recipe variants. Raw keeps the exact serial kernel bytes, so the
/// game thread can rebuild every layer's alpha exactly as the serial fallback does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PublicationMode { Opaque255, VertexAlpha, Raw }
impl PublicationMode {
    pub(crate) fn recipe(vertex_alpha: bool) -> Self {
        if vertex_alpha { Self::VertexAlpha } else { Self::Opaque255 }
    }
}
pub(crate) fn normalize(mut vertex: Vertex, vertex_alpha: bool) -> Vertex {
    vertex.color[3] = if vertex_alpha {
        (255. * (vertex.color[3] as f32 / 255.).clamp(0., 1.)) as u8
    } else { 255 };
    vertex
}
/// UVs remain caller-owned and are checked/published on the game thread.
pub(crate) fn equal_written(a: &Vertex, b: &Vertex) -> bool {
    a.position.to_array().map(f32::to_bits) == b.position.to_array().map(f32::to_bits)
        && a.normal.to_array().map(f32::to_bits) == b.normal.to_array().map(f32::to_bits)
        && a.color == b.color
}
pub(crate) struct Publication<'a> {
    pub revision: Revision,
    pub vertices: &'a [[Vertex; 2]],
    pub indices: &'a [u16],
}
