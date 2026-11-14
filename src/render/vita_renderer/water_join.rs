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

/// The identity a run requires of every member. Equal keys mean the same program, the same
/// packed vertex space, the same texture pair and an equal uniform payload: the caller only
/// forms a key when `world_parameter_reuse` is on, and the validation enforces it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Key {
    pub pipeline: Pipeline,
    pub vertex: BufferId,
    pub atlas: TextureId,
    pub texture: TextureId,
    pub parameter: usize,
}

#[derive(Clone, Copy)]
pub(super) struct Run<K> {
    pub key: K,
    /// Index buffer range of the whole run.
    pub offset: usize,
    pub count: usize,
    /// Template id of the run's first member, the list entry that draws it.
    pub first: usize,
    pub groups: usize,
}

/// A list entry that ends any open run. The kinds are distinguished only so callers and tests
/// name the fence they mean; every kind has the same effect.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Fence {
    Actor,
    Puff,
    NonLiquid,
    /// A liquid group with no shared-buffer index copy to join.
    NoIndices,
    /// Left to the `world_alpha_lite` program.
    AlphaLite,
}

/// One entry of the joint alpha list, as far as joining is concerned.
#[derive(Clone, Copy)]
pub(super) enum Entry<'a, K> {
    Fence(Fence),
    Join { key: K, first: usize, items: &'a [u16] },
}

/// The frame's runs and their index scratch. Planning is pure so the fence, key and capacity
/// rules are testable without a rendering backend; `plan_water_runs` only maps the joint list
/// onto these entries and uploads the scratch.
pub(super) struct Plan<K> {
    /// This frame's runs, in submission order.
    pub runs: Vec<Run<K>>,
    /// Each run's template ids, in submission order.
    pub members: Vec<Vec<usize>>,
    /// Template id to run index, `u32::MAX` for a draw that is not joined.
    pub member: Vec<u32>,
    /// This frame's joined indices, one run after another.
    pub scratch: Vec<u16>,
    open: Option<usize>,
}
impl<K> Default for Plan<K> {
    fn default() -> Self {
        Self { runs: Vec::new(), members: Vec::new(), member: Vec::new(), scratch: Vec::new(), open: None }
    }
}
impl<K: Copy + PartialEq> Plan<K> {
    pub fn begin(&mut self, prepared: usize) {
        self.scratch.clear();
        self.runs.clear();
        self.members.clear();
        self.member.clear();
        self.member.resize(prepared, u32::MAX);
        self.open = None;
    }
    /// Take one list entry. A fence ends the open run; a joinable group continues it only when
    /// its key matches and its indices fit, otherwise it opens a run at its own place. A group
    /// that cannot fit keeps its original draw and ends the current run.
    pub fn entry(&mut self, entry: Entry<'_, K>, capacity: usize) {
        let (key, first, items) = match entry {
            Entry::Fence(_) => { self.open = None; return; }
            Entry::Join { key, first, items } => (key, first, items),
        };
        let offset = self.scratch.len();
        if offset + items.len() > capacity {
            self.open = None;
            return;
        }
        let continues = self.open.is_some_and(|run| self.runs[run].key == key);
        if !continues {
            self.runs.push(Run { key, offset, count: 0, first, groups: 0 });
            self.members.push(Vec::new());
            self.open = Some(self.runs.len() - 1);
        }
        let run = self.open.expect("a run was opened or continued");
        debug_assert!(first < self.member.len(), "a prepared template id");
        self.member[first] = run as u32;
        self.runs[run].count += items.len();
        self.runs[run].groups += 1;
        self.members[run].push(first);
        self.scratch.extend_from_slice(items);
    }
}

pub(super) struct State {
    pub buffer: BufferId,
    /// Index capacity of `buffer`, in u16 elements.
    pub capacity: usize,
    /// This frame's runs and index scratch.
    pub plan: Plan<Key>,
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
        Self { buffer, capacity, plan: Plan::default(), indices, upload_bytes: 0 }
    }
    pub fn begin_frame(&mut self, prepared: usize) {
        self.plan.begin(prepared);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Mirrors `Key`: every field names one part of the submission identity.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    struct TestKey { pipeline: u32, vertex: u32, atlas: u32, texture: u32, parameter: u32 }
    const BASE: TestKey = TestKey { pipeline: 1, vertex: 2, atlas: 3, texture: 4, parameter: 5 };

    fn plan(prepared: usize, capacity: usize, entries: &[Entry<'_, TestKey>]) -> Plan<TestKey> {
        let mut plan = Plan::default();
        plan.begin(prepared);
        for &entry in entries { plan.entry(entry, capacity); }
        plan
    }
    fn join(key: TestKey, first: usize, items: &[u16]) -> Entry<'_, TestKey> {
        Entry::Join { key, first, items }
    }

    #[test]
    fn differing_effective_uniforms_split_the_run() {
        let items = [0u16; 6];
        let mut other = BASE;
        other.parameter += 1;
        let plan = plan(4, 64, &[
            join(BASE, 0, &items),
            join(other, 1, &items),
            join(other, 2, &items),
        ]);
        assert_eq!(plan.runs.len(), 2, "a different parameter means a different payload");
        assert_eq!((plan.runs[0].groups, plan.runs[0].count, plan.runs[0].offset), (1, 6, 0));
        assert_eq!((plan.runs[1].groups, plan.runs[1].count, plan.runs[1].offset), (2, 12, 6));
        assert_eq!(plan.member, [0, 1, 1, u32::MAX]);
        assert_eq!(plan.members, [vec![0], vec![1, 2]]);
        assert_eq!(plan.scratch.len(), 18);
    }

    #[test]
    fn actor_puff_and_non_liquid_fences_end_the_run() {
        let items = [0u16; 3];
        for fence in [Fence::Actor, Fence::Puff, Fence::NonLiquid] {
            let plan = plan(4, 64, &[
                join(BASE, 0, &items),
                Entry::Fence(fence),
                join(BASE, 1, &items),
            ]);
            assert_eq!(plan.runs.len(), 2, "{fence:?} must not be crossed");
            assert_eq!(plan.member, [0, 1, u32::MAX, u32::MAX], "{fence:?}");
            assert_eq!(plan.members, [vec![0], vec![1]], "{fence:?}");
            assert_eq!(plan.scratch, [0; 6], "{fence:?}");
        }
    }

    #[test]
    fn different_vertex_buffers_never_join() {
        let items = [0u16; 3];
        let mut other = BASE;
        other.vertex += 1;
        let plan = plan(4, 64, &[join(BASE, 0, &items), join(other, 1, &items), join(BASE, 2, &items)]);
        assert_eq!(plan.runs.len(), 3, "indices are only valid in their own packed buffer");
        assert_eq!(plan.runs.iter().map(|run| run.key.vertex).collect::<Vec<_>>(), [2, 3, 2]);
        assert!(plan.runs.iter().all(|run| run.groups == 1 && run.count == 3));
    }

    #[test]
    fn counts_above_65535_are_not_truncated() {
        let first = vec![1u16; 40_000];
        let second = vec![2u16; 40_000];
        let plan = plan(3, 100_000, &[join(BASE, 0, &first), join(BASE, 1, &second)]);
        assert_eq!(plan.runs.len(), 1, "u16 bounds index values, not the index count");
        assert_eq!(plan.runs[0].count, 80_000);
        assert!(plan.runs[0].count > u16::MAX as usize);
        assert_eq!(plan.scratch.len(), 80_000);
        assert_eq!(plan.scratch[39_999], 1);
        assert_eq!(plan.scratch[40_000], 2, "the run crosses the u16 count boundary intact");
        assert_eq!(plan.scratch[79_999], 2, "the last index is still present");
        assert_eq!(plan.member, [0, 0, u32::MAX]);
        assert_eq!(plan.members, [vec![0, 1]]);
    }

    #[test]
    fn capacity_exhaustion_keeps_the_overflowing_draw_original() {
        let six = [3u16; 6];
        let four = [4u16; 4];
        let mut other = BASE;
        other.parameter += 1;
        let plan = plan(4, 10, &[join(BASE, 0, &six), join(other, 1, &six), join(other, 2, &four)]);
        assert_eq!(plan.scratch.len(), 10);
        assert_eq!(plan.runs.len(), 2);
        assert_eq!((plan.runs[0].offset, plan.runs[0].count), (0, 6));
        assert_eq!((plan.runs[1].offset, plan.runs[1].count), (6, 4));
        assert_eq!(plan.member, [0, u32::MAX, 1, u32::MAX], "the overflowing group draws as before");
        assert_eq!(plan.members, [vec![0], vec![2]]);
        assert!(plan.runs.iter().all(|run| run.offset + run.count <= 10));
    }

    #[test]
    fn a_capacity_below_every_group_leaves_all_draws_original() {
        let seven = [0u16; 7];
        let plan = plan(3, 6, &[join(BASE, 0, &seven), join(BASE, 1, &seven)]);
        assert!(plan.runs.is_empty() && plan.scratch.is_empty());
        assert_eq!(plan.member, [u32::MAX; 3]);
        assert!(plan.members.is_empty());
    }
}
