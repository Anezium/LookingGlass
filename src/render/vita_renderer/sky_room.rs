//! `sky_live`: the authored sky room drawn every frame from retained buffers.
//!
//! The legacy `authored_sky` draws the same room through Macroquad: one draw call per layer with
//! its own vertex and index upload, CPU vertex transforms, about 45 uniform writes, and the
//! all-in-one fragment program with `discard`, eight per-fragment lights and the fog function.
//! Here the room is packed once at load into immutable buffers of its own, and each frame costs a
//! frustum test per batch, one uniform block per layer and one draw per layer. No geometry upload,
//! no CPU vertex work, nothing queued in Macroquad.
//!
//! What a set holds, as the legacy sky room pass selects it for one sky camera: the static world
//! surfaces the sky camera's cluster can see, the static models whose bounds that cluster can see,
//! and every sky prop. Every stage of every surface is kept, in order, with its own blend and its
//! `depthfunc equal`: the world renderer's `max_layers` and five blend classes do not apply, because
//! a sky is mostly stages (the chess sky is three, two of them equal-depth).
//!
//! Order, as the legacy path: opaque batches then opaque props with depth writes on, translucent
//! batches then translucent props with depth writes off, each batch drawing its layers in stage
//! order; then a depth clear; then the caller draws the sky faces into depth only; then the world.
//! Scrolling stages, sky props with scripted motion, the sky rotation and scripted sky camera
//! moves are uniforms evaluated each frame, so the sky moves as on the desktop renderer.
//!
//! A map can own several sky cameras (the hedge maps, grounds1, keep) and level code can move
//! one. Every candidate gets a set at load, deduplicated by cluster; a frame draws the set whose
//! camera is nearest to the live sky origin. No legacy geometry stays alive for that.
//!
//! `sky_live_lite` (on by default with `sky_live`) draws the room through two lean fragment
//! programs: texture times gain, and the same with the alpha test for surfaces that have one.
//! The room is fullbright and unfogged, so that is what the world program computes there, at a
//! fraction of the per-fragment work. Sky rooms are mostly blended layers (up to four screens of
//! them on the fortress maps), which is where this GPU pays. Every stage of one surface goes
//! through the same program, so its equal-depth stages meet the depth its first stage wrote.
//!
//! `sky_live_scale` below 1 draws the room into a smaller target of its own first and copies it
//! over the 3D target with one full-screen triangle, to cut the fill of translucent sky layers.
//! The room then needs no depth clear in the 3D target. It costs one more GXM scene a frame.
//!
//! Two things do change vertices each frame, and each goes through one small stream buffer:
//! props with vertex animation (the hatter watch hands) get their positions from the same
//! `Frames::position` the legacy path calls, and the sky faces that carry a sky shader instead of
//! showing the room (grounds1, grounds2, qlair, tower3) get the legacy cloud projection of the
//! eye-to-vertex ray as texture coordinates. Those faces are drawn after the depth clear, merged
//! per shader, with every stage.
//!
//! Not reproduced: autosprite deforms (the quads keep their authored facing), wave-normal
//! ripples, and the fortress1 `SkyFade` backdrop.
use super::*;

/// The most sky cameras one map gets a set for.
const MAX_SETS: usize = 12;

/// Which program draws a surface: the world's full one, or one of the two lean ones.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Program { World, Lite, LiteCut }
/// One stage of one batch.
struct Layer {
    recipe: Recipe,
    pipeline: Pipeline,
    program: Program,
}
/// One legacy batch as the sky room draws it: a range of a set's buffer and its stages.
struct Batch {
    slot: u32,
    base_element: i32,
    indices: i32,
    min: Vec3,
    max: Vec3,
    layers: std::ops::Range<u32>,
}
/// `Batch::slot` of a batch whose vertices live in the set's stream buffer.
const STREAM: u32 = u32::MAX;
/// Vertices rewritten every frame: one stream vertex buffer, one immutable index buffer.
struct Stream {
    vertex: BufferId,
    index: BufferId,
    template: Vec<WorldVertex>,
}
/// One vertex-animated prop inside a set's stream: where its vertices start and which source
/// vertex each one is.
struct Clip {
    frames: crate::ambient_animation::Frames,
    first: usize,
    source: Vec<u16>,
}
/// Everything one sky camera can show.
struct Set {
    origin: Vec3,
    cluster: Option<usize>,
    buffers: Vec<(BufferId, BufferId)>,
    stream: Option<Stream>,
    clips: Vec<Clip>,
    opaque: Vec<Batch>,
    alpha: Vec<Batch>,
    layers: Vec<Layer>,
    logged: bool,
}
/// The sky faces that carry a sky shader, merged per shader. Positions never change; the
/// texture coordinates are the cloud projection of the ray from the eye.
struct SkyLayers {
    stream: Stream,
    batches: Vec<Batch>,
    layers: Vec<Layer>,
}
#[derive(Default, Clone, Copy)]
pub(super) struct Counts {
    pub tested: u64,
    pub opaque: u64,
    pub alpha: u64,
    pub triangles: u64,
    pub copies: u64,
    pub lite: u64,
    pub upload_bytes: u64,
    pub face_layers: u64,
}
/// `sky_live_lite`: the two lean programs, each with its uniforms. `cut` is `None` when only
/// the plain one built; surfaces with an alpha test then stay on the world program.
struct Lite {
    shader: ShaderId,
    cache: crate::render::uniform_fields::Cache,
    cut: Option<(ShaderId, crate::render::uniform_fields::Cache)>,
}
/// FRAGMENT for a sky room surface without alpha test. The room is drawn fullbright (Surface.w=1
/// makes the lighting factor exactly 1) and without fog (`fogged` returns its input), and
/// Surface.z=0 never discards, so each removed line is a no-op there. The products keep
/// FRAGMENT's variables. The uniform block is FRAGMENT's; the fields this program does not read
/// have no location.
pub(super) const LITE_FRAGMENT: &str = r#"#version 100
precision mediump float;
varying highp vec2 uv;
varying lowp vec4 color;
uniform sampler2D Diffuse;
uniform highp vec4 Surface, Gain;
void main(){
 vec2 coord=mix(uv,clamp(uv,vec2(0.001),vec2(0.999)),Surface.x);
 vec4 diffuse=texture2D(Diffuse,coord);
 float a=diffuse.a*Gain.a*color.a;
 vec3 rgb=diffuse.rgb*Gain.rgb;
 gl_FragColor=vec4(rgb,a);
}"#;
/// LITE_FRAGMENT with FRAGMENT's alpha test, for a surface with a tested stage (the chess sky's
/// checker mask, gear teeth). Its other stages run here too with Surface.z=0.
pub(super) const LITE_CUT_FRAGMENT: &str = r#"#version 100
precision mediump float;
varying highp vec2 uv;
varying lowp vec4 color;
uniform sampler2D Diffuse;
uniform highp vec4 Surface, Gain, Alpha;
void main(){
 vec2 coord=mix(uv,clamp(uv,vec2(0.001),vec2(0.999)),Surface.x);
 vec4 diffuse=texture2D(Diffuse,coord);
 float a=diffuse.a*Gain.a*color.a;
 float cut=Surface.z;
 if((cut>0.5&&cut<1.5&&a<=0.0)||(cut>1.5&&cut<2.5&&a<0.5)
  ||(cut>2.5&&cut<3.5&&a>=0.5)||(cut>3.5&&cut<4.5&&a<0.35))discard;
 if((cut>4.5&&cut<5.5&&a<Alpha.x)||(cut>5.5&&cut<6.5&&a<=Alpha.x)
  ||(cut>6.5&&cut<7.5&&a>=Alpha.x)||(cut>7.5&&a>Alpha.x))discard;
 vec3 rgb=diffuse.rgb*Gain.rgb;
 gl_FragColor=vec4(rgb,a);
}"#;
/// `sky_live_scale` below 1: the copy program, and the target once the 3D size is known.
struct CopyPass {
    shader: ShaderId,
    pipeline: Pipeline,
    target: Option<Target>,
}
struct Target {
    rt: RenderTarget,
    #[cfg(not(target_os = "vita"))]
    depth: macroquad::miniquad::TextureId,
    size: [u32; 2],
}
const COPY_VERTEX: &str = r#"#version 100
attribute vec2 position;
uniform highp vec4 Flip;
varying highp vec2 uv;
void main(){uv=position*0.5+0.5;if(Flip.x>0.5)uv.y=1.0-uv.y;gl_Position=vec4(position,0.999,1.0);}"#;
const COPY_FRAGMENT: &str = r#"#version 100
precision mediump float;
varying highp vec2 uv;
uniform sampler2D Sky;
void main(){gl_FragColor=vec4(texture2D(Sky,uv).rgb,1.0);}"#;

pub(super) struct SkyRoom {
    sets: Vec<Set>,
    faces: Option<SkyLayers>,
    pipelines: Vec<Pipeline>,
    copy: Option<CopyPass>,
    lite: Option<Lite>,
    atlas: TextureId,
    current: Option<usize>,
}

/// The portal pose as a matrix: a sky room point `p` lands at `eye + r * (p - origin)`, with `r`
/// the inverse of the sky rotation times the scripted camera motion. Same values as
/// `authored_sky::portal_pose` applies per vertex.
pub(super) fn pose(eye: Vec3, origin: Vec3, rotation: Quat, motion: Quat) -> Mat4 {
    let r = (rotation * motion).inverse();
    Mat4::from_translation(eye) * Mat4::from_quat(r) * Mat4::from_translation(-origin)
}

/// The legacy sky shader projection: infinite and curved, it follows the view direction only,
/// so the clouds do not slide with the player. Same expression as `draw_batch` evaluates per
/// vertex of a sky face.
pub(super) fn cloud_uv(position: Vec3, eye: Vec3) -> Vec2 {
    let ray = (position - eye).normalize_or_zero();
    ray.truncate() / (ray.z.abs() + 0.25) * 0.25 + Vec2::splat(0.5)
}

/// Which indices of a legacy batch one sky camera draws. `mask`: the faces its cluster sees
/// (`None` sees all), `bounds_visible`: whether that cluster sees a box. A batch that cannot be
/// culled (a deform that moves vertices, or a scripted motion) is drawn whole or not at all.
pub(super) fn selection(parts: &[(usize, std::ops::Range<usize>)], total: usize, whole: bool,
                        mask: Option<&[bool]>, bounds_visible: bool) -> Vec<std::ops::Range<usize>> {
    let seen = |face: usize| mask.is_none_or(|m| m.get(face).copied().unwrap_or(true));
    if parts.is_empty() {
        return if bounds_visible && total > 0 { vec![0..total] } else { Vec::new() };
    }
    if whole {
        return if parts.iter().any(|p| seen(p.0)) { vec![0..total] } else { Vec::new() };
    }
    parts.iter().filter(|p| seen(p.0)).map(|p| p.1.clone()).collect()
}

/// Vertices and indices of the buffer pairs being filled.
#[derive(Default)]
struct Packing {
    buffers: Vec<(Vec<WorldVertex>, Vec<u16>)>,
}
impl Packing {
    /// Append the triangles of `ranges`, with only the vertices they use. Returns the buffer,
    /// the first index, the index count and the bounds, or `None` for nothing to draw.
    fn append(&mut self, b: &crate::render::Batch, ranges: &[std::ops::Range<usize>]) -> Option<(u32, i32, i32, Vec3, Vec3)> {
        let count: usize = ranges.iter().map(|r| r.len()).sum();
        if count == 0 || b.base.len() > u16::MAX as usize { return None; }
        if self.buffers.last().is_none_or(|(v, _)| v.len() + b.base.len() > u16::MAX as usize) {
            self.buffers.push((Vec::new(), Vec::new()));
        }
        let slot = self.buffers.len() - 1;
        let (base_element, count, min, max, _) = pack(self.buffers.last_mut().unwrap(), b, ranges)?;
        Some((slot as u32, base_element, count, min, max))
    }
}
/// Append to one buffer pair. Also returns, for the vertices this call added and in their
/// order, which source vertex each one is. `None` when nothing is drawn or the pair is full.
fn pack(target: &mut (Vec<WorldVertex>, Vec<u16>), b: &crate::render::Batch, ranges: &[std::ops::Range<usize>])
    -> Option<(i32, i32, Vec3, Vec3, Vec<u16>)> {
    let count: usize = ranges.iter().map(|r| r.len()).sum();
    if count == 0 || target.0.len() + b.base.len() > u16::MAX as usize { return None; }
    let (vertices, indices) = target;
    let base_element = indices.len();
    let mut remap = vec![u16::MAX; b.base.len()];
    let mut source = Vec::new();
    let (mut min, mut max) = (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY));
    for range in ranges {
        for &index in &b.full_indices[range.clone()] {
            let new = &mut remap[index as usize];
            if *new == u16::MAX {
                let v = b.base[index as usize];
                min = min.min(v.position);
                max = max.max(v.position);
                *new = vertices.len() as u16;
                source.push(index);
                vertices.push(WorldVertex {
                    position: v.position.to_array(), uv: v.uv.to_array(), color: v.color,
                    lightmap: v.normal.to_array(), normal: b.normals[index as usize].to_array(),
                });
            }
            indices.push(*new);
        }
    }
    Some((base_element as i32, count as i32, min, max, source))
}
/// The stream pair of `data`: the vertices are uploaded each frame, the indices once.
fn stream(ctx: &mut dyn RenderingBackend, resources: &mut ConstructionResources,
          data: (Vec<WorldVertex>, Vec<u16>)) -> Option<Stream> {
    if data.1.is_empty() { return None; }
    let vertex = ctx.new_buffer(BufferType::VertexBuffer, BufferUsage::Stream, BufferSource::empty::<WorldVertex>(data.0.len()));
    let index = ctx.new_buffer(BufferType::IndexBuffer, BufferUsage::Immutable, BufferSource::slice(&data.1));
    resources.buffers.extend([vertex, index]);
    Some(Stream { vertex, index, template: data.0 })
}

/// The uniform cache of the program a layer draws with. A layer is only given a lean program
/// that exists, so the fallback arms are not reached.
fn lean_cache<'a>(program: Program, lite: &'a mut Option<Lite>, world: &'a mut crate::render::uniform_fields::Cache)
    -> &'a mut crate::render::uniform_fields::Cache {
    match (program, lite.as_mut()) {
        (Program::Lite, Some(lite)) => &mut lite.cache,
        (Program::LiteCut, Some(Lite { cut: Some((_, cache)), .. })) => cache,
        _ => world,
    }
}

impl SkyRoom {
    /// `None` when the key is off or the map has no sky camera. Buffers, pipelines and programs
    /// are registered with `resources`, so a failed construction releases them. `shader` is the
    /// world's full program.
    pub(super) fn new(scene: &Scene, ctx: &mut dyn RenderingBackend, resources: &mut ConstructionResources,
                      shader: ShaderId) -> Option<Self> {
        let c = config();
        if !c.sky_live || !c.sky || c.authored_sky { return None; }
        // One set per cluster a sky camera can stand in, the load-time camera first.
        let mut cameras: Vec<(Vec3, Option<usize>)> = Vec::new();
        for origin in scene.sky_origin.into_iter().chain(scene.sky_candidates.iter().copied()) {
            let cluster = scene.map.visibility.cluster(origin, &scene.map.planes);
            if !cameras.iter().any(|c| c.1 == cluster) { cameras.push((origin, cluster)); }
        }
        // The sky faces that show a sky shader, not the room.
        let shaded = scene.batches.iter().filter(|b| b.sky && b.model.is_none() && b.animation.is_none()
            && b.motion.is_none() && !b.deforms.iter().any(Deform::moves_vertices)
            && !b.layers.iter().all(|l| l.spec.blend == Blend::Invisible)).collect::<Vec<_>>();
        if cameras.is_empty() && shaded.is_empty() { return None; }
        if cameras.len() > MAX_SETS {
            println!("[lg/sky-live] warning sky_cameras={} capped_at={MAX_SETS}", cameras.len());
            cameras.truncate(MAX_SETS);
        }
        // A lean program that does not build leaves its surfaces on the world program, with a warning.
        let mut lean = |ctx: &mut dyn RenderingBackend, name: &'static str, fragment: &str| {
            crate::frame_profile::shader_creation();
            let _name = crate::shader_cache::Name::new(name);
            let meta = ShaderMeta { images: vec!["Diffuse".into()], uniforms: shader::meta().uniforms };
            let built = ctx.new_shader(ShaderSource::Glsl { vertex: &shader::vertex(c.water_normals), fragment }, meta)
                .map_err(|e| eprintln!("[lg/sky-live] warning {name} program failed, its surfaces stay on the world program: {e:?}")).ok();
            if let Some(shader) = built { resources.shaders.push(shader); }
            built.map(|shader| (shader, crate::render::uniform_fields::Cache::new(shader::meta())))
        };
        let plain = if c.sky_live_lite { lean(ctx, "retained-sky-live-lite", LITE_FRAGMENT) } else { None };
        let lite = plain.map(|(shader, cache)| Lite { shader, cache,
            cut: lean(ctx, "retained-sky-live-lite-cut", LITE_CUT_FRAGMENT) });
        // Pipelines by (lite program, legacy blend index, cull, equal depth), built on first use.
        let attributes = [
            VertexAttribute::new("position", VertexFormat::Float3),
            VertexAttribute::new("texcoord", VertexFormat::Float2),
            VertexAttribute::new("color0", VertexFormat::Byte4),
            VertexAttribute::new("lightmap", VertexFormat::Float4),
            VertexAttribute::new("normal", VertexFormat::Float3),
        ];
        let programs = [Some(shader), lite.as_ref().map(|l| l.shader), lite.as_ref().and_then(|l| l.cut.as_ref().map(|c| c.0))];
        // One program per surface: the plain lean one, the tested lean one when a stage has an
        // alpha test, the world's when the lean one it needs is missing.
        let program_of = |b: &crate::render::Batch| {
            let tested = b.layers.iter().any(|l| l.spec.alpha_test != 0);
            match (tested, programs[1].is_some(), programs[2].is_some()) {
                (false, true, _) => Program::Lite,
                (true, _, true) => Program::LiteCut,
                _ => Program::World,
            }
        };
        let mut pipelines: BTreeMap<(Program, usize, usize, bool), Pipeline> = BTreeMap::new();
        let mut sets = Vec::new();
        let (mut vertices, mut triangles, mut animated, mut sprites, mut gpu_bytes) = (0, 0, 0, 0, 0);
        // Host experiment: take every static model and every batch that cannot be culled, as the
        // legacy pass does when it skips their PVS test, instead of only those the camera can see.
        #[cfg(not(target_os = "vita"))]
        let everything = std::env::var_os("LOOKING_GLASS_SKY_LIVE_ALL").is_some();
        #[cfg(target_os = "vita")]
        let everything = false;
        for (origin, cluster) in cameras {
            let mask = scene.map.visibility.faces(cluster);
            let mut packing = Packing::default();
            let mut moving = (Vec::new(), Vec::new());
            let mut set = Set { origin, cluster, buffers: Vec::new(), stream: None, clips: Vec::new(),
                opaque: Vec::new(), alpha: Vec::new(), layers: Vec::new(), logged: false };
            let room = scene.batches.iter().filter(|b| !b.sky && b.model.is_none()).map(|b| (b, false));
            for (b, prop) in room.chain(scene.sky_props.iter().map(|b| (b, true))) {
                let whole = b.deforms.iter().any(Deform::moves_vertices) || b.motion.is_some();
                let ranges = if prop || (everything && (whole || b.parts.is_empty())) { vec![0..b.full_indices.len()] } else {
                    let parts = b.parts.iter().map(|p| (p.face, p.indices.clone())).collect::<Vec<_>>();
                    selection(&parts, b.full_indices.len(), whole, mask.as_deref(),
                        b.parts.is_empty() && scene.map.visibility.bounds(cluster, b.bounds.0, b.bounds.1))
                };
                // A vertex-animated batch goes to the set's stream; its bounds cover every frame.
                let (slot, base_element, indices, min, max) = if let Some(animation) = &b.animation {
                    let first = moving.0.len();
                    let Some((base_element, indices, _, _, source)) = pack(&mut moving, b, &ranges) else { continue };
                    set.clips.push(Clip { first, source, frames: crate::ambient_animation::Frames {
                        positions: animation.positions.clone(), frame_time: animation.frame_time,
                        transform: animation.transform, scale: animation.scale } });
                    animated += 1;
                    (STREAM, base_element, indices, b.bounds.0, b.bounds.1)
                } else {
                    let Some(packed) = packing.append(b, &ranges) else { continue };
                    packed
                };
                sprites += usize::from(b.deforms.iter().any(|d| matches!(d, Deform::Sprite(_))));
                let deform = b.deforms.iter()
                    .find(|d| matches!(d, Deform::Wave(..) | Deform::Move(..) | Deform::Bulge(..))).cloned();
                let extent = deform_extent(&deform);
                let cull = match b.face_side { FaceSide::Both => 0, FaceSide::Front => 1, FaceSide::Back => 2 };
                let first = set.layers.len() as u32;
                let program = program_of(b);
                for (rank, layer) in b.layers.iter().enumerate() {
                    let blend = layer.material % crate::render::BLEND_COUNT;
                    let equal = layer.material >= crate::render::BLEND_COUNT;
                    let pipeline = *pipelines.entry((program, blend, cull, equal)).or_insert_with(|| {
                        crate::frame_profile::pipeline_creation();
                        let state = crate::render::blend(blend);
                        ctx.new_pipeline(&[BufferLayout { stride: 52, ..Default::default() }], &attributes,
                            programs[program as usize].unwrap(),
                            PipelineParams {
                                depth_test: if equal { Comparison::Equal } else { Comparison::LessOrEqual },
                                depth_write: true,
                                cull_face: [CullFace::Nothing, CullFace::Back, CullFace::Front][cull],
                                front_face_order: FrontFaceOrder::Clockwise,
                                color_blend: state, alpha_blend: state,
                                ..Default::default()
                            })
                    });
                    set.layers.push(Layer {
                        // `blend` only picks a fog mode in the uniforms, and the room has no fog.
                        recipe: Recipe { stage: layer.spec.clone(), textures: layer.textures.clone(), lit: false, rank,
                            blend: 0, opaque: !b.transparent(), cull, model: None, motion: b.motion.clone(),
                            deform: deform.clone(), wave_normal: None },
                        pipeline,
                        program,
                    });
                    triangles += indices as usize / 3;
                }
                let batch = Batch { slot, base_element, indices, min: min - extent, max: max + extent,
                    layers: first..set.layers.len() as u32 };
                if b.transparent() { set.alpha.push(batch); } else { set.opaque.push(batch); }
            }
            for (v, i) in &packing.buffers {
                let vertex = ctx.new_buffer(BufferType::VertexBuffer, BufferUsage::Immutable, BufferSource::slice(v));
                let index = ctx.new_buffer(BufferType::IndexBuffer, BufferUsage::Immutable, BufferSource::slice(i));
                resources.buffers.extend([vertex, index]);
                set.buffers.push((vertex, index));
                vertices += v.len();
                gpu_bytes += v.len() * std::mem::size_of::<WorldVertex>() + i.len() * 2;
            }
            vertices += moving.0.len();
            gpu_bytes += moving.0.len() * std::mem::size_of::<WorldVertex>() + moving.1.len() * 2;
            set.stream = stream(ctx, resources, moving);
            println!("[lg/sky-live] set={} origin={:?} cluster={:?} opaque_batches={} alpha_batches={} layers={} buffers={} animated_props={} stream_vertices={}",
                sets.len(), origin.to_array(), cluster, set.opaque.len(), set.alpha.len(), set.layers.len(), set.buffers.len(),
                set.clips.len(), set.stream.as_ref().map_or(0, |s| s.template.len()));
            sets.push(set);
        }
        // Sky shader faces: one batch per shader, every stage, in the legacy stage order.
        let mut merged: Vec<(String, Vec<&crate::render::Batch>)> = Vec::new();
        for b in shaded {
            let key = format!("{:?}", b.layers.iter().map(|l| (format!("{:?}", l.spec),
                l.textures.iter().map(Texture2D::raw_miniquad_id).collect::<Vec<_>>(), l.material)).collect::<Vec<_>>());
            match merged.iter_mut().find(|m| m.0 == key) {
                Some(m) => m.1.push(b),
                None => merged.push((key, vec![b])),
            }
        }
        let mut face_data = (Vec::new(), Vec::new());
        let (mut face_batches, mut face_layers, mut face_skipped) = (Vec::new(), Vec::new(), 0);
        for (_, group) in &merged {
            let start = face_data.1.len();
            let (mut min, mut max) = (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY));
            for b in group {
                match pack(&mut face_data, b, &[0..b.full_indices.len()]) {
                    Some((_, _, lo, hi, _)) => { min = min.min(lo); max = max.max(hi); }
                    None => face_skipped += 1,
                }
            }
            if face_data.1.len() == start { continue; }
            let first = face_layers.len() as u32;
            let program = program_of(group[0]);
            for (rank, layer) in group[0].layers.iter().enumerate() {
                let blend = layer.material % crate::render::BLEND_COUNT;
                let equal = layer.material >= crate::render::BLEND_COUNT;
                let pipeline = *pipelines.entry((program, blend, 0, equal)).or_insert_with(|| {
                    crate::frame_profile::pipeline_creation();
                    let state = crate::render::blend(blend);
                    ctx.new_pipeline(&[BufferLayout { stride: 52, ..Default::default() }], &attributes,
                        programs[program as usize].unwrap(),
                        PipelineParams {
                            depth_test: if equal { Comparison::Equal } else { Comparison::LessOrEqual },
                            depth_write: true, cull_face: CullFace::Nothing,
                            front_face_order: FrontFaceOrder::Clockwise,
                            color_blend: state, alpha_blend: state,
                            ..Default::default()
                        })
                });
                face_layers.push(Layer {
                    recipe: Recipe { stage: layer.spec.clone(), textures: layer.textures.clone(), lit: false, rank,
                        blend: 0, opaque: true, cull: 0, model: None, motion: None, deform: None, wave_normal: None },
                    pipeline, program,
                });
                triangles += (face_data.1.len() - start) / 3;
            }
            face_batches.push(Batch { slot: STREAM, base_element: start as i32, indices: (face_data.1.len() - start) as i32,
                min, max, layers: first..face_layers.len() as u32 });
        }
        vertices += face_data.0.len();
        gpu_bytes += face_data.0.len() * std::mem::size_of::<WorldVertex>() + face_data.1.len() * 2;
        let face_vertices = face_data.0.len();
        let faces = stream(ctx, resources, face_data).map(|stream| SkyLayers { stream, batches: face_batches, layers: face_layers });
        if let Some(faces) = &faces {
            println!("[lg/sky-live] faces shaders={} layers={} vertices={face_vertices} skipped_batches={face_skipped}",
                faces.batches.len(), faces.layers.len());
        }
        let pipelines = pipelines.into_values().collect::<Vec<_>>();
        resources.pipelines.extend(pipelines.iter().copied());
        // A copy program that does not build leaves the room at full resolution, with a warning.
        let copy = (c.sky_live_scale < 1.).then(|| {
            crate::frame_profile::shader_creation();
            let _name = crate::shader_cache::Name::new("retained-sky-live-copy");
            ctx.new_shader(ShaderSource::Glsl { vertex: COPY_VERTEX, fragment: COPY_FRAGMENT },
                ShaderMeta { images: vec!["Sky".into()], uniforms: UniformBlockLayout {
                    uniforms: vec![UniformDesc::new("Flip", UniformType::Float4)] } })
                .map_err(|e| eprintln!("[lg/sky-live] warning copy program failed, sky_live_scale ignored: {e:?}")).ok()
        }).flatten().map(|shader| {
            resources.shaders.push(shader);
            crate::frame_profile::pipeline_creation();
            // The gradient's state: no depth test or write.
            let pipeline = ctx.new_pipeline(&[BufferLayout::default()],
                &[VertexAttribute::new("position", VertexFormat::Float2)], shader, PipelineParams::default());
            resources.pipelines.push(pipeline);
            CopyPass { shader, pipeline, target: None }
        });
        println!("[lg/sky-live] map_props={} sets={} vertices={vertices} layer_triangles={triangles} gpu_bytes={gpu_bytes} pipelines={} animated_props={animated} frozen_sprites={sprites} face_shaders={} alpha={} scale={} copy_program={} lite_program={}",
            scene.sky_props.len(), sets.len(), pipelines.len(), faces.as_ref().map_or(0, |f| f.batches.len()),
            c.sky_live_alpha, c.sky_live_scale, copy.is_some(), lite.is_some());
        Some(Self { sets, faces, pipelines, copy, lite, atlas: scene.atlas.raw_miniquad_id(), current: None })
    }
    pub(super) fn delete(&mut self, ctx: &mut dyn RenderingBackend) {
        for set in &self.sets {
            for &(vertex, index) in &set.buffers { ctx.delete_buffer(vertex); ctx.delete_buffer(index); }
            if let Some(stream) = &set.stream { ctx.delete_buffer(stream.vertex); ctx.delete_buffer(stream.index); }
        }
        if let Some(faces) = &self.faces { ctx.delete_buffer(faces.stream.vertex); ctx.delete_buffer(faces.stream.index); }
        for &pipeline in &self.pipelines { ctx.delete_pipeline(pipeline); }
        if let Some(lite) = self.lite.take() {
            ctx.delete_shader(lite.shader);
            if let Some((cut, _)) = lite.cut { ctx.delete_shader(cut); }
        }
        if let Some(copy) = self.copy.take() {
            ctx.delete_pipeline(copy.pipeline);
            ctx.delete_shader(copy.shader);
            #[cfg(not(target_os = "vita"))]
            if let Some(target) = &copy.target { ctx.delete_texture(target.depth); }
        }
    }
    /// True when the map has sky faces that carry a sky shader.
    pub(super) fn has_faces(&self) -> bool { self.faces.is_some() }
    /// The set whose sky camera is nearest to `origin`.
    fn nearest(&self, origin: Vec3) -> Option<usize> {
        (0..self.sets.len()).min_by(|&a, &b| self.sets[a].origin.distance_squared(origin)
            .total_cmp(&self.sets[b].origin.distance_squared(origin)))
    }
    /// The room's own target at `scale` of the 3D target, built on first use and again when the
    /// 3D target changes size. RGBA8 with the same depth arrangement as the 3D target.
    fn target(&mut self, ctx: &mut dyn RenderingBackend, scale: f32) -> Option<(macroquad::miniquad::RenderPass, TextureId)> {
        let copy = self.copy.as_mut()?;
        let [w, h] = super::diagnostic_target().unwrap_or([screen_width() as u32, screen_height() as u32]);
        let size = [((w as f32 * scale).round() as u32).max(16), ((h as f32 * scale).round() as u32).max(16)];
        if copy.target.as_ref().is_none_or(|t| t.size != size) {
            #[cfg(not(target_os = "vita"))]
            if let Some(old) = copy.target.take() { ctx.delete_texture(old.depth); }
            let rt = render_target_ex(size[0], size[1], RenderTargetParams { sample_count: 0, depth: false });
            rt.texture.set_filter(macroquad::texture::FilterMode::Linear);
            #[cfg(not(target_os = "vita"))]
            let depth = crate::render::sky_cube::host_depth(&rt, size[0], size[1]);
            println!("[lg/sky-live] target {}x{} RGBA8 of 3d {w}x{h} scale={scale}", size[0], size[1]);
            copy.target = Some(Target { rt,
                #[cfg(not(target_os = "vita"))]
                depth,
                size });
        }
        let _ = &ctx;
        copy.target.as_ref().map(|t| (t.rt.render_pass.raw_miniquad_id(), t.rt.texture.raw_miniquad_id()))
    }
}

impl Renderer {
    /// Draw the sky room into the bound 3D pass, then clear its depth. The caller has begun the
    /// pass and set the world viewport, and draws the sky faces into depth afterwards.
    pub(super) fn draw_sky_room(&mut self, ctx: &mut dyn RenderingBackend, scene: &Scene, eye: Vec3, time: f32,
                                projection: Mat4) -> Counts {
        let Some(origin) = scene.sky_origin else { return Counts::default() };
        if self.sky_room.as_ref().is_none_or(|room| room.sets.is_empty()) { return Counts::default(); }
        let Some(mut room) = self.sky_room.take() else { return Counts::default() };
        let c = config();
        let mut counts = Counts::default();
        // `sky_live_scale`: the room goes to its own target, cleared to the background colour.
        let own = room.target(ctx, c.sky_live_scale);
        if let Some((pass, _)) = own {
            let bg = scene.atmosphere.background();
            unsafe { macroquad::miniquad::gl::glDepthMask(1); }
            ctx.begin_pass(Some(pass), PassAction::Clear { color: Some((bg.r, bg.g, bg.b, 1.)), depth: Some(1.), stencil: None });
        }
        let motion = scene.sky_motion.as_ref().map_or(Quat::IDENTITY, |m| m.transform(time).1);
        let pose = pose(eye, origin, scene.sky_rotation, motion);
        let current = room.nearest(origin);
        if current != room.current {
            room.current = current;
            if let Some(index) = current {
                println!("[lg/sky-live] camera set={index} origin={:?} distance={:.1}", origin.to_array(),
                    room.sets[index].origin.distance(origin));
            }
        }
        let atlas = room.atlas;
        if let Some(lite) = &mut room.lite {
            lite.cache.interrupt();
            if let Some((_, cache)) = &mut lite.cut { cache.interrupt(); }
        }
        self.field_uniforms[0].interrupt();
        if let Some(index) = current {
            let first_set = !room.sets.iter().any(|s| s.logged);
            let SkyRoom { sets, lite, .. } = &mut room;
            let set = &mut sets[index];
            // Vertex-animated props: this frame's positions, as `draw_batch` reads them.
            if let Some(stream) = &mut set.stream {
                for clip in &set.clips {
                    for (k, &source) in clip.source.iter().enumerate() {
                        stream.template[clip.first + k].position = clip.frames.position(source as usize, time).to_array();
                    }
                }
                ctx.buffer_update(stream.vertex, BufferSource::slice(&stream.template));
                counts.upload_bytes += (stream.template.len() * std::mem::size_of::<WorldVertex>()) as u64;
            }
            let mut previous_pipeline: Option<Pipeline> = None;
            let mut previous_binding: Option<(BufferId, BufferId, TextureId)> = None;
            for (alpha, batches) in [(false, &set.opaque), (true, &set.alpha)] {
                if alpha && !c.sky_live_alpha { break; }
                unsafe { macroquad::miniquad::gl::glDepthMask(if alpha { 0 } else { 1 }); }
                for b in batches {
                    counts.tested += 1;
                    let layers = &set.layers[b.layers.start as usize..b.layers.end as usize];
                    let Some(first) = layers.first() else { continue };
                    let model = pose * first.recipe.motion.as_ref().map_or(Mat4::IDENTITY, |m| m.matrix(time));
                    if !in_frustum(projection * model, b.min, b.max) { continue; }
                    let (vertex, index) = match (b.slot, &set.stream) {
                        (STREAM, Some(stream)) => (stream.vertex, stream.index),
                        (STREAM, None) => continue,
                        (slot, _) => set.buffers[slot as usize],
                    };
                    for layer in layers {
                        let u = uniforms(&layer.recipe, time, eye, model, projection, true, scene.atlas_grid);
                        let texture = layer.recipe.textures[layer.recipe.stage.frame(time)].raw_miniquad_id();
                        #[cfg(all(feature = "sim_maps_probe", not(target_os = "vita")))]
                        crate::sim_maps::probe::draw_query::mark("sky_room",
                            match (layer.program, alpha) {
                                (Program::Lite, false) => "sky_room_lite", (Program::Lite, true) => "sky_room_lite_alpha",
                                (Program::LiteCut, false) => "sky_room_cut", (Program::LiteCut, true) => "sky_room_cut_alpha",
                                (Program::World, false) => "sky_room_full", (Program::World, true) => "sky_room_full_alpha" },
                            0, None, layer.recipe.stage.alpha_test,
                            layer.recipe.stage.images.get(layer.recipe.stage.frame(time)).map(String::as_str));
                        if previous_pipeline != Some(layer.pipeline) {
                            ctx.apply_pipeline(&layer.pipeline);
                            previous_pipeline = Some(layer.pipeline);
                            previous_binding = None;
                        }
                        let binding = (vertex, index, texture);
                        if previous_binding != Some(binding) {
                            // The lean programs sample the stage texture only.
                            if layer.program == Program::World { ctx.apply_bindings_from_slice(&[vertex], index, &[texture, atlas]); }
                            else { ctx.apply_bindings_from_slice(&[vertex], index, &[texture]); }
                            previous_binding = Some(binding);
                        }
                        lean_cache(layer.program, lite, &mut self.field_uniforms[0]).apply(ctx, u.values(), true);
                        ctx.draw(b.base_element, b.indices, 1);
                        counts.lite += u64::from(layer.program != Program::World);
                        counts.triangles += b.indices as u64 / 3;
                        if alpha { counts.alpha += 1; } else { counts.opaque += 1; }
                    }
                }
            }
            if !set.logged {
                set.logged = true;
                println!("[lg/sky-live] {} set={index} cluster={:?} batches={} tested={} opaque_draws={} alpha_draws={} lite_draws={} triangles={}",
                    if first_set { "first_frame" } else { "later_set" }, set.cluster, set.opaque.len() + set.alpha.len(),
                    counts.tested, counts.opaque, counts.alpha, counts.lite, counts.triangles);
            }
        }
        // The world pass starts as on RC17: both caches unknown, depth writes on.
        for cache in &mut self.field_uniforms { cache.interrupt(); }
        unsafe { macroquad::miniquad::gl::glDepthMask(1); }
        if let Some((_, texture)) = own {
            // Back to the 3D target: one opaque full-screen triangle, no depth test or write. The
            // Vita stores a target top row first, as `scale.rs` and the sky cube read it.
            ctx.end_render_pass();
            begin_3d_pass(PassAction::Nothing);
            apply_world_viewport();
            let flip = cfg!(target_os = "vita") != (c.scaled_target == "direct_flip");
            ctx.apply_pipeline(&room.copy.as_ref().unwrap().pipeline);
            ctx.apply_bindings_from_slice(&self.sky_bindings.vertex_buffers, self.sky_bindings.index_buffer, &[texture]);
            ctx.apply_uniforms(UniformsSource::table(&[f32::from(flip), 0., 0., 0.]));
            #[cfg(all(feature = "sim_maps_probe", not(target_os = "vita")))]
            crate::sim_maps::probe::draw_query::mark("sky_room", "sky_target_copy", 0, None, 0, None);
            ctx.draw(0, 3, 1);
            counts.copies = 1;
        } else {
            // The room keeps no depth: the world and the sky faces start from a far plane, as
            // after the legacy path's clear. The clear leaves the pass bound; the viewport is set again.
            begin_3d_pass(PassAction::Clear { color: None, depth: Some(1.), stencil: None });
            apply_world_viewport();
        }
        self.sky_room = Some(room);
        counts
    }
    /// Draw the sky faces that carry a sky shader, with depth test and write, where the legacy
    /// path draws them: after the room and its depth clear, before the world.
    pub(super) fn draw_sky_faces(&mut self, ctx: &mut dyn RenderingBackend, scene: &Scene, eye: Vec3, time: f32,
                                 projection: Mat4, counts: &mut Counts) {
        let Some(mut room) = self.sky_room.take() else { return };
        let atlas = room.atlas;
        let SkyRoom { faces, lite, .. } = &mut room;
        if let Some(faces) = faces {
            for v in &mut faces.stream.template {
                v.uv = cloud_uv(Vec3::from_array(v.position), eye).to_array();
            }
            ctx.buffer_update(faces.stream.vertex, BufferSource::slice(&faces.stream.template));
            counts.upload_bytes += (faces.stream.template.len() * std::mem::size_of::<WorldVertex>()) as u64;
            if let Some(lite) = lite.as_mut() {
                lite.cache.interrupt();
                if let Some((_, cache)) = &mut lite.cut { cache.interrupt(); }
            }
            self.field_uniforms[0].interrupt();
            unsafe { macroquad::miniquad::gl::glDepthMask(1); }
            let mut previous_pipeline: Option<Pipeline> = None;
            let mut previous_texture: Option<TextureId> = None;
            for b in &faces.batches {
                if !in_frustum(projection, b.min, b.max) { continue; }
                for layer in &faces.layers[b.layers.start as usize..b.layers.end as usize] {
                    let u = uniforms(&layer.recipe, time, eye, Mat4::IDENTITY, projection, true, scene.atlas_grid);
                    let texture = layer.recipe.textures[layer.recipe.stage.frame(time)].raw_miniquad_id();
                    #[cfg(all(feature = "sim_maps_probe", not(target_os = "vita")))]
                    crate::sim_maps::probe::draw_query::mark("sky_room", if layer.program == Program::World { "sky_faces_full" } else { "sky_faces_lite" },
                        0, None, layer.recipe.stage.alpha_test,
                        layer.recipe.stage.images.get(layer.recipe.stage.frame(time)).map(String::as_str));
                    if previous_pipeline != Some(layer.pipeline) {
                        ctx.apply_pipeline(&layer.pipeline);
                        previous_pipeline = Some(layer.pipeline);
                        previous_texture = None;
                    }
                    if previous_texture != Some(texture) {
                        if layer.program == Program::World { ctx.apply_bindings_from_slice(&[faces.stream.vertex], faces.stream.index, &[texture, atlas]); }
                        else { ctx.apply_bindings_from_slice(&[faces.stream.vertex], faces.stream.index, &[texture]); }
                        previous_texture = Some(texture);
                    }
                    lean_cache(layer.program, lite, &mut self.field_uniforms[0]).apply(ctx, u.values(), true);
                    ctx.draw(b.base_element, b.indices, 1);
                    counts.face_layers += 1;
                    counts.lite += u64::from(layer.program != Program::World);
                    counts.triangles += b.indices as u64 / 3;
                }
            }
            for cache in &mut self.field_uniforms { cache.interrupt(); }
        }
        self.sky_room = Some(room);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    /// The matrix places a sky room point exactly where the legacy per-vertex pose does.
    #[test]
    fn sky_live_pose_matches_the_legacy_portal_pose() {
        let origin = vec3(120., -40., 32.);
        let rotation = Quat::from_rotation_z(0.7);
        let motion = Quat::from_rotation_x(-0.3);
        for eye in [Vec3::ZERO, vec3(800., -900., 40.)] {
            let m = pose(eye, origin, rotation, motion);
            let (offset, (_, translation, r)) = crate::render::authored_sky::portal_pose(eye, origin, rotation, motion);
            for point in [origin, origin + vec3(4., 7., 11.), vec3(-300., 12., 900.)] {
                let legacy = translation + r * point + offset;
                assert!(m.transform_point3(point).distance(legacy) < 0.001, "SKY_LIVE_POSE {point}");
            }
            assert!(m.transform_point3(origin).distance(eye) < 0.001, "the sky camera sits on the eye");
        }
    }
    /// The index ranges follow the legacy sky room rules: world faces by the cluster's face mask,
    /// models by their bounds, and a batch that cannot be culled whole or not at all.
    #[test]
    fn sky_live_selection_follows_the_legacy_room_rules() {
        let parts = vec![(0usize, 0..6usize), (1, 6..9), (2, 9..15)];
        let mask = [true, false, true];
        assert_eq!(selection(&parts, 15, false, Some(&mask), false), vec![0..6, 9..15], "faces the cluster sees");
        assert_eq!(selection(&parts, 15, false, None, false), vec![0..6, 6..9, 9..15], "no PVS sees every face");
        assert!(selection(&parts, 15, false, Some(&[false; 3]), true).is_empty(), "hidden faces draw nothing");
        assert_eq!(selection(&parts, 15, true, Some(&mask), false), vec![0..15], "a deformed batch is drawn whole");
        assert!(selection(&parts, 15, true, Some(&[false; 3]), false).is_empty(), "a deformed batch out of sight is dropped");
        assert_eq!(selection(&[], 12, false, Some(&mask), true), vec![0..12], "a model in sight");
        assert!(selection(&[], 12, false, Some(&mask), false).is_empty(), "a model out of sight");
        assert!(selection(&[], 0, false, None, true).is_empty(), "an empty model");
        assert_eq!(selection(&parts, 15, false, Some(&[true]), false), vec![0..6, 6..9, 9..15], "a short mask fails open");
    }
    /// The sky shader projection is the legacy one: it depends on the view direction only.
    #[test]
    fn sky_live_cloud_projection_follows_the_ray_not_the_position() {
        let eye = vec3(100., -50., 20.);
        for direction in [vec3(0., 0., 1.), vec3(1., 2., 3.), vec3(-4., 1., 0.5), vec3(0.3, -0.2, -1.)] {
            let near = cloud_uv(eye + direction * 10., eye);
            let far = cloud_uv(eye + direction * 5000., eye);
            assert!(near.distance(far) < 1e-4, "{direction}");
            let ray = direction.normalize();
            let expected = ray.truncate() / (ray.z.abs() + 0.25) * 0.25 + Vec2::splat(0.5);
            assert!(near.distance(expected) < 1e-6, "{direction}");
        }
        assert_eq!(cloud_uv(eye, eye), Vec2::splat(0.5), "a vertex on the eye has no ray");
    }
    /// The lite program is the world program with the lines that do nothing in a fullbright,
    /// unfogged room taken out: no discard, no light atlas, no fog.
    #[test]
    fn sky_live_lite_fragment_keeps_only_texture_gain_and_clamp() {
        for gone in ["discard", "LightAtlas", "fogged", "Alpha", "lm"] {
            assert!(!LITE_FRAGMENT.contains(gone), "{gone}");
        }
        for kept in ["texture2D(Diffuse,coord)", "vec3 rgb=diffuse.rgb*Gain.rgb;", "float a=diffuse.a*Gain.a*color.a;", "Surface.x"] {
            assert!(LITE_FRAGMENT.contains(kept), "{kept}");
        }
        // The same clamp and the same products as the world program.
        assert!(shader::FRAGMENT.contains("vec2 coord=mix(uv,clamp(uv,vec2(0.001),vec2(0.999)),Surface.x);"));
        assert!(shader::FRAGMENT.contains("float a=diffuse.a*Gain.a*color.a;"));
        // The tested variant is the plain one plus the world program's alpha test, line for line.
        let test = shader::FRAGMENT.split(" float cut=Surface.z;").nth(1).unwrap().split(" vec3 lighting=").next().unwrap();
        assert!(test.matches("discard").count() == 2 && LITE_CUT_FRAGMENT.contains(test), "the world alpha test, unchanged");
        assert_eq!(LITE_CUT_FRAGMENT.replace(&format!(" float cut=Surface.z;{test}"), "").replace(", Alpha;", ";"), LITE_FRAGMENT);
    }
}
