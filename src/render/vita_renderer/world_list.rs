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

/// Uniform payloads by value, so a draw can compare one integer instead of 143 words.
///
/// Many parameters hold equal bits: the parameter key also covers textures, which are not uniforms.
/// `class` is a stable id per distinct payload, checked word for word when it is taken, so equal
/// ids mean equal bits and different ids mean different bits. Ids hold until `reserve` starts a
/// new epoch, and that happens only between passes. `Parameter` remembers its id and the epoch.
pub(super) struct Classes { bits: Vec<[u32; 143]>, hashes: Vec<u32>, capacity: usize, pub epoch: u32 }
impl Classes {
    pub fn new(parameters: usize) -> Self {
        // Room for a full set in every pass of a slow turn before a reset is needed.
        let capacity = (parameters * 4).max(16);
        Self { bits: Vec::with_capacity(capacity), hashes: Vec::with_capacity(capacity), capacity, epoch: 1 }
    }
    /// At the start of a pass. Guarantees `incoming` new payloads fit without a mid-pass reset.
    pub fn reserve(&mut self, incoming: usize) {
        if self.bits.len() + incoming > self.capacity {
            self.bits.clear();
            self.hashes.clear();
            self.epoch = self.epoch.wrapping_add(1);
        }
    }
    pub fn class(&mut self, bits: &[u32; 143]) -> u32 {
        let hash = bits.iter().fold(0x811c_9dc5_u32, |h, w| (h ^ *w).wrapping_mul(0x0100_0193));
        for (i, known) in self.hashes.iter().enumerate() {
            if *known == hash && self.bits[i] == *bits { return i as u32; }
        }
        self.bits.push(*bits);
        self.hashes.push(hash);
        (self.bits.len() - 1) as u32
    }
    #[cfg(test)] pub fn len(&self) -> usize { self.bits.len() }
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
            let class: Vec<u32> = bits.iter().map(|b| classes.class(b)).collect();
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
    fn classes_name_payloads_exactly_and_reset_only_between_passes() {
        let mut classes = Classes::new(4);
        let a = [1; 143];
        let mut b = a;
        b[142] = 2;
        let mut negative_zero = [0_u32; 143];
        negative_zero[3] = (-0_f32).to_bits();
        let (ca, cb, cz) = (classes.class(&a), classes.class(&b), classes.class(&negative_zero));
        assert!(ca != cb && cb != cz && ca != cz);
        assert_eq!(classes.class(&a), ca);
        assert_eq!(classes.class(&b), cb);
        assert_eq!(classes.len(), 3);
        // A forced hash collision must still compare words.
        classes.hashes[cb as usize] = classes.hashes[ca as usize];
        assert_eq!(classes.class(&b), cb);
        assert_eq!(classes.class(&a), ca);
        let epoch = classes.epoch;
        classes.reserve(4);
        assert_eq!((classes.epoch, classes.len()), (epoch, 3), "room left: ids stay valid");
        classes.reserve(16);
        assert_eq!((classes.epoch, classes.len()), (epoch + 1, 0), "no room: new epoch, empty");
        let capacity = classes.bits.capacity();
        classes.reserve(16);
        for i in 0..16 { let mut p = a; p[0] = i; classes.class(&p); }
        assert_eq!(classes.bits.capacity(), capacity, "a reserved pass does not reallocate");
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
