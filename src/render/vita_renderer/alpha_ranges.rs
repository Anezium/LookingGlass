//! Exact index gathering for consecutive same-state alpha commands in the final joint list.
//! No vertex rebasing or material/depth sorting occurs here. Actors and puffs are fences.
//! Only polygon-only BSP world surfaces participate. Patches, triangle soup and
//! mixed/ambient source batches retain their original draw barriers. Frozen model meshes preserve their
//! original draw barriers: strict host pixels found those meshes sensitive to joining,
//! even with equal state/indices and disjoint projected bounds.
use super::*;
use crate::render_fx::retained::Source;
use std::ops::Range;
// At most 512 KiB per source/scratch/store and 16K metadata entries. With the
// linked four-frame retirement policy, budget five GPU stores (2.5 MiB), plus
// 1 MiB CPU index arrays and bounded metadata. No per-run orphan allocations.
const MAX_INDEX_BYTES: usize = 512 * 1024;
const MAX_GROUPS: usize = 16 * 1024;

fn claim_frame(previous: &std::cell::Cell<Option<u32>>, frame: u32) -> bool {
    if previous.get() == Some(frame) {
        return false;
    }
    previous.set(Some(frame));
    true
}
#[cfg(target_os = "vita")]
fn claim_upload_frame() -> bool {
    // This is the linked VitaGL swap counter, NOT a diagnostic/gameplay counter.
    // A thread-local token covers all renderers, including replacement/preview loads.
    unsafe extern "C" {
        static mut vgl_framecount: u32;
    }
    thread_local! { static PREVIOUS: std::cell::Cell<Option<u32>>=const { std::cell::Cell::new(None) }; }
    let frame = unsafe { std::ptr::read_volatile(std::ptr::addr_of!(vgl_framecount)) };
    PREVIOUS.with(|previous| claim_frame(previous, frame))
}
#[cfg(not(target_os = "vita"))]
fn claim_upload_frame() -> bool {
    true
}

pub(super) fn eligible(r: &Recipe) -> bool {
    !r.opaque && r.model.is_none() && r.motion.is_none() && r.deform.is_none()
}
#[derive(Clone, Copy, Debug)]
pub(super) enum Emission {
    Single,
    Skip,
    Joined {
        base: i32,
        count: i32,
        first: usize,
        end: usize,
    },
}
#[derive(Clone)]
struct Original {
    range: Range<usize>,
    bank: usize,
    vertices: usize,
    base: i32,
}
#[derive(PartialEq, Eq)]
struct Key {
    pipeline: Pipeline,
    vertex: BufferId,
    index: BufferId,
    texture: TextureId,
    atlas: TextureId,
    rank: usize,
    bits: [u32; 143],
}
fn key(
    id: usize,
    prepared: &[Prepared],
    groups: &[Group],
    parameters: &[Parameter],
    pipelines: &[Pipeline],
) -> Key {
    let p = &prepared[id];
    let g = &groups[p.group];
    let u = p
        .uniforms
        .as_ref()
        .unwrap_or(&parameters[g.parameter].uniforms);
    Key {
        pipeline: pipelines[g.recipe.blend * 3 + g.recipe.cull],
        vertex: g.bindings.vertex_buffers[0],
        index: g.bindings.index_buffer,
        texture: p.texture,
        atlas: g.bindings.images[1],
        rank: g.recipe.rank,
        bits: u.bits(),
    }
}
/// Total indices and bytes must both fit the backend's signed GL parameters. Each stored
/// source is a whole triangle list. u16 values already address its immutable vertex bank.
fn checked_end(base: usize, count: usize, limit: usize) -> Option<usize> {
    let end = base.checked_add(count)?;
    (count > 0
        && count % 3 == 0
        && end <= limit
        && end <= i32::MAX as usize
        && end.checked_mul(std::mem::size_of::<u16>())? <= i32::MAX as usize)
        .then_some(end)
}
/// Locate one same-state span without crossing even a skipped actor or puff. State
/// is supplied by the live draw path, including actual programs/buffers and 143 uniform words.
fn span<K: Eq>(
    joint: &[(f32, Source)],
    start: usize,
    limit: usize,
    mut item: impl FnMut(usize) -> Option<(K, usize)>,
) -> (usize, usize) {
    let Source::World(id) = joint[start].1 else {
        return (start + 1, 0);
    };
    let Some((state, mut count)) = item(id) else {
        return (start + 1, 0);
    };
    let mut end = start + 1;
    while end < joint.len() {
        let Source::World(id) = joint[end].1 else {
            break;
        };
        let Some((next, size)) = item(id) else { break };
        if next != state {
            break;
        }
        let Some(total) = checked_end(count, size, limit) else {
            break;
        };
        count = total;
        end += 1;
    }
    (end, count)
}
pub(super) struct Gather {
    originals: Vec<Option<Original>>,
    indices: Vec<u16>,
    scratch: Vec<u16>,
    emissions: Vec<Emission>,
    members: Vec<usize>,
    buffer: Option<BufferId>,
    limit: usize,
    banks: Vec<(BufferId, BufferId)>,
    seen: Vec<bool>,
}
impl Gather {
    pub fn new(
        packed: &[(usize, &[u16])],
        allocations: &[(usize, i32, i32)],
        eligible: impl Iterator<Item = bool>,
    ) -> Option<Self> {
        if allocations.len() > MAX_GROUPS {
            return None;
        }
        let mut originals = Vec::new();
        originals.try_reserve_exact(allocations.len()).ok()?;
        let mut indices = Vec::new();
        for (&(bank, base, count), yes) in allocations.iter().zip(eligible) {
            let original = if yes {
                let (vertices, source) = packed.get(bank)?;
                let (base, count) = (usize::try_from(base).ok()?, usize::try_from(count).ok()?);
                let end = checked_end(base, count, source.len())?;
                let slice = &source[base..end];
                if slice.iter().any(|&i| usize::from(i) >= *vertices) {
                    return None;
                }
                let start = indices.len();
                checked_end(start, count, MAX_INDEX_BYTES / 2)?;
                indices.try_reserve_exact(count).ok()?;
                indices.extend_from_slice(slice);
                Some(Original {
                    range: start..indices.len(),
                    bank,
                    vertices: *vertices,
                    base: base as i32,
                })
            } else {
                None
            };
            originals.push(original);
        }
        if indices.is_empty() {
            return None;
        }
        let limit = indices.len();
        let mut scratch = Vec::new();
        scratch.try_reserve_exact(limit).ok()?;
        let mut emissions = Vec::new();
        emissions.try_reserve_exact(allocations.len()).ok()?;
        let mut members = Vec::new();
        members.try_reserve_exact(allocations.len()).ok()?;
        eprintln!("[lg/alpha-ranges] source_indices={} source_bytes={} scratch_max_bytes={} gpu_store_max_bytes={} (VitaGL keeps retired stores separately)",
            limit,limit*2,limit*2,limit*2);
        let mut seen = Vec::new();
        seen.try_reserve_exact(allocations.len()).ok()?;
        Some(Self {
            originals,
            indices,
            scratch,
            emissions,
            members,
            buffer: None,
            limit,
            banks: Vec::new(),
            seen,
        })
    }
    pub fn bind_sources(&mut self, banks: &[(BufferId, BufferId)]) -> bool {
        if banks.len() > MAX_GROUPS || self.banks.try_reserve_exact(banks.len()).is_err() {
            return false;
        }
        self.banks.extend_from_slice(banks);
        let metadata = self.originals.capacity() * std::mem::size_of::<Option<Original>>()
            + self.emissions.capacity() * std::mem::size_of::<Emission>()
            + self.members.capacity() * std::mem::size_of::<usize>()
            + self.seen.capacity() * std::mem::size_of::<bool>()
            + self.banks.capacity() * std::mem::size_of::<(BufferId, BufferId)>();
        eprintln!("[lg/alpha-ranges] metadata_capacity_bytes={} cpu_index_capacity_bytes={} five_gpu_store_bound_bytes={} payload_bound_with_metadata_bytes={}",
            metadata,(self.indices.capacity()+self.scratch.capacity())*2,self.limit*2*5,
            metadata+(self.indices.capacity()+self.scratch.capacity())*2+self.limit*2*5);
        true
    }
    pub fn buffer(&self) -> Option<BufferId> {
        self.buffer
    }
    pub fn delete(&mut self, ctx: &mut dyn RenderingBackend) {
        if let Some(buffer) = self.buffer.take() {
            ctx.delete_buffer(buffer);
        }
    }
    pub fn emission(&self, id: usize) -> Emission {
        self.emissions.get(id).copied().unwrap_or(Emission::Single)
    }
    pub fn members(&self, first: usize, end: usize) -> &[usize] {
        &self.members[first..end]
    }
    pub fn prepare(
        &mut self,
        joint: &[(f32, Source)],
        prepared: &[Prepared],
        groups: &[Group],
        parameters: &[Parameter],
        pipelines: &[Pipeline],
    ) {
        self.scratch.clear();
        self.members.clear();
        self.emissions.clear();
        // Publication can contain more occurrences than the map's unique groups. Fall back,
        // rather than grow buffers or consume a partial publication on an allocation failure.
        if prepared.len() > self.originals.len() {
            return;
        }
        self.emissions.resize(prepared.len(), Emission::Single);
        self.seen.clear();
        self.seen.resize(prepared.len(), false);
        for &(_, source) in joint {
            if let Source::World(id) = source {
                if id >= self.seen.len() || self.seen[id] {
                    return;
                }
                self.seen[id] = true;
            }
        }
        let mut cursor = 0;
        while cursor < joint.len() {
            let Source::World(id) = joint[cursor].1 else {
                cursor += 1;
                continue;
            };
            let p = &prepared[id];
            if self.originals[p.group].is_none() || p.indices != groups[p.group].indices {
                cursor += 1;
                continue;
            }
            let mut coverage = None;
            let (end, size) = span(
                joint,
                cursor,
                self.limit.saturating_sub(self.scratch.len()),
                |next| {
                    let p = &prepared[next];
                    let original = self.originals[p.group].as_ref()?;
                    let g = &groups[p.group];
                    let u = p
                        .uniforms
                        .as_ref()
                        .unwrap_or(&parameters[g.parameter].uniforms);
                    let rect = submit::projected(g, Mat4::from_cols_array(&u.mvp))?;
                    coverage = Some(match coverage {
                        Some(previous) if !world_list::negative("alpha-range-overlap") => {
                            submit::join_disjoint(previous, rect)?
                        }
                        _ => rect,
                    });
                    (p.indices == groups[p.group].indices).then(|| {
                        (
                            key(next, prepared, groups, parameters, pipelines),
                            original.range.len(),
                        )
                    })
                },
            );
            if end - cursor >= 2 && checked_end(self.scratch.len(), size, self.limit).is_some() {
                let base = self.scratch.len();
                let first = self.members.len();
                for &(_, source) in &joint[cursor..end] {
                    let Source::World(member) = source else {
                        unreachable!()
                    };
                    let original = self.originals[prepared[member].group].as_ref().unwrap();
                    self.scratch
                        .extend_from_slice(&self.indices[original.range.clone()]);
                    self.members.push(member);
                    self.emissions[member] = Emission::Skip;
                }
                self.emissions[id] = Emission::Joined {
                    base: base as i32,
                    count: size as i32,
                    first,
                    end: self.members.len(),
                };
            }
            cursor = end;
        }
        if self.scratch.is_empty() {
            return;
        }
        if !claim_upload_frame() {
            self.emissions.fill(Emission::Single);
            self.scratch.clear();
            self.members.clear();
            count("vita_world_alpha_range_same_frame_fallback", 1);
            return;
        }
        if world_list::negative("alpha-range-index-order") && self.scratch.len() >= 6 {
            // A quad often repeats index zero at offsets0 and3. Pick a distinct
            // value so this control actually corrupts the submitted index order.
            if let Some(other) = self.scratch.iter().position(|&v| v != self.scratch[0]) {
                self.scratch.swap(0, other);
            }
        }
        let ctx = unsafe { get_internal_gl() }.quad_context;
        // Miniquad's pinned Vita compact_stream path issues glBufferData with the actual
        // byte count, not SubData into live storage. The linked VitaGL implementation
        // marks referenced old stores dirty (four-frame purge queue), allocates a fresh
        // mapped store, and records IBO last_frame on indexed draws. One orphan per
        // finish bounds the additional purge entries. Renderer drop waits for the GPU.
        let buffer = *self.buffer.get_or_insert_with(|| {
            ctx.new_buffer(
                BufferType::IndexBuffer,
                BufferUsage::Stream,
                BufferSource::empty::<u16>(self.limit),
            )
        });
        ctx.buffer_update(buffer, BufferSource::slice(&self.scratch));
        count(
            "vita_world_alpha_range_upload_bytes",
            (self.scratch.len() * 2) as u64,
        );
        // RenderingBackend returns no Result for allocation/update. GL exposes OOM;
        // any reported error conservatively restores every original draw this finish.
        if unsafe { macroquad::miniquad::gl::glGetError() } != macroquad::miniquad::gl::GL_NO_ERROR
            || world_list::negative("alpha-range-upload-fail")
        {
            self.emissions.fill(Emission::Single);
            self.scratch.clear();
            self.members.clear();
            count("vita_world_alpha_range_upload_fallback", 1);
        }
    }
    /// Physical certificate at the actual draw boundary, separate from expanded semantic
    /// packets. It covers the command, bank, copied index payload and FINAL joint fences.
    #[cfg(all(feature = "sim_maps_probe", not(target_os = "vita")))]
    pub fn verify(
        &self,
        id: usize,
        vertex: BufferId,
        buffer: BufferId,
        base: i32,
        count: i32,
        prepared: &[Prepared],
        groups: &[Group],
        parameters: &[Parameter],
        pipelines: &[Pipeline],
        joint: &[(f32, Source)],
    ) {
        let Emission::Joined {
            base: expected,
            count: expected_count,
            first,
            end,
        } = self.emission(id)
        else {
            panic!("missing gathered command")
        };
        assert_eq!(
            (base, count, Some(buffer)),
            (expected, expected_count, self.buffer),
            "gathered physical base/count/IBO"
        );
        let members = self.members(first, end);
        assert_eq!(members[0], id);
        let state = key(id, prepared, groups, parameters, pipelines);
        assert_eq!(vertex, state.vertex, "gathered actual vertex bank");
        let start = joint
            .iter()
            .position(|&(_, s)| s == Source::World(id))
            .unwrap();
        assert_eq!(
            joint[start..start + members.len()]
                .iter()
                .map(|&(_, s)| s)
                .collect::<Vec<_>>(),
            members
                .iter()
                .copied()
                .map(Source::World)
                .collect::<Vec<_>>(),
            "gather crossed final joint fence/order"
        );
        let actual = &self.scratch[base as usize
            ..checked_end(base as usize, count as usize, self.scratch.len()).unwrap()];
        let mut offset = 0;
        let mut coverage = None;
        let first_original = self.originals[prepared[id].group].as_ref().unwrap();
        for &member in members {
            let p = &prepared[member];
            let g = &groups[p.group];
            let original = self.originals[p.group].as_ref().unwrap();
            assert!(
                g.bsp_polygons_only,
                "gathered alpha source lacks polygon-only BSP provenance"
            );
            let u = p
                .uniforms
                .as_ref()
                .unwrap_or(&parameters[g.parameter].uniforms);
            let rect = submit::projected(g, Mat4::from_cols_array(&u.mvp))
                .expect("uncertified alpha coverage");
            coverage = Some(match coverage {
                Some(previous) => submit::join_disjoint(previous, rect)
                    .expect("gathered draws share padded coverage"),
                None => rect,
            });
            assert!(
                key(member, prepared, groups, parameters, pipelines) == state,
                "gathered member state"
            );
            assert_eq!(original.bank, first_original.bank, "gathered source bank");
            assert_eq!(
                (vertex, state.index),
                self.banks[original.bank],
                "actual bound original vertex/index bank"
            );
            assert_eq!(
                (g.base_element, g.indices),
                (original.base, original.range.len() as i32)
            );
            let source = &self.indices[original.range.clone()];
            assert!(source.iter().all(|&v| usize::from(v) < original.vertices));
            assert_eq!(
                &actual[offset..offset + source.len()],
                source,
                "gathered primitive/index order"
            );
            offset += source.len();
        }
        assert_eq!(offset, actual.len());
        static TRACE: OnceLock<Option<std::path::PathBuf>> = OnceLock::new();
        if let Some(path) = TRACE
            .get_or_init(|| std::env::var_os("LOOKING_GLASS_ALPHA_RANGE_TRACE").map(Into::into))
        {
            use std::io::Write;
            let frame = crate::sim_maps::probe::frame();
            {
                let mut file = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(path)
                    .unwrap();
                writeln!(file,"{}",serde_json::json!({"frame":frame,"leader":id,"members":members,"vertex":format!("{:?}",vertex),"ibo":format!("{:?}",buffer),"base":base,"count":count,"bytes":count as usize*2,"source_bank":first_original.bank,"alpha_test":groups[prepared[id].group].recipe.stage.alpha_test,"images":groups[prepared[id].group].recipe.stage.images,"bsp_kinds":members.iter().map(|&m|groups[prepared[m].group].bsp_kinds).collect::<Vec<_>>(),"bsp_only":members.iter().map(|&m|groups[prepared[m].group].bsp_polygons_only).collect::<Vec<_>>(),"wave_normal":groups[prepared[id].group].recipe.wave_normal.is_some(),"split_control":world_list::negative("alpha-range-split-draws"),"original_ranges":members.iter().map(|&m| { let g=&groups[prepared[m].group]; (prepared[m].group,g.base_element,g.indices) }).collect::<Vec<_>>(),"verified":true})).unwrap();
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn one_upload_token_covers_same_frame_and_counter_wrap() {
        let previous = std::cell::Cell::new(None);
        for frame in [0, 1, u32::MAX, 0, 1] {
            assert!(claim_frame(&previous, frame));
            assert!(!claim_frame(&previous, frame));
        }
    }
    #[test]
    fn byte_count_and_triangle_arithmetic_is_checked() {
        assert_eq!(checked_end(0, 6, 6), Some(6));
        assert_eq!(checked_end(3, 6, 9), Some(9));
        for (base, count, limit) in [
            (0, 0, 9),
            (0, 4, 9),
            (6, 6, 9),
            (usize::MAX, 3, usize::MAX),
            (i32::MAX as usize, 3, usize::MAX),
            (0, i32::MAX as usize - 1, usize::MAX),
        ] {
            assert_eq!(checked_end(base, count, limit), None);
        }
    }
    #[test]
    fn final_joint_span_preserves_order_fences_state_and_budget() {
        use Source::*;
        let joint = [
            (0., World(3)),
            (0., World(1)),
            (0., Actor(0)),
            (0., World(2)),
            (0., World(4)),
            (0., Puff(0)),
            (0., World(5)),
            (0., World(6)),
        ];
        let state = |_: usize| Some(((17, 23, (-0_f32).to_bits(), [5_u32; 143]), 3));
        assert_eq!(span(&joint, 0, 99, state), (2, 6));
        assert_eq!(span(&joint, 3, 99, state), (5, 6));
        assert_eq!(span(&joint, 6, 99, state), (8, 6));
        assert_eq!(span(&joint, 0, 3, state), (1, 3));
        assert_eq!(span(&joint, 2, 99, state), (3, 0));
        assert_eq!(
            span(&joint, 0, 99, |id| Some(((id == 1, 23, [5_u32; 143]), 3))),
            (1, 3)
        );
        assert_eq!(
            span(&joint, 0, 99, |id| Some((
                (17, if id == 1 { 24 } else { 23 }, [5_u32; 143]),
                3
            ))),
            (1, 3)
        );
        assert_eq!(
            span(&joint, 0, 99, |id| Some((
                (17, 23, [if id == 1 { 6 } else { 5 }; 143]),
                3
            ))),
            (1, 3)
        );
        assert_eq!(
            span(&joint, 0, 99, |id| if id == 1 {
                None
            } else {
                Some((1, 3))
            }),
            (1, 3)
        );
        assert_eq!(span(&[(0., World(1))], 0, 99, state), (1, 3)); // frame boundary
    }
    #[test]
    fn construction_refuses_oversize_metadata_and_index_payloads() {
        let too_many = vec![(0, 0, 3); MAX_GROUPS + 1];
        assert!(Gather::new(&[(1, &[0, 0, 0])], &too_many, std::iter::repeat(true)).is_none());
        let too_large = vec![0_u16; MAX_INDEX_BYTES / 2 + 2];
        assert!(Gather::new(
            &[(1, &too_large)],
            &[(0, 0, too_large.len() as i32)],
            [true].into_iter()
        )
        .is_none());
    }
    #[test]
    fn immutable_sources_preserve_noncontiguous_banked_u16_indices() {
        let a = [60000, 59999, 59998, 1, 2, 3, 4, 5, 6];
        let b = [0, 2, 1];
        let g = Gather::new(
            &[(60001, &a), (3, &b)],
            &[(0, 6, 3), (1, 0, 3), (0, 0, 3)],
            [true, false, true].into_iter(),
        )
        .unwrap();
        assert_eq!(g.indices, [4, 5, 6, 60000, 59999, 59998]);
        assert_eq!(g.originals[0].as_ref().unwrap().bank, 0);
        assert!(g.originals[1].is_none());
        assert!(Gather::new(&[(60000, &a)], &[(0, 0, 3)], [true].into_iter()).is_none());
        assert!(Gather::new(&[(60001, &a)], &[(0, 7, 3)], [true].into_iter()).is_none());
    }
}

/// Explicit host-only framebuffer checkpoint. Capture at every logical callback endpoint;
/// compare the last original member against its joined run, not its early leader callback.
#[cfg(all(feature = "sim_maps_probe", not(target_os = "vita")))]
pub(super) struct PixelProbe {
    id: usize,
    group: usize,
    image: String,
    alpha_test: u8,
    mvp: [u32; 16],
    finish_projection: Option<[u32; 16]>,
    bounds: [[f32; 3]; 2],
    bsp_kinds: u8,
    bsp_polygons_only: bool,
}
#[cfg(all(feature = "sim_maps_probe", not(target_os = "vita")))]
impl PixelProbe {
    pub fn new(
        id: usize,
        prepared: &[Prepared],
        groups: &[Group],
        parameters: &[Parameter],
    ) -> Option<Self> {
        static ENABLED: OnceLock<bool> = OnceLock::new();
        if !*ENABLED.get_or_init(|| std::env::var_os("LOOKING_GLASS_ALPHA_PIXELS").is_some()) {
            return None;
        }
        static FRAME: OnceLock<u64> = OnceLock::new();
        if crate::sim_maps::probe::frame()
            != *FRAME.get_or_init(|| {
                std::env::var("LOOKING_GLASS_ALPHA_PIXELS_FRAME")
                    .unwrap_or("120".into())
                    .parse()
                    .unwrap()
            })
        {
            return None;
        }
        let group = prepared[id].group;
        let recipe = &groups[group].recipe;
        let probe = Self {
            id,
            group,
            image: recipe.stage.images.join(";"),
            alpha_test: recipe.stage.alpha_test,
            mvp: prepared[id]
                .uniforms
                .as_ref()
                .unwrap_or(&parameters[groups[group].parameter].uniforms)
                .mvp
                .map(f32::to_bits),
            finish_projection: crate::render_fx::projection()
                .map(|m| m.to_cols_array().map(f32::to_bits)),
            bounds: [groups[group].min.to_array(), groups[group].max.to_array()],
            bsp_kinds: groups[group].bsp_kinds,
            bsp_polygons_only: groups[group].bsp_polygons_only,
        };
        probe.record("before");
        Some(probe)
    }
    fn record(&self, phase: &str) {
        use sha2::{Digest, Sha256};
        use std::io::Write;
        let mut viewport = [0_i32; 4];
        unsafe {
            macroquad::miniquad::gl::glGetIntegerv(
                0x0ba2, // GL_VIEWPORT (not exported by the pinned GL bindings)
                viewport.as_mut_ptr(),
            );
        }
        let mut pixels = vec![0_u8; viewport[2] as usize * viewport[3] as usize * 4];
        unsafe {
            macroquad::miniquad::gl::glReadPixels(
                viewport[0],
                viewport[1],
                viewport[2],
                viewport[3],
                macroquad::miniquad::gl::GL_RGBA,
                macroquad::miniquad::gl::GL_UNSIGNED_BYTE,
                pixels.as_mut_ptr().cast(),
            );
        }
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(std::env::var_os("LOOKING_GLASS_ALPHA_PIXELS").unwrap())
            .unwrap();
        writeln!(file,"{}",serde_json::json!({"frame":crate::sim_maps::probe::frame(),"phase":phase,"id":self.id,"group":self.group,"image":self.image,"alpha_test":self.alpha_test,"mvp":self.mvp,"finish_projection":self.finish_projection,"bounds":self.bounds,"bsp_kinds":self.bsp_kinds,"bsp_polygons_only":self.bsp_polygons_only,"viewport":viewport,"sha256":format!("{:x}",Sha256::digest(&pixels))})).unwrap();
    }
}
#[cfg(all(feature = "sim_maps_probe", not(target_os = "vita")))]
impl Drop for PixelProbe {
    fn drop(&mut self) {
        self.record("after");
    }
}
