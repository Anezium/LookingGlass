//! Dense opaque submission behind `world_list_submit`. Default off.
//!
//! Same GL calls, in the same order, as the state-cached loop in `Renderer::draw`. What goes away
//! is the work around them: a macroquad context lookup per draw, the 143-word uniform copy and
//! compare per draw, a 596-byte `Prepared`, a texture id lookup through the context, a per-draw
//! model matrix compare, and a re-sort of an unchanged list. Records are built once at load, so
//! nothing here depends on the camera standing still except the sort and visibility shortcuts,
//! which fall back to the full work when their inputs moved.
use super::*;

/// What one group's draw needs, resolved at load.
#[derive(Clone, Copy)]
pub(super) struct ListDraw {
    pub vertex: BufferId,
    pub index: BufferId,
    pub atlas: TextureId,
    /// The stage's only texture. `animated` groups resolve theirs per frame.
    pub texture: TextureId,
    pub pipeline: Pipeline,
    pub base_element: i32,
    pub indices: i32,
    pub parameter: u32,
    pub pipeline_class: u8,
    pub low: bool,
    pub animated: bool,
    pub static_group: bool,
}
impl ListDraw {
    pub fn new(g: &Group, pipelines: &[Pipeline], low_pipelines: &[Pipeline]) -> Self {
        let low = g.recipe.stage.alpha_test == 0 && !low_pipelines.is_empty();
        let pipeline_class = g.recipe.blend * 3 + g.recipe.cull;
        Self {
            vertex: g.bindings.vertex_buffers[0],
            index: g.bindings.index_buffer,
            atlas: g.bindings.images[1],
            texture: g.bindings.images[0],
            pipeline: if low { low_pipelines[g.recipe.cull] } else { pipelines[pipeline_class] },
            base_element: g.base_element,
            indices: g.indices,
            parameter: g.parameter as u32,
            pipeline_class: pipeline_class as u8,
            low,
            animated: g.recipe.textures.len() > 1 || g.recipe.stage.images.len() > 1,
            static_group: g.recipe.model.is_none() && g.recipe.motion.is_none(),
        }
    }
}
/// What a list pass did, for the counters the state-cached loop publishes.
#[derive(Default)]
pub(super) struct Counts {
    pub low_draws: u64, pub pipeline_changes: u64, pub texture_changes: u64,
    pub applications: [u64; 4], pub uniform_traffic: [u64; 3],
    pub batches: usize, pub triangles: usize, pub static_batches: usize,
}
/// One visible draw of this pass, in submission order.
#[derive(Clone, Copy)]
pub(super) struct Item { pub group: u32, pub texture: TextureId, pub uniforms: u32 }
/// A static group's frustum answer for one visibility generation.
#[derive(Clone, Copy)]
pub(super) struct Visible { pub generation: u64, pub depth: f32, pub visible: bool, pub static_group: bool }

pub(super) struct WorldList {
    pub draws: Vec<ListDraw>,
    pub visible: Vec<Visible>,
    pub items: Vec<Item>,
    pub sort: SortCache,
    pub classes: Classes,
    /// Pass number. A `Parameter` holds the pass that last checked it.
    pub serial: u32,
}
impl WorldList {
    pub fn new(groups: &[Group], pipelines: &[Pipeline], low_pipelines: &[Pipeline], parameters: usize) -> Self {
        let draws: Vec<ListDraw> = groups.iter().map(|g| ListDraw::new(g, pipelines, low_pipelines)).collect();
        let visible = draws.iter().map(|d| Visible { generation: 0, depth: 0., visible: false, static_group: d.static_group }).collect();
        Self { items: Vec::with_capacity(draws.len()), sort: SortCache::with_capacity(draws.len()), draws, visible, classes: Classes::new(parameters), serial: 0 }
    }
    pub fn forget_visibility(&mut self) { for v in &mut self.visible { v.generation = 0; } }
}

/// Uniform payloads by value inside one pass, so a draw can compare one integer instead of 143 words.
///
/// Many parameters hold equal bits: the parameter key also covers textures, which are not uniforms.
/// The time is in every payload, so a class means nothing past its pass. Each parameter takes its
/// class once a pass, after its last write, and the table points at the first parameter that held
/// those bits. Equal ids mean equal bits and different ids mean different bits, checked word for
/// word when taken. Nothing is copied and the table never grows past the parameter count.
pub(super) struct Classes { first: Vec<(u32, u32)> }
impl Classes {
    pub fn new(parameters: usize) -> Self { Self { first: Vec::with_capacity(parameters) } }
    pub fn begin_pass(&mut self) { self.first.clear(); }
    /// `own` is the asking parameter, `bits_of` reads any parameter already in the table.
    pub fn class<'a>(&mut self, own: u32, bits: &[u32; 143], bits_of: impl Fn(u32) -> &'a [u32; 143]) -> u32 {
        let hash = bits.iter().fold(0x811c_9dc5_u32, |h, w| (h ^ *w).wrapping_mul(0x0100_0193));
        for (i, &(known, parameter)) in self.first.iter().enumerate() {
            if known == hash && bits_of(parameter) == bits { return i as u32; }
        }
        #[cfg(all(feature = "sim_maps_probe", not(target_os = "vita")))]
        assert!(self.first.len() < self.first.capacity(), "a pass asked for more classes than parameters");
        self.first.push((hash, own));
        (self.first.len() - 1) as u32
    }
    #[cfg(test)] pub fn len(&self) -> usize { self.first.len() }
    #[cfg(test)] pub fn capacity(&self) -> usize { self.first.capacity() }
}

/// The state-cached loop's decisions, with the uniform payload named by its class. Equal class
/// means equal bits, the state cache's own test, so both skip the same draws and write the same.
pub(super) struct Planner<P, B> {
    pipeline: Option<P>,
    depth: bool,
    bindings: Option<B>,
    uniforms: [Option<u32>; 2],
}
impl<P: Copy + Eq, B: Copy + Eq> Planner<P, B> {
    pub fn new() -> Self { Self { pipeline: None, depth: false, bindings: None, uniforms: [None; 2] } }
    #[inline] pub fn observe(&mut self, pipeline: P, low: bool, bindings: B, uniforms: u32) -> state_cache::Applications {
        let changed = self.pipeline != Some(pipeline);
        let program = usize::from(low);
        let result = state_cache::Applications {
            pipeline: changed,
            depth: !self.depth,
            bindings: changed || self.bindings != Some(bindings),
            uniforms: self.uniforms[program] != Some(uniforms),
        };
        self.pipeline = Some(pipeline);
        self.depth = true;
        self.bindings = Some(bindings);
        self.uniforms[program] = Some(uniforms);
        result
    }
}

/// `front_to_back` order, kept while the input is bit-identical. The comparator is a total order
/// (`total_cmp`, then id), so the same input has exactly one sorted output, whichever sort runs.
pub(super) struct SortCache { input: Vec<(u32, usize)>, output: Vec<(f32, usize)>, valid: bool }
impl SortCache {
    pub fn with_capacity(n: usize) -> Self { Self { input: Vec::with_capacity(n), output: Vec::with_capacity(n), valid: false } }
    pub fn apply(&mut self, draws: &mut Vec<(f32, usize)>) {
        let same = self.valid && self.input.len() == draws.len()
            && self.input.iter().zip(draws.iter()).all(|(a, b)| a.0 == b.0.to_bits() && a.1 == b.1);
        if same {
            draws.clear();
            draws.extend_from_slice(&self.output);
            return;
        }
        self.input.clear();
        self.input.extend(draws.iter().map(|d| (d.0.to_bits(), d.1)));
        draws.sort_unstable_by(|a, b| a.0.total_cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
        self.output.clear();
        self.output.extend_from_slice(draws);
        self.valid = true;
    }
}

/// One buffer's vertices and indices, and the key groups share it by.
pub(super) type Packed = (Option<(usize, usize)>, Vec<WorldVertex>, Vec<u16>);
const PACK_VERTICES: usize = 60000;
const PACK_INDICES: usize = 180000;
/// Groups into buffers, and per group its buffer, first index and index count. A keyed group joins
/// the first buffer of its key with room; an unkeyed one takes a buffer of its own. `shared` gives
/// every group one key, so a map has a few buffers instead of one per group. First fit keeps group
/// order inside each buffer. Sizes are reserved up front, so a full buffer does not hold twice its
/// bytes during the append.
pub(super) fn pack<'a>(groups: impl ExactSizeIterator<Item = (Option<(usize, usize)>, &'a mut Vec<WorldVertex>, &'a mut Vec<u16>)>, shared: bool)
    -> (Vec<Packed>, Vec<(usize, i32, i32)>) {
    let mut groups: Vec<_> = groups.collect();
    let shared_key = shared.then_some((usize::MAX, 0));
    let mut unpacked_vertices: usize = groups.iter().map(|g| g.1.len()).sum();
    let mut unpacked_indices: usize = groups.iter().map(|g| g.2.len()).sum();
    let mut packed: Vec<Packed> = Vec::new();
    let mut allocations = Vec::with_capacity(groups.len());
    let wrong_offset = negative("world-pack-wrong-vertex-offset");
    for (key, vertices, indices) in &mut groups {
        let key = shared_key.or(*key);
        let slot = key.and_then(|key| packed.iter().position(|(k, v, i)| {
            *k == Some(key) && v.len() + vertices.len() <= PACK_VERTICES && i.len() + indices.len() <= PACK_INDICES
        }));
        let count = indices.len() as i32;
        if slot.is_none() && shared_key.is_some() {
            packed.push((key, Vec::with_capacity(unpacked_vertices.min(PACK_VERTICES)), Vec::with_capacity(unpacked_indices.min(PACK_INDICES))));
        }
        let slot = slot.or_else(|| shared_key.map(|_| packed.len() - 1));
        unpacked_vertices -= vertices.len();
        unpacked_indices -= indices.len();
        if let Some(slot) = slot {
            let (_, v, i) = &mut packed[slot];
            allocations.push((slot, i.len() as i32, count));
            let offset = v.len() as u16;
            let offset = if wrong_offset && i.len() > 600 && !v.is_empty() { offset.wrapping_sub(1) } else { offset };
            v.append(vertices);
            i.extend(indices.drain(..).map(|x| x.checked_add(offset).expect("packed u16 index")));
            **vertices = Vec::new();
            **indices = Vec::new();
        } else {
            allocations.push((packed.len(), 0, count));
            packed.push((key, std::mem::take(*vertices), std::mem::take(*indices)));
        }
    }
    (packed, allocations)
}

/// Host oracle sabotage. The oracle must see each of these, and they exist only in probe builds.
#[cfg(all(feature = "sim_maps_probe", not(target_os = "vita")))]
pub(super) fn negative(name: &str) -> bool { crate::sim_maps::probe::negative_control() == Some(name) }
#[cfg(not(all(feature = "sim_maps_probe", not(target_os = "vita"))))]
#[inline(always)] pub(super) fn negative(_name: &str) -> bool { false }

#[cfg(test)]
mod tests {
    use super::*;
    struct Rng(u32);
    impl Rng { fn next(&mut self, n: u32) -> u32 { self.0 = self.0.wrapping_mul(1664525).wrapping_add(1013904223); (self.0 >> 8) % n } }

    /// Replays a draw sequence through a backend model that records each GL-visible action.
    /// `uniform_fields` is modelled by field: a write happens only for a field that differs.
    fn stream(steps: &[(u32, bool, (u32, u32, u32, u32), u32)], bits: &[[u32; 143]],
              decide: &mut dyn FnMut(u32, bool, (u32, u32, u32, u32), u32) -> state_cache::Applications) -> (Vec<String>, u32) {
        let mut out = Vec::new();
        let mut applications = 0;
        let mut storage: [Option<[u32; 143]>; 2] = [None; 2];
        for &(pipeline, low, binding, parameter) in steps {
            let a = decide(pipeline, low, binding, parameter);
            if a.pipeline { out.push(format!("pipeline {pipeline}")); }
            if a.depth { out.push("depth".into()); }
            if a.bindings { out.push(format!("bind {pipeline} {binding:?}")); }
            if a.uniforms {
                applications += 1;
                let program = usize::from(low);
                let new = bits[parameter as usize];
                for field in 0..143 {
                    if storage[program].is_none_or(|old| old[field] != new[field]) {
                        out.push(format!("uniform {program} {field} {}", new[field]));
                    }
                }
                storage[program] = Some(new);
            }
            out.push(format!("draw {binding:?}"));
        }
        (out, applications)
    }
    fn random_steps(rng: &mut Rng, n: usize) -> (Vec<(u32, bool, (u32, u32, u32, u32), u32)>, Vec<[u32; 143]>) {
        // Parameters with equal bits exist on purpose, and some differ in one word only.
        let mut bits: Vec<[u32; 143]> = (0..12).map(|_| {
            let mut b = [7; 143];
            b[0] = rng.next(3); b[71] = rng.next(2); b[142] = rng.next(5);
            b
        }).collect();
        bits[3] = bits[2];
        bits[7] = bits[1];
        let steps = (0..n).map(|_| {
            let low = rng.next(3) != 0;
            let pipeline = if low { 10 + rng.next(2) } else { rng.next(4) };
            (pipeline, low, (rng.next(3), rng.next(3), rng.next(4), 9), rng.next(12))
        }).collect();
        (steps, bits)
    }
    #[test]
    fn planner_makes_the_same_gl_stream_as_the_state_cache() {
        let mut rng = Rng(0x1234);
        let mut skipped = 0;
        for _ in 0..400 {
            let (steps, bits) = random_steps(&mut rng, 60);
            let mut classes = Classes::new(12);
            let class: Vec<u32> = bits.iter().enumerate().map(|(i, b)| classes.class(i as u32, b, |p| &bits[p as usize])).collect();
            let mut cache = state_cache::Cache::<u32, (u32, u32, u32, u32), 143>::new();
            let (legacy, legacy_applications) = stream(&steps, &bits, &mut |p, low, b, q| cache.observe(p, low, true, b, bits[q as usize]));
            let mut planner = Planner::<u32, (u32, u32, u32, u32)>::new();
            let (list, list_applications) = stream(&steps, &bits, &mut |p, low, b, q| {
                let a = planner.observe(p, low, b, class[q as usize]);
                skipped += u32::from(!a.uniforms);
                a
            });
            assert_eq!(legacy, list);
            // Same skips, not only the same writes: the counter stays equal too.
            assert_eq!(legacy_applications, list_applications);
        }
        assert!(skipped > 1000, "the identity branch must run: {skipped}");
    }
    #[test]
    fn planner_keeps_program_storage_separate() {
        let mut p = Planner::<u32, u32>::new();
        assert!(p.observe(1, false, 1, 5).uniforms);
        assert!(!p.observe(1, false, 1, 5).uniforms);
        // The low program's storage has never seen class 5, and pipeline A/B/A forces bindings.
        let a = p.observe(2, true, 1, 5);
        assert!(a.uniforms && a.pipeline && a.bindings && !a.depth);
        // Back on the first program its storage still holds 5.
        let a = p.observe(1, false, 1, 5);
        assert!(!a.uniforms && a.pipeline && a.bindings);
        assert!(p.observe(1, false, 2, 5).bindings);
    }
    #[test]
    fn classes_name_payloads_exactly_inside_a_pass() {
        let a = [1; 143];
        let mut b = a;
        b[142] = 2;
        let mut negative_zero = [0_u32; 143];
        negative_zero[3] = (-0_f32).to_bits();
        let mut positive_zero = negative_zero;
        positive_zero[3] = 0_f32.to_bits();
        let all = [a, b, negative_zero, positive_zero, a, b];
        let mut classes = Classes::new(all.len());
        let of = |p: u32| &all[p as usize];
        let got: Vec<u32> = all.iter().enumerate().map(|(i, bits)| classes.class(i as u32, bits, of)).collect();
        assert_eq!(got, [0, 1, 2, 3, 0, 1], "equal bits share a class, a word apart do not, -0 is not 0");
        assert_eq!(classes.len(), 4);
        // A forced hash collision must still compare words.
        classes.first[0].0 = classes.first[1].0;
        assert_eq!(classes.class(1, &b, of), 1);
        // A new pass forgets everything and keeps its storage, so a frame does not allocate.
        let capacity = classes.capacity();
        classes.begin_pass();
        assert_eq!(classes.len(), 0);
        assert_eq!(classes.class(1, &b, of), 0, "ids mean nothing across passes");
        assert_eq!(classes.capacity(), capacity);
    }
    fn vertex(tag: u32) -> WorldVertex {
        WorldVertex { position: [tag as f32, 0., 0.], uv: [0.; 2], color: [0; 4], lightmap: [0.; 4], normal: [0.; 3] }
    }
    /// A group's vertices tagged by group, and indices that walk them from the top down.
    fn group(id: u32, vertices: usize, indices: usize) -> (Vec<WorldVertex>, Vec<u16>) {
        ((0..vertices).map(|v| vertex(id * 100_000 + v as u32)).collect(), (0..indices).map(|i| (vertices - 1 - i % vertices) as u16).collect())
    }
    fn resolved(packed: &[Packed], allocation: (usize, i32, i32)) -> Vec<f32> {
        let (_, v, i) = &packed[allocation.0];
        i[allocation.1 as usize..(allocation.1 + allocation.2) as usize].iter().map(|&x| v[x as usize].position[0]).collect()
    }
    fn expected(g: &(Vec<WorldVertex>, Vec<u16>)) -> Vec<f32> { g.1.iter().map(|&x| g.0[x as usize].position[0]).collect() }
    #[test]
    fn shared_packing_resolves_each_group_to_its_own_bytes_in_order() {
        let sizes = [(3, 6), (40_000, 60_000), (20_000, 90_000), (1, 3), (15_000, 45_000), (60_000, 180_000), (5, 9), (2, 2), (1, 170_000)];
        let originals: Vec<_> = sizes.iter().enumerate().map(|(id, s)| group(id as u32, s.0, s.1)).collect();
        let mut groups = originals.clone();
        let (packed, allocations) = pack(groups.iter_mut().map(|g| (None, &mut g.0, &mut g.1)), true);
        assert_eq!(allocations.len(), sizes.len());
        for (id, allocation) in allocations.iter().enumerate() {
            assert_eq!(allocation.2 as usize, sizes[id].1);
            assert_eq!(resolved(&packed, *allocation), expected(&originals[id]), "group {id}");
        }
        for (_, v, i) in &packed {
            assert!(v.len() <= PACK_VERTICES && i.len() <= PACK_INDICES);
            assert!(i.iter().all(|&x| (x as usize) < v.len()));
            // Reserved, not grown by doubling.
            assert!(v.capacity() <= PACK_VERTICES && i.capacity() <= PACK_INDICES);
        }
        // Sources are released, and later groups fill earlier gaps by first fit in group order.
        assert!(groups.iter().all(|g| g.0.capacity() == 0 && g.1.capacity() == 0));
        assert_eq!(allocations.iter().map(|a| a.0).collect::<Vec<_>>(), [0, 0, 1, 0, 0, 2, 0, 0, 3]);
        assert!(packed.len() < sizes.len());
        // Ranges inside a buffer do not overlap and keep group order.
        for slot in 0..packed.len() {
            let mut next = 0;
            for a in allocations.iter().filter(|a| a.0 == slot) { assert_eq!(a.1, next); next += a.2; }
            assert_eq!(next as usize, packed[slot].2.len());
        }
    }
    #[test]
    fn packing_keeps_one_buffer_per_group_when_off_and_keys_when_keyed() {
        let originals: Vec<_> = (0..5).map(|id| group(id, 4, 6)).collect();
        let mut groups = originals.clone();
        let (packed, allocations) = pack(groups.iter_mut().map(|g| (None, &mut g.0, &mut g.1)), false);
        assert_eq!(packed.len(), 5);
        assert_eq!(allocations, (0..5).map(|i| (i, 0, 6)).collect::<Vec<_>>());
        for id in 0..5 { assert_eq!(resolved(&packed, allocations[id]), expected(&originals[id])); }
        // world_static_merge's keys, not shared: same key joins, different key and None do not.
        let mut groups = originals.clone();
        let keys = [Some((1, 0)), None, Some((1, 0)), Some((2, 0)), Some((1, 0))];
        let (packed, allocations) = pack(groups.iter_mut().zip(keys).map(|(g, k)| (k, &mut g.0, &mut g.1)), false);
        assert_eq!(packed.len(), 3);
        assert_eq!(allocations.iter().map(|a| (a.0, a.1)).collect::<Vec<_>>(), [(0, 0), (1, 0), (0, 6), (2, 0), (0, 12)]);
        for id in 0..5 { assert_eq!(resolved(&packed, allocations[id]), expected(&originals[id])); }
        // Shared wins over a key.
        let mut groups = originals.clone();
        let (packed, _) = pack(groups.iter_mut().zip(keys).map(|(g, k)| (k, &mut g.0, &mut g.1)), true);
        assert_eq!(packed.len(), 1);
        assert_eq!((packed[0].1.len(), packed[0].2.len()), (20, 30));
    }
    #[test]
    fn sort_cache_gives_the_legacy_order_and_reuses_only_identical_input() {
        let legacy = |d: &[(f32, usize)]| { let mut v = d.to_vec(); v.sort_unstable_by(|a, b| a.0.total_cmp(&b.0).then_with(|| a.1.cmp(&b.1))); v };
        let same = |a: &[(f32, usize)], b: &[(f32, usize)]| a.len() == b.len() && a.iter().zip(b).all(|(x, y)| x.0.to_bits() == y.0.to_bits() && x.1 == y.1);
        let mut rng = Rng(77);
        let mut cache = SortCache::with_capacity(8);
        let mut previous: Vec<(f32, usize)> = Vec::new();
        let mut hits = 0;
        for round in 0..600 {
            let input: Vec<(f32, usize)> = if round % 3 == 0 || previous.is_empty() {
                let n = rng.next(40) as usize;
                // Equal depths, -0 against 0, NaN and an equal id all stay in the input.
                (0..n).map(|i| {
                    let depth = match rng.next(9) { 0 => 0., 1 => -0., 2 => f32::NAN, 3 => 5., x => x as f32 * 1.5 };
                    (depth, i)
                }).collect()
            } else {
                hits += 1;
                previous.clone()
            };
            previous = input.clone();
            let mut out = input.clone();
            cache.apply(&mut out);
            assert!(same(&out, &legacy(&input)), "round {round}");
        }
        assert!(hits > 300);
        // One changed depth bit must not reuse the old answer, not even -0 against 0.
        let mut cache = SortCache::with_capacity(4);
        let mut a = vec![(0_f32, 1), (0_f32, 0)];
        cache.apply(&mut a);
        let mut b = vec![(-0_f32, 1), (0_f32, 0)];
        cache.apply(&mut b);
        assert!(same(&b, &legacy(&[(-0_f32, 1), (0_f32, 0)])));
    }
}
