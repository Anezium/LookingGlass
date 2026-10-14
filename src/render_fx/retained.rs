//! Whole actor/effect packets share one alpha order with immutable world groups.
//! Only dynamic LG meshes stream here; world buffers are never read or uploaded.
pub(super) mod state_cache;
pub(super) mod world_puffs;
use state_cache::{vertices_equal, UploadPlan, UniformCache, SubmitCache};
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
    HOST_COUNTS.with(|c| { for value in c.borrow_mut().values_mut() { *value = 0; } });
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
        let mut f = f.borrow_mut();
        let mut groups = std::mem::take(&mut f.groups);
        groups.clear();
        let group_pool = std::mem::take(&mut f.group_pool);
        let mut world_puffs = std::mem::take(&mut f.world_puffs);
        world_puffs.clear();
        *f = Frame {
            active: true,
            direct: true,
            groups, group_pool, world_puffs,
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
/// Frame constants and lighting are expanded at most once per model/lit pair.
/// Material/appearance fields remain live per pass; the whole payload still uploads.
fn prepared_uniforms(p: &Pass, templates: &mut [Option<[f32; UNIFORM_FLOATS]>; 4]) -> [f32; UNIFORM_FLOATS] {
    let key=usize::from(p.model)*2+usize::from(p.lit);
    let mut u=*templates[key].get_or_insert_with(|| FRAME.with(|f|uniforms(p,&f.borrow())));
    u[35]=f32::from(p.lit); u[36]=f32::from(p.clamp); u[37]=p.cutout; u[38]=p.cutout_reference;
    u[39..43].copy_from_slice(&p.gain.to_array()); u[43]=p.fog; u[47]=f32::from(p.model);
    u[48..52].copy_from_slice(&[p.appearance.dissolve,f32::from(p.appearance.ghost),p.appearance.power,p.filter_fade]);
    u
}
#[derive(Default)]
struct Stats {
    draws: u64,
    triangles: u64,
    uniform_bytes: u64,
    upload_bytes: u64,
    upload_gl_us: u64,
    uploads: u64,
    growths: u64,
    upload_hits: u64,
    upload_requests: u64, identical_uploads: u64, uniform_requests: u64, identical_uniforms: u64,
    index_skips: u64, uniform_skips: u64,
    pipeline_sets: u64, binding_sets: u64,
    render: [crate::light_profile::RenderMetrics; 2],
}
impl Stats {
    fn report(&self) {
        crate::light_profile::render_add(crate::light_profile::RenderPhase::Actors, self.render[0]);
        crate::light_profile::render_add(crate::light_profile::RenderPhase::Fx, self.render[1]);
        crate::frame_profile::route_fx_draws(self.draws);
        crate::frame_profile::route_gpu_states([self.upload_requests, self.identical_uploads, self.uniform_requests, self.identical_uniforms]);
        let values = [
            ("vita_fx_gpu_draws", self.draws),
            ("vita_fx_triangles", self.triangles),
            ("vita_fx_state_applications", self.pipeline_sets),
            ("vita_fx_pipeline_depth_sets_skipped", self.draws - self.pipeline_sets),
            ("vita_fx_binding_applications", self.binding_sets),
            ("vita_fx_binding_sets_skipped", self.draws - self.binding_sets),
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
    version: Option<super::prepared::Version>,
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
        version: None,
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
    _game_thread: crate::game_thread::GameThread,
    shader: ShaderId,
    pipelines: BTreeMap<(usize, bool), Pipeline>,
    buffers: Vec<Buffers>,
    white: Texture2D,
    ring_frame: usize,
    ring_phase: usize,
    stats: Stats,
    preparation: [Vec<super::prepared::Cache>; 2],
    packet_storage: Vec<(usize, usize, usize, [f32; UNIFORM_FLOATS])>,
    range_storage: Vec<std::ops::Range<usize>>,
    shared_storage: std::collections::HashMap<usize, usize>,
    alpha_storage: Vec<(f32, Source)>,
    opaque_storage: Vec<(f32, Source)>,
    recycle_storage: Vec<usize>,
    uniform_cache: UniformCache<UNIFORM_FLOATS>,
    field_uniforms: crate::render::uniform_fields::Cache,
    submit_cache: SubmitCache<(usize, bool, bool), (BufferId, BufferId, [TextureId; 3]), UNIFORM_FLOATS>,
    merge_cache: [MergeCache; 4],
    merge_meshes: [BTreeMap<usize, super::prepared::Geometry>; 2],
    puff_batches: [world_puffs::Bank; 2],
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
            _game_thread: crate::game_thread::GameThread::new(),
            shader,
            pipelines: BTreeMap::new(),
            buffers: Vec::new(),
            white,
            ring_frame: 0,
            ring_phase: 0,
            stats: Stats::default(),
            preparation: Default::default(), packet_storage: Vec::new(),
            range_storage: Vec::new(), shared_storage: Default::default(),
            alpha_storage: Vec::new(), opaque_storage: Vec::new(), recycle_storage: Vec::new(),
            uniform_cache: UniformCache::default(), submit_cache: SubmitCache::default(),
            field_uniforms: crate::render::uniform_fields::Cache::new(meta()),
            merge_cache: Default::default(),
            merge_meshes: Default::default(),
            puff_batches: Default::default(),
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
        crate::game_thread::assert_current();
        let mesh=p.geometry();
        ensure!(
            if crate::render::vita_renderer::config().fx_prepare_cache {
                p.geometry.as_ref().map_or_else(|| valid_mesh(mesh), |g| g.valid)
            } else { valid_mesh(mesh) },
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
            ring_slot(slot, self.ring_frame, self.ring_phase,
                crate::render::vita_renderer::config().fx_ring_safe)
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
        let version = if c.fx_prepare_cache { p.geometry.as_ref().map(|g| g.version) } else { None };
        let track_vertex = c.fx_upload_cache || c.gpu_state_probe || version.is_some();
        let track_index = track_vertex || c.gpu_index_dedupe;
        let same_vertex = if let Some(v) = version { b.version.is_some_and(|old| old.identity == v.identity && old.vertices == v.vertices) }
            else { track_vertex && vertices_equal(&b.previous_vertices, &mesh.vertices) };
        let same_index = if let Some(v) = version { b.version.is_some_and(|old| old.identity == v.identity && old.topology == v.topology) }
            else { track_index && b.previous_indices == mesh.indices };
        // Generation equality is sufficient only within the same publication identity.
        if version.is_some() && same_vertex && same_index { count("vita_fx_prepare_upload_hits", 1); }
        if c.gpu_state_probe || c.gpu_index_dedupe {
            self.stats.upload_requests += 2;
            self.stats.identical_uploads += u64::from(same_vertex) + u64::from(same_index);
        }
        let plan = UploadPlan::new(c.fx_upload_cache || version.is_some(), c.gpu_index_dedupe || version.is_some(), same_vertex, same_index);
        if plan.full_hit { self.stats.upload_hits += 1; }
        let gl_started = std::time::Instant::now();
        let metrics = &mut self.stats.render[usize::from(p.effect)];
        metrics.buffer_updates += u64::from(plan.vertex) + u64::from(plan.index);
        metrics.buffer_bytes += u64::from(plan.vertex) * (mesh.vertices.len() * 40) as u64
            + u64::from(plan.index) * (mesh.indices.len() * 2) as u64;
        if plan.vertex {
            ctx.buffer_update(b.vertex, BufferSource::slice(&mesh.vertices));
            self.stats.upload_bytes += (mesh.vertices.len() * 40) as u64;
        }
        if plan.index {
            ctx.buffer_update(b.index, BufferSource::slice(&mesh.indices));
            self.stats.upload_bytes += (mesh.indices.len() * 2) as u64;
        } else if !plan.full_hit { self.stats.index_skips += 1; }
        if plan.vertex || plan.index {
            self.stats.uploads += 1;
            let us = gl_started.elapsed().as_micros() as u64;
            self.stats.upload_gl_us += us;
            count("vita_fx_upload_gl_us", us);
        }
        if version.is_none() && track_vertex && !same_vertex {
            b.previous_vertices.clear();
            b.previous_vertices.extend_from_slice(&mesh.vertices);
        }
        if version.is_none() && track_index && !same_index { b.previous_indices.clone_from(&mesh.indices); }
        b.version = version;
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
        let _owner = crate::render_tail::owner(if p.effect { crate::light_profile::RenderPhase::Fx } else { crate::light_profile::RenderPhase::Actors });
        let started = crate::render::vita_renderer::config().render_draw_timing
            .then(std::time::Instant::now);
        let key = (p.blend_index, p.depth_equal);
        let pipeline = self
            .pipelines
            .get(&key)
            .expect("prewarmed LG actor pipeline");
        // Several actor layers can share one uploaded vertex/index page.
        // Texture bindings still belong to the individual layer packet.
        self.buffers[slot].bindings.images[0]=p.mesh.texture.as_ref().map_or_else(||self.white.raw_miniquad_id(),Texture2D::raw_miniquad_id);
        let ctx = unsafe { get_internal_gl() }.quad_context;
        let c = crate::render::vita_renderer::config();
        let b = &self.buffers[slot].bindings;
        let apply = if c.fx_submit_state_cache {
            self.submit_cache.observe((key.0, key.1, depth_write),
                (self.buffers[slot].vertex, b.index_buffer, [b.images[0], b.images[1], b.images[2]]), u)
        } else { [true; 3] };
        if apply[0] {
            ctx.apply_pipeline(pipeline);
            unsafe { macroquad::miniquad::gl::glDepthMask(u8::from(depth_write)); }
            self.stats.pipeline_sets += 1;
        }
        if apply[1] { ctx.apply_bindings(b); self.stats.binding_sets += 1; }
        // The old uniform-only probe remains independently selectable with the
        // new cache disabled. Both observe the complete bit-exact payload.
        let same = (c.gpu_state_probe || c.gpu_uniform_dedupe) && self.uniform_cache.observe(u);
        if c.gpu_state_probe || c.gpu_uniform_dedupe {
            self.stats.uniform_requests += 1;
            self.stats.identical_uniforms += u64::from(same);
        }
        let skip = if c.fx_submit_state_cache { !apply[2] } else { c.gpu_uniform_dedupe && same };
        let mut traffic = crate::render::uniform_fields::Traffic::default();
        if skip { self.stats.uniform_skips += 1; }
        else {
            traffic = self.field_uniforms.apply(ctx, u, c.uniform_field_cache);
            self.stats.uniform_bytes += traffic.bytes;
        }
        if p.effect { crate::render::vita_renderer::log_world_fx_depth(); }
        ctx.draw(0, p.geometry().indices.len() as i32, 1);
        self.stats.draws += 1;
        self.stats.triangles += p.geometry().indices.len() as u64 / 3;
        let metrics = &mut self.stats.render[usize::from(p.effect)];
        metrics.draws += 1;
        metrics.pipelines += u64::from(apply[0]);
        metrics.depth_states += u64::from(apply[0]);
        metrics.bindings += u64::from(apply[1]);
        metrics.texture_bind_slots += u64::from(apply[1]) * 3;
        metrics.uniform_uploads += u64::from(traffic.calls > 0);
        metrics.uniform_bytes += traffic.bytes;
        metrics.uniform_gl_calls += traffic.calls;
        if let Some(started) = started { metrics.cpu_ns += started.elapsed().as_nanos() as u64; }
    }
}
fn ring_slot(slot: usize, frame: usize, phase: usize, safe: bool) -> usize {
    let depth = if safe {5} else {3};
    slot * depth * 2 + (frame % depth) * 2 + phase
}
#[cfg(test)]
#[test]
fn ring_reuse_exceeds_driver_busy_window_and_separates_finish_phases() {
    for safe in [false,true] {
        let depth=if safe {5} else {3};
        let mut last=std::collections::BTreeMap::new();
        for frame in 0..30 {
            for slot in 0..20 {for phase in 0..2 {
                let id=ring_slot(slot,frame,phase,safe);
                if let Some(old)=last.insert(id,frame) {assert_eq!(frame-old,depth);}
                assert_eq!(id/(2*depth),slot);
                assert_eq!(id%2,phase);
            }}
        }
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
    Puff(usize),
}
fn order(mut list: Vec<(f32, Source)>, sort: bool) -> Vec<(f32, Source)> {
    // Stable sort keeps authored world layer ties and actor group submission ties.
    if sort {
        list.sort_by(|a, b| b.0.total_cmp(&a.0));
    }
    list
}
fn center(mesh: &Mesh) -> Vec3 {
    center_vertices(&mesh.vertices)
}
fn center_vertices(vertices: &[Vertex]) -> Vec3 {
    let (lo, hi) = vertices.iter().fold(
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
    previous_signature: Vec<Signature>,
    steps: Vec<(usize, usize)>,
    kept: Vec<Source>,
    destinations: Vec<(usize, Vec<usize>)>,
    versions: BTreeMap<usize, Vec<super::prepared::Version>>,
    version_scratch: BTreeMap<usize, Vec<super::prepared::Version>>,
    destination_ids: Vec<usize>,
    candidates: Vec<(usize, usize, usize, PacketKey, bool)>,
    packing: Vec<PackSource>,
}
/// Descriptors borrow immutable CPU arrays until pack_sources has drained both workers.
/// No Mesh, Rc, texture or renderer handle is present in a worker closure.
#[derive(Clone, Copy)]
struct PackSource { vertices: usize, vertex_len: usize, indices: usize, index_len: usize,
    vertex_offset: usize, index_offset: usize }
fn pack_kernel(sources: &[PackSource], vertices: &mut [Vertex], indices: &mut [u16],
    vertex_base: usize, index_base: usize, copy_indices: bool) {
    for source in sources {
        // SAFETY: merge_cached owns all source groups without modifying them until
        // scoped CPU workers have joined (also on panic); lengths come from slices.
        let input = unsafe {std::slice::from_raw_parts(source.vertices as *const Vertex, source.vertex_len)};
        let start = source.vertex_offset - vertex_base;
        vertices[start..start+input.len()].copy_from_slice(input);
        if copy_indices {
            let input = unsafe {std::slice::from_raw_parts(source.indices as *const u16, source.index_len)};
            let start = source.index_offset - index_base;
            for (dst, &i) in indices[start..start+input.len()].iter_mut().zip(input) {
                *dst = i + source.vertex_offset as u16;
            }
        }
    }
}
fn pack_sources(sources: &[PackSource], vertices: &mut [Vertex], indices: &mut [u16], copy_indices: bool, worker: bool) {
    if worker && vertices.len() >= 256 && sources.len() >= 2 {
        let mid = sources.len()/2;
        let vb = sources[mid].vertex_offset; let ib = sources[mid].index_offset;
        let (va,vz) = vertices.split_at_mut(vb); let (ia,iz) = indices.split_at_mut(ib);
        let (a,z) = sources.split_at(mid);
        crate::character::skinning::overlap::cpu_pair(
            ||pack_kernel(a,va,ia,0,0,copy_indices), ||pack_kernel(z,vz,iz,vb,ib,copy_indices));
    } else { pack_kernel(sources,vertices,indices,0,0,copy_indices); }
}
fn merge_cached<'a>(cache: &'a mut MergeCache, groups: &mut [super::Group], list: &mut Vec<(f32,Source)>,
    material: bool, reuse: bool, pool: &mut BTreeMap<usize, super::prepared::Geometry>) -> &'a [usize] {
    merge_cached_mode(cache, groups, list, material, reuse, pool, crate::render::vita_renderer::config().fx_merge_in_place)
}
fn merge_cached_mode<'a>(
    cache: &'a mut MergeCache,
    groups: &mut [super::Group],
    list: &mut Vec<(f32, Source)>,
    material: bool,
    reuse: bool,
    pool: &mut BTreeMap<usize, super::prepared::Geometry>,
    in_place: bool,
) -> &'a [usize] {
    // Only packet topology/state is cached. Animated vertices always stream anew.
    // A world packet or actor is a barrier for particle material regrouping.
    let mut signature = std::mem::take(&mut cache.signature);
    signature.clear();
    signature.extend(list
        .iter()
        .map(|&(_, source)| {
            if let Source::Actor(id) = source {
                let g = &groups[id];
                let p = g.passes.first();
                let eligible = g.passes.len() == 1 && p.is_some_and(|p| p.geometry.as_ref().map_or_else(|| valid_mesh(p.geometry()), |g| g.valid) && (p.effect || crate::render::vita_renderer::config().actor_batching));
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
        );
    // Keep the prior signature in separate reusable storage for exact plan matching.
    let hit = reuse && cache.previous_signature == signature;
    if hit {
        count("vita_fx_merge_plan_hits", 1);
    } else {
        count("vita_fx_merge_plan_builds", 1);
        cache.steps.clear();
        cache.kept.clear();
        let mut candidates = std::mem::take(&mut cache.candidates); candidates.clear();
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
        cache.candidates = candidates;
        cache.previous_signature.clone_from(&signature);
        cache.destinations.clear();
        for &(target, source) in &cache.steps {
            if let Some((_, sources)) = cache.destinations.iter_mut().find(|(t,_)| *t == target) { sources.push(source); }
            else { cache.destinations.push((target, vec![source])); }
        }
        cache.versions.clear();
        cache.destination_ids.clear();
        cache.destination_ids.extend(cache.destinations.iter().map(|(id,_)| *id));
    }
    cache.signature = signature;
    for (target, sources) in &cache.destinations {
        let target = *target;
        let mut version_storage = std::mem::take(cache.version_scratch.entry(target).or_default());
        version_storage.clear();
        let certified = crate::render::vita_renderer::config().fx_prepare_cache &&
            std::iter::once(target).chain(sources.iter().copied()).all(|id| {
                if let Some(g) = &groups[id].passes[0].geometry { version_storage.push(g.version); true } else { false }
            });
        let versions = certified.then_some(&version_storage);
        let old_version = pool.get(&target).map(|g|g.version);
        let same_topology = hit && versions.as_ref().zip(cache.versions.get(&target)).is_some_and(|(a,b)|
            a.len()==b.len() && a.iter().zip(b).all(|(a,b)|a.identity==b.identity && a.topology==b.topology));
        let mut cached = pool.remove(&target);
        if let Some(previous) = cached.as_ref() {
            let version_hit = hit && versions.is_some_and(|v| cache.versions.get(&target) == Some(v));
            if hit && (version_hit || versions.is_none() && unchanged_merged(previous, groups, target, sources)) {
                let cached = cached.take().unwrap();
                if versions.is_some() { groups[target].passes[0].geometry=Some(std::rc::Rc::new(cached)); }
                else { groups[target].passes[0].geometry=None; groups[target].passes[0].mesh=cached.mesh; }
                for &source in sources {
                    groups[source].passes.clear();
                }
                count("vita_fx_unchanged_merged_groups", 1);
                cache.version_scratch.insert(target, version_storage);
                continue;
            }
        }
        if in_place {
            let mut dest = cached.map(|g|g.mesh).unwrap_or_else(|| Mesh {vertices:Vec::new(),indices:Vec::new(),texture:None});
            dest.texture = groups[target].passes[0].mesh.texture.clone();
            cache.packing.clear();
            let mut vertices = 0; let mut indices = 0;
            for id in std::iter::once(target).chain(sources.iter().copied()) {
                let source = groups[id].passes[0].geometry();
                cache.packing.push(PackSource { vertices:source.vertices.as_ptr() as usize, vertex_len:source.vertices.len(),
                    indices:source.indices.as_ptr() as usize, index_len:source.indices.len(), vertex_offset:vertices, index_offset:indices });
                vertices += source.vertices.len(); indices += source.indices.len();
            }
            dest.vertices.resize(vertices, Vertex::new(0.,0.,0.,0.,0.,WHITE));
            let keep_indices = same_topology && dest.indices.len() == indices;
            if !keep_indices { dest.indices.resize(indices, 0); }
            pack_sources(&cache.packing, &mut dest.vertices, &mut dest.indices, !keep_indices,
                crate::render::vita_renderer::config().fx_worker_prep);
            count("vita_fx_pack_vertices", vertices as u64);
            count("vita_fx_pack_indices", if keep_indices {0} else {indices as u64});
            count("vita_fx_certified_indices_reused", if keep_indices {indices as u64} else {0});
            groups[target].passes[0].geometry = None;
            groups[target].passes[0].mesh = dest;
            for &source in sources {
                if !groups[source].passes[0].effect {count("vita_actor_batched_packets",1);}
                groups[source].passes.clear();
            }
        } else {
        if let Some(mut cached) = cached {
            let old = groups[target].passes[0].geometry();
            cached.mesh.texture = groups[target].passes[0].mesh.texture.clone();
            cached.mesh.vertices.clear();
            cached.mesh.indices.clear();
            cached.mesh.vertices.extend_from_slice(&old.vertices);
            cached.mesh.indices.extend_from_slice(&old.indices);
            groups[target].passes[0].geometry=None;
            groups[target].passes[0].mesh = cached.mesh;
        }
        groups[target].passes[0].own_geometry();
        for &source in sources {
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
        if certified {
            let previous = cache.versions.insert(target, version_storage).unwrap_or_default();
            cache.version_scratch.insert(target, previous);
            let texture = groups[target].passes[0].mesh.texture.clone();
            let mesh = std::mem::replace(&mut groups[target].passes[0].mesh, Mesh { vertices: Vec::new(), indices: Vec::new(), texture });
            groups[target].passes[0].geometry = Some(std::rc::Rc::new(super::prepared::Geometry::merged(mesh, if hit {old_version} else {None}, same_topology)));
        } else { cache.version_scratch.insert(target, version_storage); }
    }
    count("vita_fx_merged_meshes", cache.steps.len() as u64);
    list.clear();
    list.extend(cache.kept.iter().map(|&s| (0., s)));
    &cache.destination_ids
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
    crate::game_thread::assert_current();
    let _owner = crate::render_tail::owner(crate::light_profile::RenderPhase::Fx);
    let finish_started = std::time::Instant::now();
    let mut world_ns = 0u64;
    let _span = crate::frame_profile::span("vita_3d_finish");
    if flush {
        gpu.ring_frame += 1;
        gpu.ring_phase = 0;
    } else {
        gpu.ring_phase = 1;
    }
    gpu.stats = Stats::default();
    gpu.uniform_cache.interrupt(); // Probe only uninterrupted draws of this owned program.
    gpu.submit_cache.interrupt();
    gpu.field_uniforms.interrupt();
    let (mut groups, mut puffs, camera, direction, dropped) = FRAME.with(|f| {
        let mut f = f.borrow_mut();
        (
            std::mem::take(&mut f.groups),
            std::mem::take(&mut f.world_puffs),
            f.camera,
            f.direction,
            f.dropped,
        )
    });
    if flush {
        boundary("flush_vita_3d_packets");
    }
    let preparation_started = std::time::Instant::now();
    let mut list = {
        let _span = crate::frame_profile::span("vita_joint_alpha_sort");
        let mut list = std::mem::take(&mut gpu.alpha_storage); list.clear();
        list.extend(world.iter().map(|&(d,id)| (d,Source::World(id))));
        let mut next_puff = 0;
        for id in 0..=groups.len() {
            while next_puff < puffs.len() && puffs[next_puff].before_group == id {
                list.push(((center_vertices(&puffs[next_puff].vertices) - camera).dot(direction), Source::Puff(next_puff)));
                next_puff += 1;
            }
            if let Some(g) = groups.get(id).filter(|g| g.transparent &&
                g.passes.first().is_some_and(|p| !p.geometry().vertices.is_empty())) {
                list.push(((center(g.passes[0].geometry()) - camera).dot(direction), Source::Actor(id)));
            }
        }
        debug_assert_eq!(next_puff, puffs.len());
        order(list, crate::render::vita_renderer::config().sort_alpha)
    };
    if !puffs.is_empty() {
        gpu.puff_batches[gpu.ring_phase].build(&mut groups, &puffs, &mut list,
            crate::render::vita_renderer::config().fx_batching == "material");
    }
    count("vita_fx_particle_input_groups", groups.iter()
        .filter(|g| !g.passes.is_empty() && g.passes.iter().all(|p| p.effect)).count() as u64);
    let mut recycled = std::mem::take(&mut gpu.recycle_storage); recycled.clear();
    let c = crate::render::vita_renderer::config();
    if c.fx_prepare_cache {
        let storage = &mut gpu.preparation[gpu.ring_phase];
        let mut index = 0;
        for group in &mut groups { for p in &mut group.passes {
            if storage.len() <= index { storage.push(Default::default()); }
            if p.geometry.is_none() { p.geometry = Some(storage[index].observe(&p.mesh, None)); }
            index += 1;
        }}
        storage.truncate(index); // Unused producers cannot alias a future occupant.
    }
    crate::light_profile::fx_add(crate::light_profile::FxMetric::FinishPrepare, preparation_started.elapsed().as_micros() as u64);
    if (c.merge_fx && c.fx_batching != "none") || c.actor_batching {
        let _timer = crate::light_profile::fx_timer(crate::light_profile::FxMetric::Merge);
        let _span = crate::frame_profile::span("vita_fx_merge_groups");
        let mut opaque = std::mem::take(&mut gpu.opaque_storage); opaque.clear();
        opaque.extend(groups.iter().enumerate().filter(|(_,g)| !g.transparent).map(|(id,_)| (0.,Source::Actor(id))));
        for (which, order) in [&mut list, &mut opaque].into_iter().enumerate() {
            let destinations = merge_cached(
                &mut gpu.merge_cache[gpu.ring_phase * 2 + which],
                &mut groups,
                order,
                c.fx_batching == "material",
                c.fx_merge_cache,
                &mut gpu.merge_meshes[gpu.ring_phase],
            );
            recycled.extend_from_slice(destinations);
        }
        opaque.clear(); gpu.opaque_storage = opaque;
    }
    let mut packets = std::mem::take(&mut gpu.packet_storage); packets.clear();
    let mut ranges = std::mem::take(&mut gpu.range_storage); ranges.clear();
    let mut invalid = 0;
    {
        let upload_started = std::time::Instant::now();
        let _span = crate::frame_profile::span("vita_fx_upload");
        let mut uniform_templates=[None;4];
        let mut shared_uploads=std::mem::take(&mut gpu.shared_storage); shared_uploads.clear();
        for (group_id, group) in groups.iter().enumerate() {
            let start = packets.len();
            for (pass_id, p) in group.passes.iter().enumerate() {
                if p.geometry().indices.is_empty() {
                    continue;
                }
                let shared=p.geometry.as_ref().map(|m|std::rc::Rc::as_ptr(m) as usize);
                let previous=shared.and_then(|id|shared_uploads.get(&id).copied());
                let upload=if let Some(b)=previous {count("vita_actor_geometry_uploads_skipped",1);Ok(b)}else{gpu.upload(packets.len(),p)};
                match upload {
                    Ok(b) => {
                        if let Some(id)=shared {shared_uploads.insert(id,b);}
                        let u = if c.fx_prepare_cache { prepared_uniforms(p, &mut uniform_templates) }
                            else { FRAME.with(|f| uniforms(p, &f.borrow())) };
                        packets.push((group_id, pass_id, b, u));
                    }
                    Err(e) => {
                        eprintln!("[lg/vita-renderer] rejected actor packet: {e:#}");
                        invalid += 1;
                    }
                }
            }
            ranges.push(start..packets.len());
        }
        gpu.shared_storage = shared_uploads;
        let cpu_us = (upload_started.elapsed().as_micros() as u64).saturating_sub(gpu.stats.upload_gl_us);
        crate::light_profile::fx_add(crate::light_profile::FxMetric::UploadCpu, cpu_us);
        crate::light_profile::fx_add(crate::light_profile::FxMetric::UploadGl, gpu.stats.upload_gl_us);
        crate::light_profile::fx_add(crate::light_profile::FxMetric::UploadBytes, gpu.stats.upload_bytes);
        count("vita_fx_upload_cpu_us", cpu_us);
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
            let (group, pass, b, u) = &packets[i];
            gpu.draw(&groups[*group].passes[*pass], *b, u, !groups[id].transparent);
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
    gpu.field_uniforms.interrupt();
    gpu.submit_cache.interrupt(); // External depth-mask change at the alpha boundary.
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
                    gpu.submit_cache.interrupt();
                    gpu.field_uniforms.interrupt();
                    let start = world_totals.and_then(|t| t.borrow().start());
                    let world_started = std::time::Instant::now();
                    draw_world(id);
                    world_ns += world_started.elapsed().as_nanos() as u64;
                    world_ms += crate::frame_profile::Totals::<3>::elapsed(start);
                },
                Source::Actor(id) => draw_group(gpu, id),
                Source::Puff(_) => unreachable!("unpublished world puff"),
            }
        }
        if let Some(totals) = world_totals {
            totals.borrow().publish_under("vita_world_alpha_submit", world_ms,
                ["vita_world_alpha_state_bindings", "vita_world_alpha_uniforms", "vita_world_alpha_draws"]);
        }
    }
    crate::render::vita_renderer::finish_depth_sentinel(flush);
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
    let _recycle_timer = crate::light_profile::fx_timer(crate::light_profile::FxMetric::FinishRecycle);
    gpu.submit_cache.interrupt();
    gpu.uniform_cache.interrupt();
    packets.clear(); gpu.packet_storage = packets;
    ranges.clear(); gpu.range_storage = ranges;
    for &id in &recycled {
        if let Some(p) = groups[id].passes.first_mut() {
            let geometry = if let Some(g) = p.geometry.take() {
                std::rc::Rc::try_unwrap(g).unwrap_or_else(|g| super::prepared::Geometry::new(super::copy_mesh(&g)))
            } else { super::prepared::Geometry::new(std::mem::replace(&mut p.mesh,
                Mesh { vertices: Vec::new(), indices: Vec::new(), texture: None })) };
            gpu.merge_meshes[gpu.ring_phase].insert(id, geometry);
        }
    }
    gpu.merge_meshes[gpu.ring_phase].retain(|id, _| recycled.contains(id));
    if c.actor_layer_reuse {
        for group in &mut groups {super::actor_cache::recycle(&mut group.passes);}
    }
    let submit_ns = gpu.stats.render.iter().map(|m| m.cpu_ns).sum::<u64>();
    crate::light_profile::render_add(crate::light_profile::RenderPhase::Fx,
        crate::light_profile::RenderMetrics { cpu_ns: (finish_started.elapsed().as_nanos() as u64)
            .saturating_sub(world_ns + submit_ns), ..Default::default() });
    gpu.stats.report();
    let list_len = list.len(); list.clear(); gpu.alpha_storage = list;
    recycled.clear(); gpu.recycle_storage = recycled;
    FRAME.with(|f| {
        let mut f = f.borrow_mut();
        let pool_limit = groups.len();
        for group in &mut groups {
            group.passes.clear();
            if f.group_pool.len() < pool_limit { f.group_pool.push(std::mem::take(&mut group.passes)); }
        }
        groups.clear(); f.groups = groups;
        puffs.clear(); f.world_puffs = puffs;
    });
    (list_len, dropped + invalid)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn owned_particle_publications_match_legacy_packet_geometry_and_blend_policy() {
        for blend in [crate::materials::Blend::Alpha,crate::materials::Blend::Add,crate::materials::Blend::AlphaAdd] {
            FRAME.with(|f| *f.borrow_mut()=Frame {active:true,direct:true,..Default::default()});
            let mesh=Mesh {vertices:vec![Vertex::new(1.,2.,3.,0.5,0.4,Color::new(0.7,0.4,0.2,0.3));4],
                indices:vec![0,1,2,0,2,3],texture:None};
            super::super::effect(&mesh,blend);
            let publication=std::rc::Rc::new(super::super::prepared::Geometry::new(super::super::copy_mesh(&mesh)));
            super::super::effect_geometry(publication.clone(),None,blend);
            let frame=FRAME.with(|f|std::mem::take(&mut *f.borrow_mut()));
            assert_eq!(frame.groups.len(),2);assert_eq!(frame.vertices,8);
            assert!(frame.groups.iter().all(|g|g.transparent));
            let (a,b)=(&frame.groups[0].passes[0],&frame.groups[1].passes[0]);
            assert!(same_packet(a,b));assert!(a.effect && b.effect);
            assert!(vertices_equal(&a.geometry().vertices,&b.geometry().vertices));
            assert_eq!(a.geometry().indices,b.geometry().indices);
            assert!(std::rc::Rc::ptr_eq(&publication,b.geometry.as_ref().unwrap()));
            assert!(b.mesh.vertices.is_empty() && b.mesh.indices.is_empty());
        }
    }
    #[test]
    fn in_place_merge_reuses_certified_indices_and_copies_less_than_owned_sources() {
        let mut publications = (0..128).map(|_|super::super::prepared::Cache::default()).collect::<Vec<_>>();
        let mut caches = [MergeCache::default(),MergeCache::default()];
        let mut pools: [BTreeMap<usize,super::super::prepared::Geometry>;2] = Default::default();
        let mut allocated = [0u64;2];
        let mut index_pointer = None;
        for frame in 0..8 {
            let mut published=Vec::new();
            for (i,c) in publications.iter_mut().enumerate() {
                let mut m=fixture_group().passes.pop().unwrap().mesh;
                m.vertices[0].position.x = (frame+i) as f32;
                if frame==6 && i==31 {m.indices.swap(0,1);}
                published.push(c.observe(&m,None));
            }
            let mut outputs=Vec::new();
            for arm in 0..2 {
                let mut groups=(0..128).map(|i| {
                    let mut g=fixture_group();g.passes[0].effect=true;
                    g.passes[0].mesh.vertices.clear();g.passes[0].mesh.indices.clear();
                    g.passes[0].geometry=Some(published[i].clone());g
                }).collect::<Vec<_>>();
                let mut list=(0..128).map(|i| (i as f32,Source::Actor(i))).collect();
                let scope=crate::kernel_alloc::Scope::begin();
                let ids=merge_cached_mode(&mut caches[arm],&mut groups,&mut list,false,true,&mut pools[arm],arm==1);
                let calls=scope.finish(); if frame>2 {allocated[arm]+=calls;}
                let mesh=groups[0].passes[0].geometry();
                outputs.push((mesh.vertices.clone(),mesh.indices.clone(),list.clone()));
                if arm==1 && frame<6 {
                    if let Some(pointer)=index_pointer {assert_eq!(pointer,mesh.indices.as_ptr());}
                    index_pointer=Some(mesh.indices.as_ptr());
                    if frame>0 {assert!(host_report()["vita_fx_certified_indices_reused"].as_u64().unwrap()>0);}
                }
                for &id in ids {
                    let geometry=groups[id].passes[0].geometry.take().unwrap();
                    pools[arm].insert(id,std::rc::Rc::try_unwrap(geometry).ok().unwrap());
                }
            }
            assert!(vertices_equal(&outputs[0].0,&outputs[1].0));
            assert_eq!(outputs[0].1,outputs[1].1);assert_eq!(outputs[0].2,outputs[1].2);
        }
        assert!(allocated[1]<allocated[0], "legacy={} in_place={}",allocated[0],allocated[1]);
    }
    #[test]
    fn packing_successor_batches_drain_panics_and_match_serial_with_zero_kernel_allocations() {
        crate::character::skinning::overlap::startup(2,false);
        let vertices=vec![Vertex::new(1.,2.,3.,0.4,0.5,WHITE);400];
        let indices=vec![0,1,2];
        let sources=[PackSource {vertices:vertices.as_ptr() as usize,vertex_len:400,
            indices:indices.as_ptr() as usize,index_len:3,vertex_offset:0,index_offset:0},
            PackSource {vertices:vertices.as_ptr() as usize,vertex_len:400,
            indices:indices.as_ptr() as usize,index_len:3,vertex_offset:400,index_offset:3}];
        let mut a=vec![Vertex::new(0.,0.,0.,0.,0.,BLACK);800];let mut b=a.clone();
        let mut ia=vec![0;6];let mut ib=ia.clone();
        pack_sources(&sources,&mut a,&mut ia,true,false);
        pack_sources(&sources,&mut b,&mut ib,true,true); // Warm telemetry and wake paths.
        let scope=crate::kernel_alloc::Scope::begin();
        pack_sources(&sources,&mut b,&mut ib,true,true);
        assert_eq!(scope.finish(),0);
        assert!(vertices_equal(&a,&b));assert_eq!(ia,ib);
        crate::character::skinning::overlap::cpu_pair(|| {
            let scope=crate::kernel_alloc::Scope::begin();
            let mut v=[Vertex::new(0.,0.,0.,0.,0.,BLACK);400];let mut i=[0;3];
            pack_kernel(&sources[..1],&mut v,&mut i,0,0,true);assert_eq!(scope.finish(),0);
        },||{});
        let finished=std::sync::atomic::AtomicBool::new(false);
        let failure=std::panic::catch_unwind(std::panic::AssertUnwindSafe(||
            crate::character::skinning::overlap::cpu_pair(||panic!("injected packing failure"),||finished.store(true,std::sync::atomic::Ordering::Relaxed))));
        assert!(failure.is_err());assert!(finished.load(std::sync::atomic::Ordering::Relaxed));
        pack_sources(&sources,&mut b,&mut ib,true,true);
        assert!(vertices_equal(&a,&b));
    }
    #[test]
    fn renderer_owners_and_packets_cannot_be_sent_or_shared_with_workers() {
        trait AmbiguousIfSend<A> { fn check() {} }
        impl<T: ?Sized> AmbiguousIfSend<()> for T {}
        impl<T: ?Sized + Send> AmbiguousIfSend<u8> for T {}
        trait AmbiguousIfSync<A> { fn check() {} }
        impl<T: ?Sized> AmbiguousIfSync<()> for T {}
        impl<T: ?Sized + Sync> AmbiguousIfSync<u8> for T {}
        // This fails to compile through ambiguous inference if any owner gains Send/Sync.
        let _ = <Gpu as AmbiguousIfSend<_>>::check;
        let _ = <Pass as AmbiguousIfSend<_>>::check;
        let _ = <crate::render::vita_renderer::Renderer as AmbiguousIfSend<_>>::check;
        let _ = <Gpu as AmbiguousIfSync<_>>::check;
        let _ = <Pass as AmbiguousIfSync<_>>::check;
        let _ = <crate::render::vita_renderer::Renderer as AmbiguousIfSync<_>>::check;
    }
    #[test]
    fn prepared_uniforms_match_full_payload_across_material_camera_and_light_changes() {
        for frame in 0..4 {
            FRAME.with(|f| { let mut f=f.borrow_mut(); f.projection=Some(Mat4::from_scale(Vec3::splat(frame as f32+1.)));
                f.camera=vec3(frame as f32,2.,3.); f.fullbright=frame%2==0;
                f.atmosphere.distance=Vec4::splat(frame as f32); });
            let mut templates=[None;4];
            for model in [false,true] {for lit in [false,true] {for i in 0..8 {
                let mut g=fixture_group(); let p=&mut g.passes[0];
                p.model=model; p.lit=lit; p.clamp=i%2==0; p.cutout=i as f32/8.;
                p.cutout_reference=f32::from_bits(0x3f000001+i); p.gain=vec4(i as f32,-0.,0.2,0.7);
                p.fog=i as f32; p.appearance=super::super::Appearance {dissolve:i as f32/8.,ghost:i%2==0,power:i as f32};
                p.filter_fade=i as f32/9.;
                let reference=FRAME.with(|f|uniforms(p,&f.borrow()));
                assert_eq!(prepared_uniforms(p,&mut templates).map(f32::to_bits),reference.map(f32::to_bits));
            }}}
        }
    }
    #[test]
    fn versioned_merge_matches_uncached_output_on_reorder_topology_and_vertex_changes() {
        let mut cache=MergeCache::default(); let mut pool=BTreeMap::new();
        let mut publications=(0..3).map(|_|super::super::prepared::Cache::default()).collect::<Vec<_>>();
        let mut original=(0..3).map(|_|fixture_group()).collect::<Vec<_>>();
        for frame in 0..12 {
            if frame==4 {original[1].passes[0].mesh.vertices[0].position.z=-0.;}
            if frame==6 {original[2].passes[0].mesh.indices.swap(0,1);}
            if frame==8 {original[0].passes[0].gain=Vec4::splat(0.5);}
            let make=||original.iter().map(|g| {
                let mut copy=fixture_group(); copy.passes[0].mesh=super::super::copy_mesh(&g.passes[0].mesh);
                copy.passes[0].gain=g.passes[0].gain; copy
            }).collect::<Vec<_>>();
            let mut baseline=make(); let mut changed=make();
            for (g,c) in changed.iter_mut().zip(&mut publications) {
                g.passes[0].geometry=Some(c.observe(&g.passes[0].mesh,None));
            }
            let mut a=if frame<10 {vec![(3.,Source::Actor(0)),(2.,Source::Actor(1)),(1.,Source::Actor(2))]}
                else {vec![(3.,Source::Actor(2)),(2.,Source::World(0)),(1.,Source::Actor(0)),(0.,Source::Actor(1))]};
            let mut b=a.clone();
            merge_adjacent(&mut baseline,&mut a);
            let recycled=merge_cached(&mut cache,&mut changed,&mut b,false,true,&mut pool);
            assert_eq!(a.iter().map(|x|x.1).collect::<Vec<_>>(),b.iter().map(|x|x.1).collect::<Vec<_>>());
            for (a,b) in baseline.iter().zip(&changed) {assert_eq!(a.passes.len(),b.passes.len());
                for (a,b) in a.passes.iter().zip(&b.passes) {
                    assert!(vertices_equal(&a.geometry().vertices,&b.geometry().vertices));
                    assert_eq!(a.geometry().indices,b.geometry().indices);
                }
            }
            for &id in recycled {
                let p=&mut changed[id].passes[0]; let g=p.geometry.take().unwrap();
                pool.insert(id,std::rc::Rc::try_unwrap(g).ok().unwrap());
            }
        }
    }
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
                _game_thread: crate::game_thread::GameThread::default(),
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
        let shared=std::rc::Rc::new(super::super::prepared::Geometry::new(super::super::copy_mesh(&groups[0].passes[0].mesh)));
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
        pool.insert(0, super::super::prepared::Geometry::new(merged));
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
