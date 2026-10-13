//! Whole actor/effect packets share one alpha order with immutable world groups.
//! Only dynamic LG meshes stream here; world buffers are never read or uploaded.
mod state_cache;
use state_cache::{vertices_equal, UploadPlan, UniformCache};
use super::{Frame, Pass, FRAME};
use crate::environment::Atmosphere;
use anyhow::{ensure, Context, Result};
use macroquad::{miniquad::*, prelude::*};
use std::collections::BTreeMap;

#[cfg(not(target_os = "vita"))]
thread_local! { static HOST_COUNTS:std::cell::RefCell<BTreeMap<&'static str,u64>>=std::cell::RefCell::default(); }
#[cfg(not(target_os = "vita"))]
pub(crate) fn host_report() -> serde_json::Value {
    HOST_COUNTS.with(|c| serde_json::to_value(&*c.borrow()).unwrap())
}
#[cfg(not(target_os = "vita"))]
pub(crate) fn host_reset() {
    HOST_COUNTS.with(|c| c.borrow_mut().clear());
}
pub(crate) fn count(name: &'static str, value: u64) {
    #[cfg(target_os = "vita")]
    crate::frame_profile::counts(&[(name, value)]);
    #[cfg(not(target_os = "vita"))]
    HOST_COUNTS.with(|c| *c.borrow_mut().entry(name).or_default() += value);
}
fn boundary(site: &'static str) {
    let _span = crate::frame_profile::span("gl_flush");
    let _site = crate::frame_profile::span(site);
    #[cfg(target_os = "vita")]
    crate::frame_profile::flush_boundary(site);
    unsafe {
        get_internal_gl().flush();
    }
    count("vita_3d_interop_flushes", 1);
}
pub(crate) fn begin_view(
    camera: &Camera3D,
    time: f32,
    atmosphere: &Atmosphere,
    fullbright: bool,
    flush: bool,
) {
    use macroquad::camera::Camera;
    #[cfg(not(target_os = "vita"))]
    if flush {
        host_reset();
    }
    if flush {
        boundary("flush_vita_3d_begin");
    }
    FRAME.with(|f| {
        *f.borrow_mut() = Frame {
            active: true,
            direct: true,
            camera: camera.position,
            direction: (camera.target - camera.position).normalize_or_zero(),
            time,
            atmosphere: atmosphere.clone(),
            fullbright,
            projection: Some(camera.matrix()),
            ..Default::default()
        }
    });
}
const UNIFORM_FLOATS: usize = 147;
fn meta() -> ShaderMeta {
    let mut uniforms = vec![
        UniformDesc::new("Model", UniformType::Mat4),
        UniformDesc::new("Projection", UniformType::Mat4),
    ];
    uniforms.extend(
        [
            ("AtlasGrid", UniformType::Float2),
            ("Fullbright", UniformType::Float1),
            ("Lit", UniformType::Float1),
            ("ClampUV", UniformType::Float1),
            ("Cutout", UniformType::Float1),
            ("CutoutReference", UniformType::Float1),
            ("Gain", UniformType::Float4),
            ("FogMode", UniformType::Float1),
            ("SkySize", UniformType::Float2),
            ("SkyFade", UniformType::Float1),
            ("FxModel", UniformType::Float1),
            ("FxAppearance", UniformType::Float4),
        ]
        .into_iter()
        .map(|(name, kind)| UniformDesc::new(name, kind)),
    );
    uniforms.extend(crate::lighting::uniforms());
    uniforms.extend(crate::environment::uniforms());
    ShaderMeta {
        images: vec!["Texture".into(), "LightAtlas".into(), "SkyBackdrop".into()],
        uniforms: UniformBlockLayout { uniforms },
    }
}
fn uniforms(p: &Pass, f: &Frame) -> [f32; UNIFORM_FLOATS] {
    let mut output = [0.; UNIFORM_FLOATS];
    let mut cursor = 0;
    let mut append = |values: &[f32]| {
        output[cursor..cursor + values.len()].copy_from_slice(values);
        cursor += values.len();
    };
    append(&Mat4::IDENTITY.to_cols_array());
    append(
        &f.projection
            .expect("actor camera projection")
            .to_cols_array(),
    );
    append(&[
        1.,
        1.,
        f32::from(f.fullbright),
        f32::from(p.lit),
        f32::from(p.clamp),
        p.cutout,
        p.cutout_reference,
    ]);
    append(&p.gain.to_array());
    append(&[p.fog, 1., 1., 0., f32::from(p.model)]);
    append(&[
        p.appearance.dissolve,
        f32::from(p.appearance.ghost),
        p.appearance.power,
        p.filter_fade,
    ]);
    for light in crate::lighting::packed(p.model, !f.fullbright && p.lit) {
        append(&light);
    }
    append(&f.camera.to_array());
    append(&f.atmosphere.distance.to_array());
    append(&f.atmosphere.liquid.to_array());
    for index in 0..2 {
        let (lo, hi, color) = f
            .atmosphere
            .volumes
            .get(index)
            .map_or(([0.; 3], [0.; 3], [0.; 4]), |v| {
                (v.min.to_array(), v.max.to_array(), v.color.to_array())
            });
        append(&lo);
        append(&hi);
        append(&color);
    }
    debug_assert_eq!(cursor, UNIFORM_FLOATS);
    output
}
#[derive(Default)]
struct Stats {
    draws: u64,
    triangles: u64,
    uniform_bytes: u64,
    upload_bytes: u64,
    uploads: u64,
    growths: u64,
    upload_hits: u64,
    upload_requests: u64, identical_uploads: u64, uniform_requests: u64, identical_uniforms: u64,
    index_skips: u64, uniform_skips: u64,
}
impl Stats {
    fn report(&self) {
        crate::frame_profile::route_fx_draws(self.draws);
        crate::frame_profile::route_gpu_states([self.upload_requests, self.identical_uploads, self.uniform_requests, self.identical_uniforms]);
        let values = [
            ("vita_fx_gpu_draws", self.draws),
            ("vita_fx_triangles", self.triangles),
            ("vita_fx_state_applications", self.draws),
            ("vita_fx_uniform_bytes", self.uniform_bytes),
            ("vita_fx_geometry_upload_bytes", self.upload_bytes),
            ("vita_fx_mesh_uploads", self.uploads),
            ("vita_fx_buffer_growths", self.growths),
            ("vita_fx_upload_cache_hits", self.upload_hits),
            ("gpu_probe_upload_requests", self.upload_requests), ("gpu_probe_identical_uploads", self.identical_uploads),
            ("gpu_probe_uniform_requests", self.uniform_requests), ("gpu_probe_identical_uniforms", self.identical_uniforms),
            ("vita_fx_index_uploads_skipped", self.index_skips), ("vita_fx_uniform_sets_skipped", self.uniform_skips),
        ];
        #[cfg(target_os = "vita")]
        {
            crate::frame_profile::counts(&values);
            crate::frame_profile::record_lg_submissions(self.draws);
        }
        #[cfg(not(target_os = "vita"))]
        for (name, value) in values {
            count(name, value);
        }
    }
}
struct Buffers {
    vertex: BufferId,
    index: BufferId,
    vertex_capacity: usize,
    index_capacity: usize,
    bindings: Bindings,
    previous_vertices: Vec<Vertex>,
    previous_indices: Vec<u16>,
}
fn make_buffers(
    ctx: &mut dyn RenderingBackend,
    vc: usize,
    ic: usize,
    ring: bool,
    white: TextureId,
) -> Buffers {
    let usage = if ring {
        BufferUsage::Dynamic
    } else {
        BufferUsage::Stream
    };
    let vertex = ctx.new_buffer(
        BufferType::VertexBuffer,
        usage,
        BufferSource::empty::<Vertex>(vc),
    );
    let index = ctx.new_buffer(
        BufferType::IndexBuffer,
        usage,
        BufferSource::empty::<u16>(ic),
    );
    Buffers {
        vertex,
        index,
        vertex_capacity: vc,
        index_capacity: ic,
        bindings: Bindings {
            vertex_buffers: vec![vertex],
            index_buffer: index,
            images: vec![white; 3],
        },
        previous_vertices: Vec::new(),
        previous_indices: Vec::new(),
    }
}
fn unchanged_merged(
    cached: &Mesh,
    groups: &[super::Group],
    target: usize,
    sources: &[usize],
) -> bool {
    let mut vertex = 0;
    let mut index = 0;
    for id in std::iter::once(target).chain(sources.iter().copied()) {
        let mesh = groups[id].passes[0].geometry();
        let v_end = vertex + mesh.vertices.len();
        let i_end = index + mesh.indices.len();
        if v_end > cached.vertices.len()
            || i_end > cached.indices.len()
            || !vertices_equal(&cached.vertices[vertex..v_end], &mesh.vertices)
            || !cached.indices[index..i_end]
                .iter()
                .zip(&mesh.indices)
                .all(|(&a, &b)| a as usize == b as usize + vertex)
        {
            return false;
        }
        vertex = v_end;
        index = i_end;
    }
    vertex == cached.vertices.len() && index == cached.indices.len()
}
pub(crate) struct Gpu {
    shader: ShaderId,
    pipelines: BTreeMap<(usize, bool), Pipeline>,
    buffers: Vec<Buffers>,
    white: Texture2D,
    ring_frame: usize,
    ring_phase: usize,
    stats: Stats,
    uniform_cache: UniformCache<UNIFORM_FLOATS>,
    merge_cache: [MergeCache; 4],
    merge_meshes: [BTreeMap<usize, Mesh>; 2],
}
impl Gpu {
    pub(crate) fn new() -> Result<Self> {
        let white = crate::texture::upload_rgba8(1, 1, &[255; 4]);
        let fragment =
            crate::lighting::fragment(&crate::environment::fragment(crate::render::FRAGMENT));
        let ctx = unsafe { get_internal_gl() }.quad_context;
        crate::frame_profile::shader_creation();
        let _shader_name = crate::shader_cache::Name::new("retained-actors-fx");
        let shader = ctx
            .new_shader(
                ShaderSource::Glsl {
                    vertex: crate::render::VERTEX,
                    fragment: &fragment,
                },
                meta(),
            )
            .context("retained actor shader")?;
        println!(
            "[lg/vita-renderer] actor READY grouped alpha, actor_max_layers={}",
            crate::render::vita_renderer::config().actor_max_layers
        );
        let mut gpu = Self {
            shader,
            pipelines: BTreeMap::new(),
            buffers: Vec::new(),
            white,
            ring_frame: 0,
            ring_phase: 0,
            stats: Stats::default(),
            uniform_cache: UniformCache::default(),
            merge_cache: Default::default(),
            merge_meshes: Default::default(),
        };
        // Pipeline objects share the one compiled program. Prewarm every LG blend
        // and depth key so first use of the held Blade does not create one later.
        for blend in 0..crate::render::BLEND_COUNT {
            for equal in [false, true] {
                gpu.prepare_pipeline((blend, equal));
            }
        }
        println!(
            "[lg/vita-renderer] actor pipeline objects prewarmed={} programs=1",
            gpu.pipelines.len()
        );
        Ok(gpu)
    }
    fn upload(&mut self, slot: usize, p: &Pass) -> Result<usize> {
        let mesh=p.geometry();
        ensure!(
            valid_mesh(mesh),
            "invalid actor range: {} vertices / {} indices",
            mesh.vertices.len(),
            mesh.indices.len()
        );
        // Resolve managed Texture2D IDs before borrowing the Macroquad context.
        let white = self.white.raw_miniquad_id();
        let texture = p
            .mesh
            .texture
            .as_ref()
            .map_or(white, Texture2D::raw_miniquad_id);
        let ring = crate::render::vita_renderer::config().fx_streaming == "ring";
        let slot = if ring {
            slot * 6 + (self.ring_frame % 3) * 2 + self.ring_phase
        } else {
            slot
        };
        let ctx = unsafe { get_internal_gl() }.quad_context;
        let vc = mesh.vertices.len().next_power_of_two();
        let ic = mesh.indices.len().next_power_of_two();
        while self.buffers.len() <= slot {
            self.buffers.push(make_buffers(ctx, vc, ic, ring, white));
            self.stats.growths += 1;
        }
        if self.buffers[slot].vertex_capacity < vc || self.buffers[slot].index_capacity < ic {
            let replacement = make_buffers(ctx, vc, ic, ring, white);
            let old = std::mem::replace(&mut self.buffers[slot], replacement);
            ctx.delete_buffer(old.vertex);
            ctx.delete_buffer(old.index);
            self.stats.growths += 1;
        }
        let b = &mut self.buffers[slot];
        b.bindings.images[0] = texture;
        // Adapted from NextOs-Ports/Looking-Glass-NextOS d59b16f (felc18-blip).
        // LG owns these buffers; capacity growth replaces them with empty history.
        let c = crate::render::vita_renderer::config();
        let track_vertex = c.fx_upload_cache || c.gpu_state_probe;
        let track_index = track_vertex || c.gpu_index_dedupe;
        let same_vertex = track_vertex && vertices_equal(&b.previous_vertices, &mesh.vertices);
        let same_index = track_index && b.previous_indices == mesh.indices;
        if c.gpu_state_probe || c.gpu_index_dedupe {
            self.stats.upload_requests += 2;
            self.stats.identical_uploads += u64::from(same_vertex) + u64::from(same_index);
        }
        let plan = UploadPlan::new(c.fx_upload_cache, c.gpu_index_dedupe, same_vertex, same_index);
        if plan.full_hit { self.stats.upload_hits += 1; }
        if plan.vertex {
            ctx.buffer_update(b.vertex, BufferSource::slice(&mesh.vertices));
            self.stats.upload_bytes += (mesh.vertices.len() * 40) as u64;
        }
        if plan.index {
            ctx.buffer_update(b.index, BufferSource::slice(&mesh.indices));
            self.stats.upload_bytes += (mesh.indices.len() * 2) as u64;
        } else if !plan.full_hit { self.stats.index_skips += 1; }
        if plan.vertex || plan.index { self.stats.uploads += 1; }
        if track_vertex && !same_vertex {
            b.previous_vertices.clear();
            b.previous_vertices.extend_from_slice(&mesh.vertices);
        }
        if track_index && !same_index { b.previous_indices.clone_from(&mesh.indices); }
        Ok(slot)
    }
    fn prepare_pipeline(&mut self, key: (usize, bool)) {
        let ctx = unsafe { get_internal_gl() }.quad_context;
        self.pipelines.entry(key).or_insert_with(|| {
            crate::frame_profile::pipeline_creation();
            ctx.new_pipeline(
                &[BufferLayout {
                    stride: 40,
                    ..Default::default()
                }],
                &[
                    VertexAttribute::new("position", VertexFormat::Float3),
                    VertexAttribute::new("texcoord", VertexFormat::Float2),
                    VertexAttribute::new("color0", VertexFormat::Byte4),
                    VertexAttribute::new("normal", VertexFormat::Float4),
                ],
                self.shader,
                PipelineParams {
                    depth_test: if key.1 {
                        Comparison::Equal
                    } else {
                        Comparison::LessOrEqual
                    },
                    ..crate::render::depth_pipeline(crate::render::blend(key.0))
                },
            )
        });
    }
    fn draw(&mut self, p: &Pass, slot: usize, u: &[f32; UNIFORM_FLOATS], depth_write: bool) {
        let key = (p.blend_index, p.depth_equal);
        let pipeline = self
            .pipelines
            .get(&key)
            .expect("prewarmed LG actor pipeline");
        // Several actor layers can share one uploaded vertex/index page.
        // Texture bindings still belong to the individual layer packet.
        self.buffers[slot].bindings.images[0]=p.mesh.texture.as_ref().map_or_else(||self.white.raw_miniquad_id(),Texture2D::raw_miniquad_id);
        let ctx = unsafe { get_internal_gl() }.quad_context;
        ctx.apply_pipeline(pipeline);
        unsafe {
            macroquad::miniquad::gl::glDepthMask(u8::from(depth_write));
        }
        ctx.apply_bindings(&self.buffers[slot].bindings);
        // Inspired by NextOs-Ports/Looking-Glass-NextOS 668de14 (felc18-blip).
        // Only consecutive complete payloads of this private program qualify;
        // world callbacks and each finish/view reset the cache. Bindings always apply.
        let c = crate::render::vita_renderer::config();
        let same = (c.gpu_state_probe || c.gpu_uniform_dedupe) && self.uniform_cache.observe(u);
        if c.gpu_state_probe || c.gpu_uniform_dedupe {
            self.stats.uniform_requests += 1;
            self.stats.identical_uniforms += u64::from(same);
        }
        if c.gpu_uniform_dedupe && same { self.stats.uniform_skips += 1; }
        else {
            ctx.apply_uniforms(UniformsSource::table(u));
            self.stats.uniform_bytes += std::mem::size_of_val(u) as u64;
        }
        ctx.draw(0, p.geometry().indices.len() as i32, 1);
        self.stats.draws += 1;
        self.stats.triangles += p.geometry().indices.len() as u64 / 3;
    }
}
impl Drop for Gpu {
    fn drop(&mut self) {
        let ctx = unsafe { get_internal_gl() }.quad_context;
        for b in &self.buffers {
            ctx.delete_buffer(b.vertex);
            ctx.delete_buffer(b.index);
        }
        for p in self.pipelines.values() {
            ctx.delete_pipeline(*p);
        }
        ctx.delete_shader(self.shader);
    }
}
fn valid_mesh(mesh: &Mesh) -> bool {
    !mesh.vertices.is_empty()
        && !mesh.indices.is_empty()
        && mesh.vertices.len() <= 65536
        && mesh.indices.len() % 3 == 0
        && mesh.indices.len() <= i32::MAX as usize
        && mesh
            .indices
            .iter()
            .all(|&i| (i as usize) < mesh.vertices.len())
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Source {
    World(usize),
    Actor(usize),
}
fn order(mut list: Vec<(f32, Source)>, sort: bool) -> Vec<(f32, Source)> {
    // Stable sort keeps authored world layer ties and actor group submission ties.
    if sort {
        list.sort_by(|a, b| b.0.total_cmp(&a.0));
    }
    list
}
fn center(mesh: &Mesh) -> Vec3 {
    let (lo, hi) = mesh.vertices.iter().fold(
        (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY)),
        |(lo, hi), v| (lo.min(v.position), hi.max(v.position)),
    );
    (lo + hi) * 0.5
}
#[cfg(test)]
fn same_packet(a: &Pass, b: &Pass) -> bool {
    a.blend_index == b.blend_index
        && a.depth_equal == b.depth_equal
        && a.mesh.texture.as_ref().map(Texture2D::raw_miniquad_id)
            == b.mesh.texture.as_ref().map(Texture2D::raw_miniquad_id)
        && a.lit == b.lit
        && a.clamp == b.clamp
        && a.cutout == b.cutout
        && a.cutout_reference == b.cutout_reference
        && a.gain == b.gain
        && a.fog == b.fog
        && a.model == b.model
        && a.appearance == b.appearance
        && a.filter_fade == b.filter_fade
}
#[derive(Clone, PartialEq)]
struct PacketKey {
    texture: Option<TextureId>,
    state: [u32; 20],
}
fn packet_key(p: &Pass) -> PacketKey {
    PacketKey {
        texture: p.mesh.texture.as_ref().map(Texture2D::raw_miniquad_id),
        state: [
            p.blend_index as u32,
            u32::from(p.depth_equal),
            u32::from(p.lit),
            u32::from(p.clamp),
            p.cutout.to_bits(),
            p.cutout_reference.to_bits(),
            p.gain.x.to_bits(),
            p.gain.y.to_bits(),
            p.gain.z.to_bits(),
            p.gain.w.to_bits(),
            p.fog.to_bits(),
            u32::from(p.model),
            p.appearance.dissolve.to_bits(),
            u32::from(p.appearance.ghost),
            p.appearance.power.to_bits(),
            p.filter_fade.to_bits(),
            u32::from(p.effect),
            0,
            0,
            0,
        ],
    }
}
#[derive(Clone, PartialEq)]
struct Signature {
    source: Source,
    key: Option<PacketKey>,
    vertices: usize,
    indices: usize,
    particle: bool,
}
#[derive(Default)]
struct MergeCache {
    signature: Vec<Signature>,
    steps: Vec<(usize, usize)>,
    kept: Vec<Source>,
}
fn merge_cached(
    cache: &mut MergeCache,
    groups: &mut [super::Group],
    list: &mut Vec<(f32, Source)>,
    material: bool,
    reuse: bool,
    pool: &mut BTreeMap<usize, Mesh>,
) -> Vec<usize> {
    // Only packet topology/state is cached. Animated vertices always stream anew.
    // A world packet or actor is a barrier for particle material regrouping.
    let signature = list
        .iter()
        .map(|&(_, source)| {
            if let Source::Actor(id) = source {
                let g = &groups[id];
                let p = g.passes.first();
                let eligible = g.passes.len() == 1 && p.is_some_and(|p| valid_mesh(p.geometry()) && (p.effect || crate::render::vita_renderer::config().actor_batching));
                Signature {
                    source,
                    key: eligible.then(|| packet_key(p.unwrap())),
                    vertices: p.map_or(0, |p| p.geometry().vertices.len()),
                    indices: p.map_or(0, |p| p.geometry().indices.len()),
                    particle: p.is_some_and(|p| p.effect),
                }
            } else {
                Signature {
                    source,
                    key: None,
                    vertices: 0,
                    indices: 0,
                    particle: false,
                }
            }
        })
        .collect::<Vec<_>>();
    let hit = reuse && cache.signature == signature;
    if hit {
        count("vita_fx_merge_plan_hits", 1);
    } else {
        count("vita_fx_merge_plan_builds", 1);
        cache.steps.clear();
        cache.kept.clear();
        let mut candidates: Vec<(usize, usize, usize, PacketKey, bool)> = Vec::new();
        for item in &signature {
            let Source::Actor(id) = item.source else {
                candidates.clear();
                cache.kept.push(item.source);
                continue;
            };
            let Some(key) = &item.key else {
                candidates.clear();
                cache.kept.push(item.source);
                continue;
            };
            if !material || !item.particle || candidates.last().is_some_and(|c| !c.4) {
                if candidates.len() > 1 {
                    let last = candidates.pop().unwrap();
                    candidates.clear();
                    candidates.push(last);
                }
            }
            let found = candidates.iter().position(|c| {
                c.3 == *key && c.1 + item.vertices <= 60000 && c.2 + item.indices <= 180000
            });
            if let Some(i) = found {
                let c = &mut candidates[i];
                cache.steps.push((c.0, id));
                c.1 += item.vertices;
                c.2 += item.indices;
            } else {
                if !material || !item.particle {
                    candidates.clear();
                }
                candidates.push((id, item.vertices, item.indices, key.clone(), item.particle));
                cache.kept.push(item.source);
            }
        }
        cache.signature = signature;
    }
    let mut destinations = Vec::new();
    for &(target, _) in &cache.steps {
        if !destinations.contains(&target) {
            destinations.push(target);
        }
    }
    for &target in &destinations {
        let sources = cache
            .steps
            .iter()
            .filter_map(|&(t, s)| (t == target).then_some(s))
            .collect::<Vec<_>>();
        if let Some(mut cached) = pool.remove(&target) {
            if hit && unchanged_merged(&cached, groups, target, &sources) {
                groups[target].passes[0].geometry=None;
                groups[target].passes[0].mesh = cached;
                for &source in &sources {
                    groups[source].passes.clear();
                }
                count("vita_fx_unchanged_merged_groups", 1);
                continue;
            }
            let old = groups[target].passes[0].geometry();
            cached.vertices.clear();
            cached.indices.clear();
            cached.texture = groups[target].passes[0].mesh.texture.clone();
            cached.vertices.extend_from_slice(&old.vertices);
            cached.indices.extend_from_slice(&old.indices);
            groups[target].passes[0].geometry=None;
            groups[target].passes[0].mesh = cached;
        }
        groups[target].passes[0].own_geometry();
        for source in sources {
            let mut source = groups[source].passes.pop().expect("merge plan source");
            source.own_geometry();
            let dest = &mut groups[target].passes[0].mesh;
            let offset = dest.vertices.len() as u16;
            dest.vertices.append(&mut source.mesh.vertices);
            dest.indices
                .extend(source.mesh.indices.into_iter().map(|i| i + offset));
            if !source.effect {count("vita_actor_batched_packets",1);}
        }
    }
    count("vita_fx_merged_meshes", cache.steps.len() as u64);
    list.clear();
    list.extend(cache.kept.iter().map(|&s| (0., s)));
    destinations
}
/// Coalesce only adjacent whole dynamic meshes with identical shader state.
/// The joint order is already final: a world group or layered actor is a barrier.
/// This offsets actor indices once, never reconstructs or reads world geometry.
#[cfg(test)]
fn merge_adjacent(groups: &mut [super::Group], list: &mut Vec<(f32, Source)>) {
    let mut previous = None;
    let mut kept = 0;
    for i in 0..list.len() {
        let item = list[i];
        let Source::Actor(id) = item.1 else {
            previous = None;
            list[kept] = item;
            kept += 1;
            continue;
        };
        let can_merge = previous.is_some_and(|previous: usize| {
            let (a, b) = (&groups[previous].passes, &groups[id].passes);
            a.len() == 1
                && b.len() == 1
                && valid_mesh(&a[0].mesh)
                && valid_mesh(&b[0].mesh)
                && a[0].mesh.vertices.len() + b[0].mesh.vertices.len() <= 60000
                && a[0].mesh.indices.len() + b[0].mesh.indices.len() <= 180000
                && same_packet(&a[0], &b[0])
        });
        if can_merge {
            let mut source = groups[id].passes.pop().unwrap();
            let target = &mut groups[previous.unwrap()].passes[0].mesh;
            let offset = target.vertices.len() as u16;
            count(
                "vita_fx_appended_vertices",
                source.mesh.vertices.len() as u64,
            );
            count("vita_fx_appended_indices", source.mesh.indices.len() as u64);
            target.vertices.append(&mut source.mesh.vertices);
            target
                .indices
                .extend(source.mesh.indices.into_iter().map(|i| i + offset));
            count("vita_fx_merged_meshes", 1);
        } else {
            previous = Some(id);
            list[kept] = item;
            kept += 1;
        }
    }
    list.truncate(kept);
}
#[cfg(not(target_os = "vita"))]
pub(crate) fn finish(
    gpu: &mut Gpu,
    world: &[(f32, usize)],
    flush: bool,
    mut draw_world: impl FnMut(usize),
) -> (usize, usize) {
    finish_world(gpu, world, flush, &mut draw_world, None)
}
pub(crate) fn finish_world(
    gpu: &mut Gpu, world: &[(f32, usize)], flush: bool, mut draw_world: impl FnMut(usize),
    world_totals: Option<&std::cell::RefCell<crate::frame_profile::Totals<3>>>,
) -> (usize, usize) {
    let _span = crate::frame_profile::span("vita_3d_finish");
    if flush {
        gpu.ring_frame += 1;
        gpu.ring_phase = 0;
    } else {
        gpu.ring_phase = 1;
    }
    gpu.stats = Stats::default();
    gpu.uniform_cache.interrupt(); // Probe only uninterrupted draws of this owned program.
    let (mut groups, camera, direction, dropped) = FRAME.with(|f| {
        let mut f = f.borrow_mut();
        (
            std::mem::take(&mut f.groups),
            f.camera,
            f.direction,
            f.dropped,
        )
    });
    count(
        "vita_fx_particle_input_groups",
        groups
            .iter()
            .filter(|g| g.passes.iter().all(|p| p.effect))
            .count() as u64,
    );
    if flush {
        boundary("flush_vita_3d_packets");
    }
    let mut list = {
        let _span = crate::frame_profile::span("vita_joint_alpha_sort");
        let mut list = world
            .iter()
            .map(|&(d, id)| (d, Source::World(id)))
            .collect::<Vec<_>>();
        list.extend(
            groups
                .iter()
                .enumerate()
                .filter(|(_, g)| g.transparent && !g.passes[0].geometry().vertices.is_empty())
                .map(|(id, g)| {
                    (
                        (center(g.passes[0].geometry()) - camera).dot(direction),
                        Source::Actor(id),
                    )
                }),
        );
        order(list, crate::render::vita_renderer::config().sort_alpha)
    };
    let mut recycled = Vec::new();
    let c = crate::render::vita_renderer::config();
    if (c.merge_fx && c.fx_batching != "none") || c.actor_batching {
        let _span = crate::frame_profile::span("vita_fx_merge_groups");
        let mut opaque = groups
            .iter()
            .enumerate()
            .filter(|(_, g)| !g.transparent)
            .map(|(id, _)| (0., Source::Actor(id)))
            .collect::<Vec<_>>();
        for (which, order) in [&mut list, &mut opaque].into_iter().enumerate() {
            let destinations = merge_cached(
                &mut gpu.merge_cache[gpu.ring_phase * 2 + which],
                &mut groups,
                order,
                c.fx_batching == "material",
                c.fx_merge_cache,
                &mut gpu.merge_meshes[gpu.ring_phase],
            );
            recycled.extend(destinations);
        }
    }
    let mut packets = Vec::new();
    let mut ranges = Vec::with_capacity(groups.len());
    let mut invalid = 0;
    {
        let _span = crate::frame_profile::span("vita_fx_upload");
        let mut shared_uploads=std::collections::HashMap::<usize,usize>::new();
        for group in &groups {
            let start = packets.len();
            for p in &group.passes {
                if p.geometry().indices.is_empty() {
                    continue;
                }
                let shared=p.geometry.as_ref().map(|m|std::rc::Rc::as_ptr(m) as usize);
                let previous=shared.and_then(|id|shared_uploads.get(&id).copied());
                let upload=if let Some(b)=previous {count("vita_actor_geometry_uploads_skipped",1);Ok(b)}else{gpu.upload(packets.len(),p)};
                match upload {
                    Ok(b) => {
                        if let Some(id)=shared {shared_uploads.insert(id,b);}
                        let u = FRAME.with(|f| uniforms(p, &f.borrow()));
                        packets.push((p, b, u));
                    }
                    Err(e) => {
                        eprintln!("[lg/vita-renderer] rejected actor packet: {e:#}");
                        invalid += 1;
                    }
                }
            }
            ranges.push(start..packets.len());
        }
    }
    count("vita_fx_capacity_samples", 1);
    count(
        "vita_fx_stream_capacity_bytes",
        gpu.buffers
            .iter()
            .map(|b| b.vertex_capacity * 40 + b.index_capacity * 2)
            .sum::<usize>() as u64,
    );
    count("vita_fx_invalid_packets", invalid as u64);
    count("vita_fx_dropped_groups", dropped as u64);
    count("vita_fx_sorted_triangles", 0);
    count("vita_fx_compacted_vertices", 0);
    let draw_group = |gpu: &mut Gpu, id: usize| {
        for i in ranges[id].clone() {
            let (p, b, u) = &packets[i];
            gpu.draw(p, *b, u, !groups[id].transparent);
        }
    };
    unsafe {
        crate::render::vita_renderer::begin_3d_pass(PassAction::Nothing);
        macroquad::miniquad::gl::glDepthMask(1);
    }
    {
        let _span = crate::frame_profile::span("vita_fx_opaque_submit");
        for (id, g) in groups.iter().enumerate().filter(|(_, g)| !g.transparent) {
            let _ = g;
            draw_group(gpu, id);
        }
    }
    count("vita_joint_alpha_groups", list.len() as u64);
    unsafe {
        macroquad::miniquad::gl::glDepthMask(0);
    }
    {
        let _span = crate::frame_profile::span("vita_joint_alpha_submit");
        let mut world_ms = 0.;
        for (_, source) in &list {
            match *source {
                Source::World(id) => {
                    gpu.uniform_cache.interrupt();
                    let start = world_totals.and_then(|t| t.borrow().start());
                    draw_world(id);
                    world_ms += crate::frame_profile::Totals::<3>::elapsed(start);
                },
                Source::Actor(id) => draw_group(gpu, id),
            }
        }
        if let Some(totals) = world_totals {
            totals.borrow().publish_under("vita_world_alpha_submit", world_ms,
                ["vita_world_alpha_state_bindings", "vita_world_alpha_uniforms", "vita_world_alpha_draws"]);
        }
    }
    unsafe {
        macroquad::miniquad::gl::glDepthMask(1);
        get_internal_gl().quad_context.end_render_pass();
    }
    FRAME.with(|f| {
        let mut f = f.borrow_mut();
        f.active = false;
        f.direct = false;
        f.retained.clear();
    });
    gl_use_default_material();
    drop(packets);
    for &id in &recycled {
        if let Some(p) = groups[id].passes.first_mut() {
            gpu.merge_meshes[gpu.ring_phase].insert(
                id,
                std::mem::replace(
                    &mut p.mesh,
                    Mesh {
                        vertices: Vec::new(),
                        indices: Vec::new(),
                        texture: None,
                    },
                ),
            );
        }
    }
    gpu.merge_meshes[gpu.ring_phase].retain(|id, _| recycled.contains(id));
    gpu.stats.report();
    (list.len(), dropped + invalid)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn uniform_and_vertex_layout_preserve_legacy_shader() {
        assert_eq!(std::mem::size_of::<Vertex>(), 40);
        assert_eq!(std::mem::offset_of!(Vertex, normal), 24);
        assert_eq!(
            meta()
                .uniforms
                .uniforms
                .iter()
                .map(|u| u.uniform_type.size() * u.array_count)
                .sum::<usize>(),
            UNIFORM_FLOATS * 4
        );
    }
    #[test]
    fn world_actor_effect_order_and_authored_ties() {
        let input = vec![
            (20., Source::World(0)),
            (20., Source::World(1)),
            (30., Source::Actor(0)),
            (10., Source::Actor(1)),
        ];
        let sorted = order(input.clone(), true);
        assert_eq!(
            sorted.iter().map(|p| p.1).collect::<Vec<_>>(),
            vec![
                Source::Actor(0),
                Source::World(0),
                Source::World(1),
                Source::Actor(1)
            ]
        );
        assert_eq!(order(input.clone(), false), input);
    }
    #[test]
    fn actor_ranges_reject_truncated_and_out_of_bounds_indices() {
        let mut m = Mesh {
            vertices: vec![Vertex::new(0., 0., 0., 0., 0., WHITE); 3],
            indices: vec![0, 1, 2],
            texture: None,
        };
        assert!(valid_mesh(&m));
        m.indices.push(0);
        assert!(!valid_mesh(&m));
        m.indices = vec![0, 1, 3];
        assert!(!valid_mesh(&m));
    }
    fn fixture_group() -> super::super::Group {
        super::super::Group {
            transparent: true,
            passes: vec![Pass {
                geometry:None,
                effect: false,
                blend_index: 1,
                depth_equal: false,
                mesh: Mesh {
                    vertices: vec![Vertex::new(0., 0., 0., 0., 0., WHITE); 3],
                    indices: vec![0, 1, 2],
                    texture: None,
                },
                material: None,
                lit: false,
                clamp: false,
                cutout: 0.,
                cutout_reference: 0.5,
                gain: Vec4::ONE,
                fog: 0.,
                model: true,
                appearance: Default::default(),
                filter_fade: 0.,
            }],
        }
    }
    #[test]
    fn shared_actor_layers_merge_without_mutating_source_geometry() {
        let mut groups=vec![fixture_group(),fixture_group()];
        let shared=std::rc::Rc::new(super::super::copy_mesh(&groups[0].passes[0].mesh));
        for g in &mut groups {
            g.passes[0].geometry=Some(shared.clone());
            g.passes[0].mesh.vertices.clear();g.passes[0].mesh.indices.clear();
        }
        let mut cache=MergeCache::default();let mut pool=BTreeMap::new();
        let mut list=vec![(2.,Source::Actor(0)),(1.,Source::Actor(1))];
        merge_cached(&mut cache,&mut groups,&mut list,false,true,&mut pool);
        assert_eq!(list,vec![(0.,Source::Actor(0))]);
        assert_eq!(groups[0].passes[0].geometry().indices,vec![0,1,2,3,4,5]);
        assert_eq!(shared.indices,vec![0,1,2]);
        assert_eq!(shared.vertices.len(),3);
    }
    #[test]
    fn coalescing_keeps_whole_mesh_order_and_world_barriers() {
        let mut groups = vec![fixture_group(), fixture_group(), fixture_group()];
        let mut list = vec![
            (3., Source::Actor(0)),
            (2., Source::Actor(1)),
            (1.5, Source::World(0)),
            (1., Source::Actor(2)),
        ];
        merge_adjacent(&mut groups, &mut list);
        assert_eq!(
            list.iter().map(|p| p.1).collect::<Vec<_>>(),
            vec![Source::Actor(0), Source::World(0), Source::Actor(2)]
        );
        assert_eq!(groups[0].passes[0].mesh.indices, vec![0, 1, 2, 3, 4, 5]);
        assert!(groups[1].passes.is_empty());
        assert_eq!(groups[2].passes[0].mesh.indices, vec![0, 1, 2]);
        // Layered/material differences must not be folded into one draw.
        groups = vec![fixture_group(), fixture_group()];
        groups[1].passes[0].appearance.ghost = true;
        let mut list = vec![(2., Source::Actor(0)), (1., Source::Actor(1))];
        merge_adjacent(&mut groups, &mut list);
        assert_eq!(list.len(), 2);
        groups[1].passes[0].appearance.ghost = false;
        groups[1].passes[0].mesh.indices[2] = 65535;
        merge_adjacent(&mut groups, &mut list);
        assert_eq!(list.len(), 2);
    }
    #[test]
    fn material_batch_cache_preserves_world_actor_barriers_and_streams_new_vertices() {
        let make = || {
            let mut g = (0..5).map(|_| fixture_group()).collect::<Vec<_>>();
            for x in &mut g {
                x.passes[0].effect = true;
            }
            g[1].passes[0].blend_index = 5;
            g
        };
        let order = vec![
            (5., Source::Actor(0)),
            (4., Source::Actor(1)),
            (3., Source::Actor(2)),
            (2., Source::World(0)),
            (1., Source::Actor(3)),
        ];
        let mut cache = MergeCache::default();
        let mut groups = make();
        let mut list = order.clone();
        merge_cached(
            &mut cache,
            &mut groups,
            &mut list,
            true,
            true,
            &mut BTreeMap::new(),
        );
        assert_eq!(
            list.iter().map(|p| p.1).collect::<Vec<_>>(),
            vec![
                Source::Actor(0),
                Source::Actor(1),
                Source::World(0),
                Source::Actor(3)
            ]
        );
        assert_eq!(groups[0].passes[0].mesh.indices, vec![0, 1, 2, 3, 4, 5]);
        let mut pool = BTreeMap::new();
        let merged = std::mem::replace(
            &mut groups[0].passes[0].mesh,
            Mesh {
                vertices: Vec::new(),
                indices: Vec::new(),
                texture: None,
            },
        );
        pool.insert(0, merged);
        let mut unchanged = make();
        let mut unchanged_list = order.clone();
        merge_cached(
            &mut cache,
            &mut unchanged,
            &mut unchanged_list,
            true,
            true,
            &mut pool,
        );
        assert_eq!(unchanged[0].passes[0].mesh.indices, vec![0, 1, 2, 3, 4, 5]);
        assert!(unchanged[2].passes.is_empty());
        let mut fresh = make();
        fresh[2].passes[0].mesh.vertices[0].position.x = 42.;
        let mut list = order;
        merge_cached(
            &mut cache,
            &mut fresh,
            &mut list,
            true,
            true,
            &mut BTreeMap::new(),
        );
        assert_eq!(fresh[0].passes[0].mesh.vertices[3].position.x, 42.);
        let mut fresh = make();
        fresh[1].passes[0].effect = false;
        let mut list = vec![
            (3., Source::Actor(0)),
            (2., Source::Actor(1)),
            (1., Source::Actor(2)),
        ];
        merge_cached(
            &mut cache,
            &mut fresh,
            &mut list,
            true,
            true,
            &mut BTreeMap::new(),
        );
        assert_eq!(list.len(), 3, "actor packet is a particle material barrier");
    }
    #[test]
    fn uniform_packets_keep_actor_appearance_and_depth_independent() {
        let mut group = fixture_group();
        let p = &mut group.passes[0];
        p.appearance = super::super::Appearance {
            dissolve: 0.4,
            ghost: true,
            power: 2.,
        };
        p.filter_fade = 0.25;
        let f = Frame {
            projection: Some(Mat4::IDENTITY),
            ..Default::default()
        };
        let u = uniforms(p, &f);
        assert_eq!(&u[48..52], &[0.4, 1., 2., 0.25]);
        p.appearance = Default::default();
        p.filter_fade = 0.;
        assert_eq!(&uniforms(p, &f)[48..52], &[0.; 4]);
    }
    #[test]
    fn deferred_depth_scope_restores_nested_and_empty_calls() {
        FRAME.with(|f| {
            *f.borrow_mut() = Frame {
                active: true,
                direct: true,
                ..Default::default()
            }
        });
        super::super::retained_depth_scope(|| {
            assert!(FRAME.with(|f| f.borrow().depth_read_only));
            super::super::retained_depth_scope(|| {});
            assert!(FRAME.with(|f| f.borrow().depth_read_only));
        });
        assert!(!FRAME.with(|f| f.borrow().depth_read_only));
        FRAME.with(|f| *f.borrow_mut() = Frame::default());
    }
}
