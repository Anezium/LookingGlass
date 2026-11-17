//! Whole actor/effect packets share one alpha order with immutable world groups.
//! Only dynamic LG meshes stream here; world buffers are never read or uploaded.
use crate::mesh::Mesh;
#[cfg(feature="frame_total_only")]
use crate::light_profile::InactiveInstant as MetricInstant;
#[cfg(not(feature="frame_total_only"))]
use std::time::Instant as MetricInstant;
pub(super) mod state_cache;
pub(super) mod world_puffs;
mod packet_cull;
mod fill_cuts;
mod add_program;
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
    crate::sim_maps::count(name, value);
    #[cfg(target_os = "vita")]
    crate::frame_profile::counts(&[(name, value)]);
    #[cfg(not(target_os = "vita"))]
    HOST_COUNTS.with(|c| *c.borrow_mut().entry(name).or_default() += value);
}
/// Reverse a speculative count when a frame discards the work that added it.
pub(crate) fn uncount(name: &'static str, value: u64) {
    crate::sim_maps::uncount(name, value);
    #[cfg(target_os = "vita")]
    crate::frame_profile::uncounts(&[(name, value)]);
    #[cfg(not(target_os = "vita"))]
    HOST_COUNTS.with(|c| {
        if let Some(slot) = c.borrow_mut().get_mut(name) { *slot = slot.saturating_sub(value); }
    });
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
/// Field ends in the actor uniform block. Host oracle diffs use the same cuts.
pub(crate) const UNIFORM_FIELDS: &[(&str, usize)] = &[
    ("Model", 16), ("Projection", 32), ("AtlasGrid", 34), ("Fullbright", 35), ("Lit", 36),
    ("ClampUV", 37), ("Cutout", 38), ("CutoutReference", 39), ("Gain", 43), ("FogMode", 44),
    ("SkySize", 46), ("SkyFade", 47), ("FxModel", 48), ("FxAppearance", 52), ("Lights", 116),
    ("FogEye", 119), ("DistanceFog", 123), ("LiquidFog", 127), ("FogAMin", 130), ("FogAMax", 133),
    ("FogA", 137), ("FogBMin", 140), ("FogBMax", 143), ("FogB", 147),
];
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
    vertex_updates: [u64; 2],
    index_updates: [u64; 2],
    uploaded_vertices: [u64; 2],
    uploaded_indices: [u64; 2],
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
        for (phase, names) in [
            ["actor_vertex_updates", "actor_index_updates", "actor_uploaded_vertices", "actor_uploaded_indices"],
            ["fx_vertex_updates", "fx_index_updates", "fx_uploaded_vertices", "fx_uploaded_indices"],
        ].into_iter().enumerate() {
            for (name, value) in names.into_iter().zip([self.vertex_updates[phase], self.index_updates[phase],
                self.uploaded_vertices[phase], self.uploaded_indices[phase]]) { count(name,value); }
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
    leaves: &mut crate::phase_census::FinishLeaves,
) -> Buffers {
    let usage = if ring {
        BufferUsage::Dynamic
    } else {
        BufferUsage::Stream
    };
    let start = leaves.start();
    let vertex = {
        ctx.new_buffer(
        BufferType::VertexBuffer,
        usage,
        BufferSource::empty::<Vertex>(vc),
    )};
    leaves.add(0, start);
    let start = leaves.start();
    let index = {
        ctx.new_buffer(
        BufferType::IndexBuffer,
        usage,
        BufferSource::empty::<u16>(ic),
    )};
    leaves.add(0, start);
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
/// The opt-in lean program. `fx_unlit_exact` takes precedence when both keys are set.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Specialized {
    Lite,
    UnlitExact,
}
impl Specialized {
    fn name(self) -> &'static str {
        match self {
            Self::Lite => "retained-fx-lite",
            Self::UnlitExact => "retained-fx-unlit-exact",
        }
    }
    fn fragment(self) -> String {
        match self {
            Self::Lite => crate::render::FX_LITE_FRAGMENT.to_owned(),
            Self::UnlitExact => unlit_exact_fragment(),
        }
    }
}
pub(crate) fn specialized_program(fx_lite: bool, fx_unlit_exact: bool) -> Option<Specialized> {
    if fx_unlit_exact {
        Some(Specialized::UnlitExact)
    } else if fx_lite {
        Some(Specialized::Lite)
    } else {
        None
    }
}
/// `fx_unlit_exact`: an admitted pass packs sixteen zero light vectors, so the
/// dynamic light result is exactly zero. Replace only that call and keep every
/// other operation of the full fragment source byte for byte.
pub(crate) fn unlit_exact_fragment() -> String {
    crate::render::FRAGMENT.replace("dynamicLight(worldPosition)", "vec3(0.0)")
}
/// Program choice for one draw. A pass that fails `lite_pass` always uses the full
/// program; a missing specialized program is the documented fallback.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Route {
    Full,
    Lite,
    CorpseOpaque,
    /// `fx_add_lean`: the lean additive program, fog kept.
    AddLean,
    /// `fx_add_fog_free`: the lean additive program without the fog call.
    AddFogFree,
}
/// `fx_add_lean` / `fx_add_fog_free`: the additive program for one effect packet, when its
/// key is on, its program compiled and the packet admitted. `fog_free` runs only when asked.
fn add_route(p: &Pass, programs: [bool; 2], fog_free: impl FnOnce() -> bool) -> Option<Route> {
    if !programs[0] || !add_program::lean_pass(p) { return None; }
    Some(if programs[1] && fog_free() { Route::AddFogFree } else { Route::AddLean })
}
fn route(p: &Pass, opaque: bool, specialized: bool) -> Route {
    if opaque {
        Route::CorpseOpaque
    } else if specialized && lite_pass(p) {
        Route::Lite
    } else {
        Route::Full
    }
}
/// Submit-cache key. Each program owns its own pipeline bank, so the lean route
/// offsets its blend slot past every full-program blend. The corpse program is a
/// third bank, not an alias of either.
fn submit_key(key: (usize, bool), route: Route, depth_write: bool) -> (usize, bool, bool) {
    let offset = match route {
        Route::Full => 0,
        Route::Lite => crate::render::BLEND_COUNT,
        Route::CorpseOpaque => 2 * crate::render::BLEND_COUNT,
        Route::AddLean => 3 * crate::render::BLEND_COUNT,
        Route::AddFogFree => 4 * crate::render::BLEND_COUNT,
    };
    (key.0 + offset, key.1, depth_write)
}
pub(crate) struct Gpu {
    _game_thread: crate::game_thread::GameThread,
    shader: ShaderId,
    pipelines: BTreeMap<(usize, bool), Pipeline>,
    /// `fx_lite`: the lean program, its pipelines (same keys) and its own uniform cache.
    lite: Option<Lite>,
    corpse_opaque: Option<Lite>,
    /// `fx_add_lean` and `fx_add_fog_free`: the two additive effect programs.
    fx_add: [Option<Lite>; 2],
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
    bounds: packet_cull::Bounds,
}
/// The specialized program bank of a route: 0 lean/unlit-exact, 1 corpse, 2 and 3 additive.
/// It borrows only the three bank fields, so the full program's caches stay usable.
fn program_bank<'a>(lite: &'a mut Option<Lite>, corpse: &'a mut Option<Lite>, fx_add: &'a mut [Option<Lite>; 2],
    bank: Option<usize>) -> Option<&'a mut Lite> {
    match bank? {
        0 => lite.as_mut(),
        1 => corpse.as_mut(),
        i => fx_add[i - 2].as_mut(),
    }
}
pub(crate) struct Lite {
    uniforms: UniformCache<UNIFORM_FLOATS>,
    shader: ShaderId,
    pipelines: BTreeMap<(usize, bool), Pipeline>,
    fields: crate::render::uniform_fields::Cache,
}
/// `fx_add_zero_alpha_cut`: particle loading reports each texture's verdict.
pub(crate) fn note_particle_texture_zero_alpha(texture: &Texture2D, zero: bool) {
    if crate::render::vita_renderer::config().fx_add_zero_alpha_cut { fill_cuts::note_zero_alpha(texture, zero); }
}
/// A pass whose full-program arithmetic `FX_LITE_FRAGMENT` reproduces (see there).
pub(crate) fn lite_pass(p: &Pass) -> bool {
    p.effect && !p.lit && !p.clamp && p.cutout < 0.5 && !(p.appearance.dissolve > 0.)
        && !p.appearance.ghost && !(p.appearance.power > 0.5) && p.filter_fade == 0.
}
/// The opt-in programs one configuration asks for: the lean fragment program and
/// whether the corpse-opaque program is requested. One pure decision, so the
/// combined selection can be pinned without a GL context.
fn program_choice(c: &crate::render::vita_renderer::Config) -> (Option<Specialized>, bool) {
    (specialized_program(c.fx_lite, c.fx_unlit_exact),
     c.ant_corpse_opaque_shader || c.actor_opaque_shader || c.actor_discard_free_shader || c.actor_cutout_discard_free)
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
        // A failed specialized program falls back to the full program.
        let (specialized, corpse_requested) = program_choice(crate::render::vita_renderer::config());
        let lite = specialized.and_then(|specialized| {
            crate::frame_profile::shader_creation();
            let _shader_name = crate::shader_cache::Name::new(specialized.name());
            let source = specialized.fragment();
            ctx.new_shader(ShaderSource::Glsl { vertex: crate::render::VERTEX,
                fragment: &crate::environment::fragment(&source) }, meta())
                .map_err(|e| eprintln!("[lg/vita-renderer] warning {} program failed, key off: {e:?}", specialized.name())).ok()
        }).map(|shader| Lite { uniforms: UniformCache::default(), shader, pipelines: BTreeMap::new(), fields: crate::render::uniform_fields::Cache::new(meta()) });
        let corpse_opaque = corpse_requested.then(|| {
            crate::frame_profile::shader_creation();
            let _shader_name=crate::shader_cache::Name::new("retained-ant-corpse-opaque");
            let fragment=crate::lighting::fragment(&crate::environment::fragment(&super::corpse_opaque::fragment()));
            ctx.new_shader(ShaderSource::Glsl {vertex:crate::render::VERTEX,fragment:&fragment},meta())
                .map_err(|e|eprintln!("[lg/vita-renderer] corpse opaque program failed, using original: {e:?}")).ok()
        }).flatten().map(|shader|Lite {uniforms:UniformCache::default(),shader,pipelines:BTreeMap::new(),fields:crate::render::uniform_fields::Cache::new(meta())});
        // A failed additive program leaves its packets on the route they had before.
        let c = crate::render::vita_renderer::config();
        let fx_add = [(c.fx_add_lean, add_program::LEAN), (c.fx_add_lean && c.fx_add_fog_free, add_program::FOG_FREE)];
        let fx_add = [0, 1].map(|i| fx_add[i].0.then(|| {
            crate::frame_profile::shader_creation();
            let _shader_name = crate::shader_cache::Name::new(add_program::NAMES[i]);
            ctx.new_shader(ShaderSource::Glsl { vertex: crate::render::VERTEX,
                fragment: &crate::environment::fragment(fx_add[i].1) }, meta())
                .map_err(|e| eprintln!("[lg/vita-renderer] warning {} program failed, key off: {e:?}", add_program::NAMES[i])).ok()
        }).flatten().map(|shader| Lite { uniforms: UniformCache::default(), shader, pipelines: BTreeMap::new(), fields: crate::render::uniform_fields::Cache::new(meta()) }));
        println!(
            "[lg/vita-renderer] actor READY grouped alpha, actor_max_layers={}",
            crate::render::vita_renderer::config().actor_max_layers
        );
        let mut gpu = Self {
            _game_thread: crate::game_thread::GameThread::new(),
            shader,
            pipelines: BTreeMap::new(),
            lite,
            corpse_opaque,
            fx_add,
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
            bounds: Default::default(),
        };
        // Pipeline objects share the one compiled program. Prewarm every LG blend
        // and depth key so first use of the held Blade does not create one later.
        for blend in 0..crate::render::BLEND_COUNT {
            for equal in [false, true] {
                gpu.prepare_pipeline((blend, equal));
            }
        }
        println!(
            "[lg/vita-renderer] actor pipeline objects prewarmed={} programs={}",
            gpu.pipelines.len() + gpu.lite.as_ref().map_or(0, |l| l.pipelines.len()) + gpu.corpse_opaque.as_ref().map_or(0,|l|l.pipelines.len())
                + gpu.fx_add.iter().flatten().map(|l| l.pipelines.len()).sum::<usize>(),
            1 + usize::from(gpu.lite.is_some()) + usize::from(gpu.corpse_opaque.is_some()) + gpu.fx_add.iter().flatten().count()
        );
        Ok(gpu)
    }
    fn upload(&mut self, slot: usize, p: &Pass, leaves: &mut crate::phase_census::FinishLeaves) -> Result<usize> {
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
            self.buffers.push(make_buffers(ctx, vc, ic, ring, white, leaves));
            self.stats.growths += 1;
        }
        if self.buffers[slot].vertex_capacity < vc || self.buffers[slot].index_capacity < ic {
            let replacement = make_buffers(ctx, vc, ic, ring, white, leaves);
            let old = std::mem::replace(&mut self.buffers[slot], replacement);
            let start = leaves.start();
            ctx.delete_buffer(old.vertex);ctx.delete_buffer(old.index);
            leaves.add(0, start);
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
        // Host oracle: upload every packet, so two arms differ in nothing the GL driver sees.
        #[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
        let (same_vertex,same_index)=if crate::sim_maps::probe::force_uploads() {(false,false)} else {(same_vertex,same_index)};
        // Generation equality is sufficient only within the same publication identity.
        if version.is_some() && same_vertex && same_index { count("vita_fx_prepare_upload_hits", 1); }
        if c.gpu_state_probe || c.gpu_index_dedupe {
            self.stats.upload_requests += 2;
            self.stats.identical_uploads += u64::from(same_vertex) + u64::from(same_index);
        }
        let plan = UploadPlan::new(c.fx_upload_cache || version.is_some(), c.gpu_index_dedupe || version.is_some(), same_vertex, same_index);
        if plan.full_hit { self.stats.upload_hits += 1;crate::phase_census::count(1,1); }
        crate::phase_census::count(2,u64::from(plan.vertex)*(mesh.vertices.len()*40) as u64+u64::from(plan.index)*(mesh.indices.len()*2) as u64);
        let gl_started = MetricInstant::now();
        #[cfg(feature="frame_total_only")]
        let gl_leaf = if plan.vertex || plan.index {leaves.start()} else {None};
        let metrics = &mut self.stats.render[usize::from(p.effect)];
        metrics.buffer_updates += u64::from(plan.vertex) + u64::from(plan.index);
        metrics.buffer_bytes += u64::from(plan.vertex) * (mesh.vertices.len() * 40) as u64
            + u64::from(plan.index) * (mesh.indices.len() * 2) as u64;
        if plan.vertex {
            ctx.buffer_update(b.vertex, BufferSource::slice(&mesh.vertices));
            self.stats.upload_bytes += (mesh.vertices.len() * 40) as u64;
            self.stats.vertex_updates[usize::from(p.effect)] += 1;
            self.stats.uploaded_vertices[usize::from(p.effect)] += mesh.vertices.len() as u64;
        }
        if plan.index {
            ctx.buffer_update(b.index, BufferSource::slice(&mesh.indices));
            self.stats.upload_bytes += (mesh.indices.len() * 2) as u64;
            self.stats.index_updates[usize::from(p.effect)] += 1;
            self.stats.uploaded_indices[usize::from(p.effect)] += mesh.indices.len() as u64;
        } else if !plan.full_hit { self.stats.index_skips += 1; }
        if plan.vertex || plan.index {
            self.stats.uploads += 1;
            let elapsed = gl_started.elapsed();
            #[cfg(not(feature="frame_total_only"))]
            leaves.add_duration(0, elapsed, true);
            #[cfg(feature="frame_total_only")]
            leaves.add(0, gl_leaf);
            let us = elapsed.as_micros() as u64;
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
        let shaders = std::iter::once((self.shader, &mut self.pipelines))
            .chain(self.lite.as_mut().map(|l| (l.shader, &mut l.pipelines)))
            .chain(self.corpse_opaque.as_mut().filter(|_|{let c=crate::render::vita_renderer::config();key==(0,false) || c.actor_discard_free_shader || c.actor_cutout_discard_free}).map(|l|(l.shader,&mut l.pipelines)))
            // The additive programs only ever draw the two additive blends.
            .chain(self.fx_add.iter_mut().flatten().filter(|_| add_program::additive(key.0)).map(|l| (l.shader, &mut l.pipelines)));
        for (shader, pipelines) in shaders {
        pipelines.entry(key).or_insert_with(|| {
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
                shader,
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
    }
    fn draw(&mut self, p: &Pass, slot: usize, u: &[f32; UNIFORM_FLOATS], depth_write: bool, ant_dead: Option<bool>) {
        let _owner = crate::render_tail::owner(if p.effect { crate::light_profile::RenderPhase::Fx } else { crate::light_profile::RenderPhase::Actors });
        let started = crate::render::vita_renderer::config().render_draw_timing
            .then(MetricInstant::now);
        #[cfg(all(feature="sim_maps_probe", not(target_os="vita")))]
        let census_started = (p.effect && crate::sim_maps::probe::draw_query::armed())
            .then(std::time::Instant::now);
        let key = (p.blend_index, p.depth_equal);
        let c=crate::render::vita_renderer::config();
        let environment_free = c.actor_discard_free_shader && self.corpse_opaque.is_some() && super::corpse_opaque::discard_free(p);
        if environment_free {count("vita_actor_discard_free_draws",1);}
        let cutout_free = !environment_free && c.actor_cutout_discard_free && self.corpse_opaque.is_some()
            && super::corpse_opaque::cutout_discard_free(p);
        if cutout_free {count("vita_actor_cutout_discard_free_draws",1);}
        let discard_free = environment_free || cutout_free;
        let opaque = discard_free || (ant_dead==Some(true) && c.ant_corpse_opaque_shader || c.actor_opaque_shader)
            && depth_write && self.corpse_opaque.is_some() && super::corpse_opaque::eligible(p);
        if opaque && !discard_free {
            count(if ant_dead==Some(true) {"vita_ant_corpse_opaque_draws"} else {"vita_actor_opaque_draws"},1);
        }
        #[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
        if ant_dead.is_some() && std::env::var_os("LOOKING_GLASS_CORPSE_PROBE").is_some()
            && matches!(crate::sim_maps::probe::frame(),1400 | 1980 | 2030 | 2100..=2120) {
            println!("[lg/corpse-program] frame={} dead={} opaque={} triangles={} vertices={}",
                crate::sim_maps::probe::frame(),ant_dead.unwrap(),opaque,p.geometry().indices.len()/3,p.geometry().vertices.len());
        }

        let program = add_route(p, [self.fx_add[0].is_some(), self.fx_add[1].is_some()], || add_program::fog_free(p, u))
            .unwrap_or_else(|| route(p, opaque, self.lite.is_some()));
        let lite = program == Route::Lite;
        if lite { count(if crate::render::vita_renderer::config().fx_unlit_exact {
            "vita_fx_unlit_exact_draws"
        } else { "vita_fx_lite_draws" }, 1); }
        match program {
            Route::AddLean => count("vita_fx_add_lean_draws", 1),
            Route::AddFogFree => count("vita_fx_add_fog_free_draws", 1),
            _ => {}
        }
        // The specialized bank this draw uses; `None` is the full program.
        let bank = match program {
            Route::Full => None,
            Route::Lite => Some(0),
            Route::CorpseOpaque => Some(1),
            Route::AddLean => Some(2),
            Route::AddFogFree => Some(3),
        };
        let pipeline = *match bank {
            None => self.pipelines.get(&key),
            Some(0) => self.lite.as_ref().unwrap().pipelines.get(&key),
            Some(1) => self.corpse_opaque.as_ref().unwrap().pipelines.get(&key),
            Some(i) => self.fx_add[i - 2].as_ref().unwrap().pipelines.get(&key),
        }
            .expect("prewarmed LG actor pipeline");
        // Several actor layers can share one uploaded vertex/index page.
        // Texture bindings still belong to the individual layer packet.
        self.buffers[slot].bindings.images[0]=p.mesh.texture.as_ref().map_or_else(||self.white.raw_miniquad_id(),Texture2D::raw_miniquad_id);
        let ctx = unsafe { get_internal_gl() }.quad_context;
        let c = crate::render::vita_renderer::config();
        let b = &self.buffers[slot].bindings;
        let apply = if c.fx_submit_state_cache {
            // The lean program's pipelines are other state: offset its key past every blend.
            self.submit_cache.observe(submit_key(key, program, depth_write),
                (self.buffers[slot].vertex, b.index_buffer, [b.images[0], b.images[1], b.images[2]]), u)
        } else { [true; 3] };
        if apply[0] {
            ctx.apply_pipeline(&pipeline);
            unsafe { macroquad::miniquad::gl::glDepthMask(u8::from(depth_write)); }
            self.stats.pipeline_sets += 1;
        }
        if apply[1] { ctx.apply_bindings(b); self.stats.binding_sets += 1; }
        // The old uniform-only probe remains independently selectable with the
        // new cache disabled. Both observe the complete bit-exact payload.
        let same = (c.gpu_state_probe || c.gpu_uniform_dedupe) && {
            let cache=match program_bank(&mut self.lite, &mut self.corpse_opaque, &mut self.fx_add, bank) { Some(l) => &mut l.uniforms, None => &mut self.uniform_cache };
            cache.observe(u)
        };
        if c.gpu_state_probe || c.gpu_uniform_dedupe {
            self.stats.uniform_requests += 1;
            self.stats.identical_uniforms += u64::from(same);
        }
        let skip = if c.fx_submit_state_cache { !apply[2] } else { c.gpu_uniform_dedupe && same };
        let mut traffic = crate::render::uniform_fields::Traffic::default();
        if skip { self.stats.uniform_skips += 1; }
        else {
            let fields = match program_bank(&mut self.lite, &mut self.corpse_opaque, &mut self.fx_add, bank) { Some(l) => &mut l.fields, None => &mut self.field_uniforms };
            traffic = fields.apply(ctx, u, c.uniform_field_cache);
            self.stats.uniform_bytes += traffic.bytes;
        }
        if p.effect { crate::render::vita_renderer::log_world_fx_depth(); }
        #[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
        {
            let (prop, name) = p.mesh.surface.as_ref().map(|s| s.label.clone()).unwrap_or_else(||
                crate::sim_maps::probe::draw_query::actor_label(p.mesh.texture.as_ref().map(Texture2D::raw_miniquad_id)));
            crate::sim_maps::probe::draw_query::mark(
                if p.effect {"fx"} else {"actors"}, if p.effect {"fx"} else if prop {"prop"} else {"actor"},
                p.blend_index, None, 0, name.as_deref());
        }
        #[cfg(all(feature="sim_maps_probe",target_os="macos"))]
        if crate::sim_maps::probe::skin_proof() {
            let m=p.geometry();
            let (vertices,indices)=crate::sim_maps::probe::gpu_geometry(m.vertices.len(),m.indices.len());
            crate::sim_maps::probe::packet(slot,&vertices,&indices,&m.vertices,&m.indices,u,
                &[p.effect as u32,p.blend_index as u32,p.depth_equal as u32,(!depth_write) as u32,
                    p.lit as u32,p.clamp as u32,p.cutout.to_bits(),p.cutout_reference.to_bits(),
                    p.fog.to_bits(),p.model as u32,p.filter_fade.to_bits()],p.mesh.texture.as_ref());
        }
        let class = if p.effect { crate::gpu_timing::fx_class(p.blend_index) } else { crate::gpu_timing::MASK_ACTORS };
        if !crate::gpu_timing::masked(class) {
            // Every retained pipeline has CullFace::Nothing. Restore that state before
            // any world callback or cached pipeline use. This opt-in can remove authored
            // double-sided faces and is deliberately classified as picture-changing.
            let cull = c.actor_frontface_cull && !p.effect;
            unsafe { if cull {
                macroquad::miniquad::gl::glEnable(0x0B44); // GL_CULL_FACE
                macroquad::miniquad::gl::glCullFace(0x0404); // GL_FRONT; experimental authored winding
            } }
            ctx.draw(0, p.geometry().indices.len() as i32, 1);
            unsafe { if cull { macroquad::miniquad::gl::glDisable(0x0B44); } }
        }
        #[cfg(all(feature="sim_maps_probe", not(target_os="vita")))]
        if let Some(start) = census_started {
            let (_, texture) = crate::sim_maps::probe::draw_query::actor_label(
                p.mesh.texture.as_ref().map(Texture2D::raw_miniquad_id));
            println!("FX_SUBMIT {}", serde_json::json!({"frame": crate::sim_maps::probe::frame(),
                "texture": texture, "triangles": p.geometry().indices.len()/3,
                "cpu_ns": start.elapsed().as_nanos() as u64, "query_overhead_included": true}));
        }
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
        #[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
        if crate::character::skinning::census::active() && !p.effect {
            println!("[lg/submit-census] frame={} texture={:?} triangles={} vertices={} blend={} equal={} depth_write={depth_write} pipeline={} bindings={} uniform_calls={} uniform_bytes={} cpu_ns={}",
                crate::sim_maps::probe::frame(),crate::sim_maps::probe::draw_query::actor_label(p.mesh.texture.as_ref().map(Texture2D::raw_miniquad_id)),p.geometry().indices.len()/3,p.geometry().vertices.len(),p.blend_index,p.depth_equal,
                apply[0],apply[1],traffic.calls,traffic.bytes,started.map_or(0,|t|t.elapsed().as_nanos()));
        }
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
        if let Some(l)=&self.corpse_opaque {
            for p in l.pipelines.values() {ctx.delete_pipeline(*p);}
            ctx.delete_shader(l.shader);
        }
        if let Some(l) = &self.lite {
            for p in l.pipelines.values() { ctx.delete_pipeline(*p); }
            ctx.delete_shader(l.shader);
        }
        for l in self.fx_add.iter().flatten() {
            for p in l.pipelines.values() { ctx.delete_pipeline(*p); }
            ctx.delete_shader(l.shader);
        }
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
pub(crate) enum Source {
    World(usize),
    Actor(usize),
    Puff(usize),
}
#[derive(Default)]
struct WorldContinuity(bool);
impl WorldContinuity {
    fn observe(&mut self, source: Source) -> bool {
        let world = matches!(source, Source::World(_));
        let continuation = self.0 && world;
        self.0 = world;
        continuation
    }
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
    a.mesh.upload_identity == b.mesh.upload_identity
        && a.blend_index == b.blend_index
        && a.depth_equal == b.depth_equal
        && a.mesh.texture.as_ref() == b.mesh.texture.as_ref()
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
// Unmanaged test textures compare handles directly without a Macroquad GPU context.
// Production keeps the existing raw binding key.
#[cfg(test)]
type MergeTexture = Texture2D;
#[cfg(not(test))]
type MergeTexture = TextureId;
fn merge_texture(texture: &Texture2D) -> MergeTexture {
    #[cfg(test)] { texture.clone() }
    #[cfg(not(test))] { texture.raw_miniquad_id() }
}
#[derive(Clone, PartialEq)]
struct PacketKey {
    upload_identity: Option<u64>,
    texture: Option<MergeTexture>,
    state: [u32; 20],
}
fn packet_key(p: &Pass) -> PacketKey {
    PacketKey {
        upload_identity: p.mesh.upload_identity,
        texture: p.mesh.texture.as_ref().map(merge_texture),
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
    // A local certificate is valid only for this exact recycled output, even
    // when a target crosses between transparent and opaque merge banks.
    outputs: BTreeMap<usize, super::prepared::Version>,
    version_scratch: BTreeMap<usize, Vec<super::prepared::Version>>,
    destination_ids: Vec<usize>,
    candidates: Vec<(usize, usize, usize, PacketKey, bool)>,
    packing: Vec<PackSource>,
}
/// Descriptors borrow immutable CPU arrays until pack_sources has drained both workers.
/// No Mesh, Rc, texture or renderer handle is present in a worker closure.
#[derive(Clone, Copy)]
struct PackSource { vertices: usize, vertex_len: usize, indices: usize, index_len: usize,
    vertex_offset: usize, index_offset: usize, keep_vertices: bool }
fn pack_kernel<const RANGES: bool>(sources: &[PackSource], vertices: &mut [Vertex], indices: &mut [u16],
    vertex_base: usize, index_base: usize, copy_indices: bool) {
    for source in sources {
        // SAFETY: merge_cached owns all source groups without modifying them until
        // scoped CPU workers have joined (also on panic); lengths come from slices.
        if !RANGES || !source.keep_vertices {
            let input = unsafe {std::slice::from_raw_parts(source.vertices as *const Vertex, source.vertex_len)};
            let start = source.vertex_offset - vertex_base;
            vertices[start..start+input.len()].copy_from_slice(input);
        }
        if copy_indices {
            let input = unsafe {std::slice::from_raw_parts(source.indices as *const u16, source.index_len)};
            let start = source.index_offset - index_base;
            for (dst, &i) in indices[start..start+input.len()].iter_mut().zip(input) {
                *dst = i + source.vertex_offset as u16;
            }
        }
    }
}
fn pack_sources_mode<const RANGES: bool>(sources: &[PackSource], vertices: &mut [Vertex], indices: &mut [u16], copy_indices: bool, worker: bool) {
    if worker && vertices.len() >= 256 && sources.len() >= 2 {
        let mid = sources.len()/2;
        let vb = sources[mid].vertex_offset; let ib = sources[mid].index_offset;
        let (va,vz) = vertices.split_at_mut(vb); let (ia,iz) = indices.split_at_mut(ib);
        let (a,z) = sources.split_at(mid);
        crate::character::skinning::overlap::cpu_pair(
            ||pack_kernel::<RANGES>(a,va,ia,0,0,copy_indices), ||pack_kernel::<RANGES>(z,vz,iz,vb,ib,copy_indices));
    } else { pack_kernel::<RANGES>(sources,vertices,indices,0,0,copy_indices); }
}
#[cfg(test)]
fn pack_sources(sources: &[PackSource], vertices: &mut [Vertex], indices: &mut [u16], copy_indices: bool, worker: bool) {
    pack_sources_mode::<true>(sources, vertices, indices, copy_indices, worker);
}
fn merge_worker(inline: bool, copied_vertices: usize) -> bool {
    !(inline && copied_vertices <= 4096)
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
    let (ranges,inline) = crate::render::vita_renderer::fx_cpu_options();
    merge_cached_options(cache, groups, list, material, reuse, pool, in_place,
        ranges,inline)
}
fn merge_cached_options<'a>(
    cache: &'a mut MergeCache, groups: &mut [super::Group], list: &mut Vec<(f32, Source)>,
    material: bool, reuse: bool, pool: &mut BTreeMap<usize, super::prepared::Geometry>,
    in_place: bool, ranges: bool, inline: bool,
) -> &'a [usize] {
    // Packet topology/state is cached. Changed vertex ranges always stream anew.
    // A world packet or actor is a barrier for particle material regrouping.
    let local_reuse = crate::sim_maps::merge_local_reuse();
    let census_signature = crate::phase_census::span(crate::phase_census::Region::MergeSignature);
    #[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
    let signature_span = crate::frame_profile::span("fx_merge_signature");
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
    #[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
    drop(signature_span);
    drop(census_signature);
    let census_plan = crate::phase_census::span(crate::phase_census::Region::MergePlan);
    #[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
    let plan_span = crate::frame_profile::span("fx_merge_plan");
    // Keep the prior signature in separate reusable storage for exact plan matching.
    let hit = reuse && cache.previous_signature == signature;
    if hit {
        count("vita_fx_merge_plan_hits", 1);
        crate::phase_census::path_count(crate::phase_census::Counter::MergePlanHits, 1);
    } else {
        count("vita_fx_merge_plan_builds", 1);
        crate::phase_census::path_count(crate::phase_census::Counter::MergePlanBuilds, 1);
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
                crate::sim_maps::count("fx_merge_candidate_comparisons", 1);
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
            if let Some((_, sources)) = cache.destinations.iter_mut().find(|(t,_)| { crate::sim_maps::count("fx_merge_destination_comparisons", 1); *t == target }) { sources.push(source); }
            else { cache.destinations.push((target, vec![source])); }
        }
        if !local_reuse { cache.versions.clear(); cache.outputs.clear(); }
        else {
            cache.versions.retain(|id,_| cache.destinations.iter().any(|(target,_)| target == id));
            cache.outputs.retain(|id,_| cache.destinations.iter().any(|(target,_)| target == id));
        }
        cache.destination_ids.clear();
        cache.destination_ids.extend(cache.destinations.iter().map(|(id,_)| *id));
    }
    cache.signature = signature;
    #[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
    drop(plan_span);
    drop(census_plan);
    let _census_copy = crate::phase_census::span(crate::phase_census::Region::MergeCopy);
    crate::phase_census::path_count(crate::phase_census::Counter::MergeDestinations, cache.destinations.len() as u64);
    #[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
    let _copy_span = crate::frame_profile::span("fx_merge_copy");
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
        let local_hit = local_reuse && old_version.is_some_and(|old| cache.outputs.get(&target) == Some(&old));
        // Even a globally unchanged signature must certify the recycled output:
        // the other alpha/opaque bank can have occupied this target meanwhile.
        let certified_hit = if local_reuse { local_hit } else { hit };
        let same_topology = certified_hit && versions.as_ref().zip(cache.versions.get(&target)).is_some_and(|(a,b)|
            a.len()==b.len() && a.iter().zip(b).all(|(a,b)|a.identity==b.identity && a.topology==b.topology));
        let mut cached = pool.remove(&target);
        if let Some(cached) = cached.as_mut() {
            cached.mesh.upload_identity = groups[target].passes[0].mesh.upload_identity;
        }
        if let Some(previous) = cached.as_ref() {
            let version_hit = certified_hit && versions.is_some_and(|v| cache.versions.get(&target) == Some(v));
            if version_hit || hit && versions.is_none() && unchanged_merged(previous, groups, target, sources) {
                if !hit && local_hit { count("fx_merge_local_vertex_hits", 1); }
                let mut cached = cached.take().unwrap();
                if versions.is_some() { groups[target].passes[0].geometry=Some(std::rc::Rc::new(cached)); }
                else {
                    cached.mesh.surface = groups[target].passes[0].mesh.surface.clone();
                    groups[target].passes[0].geometry=None; groups[target].passes[0].mesh=cached.mesh;
                }
                for &source in sources {
                    super::actor_cache::recycle(&mut groups[source].passes);
                groups[source].passes.clear();
                }
                count("vita_fx_unchanged_merged_groups", 1);
                crate::phase_census::path_count(crate::phase_census::Counter::MergeUnchanged, 1);
                cache.version_scratch.insert(target, version_storage);
                continue;
            }
        }
        if !hit && local_hit && same_topology { count("fx_merge_local_topology_hits", 1); }
        if in_place {
            let mut dest = cached.map(|g|g.mesh).unwrap_or_else(|| Mesh { upload_identity: None, surface: None,vertices:Vec::new(),indices:Vec::new(),texture:None});
            dest.texture = groups[target].passes[0].mesh.texture.clone();
            dest.surface = groups[target].passes[0].mesh.surface.clone();
            dest.upload_identity = groups[target].passes[0].mesh.upload_identity;
            cache.packing.clear();
            let mut vertices = 0; let mut indices = 0;
            for id in std::iter::once(target).chain(sources.iter().copied()) {
                let source = groups[id].passes[0].geometry();
                cache.packing.push(PackSource { vertices:source.vertices.as_ptr() as usize, vertex_len:source.vertices.len(),
                    indices:source.indices.as_ptr() as usize, index_len:source.indices.len(), vertex_offset:vertices, index_offset:indices,
                    keep_vertices:false });
                vertices += source.vertices.len(); indices += source.indices.len();
            }
            let reusable_output = same_topology && dest.vertices.len() == vertices;
            dest.vertices.resize(vertices, Vertex::new(0.,0.,0.,0.,0.,WHITE));
            let keep_indices = same_topology && dest.indices.len() == indices;
            if !keep_indices { dest.indices.resize(indices, 0); }
            // Attribute unchanged source ranges only when the existing exact-output
            // and ordered topology certificates also prove their destination offsets.
            #[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
            if reusable_output {
                if let Some(previous) = cache.versions.get(&target) {
                    let reusable = cache.packing.iter().zip(version_storage.iter().zip(previous))
                        .filter(|(_, (current, old))| current == old)
                        .map(|(source, _)| source.vertex_len as u64).sum();
                    count("fx_merge_source_reusable_vertices", reusable);
                }
            }
            // Ordered source identities/topology prove lengths and destination offsets;
            // the output certificate also rejects a buffer recycled by another bank.
            // An exact vertex generation then permits leaving that range untouched.
            if ranges && reusable_output && local_hit {
                if let Some(previous) = cache.versions.get(&target) {
                    for (source, (current, old)) in cache.packing.iter_mut().zip(version_storage.iter().zip(previous)) {
                        source.keep_vertices = current == old;
                    }
                }
            }
            let copied = if ranges { cache.packing.iter().filter(|s| !s.keep_vertices).map(|s| s.vertex_len).sum::<usize>() }
                else { vertices };
            count("fx_merge_range_vertices_kept", (vertices - copied) as u64);
            count("fx_merge_copy_jobs", 1);
            count("fx_merge_copy_jobs_le_4096", u64::from(copied <= 4096));
            let worker = crate::render::vita_renderer::config().fx_worker_prep
                && merge_worker(inline, copied);
            count("fx_merge_inline_jobs", u64::from(!worker && vertices >= 256 && cache.packing.len() >= 2));
            if ranges {
                pack_sources_mode::<true>(&cache.packing, &mut dest.vertices, &mut dest.indices, !keep_indices, worker);
            } else {
                pack_sources_mode::<false>(&cache.packing, &mut dest.vertices, &mut dest.indices, !keep_indices, worker);
            }
            count("vita_fx_pack_vertices", copied as u64);
            count("vita_fx_pack_indices", if keep_indices {0} else {indices as u64});
            crate::phase_census::path_count(crate::phase_census::Counter::MergeCopiedVertices, copied as u64);
            crate::phase_census::path_count(crate::phase_census::Counter::MergeKeptVertices, (vertices - copied) as u64);
            crate::phase_census::path_count(crate::phase_census::Counter::MergeCopiedIndices, if keep_indices {0} else {indices as u64});
            count("vita_fx_certified_indices_reused", if keep_indices {indices as u64} else {0});
            groups[target].passes[0].geometry = None;
            groups[target].passes[0].mesh = dest;
            for &source in sources {
                if !groups[source].passes[0].effect {count("vita_actor_batched_packets",1);}
                super::actor_cache::recycle(&mut groups[source].passes);
                groups[source].passes.clear();
            }
        } else {
        if let Some(mut cached) = cached {
            let old = groups[target].passes[0].geometry();
            cached.mesh.texture = groups[target].passes[0].mesh.texture.clone();
            cached.mesh.surface = groups[target].passes[0].mesh.surface.clone();
            cached.mesh.upload_identity = groups[target].passes[0].mesh.upload_identity;
            cached.mesh.vertices.clear();
            cached.mesh.indices.clear();
            cached.mesh.vertices.extend_from_slice(&old.vertices);
            cached.mesh.indices.extend_from_slice(&old.indices);
            groups[target].passes[0].geometry=None;
            groups[target].passes[0].mesh = cached.mesh;
        }
        groups[target].passes[0].own_geometry();
        for &source in sources {
            let source_id = source;
            let mut source = groups[source_id].passes.pop().expect("merge plan source");
            if source.model && !source.effect {super::actor_cache::recycle_empty(&mut groups[source_id].passes);}
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
            let surface = groups[target].passes[0].mesh.surface.clone();
            let upload_identity = groups[target].passes[0].mesh.upload_identity;
            let mesh = std::mem::replace(&mut groups[target].passes[0].mesh, Mesh { upload_identity, surface, vertices: Vec::new(), indices: Vec::new(), texture });
            let geometry = super::prepared::Geometry::merged(mesh, if certified_hit {old_version} else {None}, same_topology);
            if local_reuse { cache.outputs.insert(target, geometry.version); }
            groups[target].passes[0].geometry = Some(std::rc::Rc::new(geometry));
        } else { cache.version_scratch.insert(target, version_storage); cache.outputs.remove(&target); }
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
    finish_world(gpu, world, flush, |event| if let WorldEvent::Draw(id, _) = event { draw_world(id) }, None)
}
/// Host probe: what reaches the joint list, and how many quads cannot touch the view.
#[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
fn funnel_before_bank(groups: &[super::Group], puffs: &[world_puffs::Puff]) {
    let matrix = FRAME.with(|f| f.borrow().projection).unwrap_or(Mat4::IDENTITY);
    let outside = |quad: &[Vertex]| {
        let clip: Vec<Vec4> = quad.iter().map(|v| matrix * v.position.extend(1.)).collect();
        clip.iter().all(|c| c.x > c.w) || clip.iter().all(|c| c.x < -c.w) || clip.iter().all(|c| c.y > c.w)
            || clip.iter().all(|c| c.y < -c.w) || clip.iter().all(|c| c.z > c.w) || clip.iter().all(|c| c.z < -c.w)
    };
    let mut quads = (0u64, 0u64);
    for g in groups {
        if g.transparent && g.passes.len() == 1 && g.passes[0].effect && g.passes[0].geometry().vertices.len() == 4 {
            quads.0 += 1;
            quads.1 += u64::from(outside(&g.passes[0].geometry().vertices));
        }
    }
    crate::sim_maps::count("fx_funnel_joint_publication_quads", quads.0);
    crate::sim_maps::count("fx_funnel_joint_publication_outside_view", quads.1);
    crate::sim_maps::count("fx_funnel_joint_raw_quads", puffs.len() as u64);
    crate::sim_maps::count("fx_funnel_joint_raw_outside_view", puffs.iter().filter(|p| outside(&p.vertices)).count() as u64);
}
/// One event of the joint alpha list. `Plan` hands over the final ordered list once, before any
/// draw, so a world submitter can plan runs of list entries; `Draw` is one world draw and says
/// whether no actor or puff sits between it and the previous world draw.
pub(crate) enum WorldEvent<'a> { Plan(&'a [(f32, Source)]), Draw(usize, bool) }
pub(crate) fn finish_world(
    gpu: &mut Gpu, world: &[(f32, usize)], flush: bool,
    mut world_event: impl for<'a> FnMut(WorldEvent<'a>),
    world_totals: Option<&std::cell::RefCell<crate::frame_profile::Totals<3>>>,
) -> (usize, usize) {
    crate::game_thread::assert_current();
    let _census = crate::phase_census::span(crate::phase_census::Region::JointFinish);
    let _owner = crate::render_tail::owner(crate::light_profile::RenderPhase::Fx);
    let finish_started = MetricInstant::now();
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
    if let Some(l) = &mut gpu.lite { l.fields.interrupt(); l.uniforms.interrupt(); }
    if let Some(l) = &mut gpu.corpse_opaque { l.fields.interrupt(); l.uniforms.interrupt(); }
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
    #[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
    census_groups("raw",&groups);
    super::corpse_triangles::process(&mut groups);
    let cuts = packet_cull::Cuts::new(crate::render::vita_renderer::config());
    if cuts.active() {
        if let Some(matrix) = FRAME.with(|f| f.borrow().projection) {
            let _span = crate::frame_profile::span("vita_fx_packet_cut");
            let c = crate::render::vita_renderer::config();
            packet_cull::cull(&mut gpu.bounds, &mut groups, &mut puffs, matrix, cuts, c.actor_layer_reuse || c.actor_single_prepare);
        }
    }
    #[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
    census_groups("culled",&groups);
    #[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
    super::corpse_triangles::report(&groups);
    {
        let c = crate::render::vita_renderer::config();
        if c.fx_add_zero_alpha_cut {
            let (cut_groups, cut_puffs) = fill_cuts::cut_zero_alpha(&mut groups, &mut puffs, c.actor_layer_reuse || c.actor_single_prepare);
            count("vita_fx_zero_alpha_groups_cut", cut_groups);
            count("vita_fx_zero_alpha_puffs_cut", cut_puffs);
        }
        if c.fx_add_max_area > 0. {
            if let Some(matrix) = FRAME.with(|f| f.borrow().projection) {
                count("vita_fx_add_capped_quads", fill_cuts::cap(&mut puffs, matrix, c.fx_add_max_area));
            }
        }
    }
    let preparation_started = MetricInstant::now();
    #[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
    let preparation_span = crate::frame_profile::span("fx_finish_prepare");
    let finish_time = crate::sim_maps::timer("r26_finish_ns");
    #[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
    funnel_before_bank(&groups, &puffs);
    let mut list = {
        let _span = crate::frame_profile::span("vita_joint_alpha_sort");
        let _leaf=crate::phase_census::span(crate::phase_census::Region::FinishSort);
        let list_time = crate::sim_maps::timer("r26_joint_list_ns");
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
                crate::sim_maps::count("joint_center_group_scans", 1);
                crate::sim_maps::count("joint_center_group_vertices", g.passes[0].geometry().vertices.len() as u64);
                list.push(((center(g.passes[0].geometry()) - camera).dot(direction), Source::Actor(id)));
            }
        }
        debug_assert_eq!(next_puff, puffs.len());
        drop(list_time);
        let _sort_time = crate::sim_maps::timer("r26_joint_sort_ns");
        order(list, crate::render::vita_renderer::config().sort_alpha)
    };
    let merge_leaf=crate::phase_census::span(crate::phase_census::Region::FinishMergePack);
    #[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
    if crate::sim_maps::fx_attached_persist() && crate::sim_maps::probe::negative_control() == Some("persist-quad-order") {
        let puff_slots: Vec<usize> = list.iter().enumerate().filter(|(_, e)| matches!(e.1, Source::Puff(_))).map(|(i, _)| i).collect();
        if let [.., a, b] = puff_slots[..] { list.swap(a, b); }
    }
    let bank_time = crate::sim_maps::timer("r26_bank_ns");
    if !puffs.is_empty() {
        let _puff_leaf=crate::phase_census::span(crate::phase_census::Region::MergePuffBuild);
        crate::phase_census::path_count(crate::phase_census::Counter::MergePuffs, puffs.len() as u64);
        gpu.puff_batches[gpu.ring_phase].build(&mut groups, &puffs, &mut list,
            crate::render::vita_renderer::config().fx_batching == "material");
    }
    drop(bank_time);
    count("vita_fx_particle_input_groups", groups.iter()
        .filter(|g| !g.passes.is_empty() && g.passes.iter().all(|p| p.effect)).count() as u64);
    let mut recycled = std::mem::take(&mut gpu.recycle_storage); recycled.clear();
    let c = crate::render::vita_renderer::config();
    if c.fx_prepare_cache {
        let _observe_leaf=crate::phase_census::span(crate::phase_census::Region::MergeObserve);
        let storage = &mut gpu.preparation[gpu.ring_phase];
        let observe_move = crate::sim_maps::fx_observe_move();
        let mut index = 0;
        for group in &mut groups { for p in &mut group.passes {
            if storage.len() <= index { storage.push(Default::default()); }
            if p.geometry.is_none() {
                p.geometry = Some(if observe_move { storage[index].observe_owned(&mut p.mesh) }
                    else { storage[index].observe(&p.mesh, None) });
            }
            index += 1;
        }}
        storage.truncate(index); // Unused producers cannot alias a future occupant.
        crate::phase_census::path_count(crate::phase_census::Counter::MergeObserved, index as u64);
    }
    crate::light_profile::fx_add(crate::light_profile::FxMetric::FinishPrepare, preparation_started.elapsed().as_micros() as u64);
    #[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
    drop(preparation_span);
    if (c.merge_fx && c.fx_batching != "none") || c.actor_batching {
        let _timer = crate::light_profile::fx_timer(crate::light_profile::FxMetric::Merge);
        let _span = crate::frame_profile::span("vita_fx_merge_groups");
        let mut opaque = std::mem::take(&mut gpu.opaque_storage); opaque.clear();
        opaque.extend(groups.iter().enumerate().filter(|(_,g)| !g.transparent && !(cuts.active() && g.passes.is_empty())).map(|(id,_)| (0.,Source::Actor(id))));
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
    drop(merge_leaf);
    #[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
    census_groups("merged",&groups);
    let mut packets = std::mem::take(&mut gpu.packet_storage); packets.clear();
    let mut ranges = std::mem::take(&mut gpu.range_storage); ranges.clear();
    let mut invalid = 0;
    {
        let _leaf=crate::phase_census::span(crate::phase_census::Region::FinishUploadCpu);
        let mut leaves = crate::phase_census::FinishLeaves::new();
        let upload_started = MetricInstant::now();
        let _span = crate::frame_profile::span("vita_fx_upload");
        let mut uniform_templates=[None;4];
        let mut shared_uploads=std::mem::take(&mut gpu.shared_storage); shared_uploads.clear();
        for (group_id, group) in groups.iter().enumerate() {
            let start = packets.len();
            for (pass_id, p) in group.passes.iter().enumerate() {
                if p.geometry().indices.is_empty() {
                    continue;
                }
                crate::phase_census::count(0,1);
                let shared=p.geometry.as_ref().map(|m|std::rc::Rc::as_ptr(m) as usize);
                let previous=shared.and_then(|id|shared_uploads.get(&id).copied());
                let upload=if let Some(b)=previous {crate::phase_census::count(1,1);count("vita_actor_geometry_uploads_skipped",1);Ok(b)}else{gpu.upload(packets.len(),p,&mut leaves)};
                match upload {
                    Ok(b) => {
                        if let Some(id)=shared {shared_uploads.insert(id,b);}
                        let uniform_leaf=leaves.start();
                        let u = if c.fx_prepare_cache { prepared_uniforms(p, &mut uniform_templates) }
                            else { FRAME.with(|f| uniforms(p, &f.borrow())) };
                        leaves.add(1,uniform_leaf);
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
        leaves.publish();
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
        let _census = crate::phase_census::span(if groups[id].passes.iter().all(|p| p.effect) {crate::phase_census::Region::FxSubmit} else {crate::phase_census::Region::ActorSubmit});
        #[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
        let census_start=crate::character::skinning::census::active().then(std::time::Instant::now);
        for i in ranges[id].clone() {
            let (group, pass, b, u) = &packets[i];
            #[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
            if crate::character::skinning::census::active() {
                let p=&groups[*group].passes[*pass];
                let m=p.geometry();
                let bounds=m.vertices.iter().fold((Vec3::splat(f32::INFINITY),Vec3::splat(f32::NEG_INFINITY)),
                    |(lo,hi),v|(lo.min(v.position),hi.max(v.position)));
                let projected=FRAME.with(|f|f.borrow().projection.and_then(|m|crate::render::projected_pixels(m,bounds.0,bounds.1)));
                println!("ACTOR_GPU_DRAW {}",serde_json::json!({"frame":crate::sim_maps::probe::frame(),
                    "group":group,"pass":pass,"owner":crate::character::skinning::census::owner(*group),
                    "vertices":m.vertices.len(),"indices":m.indices.len(),"effect":p.effect,
                    "blend":p.blend_index,"equal":p.depth_equal,"depth_write":!groups[id].transparent,
                    "cutout":p.cutout,"discard_free":super::corpse_opaque::discard_free(p),"cutout_discard_free":super::corpse_opaque::cutout_discard_free(p),
                    "surface_layers":p.mesh.surface.as_ref().map(|s|s.layers.len()),
                    "surface_transparent":p.mesh.surface.as_ref().map(|s|s.transparent),
                    "stage_env":p.mesh.surface.as_ref().map(|s|s.layers.iter().map(|l|l.stage.environment).collect::<Vec<_>>()),
                    "gain":p.gain.to_array(),"lit":p.lit,"fog":p.fog,"dissolve":p.appearance.dissolve,"ghost":p.appearance.ghost,"transparent_group":groups[id].transparent,"ant_dead":groups[id].ant_dead,
                    "label":p.mesh.surface.as_ref().map(|s|s.label.clone()),"bounds":[bounds.0.to_array(),bounds.1.to_array()],
                    "projected_long_side_display_pixels":projected}));
            }
            gpu.draw(&groups[*group].passes[*pass], *b, u, !groups[id].transparent, groups[id].ant_dead);
        }
        #[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
        if crate::character::skinning::census::active() && groups[id].passes.iter().any(|p|!p.effect) {
            println!("[lg/group-submit] frame={} group={id} owner={:?} draws={} cpu_ns={}",crate::sim_maps::probe::frame(),crate::character::skinning::census::owner(id),ranges[id].len(),census_start.map_or(0,|t|t.elapsed().as_nanos()));
        }
    };
    // The merged list is final here: let a world submitter plan its runs before any draw.
    world_event(WorldEvent::Plan(&list));
    let pass_leaf=crate::phase_census::span(crate::phase_census::Region::FinishPass);
    unsafe {
        crate::render::vita_renderer::begin_3d_pass(PassAction::Nothing);
        macroquad::miniquad::gl::glDepthMask(1);
    }
    drop(pass_leaf);
    {
        let _span = crate::frame_profile::span("vita_fx_opaque_submit");
        for (id, g) in groups.iter().enumerate().filter(|(_, g)| !g.transparent) {
            let _ = g;
            draw_group(gpu, id);
        }
    }
    let pass_leaf=crate::phase_census::span(crate::phase_census::Region::FinishPass);
    count("vita_joint_alpha_groups", list.len() as u64);
    gpu.field_uniforms.interrupt();
    if let Some(l) = &mut gpu.lite { l.fields.interrupt(); l.uniforms.interrupt(); }
    if let Some(l) = &mut gpu.corpse_opaque { l.fields.interrupt(); l.uniforms.interrupt(); }
    gpu.submit_cache.interrupt(); // External depth-mask change at the alpha boundary.
    unsafe {
        macroquad::miniquad::gl::glDepthMask(0);
    }
    drop(pass_leaf);
    {
        let _span = crate::frame_profile::span("vita_joint_alpha_submit");
        let mut world_ms = 0.;
        let mut continuity = WorldContinuity::default();
        for (_depth, source) in &list {
            #[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
            { let (kind,id)=match *source {Source::World(id)=>(0,id),Source::Actor(id)=>(1,id),Source::Puff(id)=>(2,id)};
              crate::sim_maps::probe::joint_order(kind,id,*_depth); }
            let continuation = continuity.observe(*source);
            match *source {
                Source::World(id) => {
                    gpu.uniform_cache.interrupt();
                    gpu.submit_cache.interrupt();
                    gpu.field_uniforms.interrupt();
                    if let Some(l) = &mut gpu.lite { l.fields.interrupt(); l.uniforms.interrupt(); }
                    if let Some(l) = &mut gpu.corpse_opaque { l.fields.interrupt(); l.uniforms.interrupt(); }
                    let start = world_totals.and_then(|t| t.borrow().start());
                    let world_started = MetricInstant::now();
                    // Only immediately adjacent world callbacks share state. An actor, even one
                    // ultimately skipped by its own submitter, remains an interruption fence.
                    world_event(WorldEvent::Draw(id, continuation));
                    world_ns += world_started.elapsed().as_nanos() as u64;
                    world_ms += crate::frame_profile::Totals::<3>::elapsed(start);
                },
                Source::Actor(id) => {
                    #[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
                    if groups[id].passes.iter().all(|p| p.effect) {
                        crate::sim_maps::count("fx_funnel_submit_draws", groups[id].passes.len() as u64);
                        crate::sim_maps::count("fx_funnel_submit_quads", groups[id].passes.iter()
                            .map(|p| p.geometry().indices.len() as u64 / 6).sum());
                    }
                    draw_group(gpu, id)
                }
                Source::Puff(_) => unreachable!("unpublished world puff"),
            }
        }
        if let Some(totals) = world_totals {
            totals.borrow().publish_under("vita_world_alpha_submit", world_ms,
                ["vita_world_alpha_state_bindings", "vita_world_alpha_uniforms", "vita_world_alpha_draws"]);
        }
    }
    let pass_leaf=crate::phase_census::span(crate::phase_census::Region::FinishPass);
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
    drop(pass_leaf);
    let _leaf=crate::phase_census::span(crate::phase_census::Region::FinishRecycle);
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
                Mesh { upload_identity: None, surface: None, vertices: Vec::new(), indices: Vec::new(), texture: None })) };
            gpu.merge_meshes[gpu.ring_phase].insert(id, geometry);
        }
    }
    gpu.merge_meshes[gpu.ring_phase].retain(|id, _| recycled.contains(id));
    if c.actor_layer_reuse || c.actor_single_prepare {
        for group in &mut groups {super::actor_cache::recycle(&mut group.passes);}
    }
    let submit_ns = gpu.stats.render.iter().map(|m| m.cpu_ns).sum::<u64>();
    crate::light_profile::render_add(crate::light_profile::RenderPhase::Fx,
        crate::light_profile::RenderMetrics { cpu_ns: (finish_started.elapsed().as_nanos() as u64)
            .saturating_sub(world_ns + submit_ns), ..Default::default() });
    gpu.stats.report();
    drop(finish_time);
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
    #[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
    crate::character::skinning::census::clear();
    (list_len, dropped + invalid)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn world_state_reuse_never_crosses_actor_puff_or_frame_boundaries() {
        let mut run = WorldContinuity::default();
        let input = [Source::World(0), Source::World(1), Source::Actor(7), Source::World(2),
            Source::World(3), Source::Puff(0), Source::World(4), Source::World(4)];
        assert_eq!(input.map(|s| run.observe(s)), [false,true,false,false,true,false,false,true]);
        assert!(!WorldContinuity::default().observe(Source::World(4)));
    }
    #[test]
    fn inline_merge_boundary_retains_workers_for_large_jobs() {
        for n in [0,255,256,4095,4096] { assert!(!merge_worker(true,n));assert!(merge_worker(false,n)); }
        for n in [4097,60000] { assert!(merge_worker(true,n)); }
    }
    #[test]
    fn owned_particle_publications_match_legacy_packet_geometry_and_blend_policy() {
        for blend in [crate::materials::Blend::Alpha,crate::materials::Blend::Add,crate::materials::Blend::AlphaAdd] {
            FRAME.with(|f| *f.borrow_mut()=Frame {active:true,direct:true,..Default::default()});
            let mesh=Mesh { upload_identity: None, surface: None,vertices:vec![Vertex::new(1.,2.,3.,0.5,0.4,Color::new(0.7,0.4,0.2,0.3));4],
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
            indices:indices.as_ptr() as usize,index_len:3,vertex_offset:0,index_offset:0,keep_vertices:false},
            PackSource {vertices:vertices.as_ptr() as usize,vertex_len:400,
            indices:indices.as_ptr() as usize,index_len:3,vertex_offset:400,index_offset:3,keep_vertices:false}];
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
            pack_kernel::<true>(&sources[..1],&mut v,&mut i,0,0,true);assert_eq!(scope.finish(),0);
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
    fn uniform_field_table_covers_the_block() {
        let mut previous = 0;
        for (_, end) in super::UNIFORM_FIELDS {
            assert!(*end > previous);
            previous = *end;
        }
        assert_eq!(previous, super::UNIFORM_FLOATS);
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
    /// finish_world's order with real merge banks: observe, merge, recycle the destination.
    /// Output bytes and versions-as-changes must not depend on who owns the observed arrays.
    #[test]
    fn owned_observation_keeps_merged_output_across_changes_and_recycled_banks() {
        use super::super::prepared::Cache;
        let run=|owned:bool| {
            let mut cache=MergeCache::default();let mut pool=BTreeMap::new();
            let mut publications=(0..4).map(|_|Cache::default()).collect::<Vec<_>>();
            let mut source=(0..4).map(|_|fixture_group()).collect::<Vec<_>>();
            let mut record=Vec::new();let mut emptied=0;
            for frame in 0..40u32 {
                source[(frame%4) as usize].passes[0].mesh.vertices[0].position.x=frame as f32;
                if frame==9 {source[1].passes[0].mesh.indices.swap(0,1);}
                if frame==17 {source[2].passes[0].mesh.vertices[1].normal.w=f32::from_bits(0x7fc00001);}
                if frame==23 {source[3].passes[0].mesh.vertices[2].uv.y=-0.;}
                let mut groups=source.iter().map(|g| {
                    let mut copy=fixture_group();copy.passes[0].mesh=super::super::copy_mesh(&g.passes[0].mesh);copy
                }).collect::<Vec<_>>();
                for (g,c) in groups.iter_mut().zip(&mut publications) {
                    let p=&mut g.passes[0];
                    p.geometry=Some(if owned {c.observe_owned(&mut p.mesh)} else {c.observe(&p.mesh,None)});
                    emptied+=u64::from(p.mesh.vertices.is_empty());
                }
                let mut list=if frame%13<9 {vec![(3.,Source::Actor(0)),(2.,Source::Actor(1)),(1.,Source::Actor(2)),(0.,Source::Actor(3))]}
                    else {vec![(3.,Source::Actor(2)),(2.,Source::World(0)),(1.,Source::Actor(0)),(0.,Source::Actor(1))]};
                let recycled=merge_cached(&mut cache,&mut groups,&mut list,false,true,&mut pool).to_vec();
                for g in &groups { for p in &g.passes {
                    let m=p.geometry();
                    record.push((m.vertices.iter().map(|v|(v.position.to_array().map(f32::to_bits),v.uv.to_array().map(f32::to_bits),v.color,v.normal.to_array().map(f32::to_bits))).collect::<Vec<_>>(),m.indices.clone()));
                }}
                record.push((Vec::new(),list.iter().map(|x|match x.1 {Source::Actor(i)=>i as u16,_=>9}).collect()));
                for &id in &recycled {
                    let p=&mut groups[id].passes[0];let g=p.geometry.take().unwrap();
                    pool.insert(id,std::rc::Rc::try_unwrap(g).ok().unwrap());
                }
            }
            (record,emptied)
        };
        let (off,emptied_off)=run(false);let (on,emptied_on)=run(true);
        assert_eq!(off,on);
        // Anti-vacuity: the copying arm never empties a pass, the owning arm did.
        assert_eq!(emptied_off,0);assert!(emptied_on>0);
    }
    #[test]
    fn partial_merge_ranges_match_full_copy_across_changes_and_recycled_banks() {
        crate::sim_maps::with_r2_switches([false, false, true], || {
            crate::character::skinning::overlap::startup(2, false);
            for ranges in [false, true] { for inline in [false, true] {
                let mut caches = [MergeCache::default(), MergeCache::default()];
                let mut pool = BTreeMap::new();
                let mut publications = (0..4).map(|_|super::super::prepared::Cache::default()).collect::<Vec<_>>();
                let mut kept = 0;
                for frame in 0..32 {
                    let mut expected = (0..4).map(|id| {
                        let mut g = fixture_group();
                        g.passes[0].mesh.vertices = vec![Vertex::new(id as f32, 0., 0., 0., 0., WHITE); 512];
                        g
                    }).collect::<Vec<_>>();
                    // One changing range alongside three frozen ranges; preserve all
                    // float bits, including NaN payloads and signed zero.
                    expected[0].passes[0].mesh.vertices[0].position.x = frame as f32;
                    expected[1].passes[0].mesh.vertices[2].normal.w = f32::from_bits(0x7fc00001);
                    expected[2].passes[0].mesh.vertices[3].uv.y = -0.;
                    if frame >= 5 { expected[2].passes[0].mesh.indices.swap(0, 1); }
                    if frame == 8 { publications[1] = Default::default(); }
                    if frame >= 12 { expected[3].passes[0].mesh.vertices.push(Vertex::new(1.,2.,3.,0.,0.,WHITE)); }
                    let mut actual = expected.iter().map(|g| {
                        let mut copy = fixture_group();copy.passes[0].mesh = super::super::copy_mesh(&g.passes[0].mesh);copy
                    }).collect::<Vec<_>>();
                    for (g, c) in actual.iter_mut().zip(&mut publications) {
                        g.passes[0].geometry = Some(c.observe(&g.passes[0].mesh, None));
                    }
                    let order = if frame < 16 { [0, 1, 2, 3] } else if frame % 2 == 0 { [0, 2, 1, 3] } else { [0, 3, 2, 1] };
                    let mut a = order.into_iter().map(|id|(0.,Source::Actor(id))).collect::<Vec<_>>();
                    let mut b = a.clone();merge_adjacent(&mut expected,&mut a);
                    let bank = if frame < 16 {0} else {frame % 2};
                    host_reset();
                    let recycled = merge_cached_options(&mut caches[bank], &mut actual, &mut b,
                        false, true, &mut pool, true, ranges, inline).to_vec();
                    kept += host_report()["fx_merge_range_vertices_kept"].as_u64().unwrap_or(0);
                    assert_eq!(a.iter().map(|x|x.1).collect::<Vec<_>>(), b.iter().map(|x|x.1).collect::<Vec<_>>());
                    for (a,b) in expected.iter().zip(&actual) {
                        assert_eq!(a.passes.len(),b.passes.len());
                        for (a,b) in a.passes.iter().zip(&b.passes) {
                            assert!(vertices_equal(&a.geometry().vertices,&b.geometry().vertices),"frame {frame}");
                            assert_eq!(a.geometry().indices,b.geometry().indices);
                        }
                    }
                    for id in recycled {
                        let g = actual[id].passes[0].geometry.take().unwrap();
                        pool.insert(id,std::rc::Rc::try_unwrap(g).ok().unwrap());
                    }
                }
                assert_eq!(kept > 0, ranges);
            }}
        });
    }
    #[test]
    fn local_merge_certificates_survive_unrelated_plan_misses_and_reject_other_banks() {
        for local in [false, true] {
            crate::sim_maps::with_r2_switches([false, false, local], || {
                let mut caches = [MergeCache::default(), MergeCache::default()];
                let mut pool = BTreeMap::new();
                let mut publications = (0..4).map(|_| super::super::prepared::Cache::default()).collect::<Vec<_>>();
                let mut previous = None;
                let mut preserved = 0;
                for frame in 0..20 {
                    let mut baseline = (0..4).map(|_| fixture_group()).collect::<Vec<_>>();
                    for id in 2..4 { baseline[id].passes[0].gain = Vec4::splat(0.1 + frame as f32 * 0.01); }
                    baseline[0].passes[0].mesh.vertices[0].position.x = (frame / 4) as f32;
                    if frame >= 6 { baseline[3].passes[0].mesh.indices.swap(0,1); }
                    if frame == 7 { publications[0] = Default::default(); }
                    let mut changed = baseline.iter().map(|g| {
                        let mut copy = fixture_group();
                        copy.passes[0].mesh = super::super::copy_mesh(&g.passes[0].mesh);
                        copy.passes[0].gain = g.passes[0].gain; copy
                    }).collect::<Vec<_>>();
                    for (g,c) in changed.iter_mut().zip(&mut publications) {
                        g.passes[0].geometry = Some(c.observe(&g.passes[0].mesh, None));
                    }
                    let mut order = if frame == 9 { vec![(4.,Source::Actor(1)),(3.,Source::Actor(0))] }
                        else { vec![(4.,Source::Actor(0)),(3.,Source::Actor(1))] };
                    order.extend([(2.,Source::Actor(2)),(1.,Source::Actor(3)),(0.,Source::World(frame))]);
                    let mut expected = order.clone();
                    merge_adjacent(&mut baseline, &mut expected);
                    let cache = &mut caches[if frame < 10 {0} else {frame % 2}];
                    let ids = merge_cached_mode(cache, &mut changed, &mut order, false, true, &mut pool, true).to_vec();
                    assert_eq!(order.iter().map(|x|x.1).collect::<Vec<_>>(), expected.iter().map(|x|x.1).collect::<Vec<_>>());
                    for (a,b) in baseline.iter().zip(&changed) {
                        assert_eq!(a.passes.len(), b.passes.len());
                        for (a,b) in a.passes.iter().zip(&b.passes) {
                            assert!(vertices_equal(&a.geometry().vertices, &b.geometry().vertices));
                            assert_eq!(a.geometry().indices, b.geometry().indices);
                        }
                    }
                    if frame < 3 {
                        let current = changed[0].passes[0].geometry.as_ref().unwrap().version;
                        if previous == Some(current) { preserved += 1; }
                        previous = Some(current);
                    }
                    for id in ids {
                        let g = changed[id].passes[0].geometry.take().unwrap();
                        pool.insert(id, std::rc::Rc::try_unwrap(g).ok().unwrap());
                    }
                }
                assert_eq!(preserved, if local {2} else {0});
            });
        }
    }
    #[test]
    fn unchanged_plan_rejects_recycled_geometry_from_a_different_merge_bank() {
        crate::sim_maps::with_r2_switches([false, false, true], || {
            let mut caches = [MergeCache::default(), MergeCache::default()];
            let mut pool = BTreeMap::new();
            let mut publications = (0..3).map(|_| super::super::prepared::Cache::default()).collect::<Vec<_>>();
            for frame in 0..8 {
                let mut baseline = (0..3).map(|i| {
                    let mut g = fixture_group(); g.passes[0].mesh.vertices[0].position.x = i as f32; g
                }).collect::<Vec<_>>();
                let mut changed = baseline.iter().map(|g| {
                    let mut copy = fixture_group(); copy.passes[0].mesh = super::super::copy_mesh(&g.passes[0].mesh); copy
                }).collect::<Vec<_>>();
                for (g,c) in changed.iter_mut().zip(&mut publications) { g.passes[0].geometry = Some(c.observe(&g.passes[0].mesh,None)); }
                let (source, rest) = if frame % 2 == 0 {(1,2)} else {(2,1)};
                let mut order = vec![(4.,Source::Actor(0)),(3.,Source::Actor(source)),(2.,Source::World(0)),(1.,Source::Actor(rest))];
                let mut expected = order.clone(); merge_adjacent(&mut baseline,&mut expected);
                let ids = merge_cached_mode(&mut caches[frame % 2],&mut changed,&mut order,false,true,&mut pool,true).to_vec();
                assert_eq!(order.iter().map(|x|x.1).collect::<Vec<_>>(),expected.iter().map(|x|x.1).collect::<Vec<_>>());
                assert!(vertices_equal(&changed[0].passes[0].geometry().vertices,&baseline[0].passes[0].geometry().vertices));
                assert_eq!(changed[0].passes[0].geometry().indices,baseline[0].passes[0].geometry().indices);
                for id in ids { let g = changed[id].passes[0].geometry.take().unwrap(); pool.insert(id,std::rc::Rc::try_unwrap(g).ok().unwrap()); }
            }
        });
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
        let mut m = Mesh { upload_identity: None, surface: None,
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
            ant_dead: None,
            transparent: true,
            passes: vec![Pass {
                _game_thread: crate::game_thread::GameThread::default(),
                geometry:None,
                effect: false,
                blend_index: 1,
                depth_equal: false,
                mesh: Mesh { upload_identity: None, surface: None,
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
            Mesh { upload_identity: None, surface: None,
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
    fn light_block(u: &[f32; UNIFORM_FLOATS]) -> &[f32] {
        let end = UNIFORM_FIELDS.iter().find(|(name, _)| *name == "Lights").unwrap().1;
        &u[end - 64..end]
    }
    fn nonzero_lights(u: &[f32; UNIFORM_FLOATS]) -> bool {
        light_block(u).iter().any(|v| *v != 0.)
    }
    fn frame_with_lights(fullbright: bool) {
        FRAME.with(|f| {
            *f.borrow_mut() = Frame {
                projection: Some(Mat4::IDENTITY),
                camera: vec3(8., 9., 10.),
                fullbright,
                ..Default::default()
            }
        });
        crate::lighting::fixture_lights(vec![
            crate::lighting::Light { position: vec3(1., 2., 3.), color: vec3(0.6, 0.7, 0.8), radius: 9., only_models: false, flare: false },
            crate::lighting::Light { position: vec3(-4., 5., 6.), color: vec3(0.1, 0.2, 0.3), radius: 7., only_models: true, flare: false },
        ]);
    }
    fn clear_lights() {
        crate::lighting::fixture_lights(Vec::new());
        FRAME.with(|f| *f.borrow_mut() = Frame::default());
    }
    fn effect_pass(lit: bool, model: bool) -> Pass {
        let mut p = fixture_group().passes.pop().unwrap();
        p.effect = true;
        p.lit = lit;
        p.model = model;
        p
    }
    #[test]
    fn unlit_exact_fragment_replaces_only_the_dynamic_light_call() {
        let full = crate::render::FRAGMENT;
        assert_eq!(full.matches("dynamicLight(worldPosition)").count(), 1);
        let (before, after) = full.split_once("dynamicLight(worldPosition)").unwrap();
        let exact = unlit_exact_fragment();
        assert_eq!(exact, format!("{before}vec3(0.0){after}"));
        assert!(!exact.contains("dynamicLight("));
        // Every operation outside the zero light addition is retained verbatim.
        for operation in [
            "texture2D(Texture,coords)",
            "CutoutReference",
            "discard",
            "LightAtlas",
            "lighting=min(vec3(1.0),texture2D(LightAtlas,sampleUV).rgb*2.0)",
            "diffuse.rgb*mix(vec3(1.0),lighting,Lit*(1.0-Fullbright))*Gain.rgb",
            "rgb+=diffuse.rgb*vec3(0.0)*Gain.rgb",
            "FxAppearance.x", "FxAppearance.y", "FxAppearance.z", "FxAppearance.w",
            "vec3(0.65,1.15,0.7)", "vec3(1.3,0.55,0.4)",
            "fogged(", "SkyBackdrop",
        ] {
            assert!(exact.contains(operation), "specialized source lost {operation}");
        }
        assert_eq!(Specialized::UnlitExact.fragment(), exact);
        assert_ne!(Specialized::Lite.fragment(), exact);
    }
    #[test]
    fn specialized_program_precedence_and_fallback_are_explicit() {
        assert_eq!(specialized_program(false, false), None);
        assert_eq!(specialized_program(true, false), Some(Specialized::Lite));
        assert_eq!(specialized_program(false, true), Some(Specialized::UnlitExact));
        assert_eq!(specialized_program(true, true), Some(Specialized::UnlitExact));
        assert_eq!(Specialized::Lite.name(), "retained-fx-lite");
        assert_eq!(Specialized::UnlitExact.name(), "retained-fx-unlit-exact");
        let mut p = effect_pass(false, true);
        assert!(lite_pass(&p));
        assert_eq!(route(&p, false, true), Route::Lite);
        assert_eq!(route(&p, false, false), Route::Full, "missing program falls back");
        p.lit = true;
        assert_eq!(route(&p, false, true), Route::Full, "lit pass never uses either lean program");
        p.lit = false;
        p.effect = false;
        assert_eq!(route(&p, false, true), Route::Full, "non-effect pass is not an FX specialization");
    }
    // RC10 merges two branches that each selected one opt-in program. Pin the whole
    // truth table so neither source branch's selection is lost in the union.
    #[test]
    fn combined_actor_program_selection_matches_each_source_branch() {
        let config = |keys: &str| -> crate::render::vita_renderer::Config {
            serde_json::from_str(keys).unwrap()
        };
        // All off: no lean program and no corpse program, as RC9 before either branch.
        assert_eq!(program_choice(&config("{}")), (None, false));
        // fx_lite alone: the existing lean program, unchanged by hatter2.
        assert_eq!(program_choice(&config(r#"{"fx_lite":true}"#)),
            (Some(Specialized::Lite), false));
        // fx_unlit_exact alone: hatter2's program only.
        assert_eq!(program_choice(&config(r#"{"fx_unlit_exact":true}"#)),
            (Some(Specialized::UnlitExact), false));
        // actor_opaque_shader alone: skool1's corpse program, still no lean program.
        assert_eq!(program_choice(&config(r#"{"actor_opaque_shader":true}"#)), (None, true));
        // The RC8 corpse default keeps asking for the same program.
        assert_eq!(program_choice(&config(r#"{"ant_corpse_opaque_shader":true}"#)), (None, true));
        // The merged case: hatter2's program and skool1's corpse program together.
        assert_eq!(program_choice(&config(r#"{"fx_unlit_exact":true,"actor_opaque_shader":true}"#)),
            (Some(Specialized::UnlitExact), true));
        // Combinations neither source branch could see: precedence, and both at once.
        assert_eq!(program_choice(&config(r#"{"fx_lite":true,"fx_unlit_exact":true}"#)),
            (Some(Specialized::UnlitExact), false));
        assert_eq!(program_choice(&config(r#"{"fx_lite":true,"actor_opaque_shader":true}"#)),
            (Some(Specialized::Lite), true));
    }
    #[test]
    fn specialization_rejects_each_clip_and_appearance_path_and_counts_routes() {
        let base = || effect_pass(false, true);
        for (name, adjust) in [
            ("clamp", (|p: &mut Pass| p.clamp = true) as fn(&mut Pass)),
            ("cutout gt0", |p: &mut Pass| p.cutout = 1.),
            ("cutout reference", |p: &mut Pass| p.cutout = 5.),
            ("cutout threshold half", |p: &mut Pass| p.cutout = 0.5),
            ("dissolve", |p: &mut Pass| p.appearance.dissolve = 0.0001),
            ("ghost", |p: &mut Pass| p.appearance.ghost = true),
            ("power red", |p: &mut Pass| p.appearance.power = 1.),
            ("power green", |p: &mut Pass| p.appearance.power = 2.),
            ("filter fade", |p: &mut Pass| p.filter_fade = 0.25),
            ("negative filter fade", |p: &mut Pass| p.filter_fade = -0.25),
        ] {
            let mut p = base();
            assert_eq!(route(&p, false, true), Route::Lite, "control {name}");
            adjust(&mut p);
            assert!(!lite_pass(&p), "{name} was admitted");
            assert_eq!(route(&p, false, true), Route::Full, "{name} used the lean program");
        }
        let mut sequence = Vec::new();
        for lit in [true, false, true, false] {
            sequence.push(effect_pass(lit, true));
        }
        let mut muted = base();
        muted.clamp = true;
        sequence.push(muted);
        let mut harmless = base();
        harmless.appearance.power = 0.5;
        sequence.push(harmless);
        let counts = sequence.iter().fold([0u64; 3], |mut counts, p| {
            counts[match route(p, false, true) { Route::Full => 0, Route::Lite => 1, Route::CorpseOpaque => 2 }] += 1;
            counts
        });
        assert_eq!(counts, [3, 3, 0], "both routes must be exercised");
    }
    #[test]
    fn lit_effects_keep_nonzero_dynamic_lights_through_run_time_switches() {
        frame_with_lights(false);
        let mut templates = [None; 4];
        let mut p = effect_pass(true, true);
        let lit = FRAME.with(|f| uniforms(&p, &f.borrow()));
        assert_eq!(lit[35], 1.);
        assert!(nonzero_lights(&lit));
        assert_eq!(prepared_uniforms(&p, &mut templates).map(f32::to_bits), lit.map(f32::to_bits));
        assert_eq!(route(&p, false, true), Route::Full);
        // The same payload switches to unlit: the packed lights become zero and
        // the draw changes program. Switching back restores the lit payload.
        p.lit = false;
        let unlit = FRAME.with(|f| uniforms(&p, &f.borrow()));
        assert_eq!(unlit[35], 0.);
        assert!(!nonzero_lights(&unlit));
        assert_eq!(prepared_uniforms(&p, &mut templates).map(f32::to_bits), unlit.map(f32::to_bits));
        assert_eq!(route(&p, false, true), Route::Lite);
        p.lit = true;
        let lit_again = FRAME.with(|f| uniforms(&p, &f.borrow()));
        assert!(nonzero_lights(&lit_again));
        assert_eq!(lit_again.map(f32::to_bits), lit.map(f32::to_bits));
        assert_eq!(route(&p, false, true), Route::Full);
        // Fullbright zeroes the packed payload but must never admit a lit pass.
        FRAME.with(|f| f.borrow_mut().fullbright = true);
        let fullbright = FRAME.with(|f| uniforms(&p, &f.borrow()));
        assert_eq!(fullbright[34], 1.);
        assert!(!nonzero_lights(&fullbright));
        assert_eq!(route(&p, false, true), Route::Full);
        p.lit = false;
        assert_eq!(route(&p, false, true), Route::Lite);
        // Non-model passes exclude models-only lights; lit non-model passes
        // still keep the full program and their allowed lights.
        FRAME.with(|f| f.borrow_mut().fullbright = false);
        p.lit = true;
        p.model = false;
        let non_model = FRAME.with(|f| uniforms(&p, &f.borrow()));
        assert!(nonzero_lights(&non_model));
        assert_eq!(&non_model[52..56], &[1., 2., 3., 9.]);
        assert_eq!(&non_model[64..68], &[0.; 4], "models-only light leaked into a non-model pass");
        assert_eq!(route(&p, false, true), Route::Full);
        clear_lights();
    }
    #[test]
    fn lightmap_bearing_geometry_is_classified_by_lit_not_by_its_atlas_coordinates() {
        let mut p = effect_pass(false, false);
        for v in &mut p.mesh.vertices {
            // lm.xy are atlas coordinates and lm.z selects the lightmap tile.
            v.normal = vec4(0.25, 0.5, 2., 0.);
        }
        assert_eq!(route(&p, false, true), Route::Lite, "unlit lightmapped FX must stay specialized");
        p.lit = true;
        assert_eq!(route(&p, false, true), Route::Full, "lit lightmapped FX must keep the full program");
        // The retained lightmap branch is still present in the exact source; it is
        // multiplied through Lit=0 for an admitted pass instead of being removed.
        assert!(unlit_exact_fragment().contains("lm.z>0.5&&FxModel<0.5"));
        assert!(unlit_exact_fragment().contains("Lit*(1.0-Fullbright)"));
    }
    #[test]
    fn lit_and_unlit_draws_never_share_cached_state() {
        frame_with_lights(false);
        let mut templates = [None; 4];
        let mut p = effect_pass(false, true);
        let unlit = prepared_uniforms(&p, &mut templates);
        assert!(!nonzero_lights(&unlit));
        p.lit = true;
        let lit = prepared_uniforms(&p, &mut templates);
        assert!(nonzero_lights(&lit), "the unlit template leaked into the lit payload");
        p.lit = false;
        let unlit_again = prepared_uniforms(&p, &mut templates);
        assert_eq!(unlit_again.map(f32::to_bits), unlit.map(f32::to_bits));
        // model/non-model is a separate template axis as well.
        p.lit = true;
        p.model = false;
        let non_model = prepared_uniforms(&p, &mut templates);
        assert_eq!(&non_model[52..56], &[1., 2., 3., 9.]);
        assert_eq!(&non_model[64..68], &[0.; 4]);
        // Merge and packet identity carry the lit bit.
        p.model = true;
        p.lit = false;
        let unlit_key = packet_key(&p);
        p.lit = true;
        let lit_key = packet_key(&p);
        assert!(unlit_key != lit_key);
        let mut groups = vec![fixture_group(), fixture_group()];
        for g in &mut groups {
            g.passes[0].effect = true;
            g.passes[0].lit = false;
        }
        groups[1].passes[0].lit = true;
        let mut list = vec![(2., Source::Actor(0)), (1., Source::Actor(1))];
        merge_adjacent(&mut groups, &mut list);
        assert_eq!(list.len(), 2, "lit and unlit packets were merged");
        // Each program owns a distinct submit-cache slot even when the payload
        // bytes, bindings and blend are identical.
        let full_key = submit_key((7, false), Route::Full, true);
        let lite_key = submit_key((7, false), Route::Lite, true);
        let opaque_key = submit_key((7, false), Route::CorpseOpaque, true);
        assert_ne!(full_key, lite_key);
        assert_ne!(full_key, opaque_key);
        assert_ne!(lite_key, opaque_key);
        let bindings = (1, [2, 3, 4]);
        let u = [1., 2., 3., 4.];
        let mut cache = SubmitCache::<(usize, bool, bool), (usize, [usize; 3]), 4>::default();
        assert_eq!(cache.observe(full_key, bindings, &u), [true; 3]);
        assert_eq!(cache.observe(full_key, bindings, &u), [false; 3]);
        assert_eq!(cache.observe(lite_key, bindings, &u), [true; 3], "program switch kept the full cache");
        assert_eq!(cache.observe(lite_key, bindings, &u), [false; 3]);
        assert_eq!(cache.observe(full_key, bindings, &u), [true; 3]);
        clear_lights();
    }
    /// The producer side of an electric/frozen shell, without a GL context:
    /// `effect` and `lit` come from the real `skin*` path, including the actor
    /// cache used by opaque frozen geometry.
    fn produced_classification(stage: crate::materials::Stage, texture: u32, effect: bool,
        appearance: super::super::Appearance) -> (bool, bool, bool) {
        use crate::render_fx::test_support;
        let base = test_support::texture(texture);
        let _surface = test_support::register(&base, vec![(stage, vec![base.clone()])], true);
        let mesh = Mesh { upload_identity: None, surface: None,
            vertices: vec![
                Vertex { position: vec3(-1., -1., 0.), uv: vec2(0., 0.),
                    color: [255, 255, 255, 200], normal: vec4(0.25, 0.5, 2., 0.) },
                Vertex { position: vec3(1., -1., 0.), uv: vec2(1., 0.),
                    color: [255, 255, 255, 200], normal: vec4(0.25, 0.5, 2., 0.) },
                Vertex { position: vec3(0., 1., 0.), uv: vec2(0.5, 1.),
                    color: [255, 255, 255, 200], normal: vec4(0.25, 0.5, 2., 0.) },
            ],
            indices: vec![0, 1, 2],
            texture: Some(base),
        };
        test_support::begin_direct(vec3(0., 0., 10.), Vec3::Z, 0.25, appearance);
        if effect { crate::render_fx::skin_effect(&mesh, 0.2, 1.); } else { crate::render_fx::skin(&mesh); }
        let row = FRAME.with(|f| {
            let f = f.borrow();
            assert_eq!(f.groups.len(), 1, "one shell must submit one group");
            assert_eq!(f.groups[0].passes.len(), 1, "one stage must submit one pass");
            let p = &f.groups[0].passes[0];
            (p.effect, p.lit, lite_pass(p))
        });
        test_support::end_frame();
        test_support::clear();
        row
    }
    fn parsed_stage(source: &str, name: &str) -> crate::materials::Stage {
        crate::materials::parse(source).unwrap()[name].stages[0].clone()
    }
    #[test]
    fn lit_material_producers_keep_lighting_and_electric_frozen_and_tints_stay_full() {
        use super::super::Appearance;
        let shock = parsed_stage("shock\n{\n{\nalphatest greaterequal 0.3\nmap lightning.tga\nblendFunc GL_SRC_ALPHA GL_ONE_MINUS_SRC_ALPHA\nalphaGen wave inversesawtooth -2 3 0 1\n}\n}", "shock");
        let (effect, lit, admitted) = produced_classification(shock, 0x7f01, true, Appearance::default());
        assert!(effect && lit);
        assert!(!admitted, "electric shell entered the unlit program");
        for (index, generator) in ["lightingdiffuse", "vertex", "entity"].into_iter().enumerate() {
            let stage = parsed_stage(&format!("lit\n{{\n{{\nmap {generator}.tga\nrgbGen {generator}\n}}\n}}"), "lit");
            let (effect, lit, admitted) = produced_classification(stage, 0x7f10 + index as u32, true, Appearance::default());
            assert!(effect, "{generator} effect was dropped");
            assert!(lit, "rgbGen {generator} lost its lighting classification");
            assert!(!admitted, "rgbGen {generator} entered the unlit program");
        }
        let alpha_lit = parsed_stage("lit\n{\n{\nmap alpha.tga\nalphaGen lightingSpecular\n}\n}", "lit");
        let (effect, lit, admitted) = produced_classification(alpha_lit, 0x7f20, true, Appearance::default());
        assert!(effect && lit);
        assert!(!admitted, "lighting-dependent alpha entered the unlit program");
        for (index, material) in ["powerups/iceblock1", "powerups/iceblock2"].into_iter().enumerate() {
            let name = material.rsplit('/').next().unwrap();
            let stage = parsed_stage(&format!("{material}\n{{\n{{\nmap ice.tga\nblendFunc GL_ONE GL_ONE\nrgbGen entity\n}}\n}}"), material);
            let (effect, lit, admitted) = produced_classification(stage, 0x7f30 + index as u32, false, Appearance::default());
            assert!(!effect, "{name} frozen shell was marked as an effect");
            assert!(lit, "{name} lost its entity lighting");
            assert!(!admitted, "{name} frozen shell entered the unlit program");
        }
        // Anti-vacuity: an unlit, untinted effect does take the specialized route,
        // and the same material through frozen's skin() does not become an effect.
        let unlit = parsed_stage("steam\n{\n{\nmap steam.tga\nrgbGen identity\n}\n}", "steam");
        assert_eq!(produced_classification(unlit.clone(), 0x7f40, true, Appearance::default()), (true, false, true));
        assert_eq!(produced_classification(unlit.clone(), 0x7f41, false, Appearance::default()), (false, false, false));
        for (power, label) in [(1., "red"), (2., "green")] {
            let tinted = Appearance { power, ..Default::default() };
            assert_eq!(produced_classification(unlit.clone(), 0x7f42 + (power as u32), true, tinted), (true, false, false), "{label} power tint was admitted");
        }
    }
    #[test]
    fn one_effect_switching_lit_and_unlit_at_run_time_switches_programs_and_payloads() {
        use crate::render_fx::test_support;
        use super::super::Appearance;
        frame_with_lights(false);
        let lit_stage = parsed_stage("lit\n{\n{\nmap lit.tga\nrgbGen lightingDiffuse\n}\n}", "lit");
        let unlit_stage = parsed_stage("unlit\n{\n{\nmap unlit.tga\nrgbGen identity\n}\n}", "unlit");
        let lit_texture = test_support::texture(0x7f50);
        let unlit_texture = test_support::texture(0x7f51);
        let _lit = test_support::register(&lit_texture, vec![(lit_stage, vec![lit_texture.clone()])], true);
        let _unlit = test_support::register(&unlit_texture, vec![(unlit_stage, vec![unlit_texture.clone()])], true);
        let mut mesh = Mesh { upload_identity: None, surface: None,
            vertices: vec![Vertex::new(0., 0., 0., 0., 0., WHITE); 3],
            indices: vec![0, 1, 2],
            texture: Some(lit_texture.clone()),
        };
        let mut routes = Vec::new();
        for (lit_material, fullbright) in [(true, false), (false, false), (true, false), (false, true), (true, true)] {
            mesh.texture = Some(if lit_material { lit_texture.clone() } else { unlit_texture.clone() });
            test_support::begin_direct(vec3(0., 0., 10.), Vec3::Z, 0.25, Appearance::default());
            FRAME.with(|f| {
                let mut f = f.borrow_mut();
                f.projection = Some(Mat4::IDENTITY);
                f.fullbright = fullbright;
            });
            crate::render_fx::skin_effect(&mesh, 0.2, 1.);
            let (route, has_lights, lit) = FRAME.with(|f| {
                let f = f.borrow();
                let p = &f.groups[0].passes[0];
                (route(p, false, true), nonzero_lights(&uniforms(p, &f)), p.lit)
            });
            routes.push(route);
            if lit && !fullbright {
                assert!(has_lights, "lit stage lost its dynamic lights");
            } else {
                assert!(!has_lights, "zero-light pass kept a nonzero payload");
            }
            test_support::end_frame();
        }
        test_support::clear();
        assert_eq!(routes, vec![Route::Full, Route::Lite, Route::Full, Route::Lite, Route::Full]);
        clear_lights();
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

#[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
fn census_groups(stage:&str,groups:&[super::Group]) {
    if !crate::character::skinning::census::active() {return;}
    for (id,g) in groups.iter().enumerate() {
        println!("[lg/group-census] frame={} stage={stage} group={id} owner={:?} layers={} triangles={} effect={} textures={:?}",crate::sim_maps::probe::frame(),crate::character::skinning::census::owner(id),g.passes.len(),g.passes.iter().map(|p|p.geometry().indices.len()/3).sum::<usize>(),g.passes.iter().all(|p|p.effect),g.passes.iter().map(|p|crate::sim_maps::probe::draw_query::actor_label(p.mesh.texture.as_ref().map(Texture2D::raw_miniquad_id))).collect::<Vec<_>>());
    }
}
