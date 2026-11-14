//! `water_draw_join`: one indexed draw for a consecutive run of liquid world alpha draws.
//!
//! The run keeps the submitted primitive order exactly: the dynamic index buffer holds each
//! group's own indices, copied from the same shared vertex buffer, one group after the other,
//! and the run is drawn at the first member's place in the joint alpha list. A run is taken
//! only when every draw shares the pipeline, the vertex buffer, the atlas, the texture and the
//! uniform parameter, and when no actor or puff sits between them. The alpha pass writes no
//! depth, so the only order-dependent operation is blending, and blending is defined per
//! primitive in submission order; the fragments, their depth tests and their uniforms are the
//! ones the separate draws used. Default off.
use super::*;

#[derive(Clone, Copy)]
pub(super) struct Run {
    pub pipeline: Pipeline,
    pub vertex: BufferId,
    pub atlas: TextureId,
    pub texture: TextureId,
    pub parameter: usize,
    /// Index buffer range of the whole run.
    pub offset: usize,
    pub count: usize,
    /// Template id of the run's first member, the list entry that draws it.
    pub first: usize,
    pub groups: usize,
}

pub(super) struct State {
    pub buffer: BufferId,
    /// Index capacity of `buffer`, in u16 elements.
    pub capacity: usize,
    /// This frame's joined indices, one run after another.
    pub scratch: Vec<u16>,
    /// This frame's runs, in submission order.
    pub runs: Vec<Run>,
    /// Each run's template ids, in submission order.
    pub members: Vec<Vec<usize>>,
    /// Template id to run index, `u32::MAX` for a draw that is not joined.
    pub member: Vec<u32>,
    /// The packed indices of each group that can join, in the shared buffer's numbering.
    pub indices: Vec<Option<Vec<u16>>>,
    pub upload_bytes: u64,
}
impl State {
    pub fn new(ctx: &mut dyn RenderingBackend, indices: Vec<Option<Vec<u16>>>) -> Self {
        let total: usize = indices.iter().flatten().map(Vec::len).sum();
        let capacity = total.max(1);
        let buffer = ctx.new_buffer(BufferType::IndexBuffer, BufferUsage::Stream,
            BufferSource::empty::<u16>(capacity));
        Self { buffer, capacity, scratch: Vec::with_capacity(capacity), runs: Vec::new(),
            members: Vec::new(), member: Vec::new(), indices, upload_bytes: 0 }
    }
    pub fn begin_frame(&mut self, prepared: usize) {
        self.scratch.clear();
        self.runs.clear();
        self.members.clear();
        self.member.clear();
        self.member.resize(prepared, u32::MAX);
    }
}
