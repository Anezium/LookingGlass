//! Conservative whole-surface rigid deformation. Numeric metadata may cross to
//! workers; programs, buffers and descriptors belong exclusively to the game thread.
use super::*;
use crate::character::skinning::Params;
use crate::skeletal::{SkinSurface, SkinVertex};
use macroquad::miniquad::*;
use std::sync::{Arc, Weak as ArcWeak};

const STATIC_LIMIT: usize = 128;
const DRAW_LIMIT: usize = 1024;
const BYTE_LIMIT: usize = 2 * 1024 * 1024;
const EXTRA_FLOATS: usize = 28;
fn descriptor_available(draws: &[Draw]) -> bool {
    draws.len() < DRAW_LIMIT
}
fn static_available(count: usize, used: usize, next: usize) -> bool {
    count < STATIC_LIMIT && used.checked_add(next).is_some_and(|n| n <= BYTE_LIMIT)
}

pub(crate) struct Rigid {
    bone: usize,
    vertices: Vec<Vertex>,
    radius: f32,
    // Maximum area-sum sensitivity to a perturbation of each position.
    sensitivity: f32,
}
impl Rigid {
    pub(crate) fn new(input: &[SkinVertex], indices: &[u16]) -> Option<Self> {
        let bone = input.first()?.weights.first()?.bone;
        if input.len() > 65536
            || indices.is_empty()
            || indices.len() % 3 != 0
            || indices.iter().any(|&i| i as usize >= input.len())
            || !input.iter().all(|v| {
                v.weights.len() == 1
                    && v.weights[0].bone == bone
                    && v.weights[0].amount == 1.
                    && v.weights[0].offset.is_finite()
                    && v.uv.is_finite()
            })
        {
            return None;
        }
        let mut vertices: Vec<_> = input
            .iter()
            .map(|v| Vertex {
                position: v.weights[0].offset,
                uv: v.uv,
                normal: Vec4::ZERO,
                color: [255; 4],
            })
            .collect();
        let mut errors = vec![0.; input.len()];
        for t in indices.chunks_exact(3) {
            let [a, b, c] = [t[0] as usize, t[1] as usize, t[2] as usize];
            let e = vertices[b].position - vertices[a].position;
            let f = vertices[c].position - vertices[a].position;
            let n = e.cross(f);
            for i in [a, b, c] {
                vertices[i].normal += n.extend(0.);
                errors[i] += 2. * (e.length() + f.length());
            }
        }
        let mut sensitivity = 0_f32;
        for (v, error) in vertices.iter().zip(errors) {
            let len = v.normal.truncate().length();
            // Unreferenced/zero normals and cancellations retain exact CPU handling.
            if !len.is_finite() || len < 1e-6 {
                return None;
            }
            sensitivity = sensitivity.max(error / len);
        }
        let radius = vertices
            .iter()
            .map(|v| v.position.length())
            .fold(0_f32, f32::max);
        Some(Self {
            bone,
            vertices,
            radius,
            sensitivity,
        })
    }
}
#[derive(Clone, Copy)]
struct Deformation {
    linear: Mat3,
    origin: Vec3,
    cofactor: Mat3,
    args: Vec4,
}
fn deformation(r: &Rigid, p: Params<'_>) -> Option<Deformation> {
    let bone = p.pose.get(r.bone)?;
    let scale = p.scales.get(r.bone).copied().unwrap_or(1.);
    let (local, translation) = if let Some(&m) = p.palette.get(r.bone) {
        m
    } else {
        (
            crate::character::skinning::rotation(bone.rotation) * scale,
            bone.translation,
        )
    };
    let world = crate::character::skinning::rotation(p.transform.rotation);
    let linear = world * (local * p.scale);
    let origin = world * (translation * p.scale) + p.transform.translation;
    let cofactor = Mat3::from_cols(
        linear.y_axis.cross(linear.z_axis),
        linear.z_axis.cross(linear.x_axis),
        linear.x_axis.cross(linear.y_axis),
    );
    if !linear.is_finite() || !origin.is_finite() || !cofactor.is_finite() || !p.alpha.is_finite() {
        return None;
    }
    let norm = linear.x_axis.length() + linear.y_axis.length() + linear.z_axis.length();
    let inverse = linear.inverse();
    if !inverse.is_finite() {
        return None;
    }
    let inv_norm = inverse.x_axis.length() + inverse.y_axis.length() + inverse.z_axis.length();
    let condition = norm * inv_norm;
    // Bound cancellation from world-coordinate subtraction in the CPU area sum.
    // Reject singular/extreme affine maps and poorly conditioned geometry before
    // omitting CPU output. Cofactors preserve the sign of mirrored triangle areas.
    let coordinate = origin.abs().max_element() + norm * r.radius;
    // Translation rounding enters the CPU triangle subtraction once per endpoint;
    // arithmetic before world translation depends on bone/local coordinates.
    let local_coordinate = norm * r.radius
        + translation.length()
            * p.scale.abs()
            * world
                .x_axis
                .length()
                .max(world.y_axis.length())
                .max(world.z_axis.length());
    let error = f32::EPSILON * coordinate.max(1.) + 8. * f32::EPSILON * local_coordinate;
    let cofactor_inverse = cofactor.inverse();
    let affine_norm = (linear.x_axis.length_squared()
        + linear.y_axis.length_squared()
        + linear.z_axis.length_squared())
    .sqrt();
    let cofactor_inverse_norm = (cofactor_inverse.x_axis.length_squared()
        + cofactor_inverse.y_axis.length_squared()
        + cofactor_inverse.z_axis.length_squared())
    .sqrt();
    if condition > 12.
        || error > 0.001
        || (error + 8. * f32::EPSILON * norm * r.radius)
            * r.sensitivity
            * affine_norm
            * cofactor_inverse_norm
            > 0.004
    {
        return None;
    }
    let alpha = (p.alpha.clamp(0., 1.) * 255.) as u8;
    Some(Deformation {
        linear,
        origin,
        cofactor,
        args: vec4(f32::from(p.fullbright), f32::from(p.flat), alpha as f32, 0.),
    })
}
fn material(mesh: &Mesh) -> Option<Rc<Surface>> {
    let id = mesh.texture.as_ref()?.raw_miniquad_id();
    SURFACE_LOOKUP.with(|s| s.borrow().get(&id).and_then(Weak::upgrade))
}
fn simple(surface: &Surface) -> bool {
    surface.layers.iter().all(|l| {
        let s = &l.stage;
        !s.environment && s.vector_uv.is_none() && s.dot_alpha.is_none() && s.mods.is_empty()
    })
}
pub(crate) fn eligible(
    cpu: &SkinSurface,
    mesh: &Mesh,
    p: Params<'_>,
    preserve_allowed: bool,
) -> bool {
    if !crate::render::vita_renderer::config().gpu_skinning
        || !preserve_allowed
        || !crate::render::vita_renderer::config().actor_fast_path
    {
        return false;
    }
    let Some(r) = &cpu.rigid else {
        return false;
    };
    if mesh.vertices.len() != r.vertices.len()
        || mesh.indices != cpu.indices
        || !mesh
            .vertices
            .iter()
            .zip(&r.vertices)
            .all(|(a, b)| a.uv == b.uv)
        || deformation(r, p).is_none()
    {
        return false;
    }
    if !material(mesh).is_some_and(|s| simple(&s)) {
        return false;
    }
    STATE.with(|s| s.borrow().is_some())
}
struct Static {
    owner: ArcWeak<SkinSurface>,
    vertex: BufferId,
    index: BufferId,
    bindings: Bindings,
    bytes: usize,
}
struct Draw {
    // The descriptor owns its immutable data through final ordered submission.
    owner: Arc<SkinSurface>,
    slot: usize,
    deformation: Deformation,
    center: Vec3,
}
struct State {
    _game_thread: crate::game_thread::GameThread,
    shader: ShaderId,
    pipelines: Vec<Pipeline>,
    static_data: Vec<Static>,
    draws: Vec<Draw>,
}
thread_local! {
    static STATE: RefCell<Option<State>> = const {RefCell::new(None)};
    static ATTEMPTED:std::cell::Cell<bool> = const {std::cell::Cell::new(false)};
    static OWNERS:std::cell::Cell<usize> = const {std::cell::Cell::new(0)};
}
pub(super) fn retain_renderer() {
    if crate::render::vita_renderer::config().gpu_skinning {
        OWNERS.with(|c| c.set(c.get() + 1));
    }
}
pub(super) fn release_renderer() {
    if crate::render::vita_renderer::config().gpu_skinning {
        OWNERS.with(|c| {
            let n = c.get().checked_sub(1).unwrap();
            c.set(n);
            if n == 0 {
                shutdown();
            }
        });
    }
}

pub(super) fn init() {
    if !crate::render::vita_renderer::config().gpu_skinning {
        return;
    }
    crate::game_thread::assert_current();
    if ATTEMPTED.with(|s| s.replace(true)) {
        return;
    }
    let fragment =
        crate::lighting::fragment(&crate::environment::fragment(crate::render::FRAGMENT));
    let mut meta = super::retained::meta();
    for name in [
        "SkinRow0",
        "SkinRow1",
        "SkinRow2",
        "NormalRow0",
        "NormalRow1",
        "NormalRow2",
        "SkinArgs",
    ] {
        meta.uniforms
            .uniforms
            .push(UniformDesc::new(name, UniformType::Float4));
    }
    let ctx = unsafe { get_internal_gl() }.quad_context;
    let _name = crate::shader_cache::Name::new("rigid-actor-skin");
    #[cfg(target_os = "vita")]
    let semantics = unsafe {
        extern "C" {
            static glsl_sema_mode: i32;
            fn vglSetSemanticBindingMode(mode: u32);
        }
        let previous = glsl_sema_mode;
        vglSetSemanticBindingMode(0);
        if !compile_pair(VERTEX, &fragment) {
            vglSetSemanticBindingMode(previous as u32);
            eprintln!("[lg/gpu-skin] Vita shader compile rejected; CPU fallback");
            return;
        }
        previous
    };
    // Pinned miniquad asserts on a rejected compiler with an empty info log.
    // Pair mode checks each binary before any link, avoiding deferred GXM failure.
    #[cfg(target_os = "vita")]
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        ctx.new_shader(
            ShaderSource::Glsl {
                vertex: VERTEX,
                fragment: &fragment,
            },
            meta,
        )
    }));
    #[cfg(target_os = "vita")]
    unsafe {
        extern "C" {
            fn vglSetSemanticBindingMode(mode: u32);
        }
        vglSetSemanticBindingMode(semantics as u32);
    }
    #[cfg(target_os = "vita")]
    let result = match result {
        Ok(r) => r,
        Err(_) => {
            eprintln!("[lg/gpu-skin] compiler rejected without log; CPU fallback");
            return;
        }
    };
    #[cfg(not(target_os = "vita"))]
    let result = ctx.new_shader(
        ShaderSource::Glsl {
            vertex: VERTEX,
            fragment: &fragment,
        },
        meta,
    );
    let shader = match result {
        Ok(s) => s,
        Err(e) => {
            eprintln!("[lg/gpu-skin] rejected shader, CPU fallback: {e:?}");
            return;
        }
    };
    let mut pipelines = Vec::with_capacity(crate::render::BLEND_COUNT * 2);
    for blend in 0..crate::render::BLEND_COUNT {
        for equal in [false, true] {
            pipelines.push(ctx.new_pipeline(
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
                    depth_test: if equal {
                        Comparison::Equal
                    } else {
                        Comparison::LessOrEqual
                    },
                    ..crate::render::depth_pipeline(crate::render::blend(blend))
                },
            ));
        }
    }
    STATE.with(|s| {
        *s.borrow_mut() = Some(State {
            _game_thread: crate::game_thread::GameThread::new(),
            shader,
            pipelines,
            static_data: Vec::with_capacity(STATIC_LIMIT),
            draws: Vec::with_capacity(DRAW_LIMIT),
        })
    });
    println!("[lg/gpu-skin] READY rigid whole surfaces; 4 attributes, 15 vertex vec4; default off");
}
/// Called between completed views, never while descriptors are in flight.
pub(super) fn shutdown() {
    ATTEMPTED.with(|c| c.set(false));
    STATE.with(|s| {
        s.borrow_mut().take();
    });
}
pub(super) fn begin_view() {
    #[cfg(not(target_os = "vita"))]
    SURFACE_COUNTS.with(|s| {
        for c in &mut *s.borrow_mut() {
            c.2 = [0; 2];
        }
    });
    if !crate::render::vita_renderer::config().gpu_skinning {
        return;
    }
    init();
    STATE.with(|s| {
        if let Some(s) = s.borrow_mut().as_mut() {
            s.draws.clear();
            s.static_data.retain(|b| {
                if b.owner.strong_count() > 0 {
                    return true;
                }
                let ctx = unsafe { get_internal_gl() }.quad_context;
                ctx.delete_buffer(b.vertex);
                ctx.delete_buffer(b.index);
                false
            });
        }
    });
}
pub(crate) fn submit(cpu: Arc<SkinSurface>, mesh: &Mesh, p: Params<'_>) -> bool {
    crate::game_thread::assert_current();
    if !retained_active() {
        return false;
    }
    let Some(surface) = material(mesh).filter(|s| simple(s)) else {
        return false;
    };
    let Some(r) = &cpu.rigid else {
        return false;
    };
    let Some(d) = deformation(r, p) else {
        return false;
    };
    #[cfg(not(target_os = "vita"))]
    let allocations = crate::kernel_alloc::Scope::begin();
    let descriptor = STATE.with(|s| {
        let mut state = s.borrow_mut();
        let s = state.as_mut()?;
        if !descriptor_available(&s.draws) {
            super::retained::count("gpu_skin_descriptor_fallbacks", 1);
            return None;
        }
        let slot = if let Some(i) = s
            .static_data
            .iter()
            .position(|b| b.owner.ptr_eq(&Arc::downgrade(&cpu)))
        {
            i
        } else {
            let bytes = r.vertices.len() * 40 + cpu.indices.len() * 2;
            if !static_available(
                s.static_data.len(),
                s.static_data.iter().map(|b| b.bytes).sum(),
                bytes,
            ) {
                super::retained::count("gpu_skin_static_fallbacks", 1);
                return None;
            }
            let ctx = unsafe { get_internal_gl() }.quad_context;
            let vertex = ctx.new_buffer(
                BufferType::VertexBuffer,
                BufferUsage::Immutable,
                BufferSource::slice(&r.vertices),
            );
            let index = ctx.new_buffer(
                BufferType::IndexBuffer,
                BufferUsage::Immutable,
                BufferSource::slice(&cpu.indices),
            );
            let texture = mesh.texture.as_ref().unwrap().raw_miniquad_id();
            s.static_data.push(Static {
                owner: Arc::downgrade(&cpu),
                vertex,
                index,
                bytes,
                bindings: Bindings {
                    vertex_buffers: vec![vertex],
                    index_buffer: index,
                    images: vec![texture; 3],
                },
            });
            super::retained::count("gpu_skin_static_uploads", 1);
            super::retained::count("gpu_skin_static_upload_bytes", bytes as u64);
            s.static_data.len() - 1
        };
        let (lo, hi) = r.vertices.iter().fold(
            (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY)),
            |(lo, hi), v| {
                let bone = &p.pose[r.bone];
                let local = if let Some(&(rotation, translation)) = p.palette.get(r.bone) {
                    rotation * v.position + translation
                } else {
                    bone.point(v.position * p.scales.get(r.bone).copied().unwrap_or(1.))
                };
                let pos = if p.palette.is_empty() {
                    p.transform.point(local * p.scale)
                } else {
                    crate::character::skinning::rotation(p.transform.rotation) * (local * p.scale)
                        + p.transform.translation
                };
                (lo.min(pos), hi.max(pos))
            },
        );
        let id = s.draws.len();
        s.draws.push(Draw {
            owner: cpu.clone(),
            slot,
            deformation: d,
            center: (lo + hi) * 0.5,
        });
        Some(id)
    });
    #[cfg(not(target_os = "vita"))]
    {
        let n = allocations.finish();
        super::retained::count("gpu_skin_descriptor_allocation_calls", n);
    }
    let Some(id) = descriptor else {
        return false;
    };
    let (time, appearance) = FRAME.with(|f| {
        let f = f.borrow();
        (f.time, f.appearance)
    });
    let opacity = SKIN_OPACITY.with(|a| a.get());
    let limit = crate::render::vita_renderer::config()
        .actor_max_layers
        .min(surface.layers.len());
    let mut passes = super::actor_cache::pass_storage(limit);
    for l in surface.layers.iter().take(limit) {
        let s = &l.stage;
        let mut pass = pass_at(
            Mesh {
                vertices: Vec::new(),
                indices: Vec::new(),
                texture: Some(l.textures[s.frame(time)].clone()),
            },
            s,
            true,
            !s.identity,
            appearance,
            time,
        )
        .expect("retained skin pass");
        pass.gpu_skin = Some((id, s.vertex_alpha));
        // CPU computes floor(vertex_byte * entity_opacity) before rasterization.
        if opacity < 1. {
            if s.blend == Blend::Opaque {
                pass.blend_index = crate::render::blend_index(Blend::Alpha);
            }
            pass.cutout = 0.;
            pass.depth_equal = false;
        }
        pass.filter_fade = match s.blend {
            Blend::Filter | Blend::Factors(0, 2) | Blend::Factors(4, 0) => 1. - opacity,
            Blend::DoubleFilter | Blend::Factors(4, 2) => opacity - 1.,
            _ => 0.,
        };
        passes.push(pass);
    }
    STATE.with(|s| {
        let mut s = s.borrow_mut();
        let draw = &mut s.as_mut().unwrap().draws[id];
        draw.deformation.args.w = opacity;
    });
    record(&cpu, true);
    super::retained::count("gpu_skin_eligible_surfaces", 1);
    super::retained::count("gpu_skin_eligible_vertices", r.vertices.len() as u64);
    super::submit(passes, surface.transparent || opacity < 1.);
    true
}
pub(super) fn center(id: usize) -> Vec3 {
    STATE.with(|s| s.borrow().as_ref().unwrap().draws[id].center)
}
pub(super) fn vertex_count(id: usize) -> usize {
    STATE.with(|s| s.borrow().as_ref().unwrap().draws[id].owner.vertices.len())
}
pub(super) fn draw(
    id: usize,
    vertex_alpha: bool,
    pass: &Pass,
    uniforms: &[f32; super::retained::UNIFORM_FLOATS],
    depth_write: bool,
) -> u64 {
    let _owner = crate::render_tail::owner(crate::light_profile::RenderPhase::Actors);
    let started = std::time::Instant::now();
    STATE.with(|s| {
        let mut s = s.borrow_mut();
        let s = s.as_mut().unwrap();
        let draw = &s.draws[id];
        let mut u = [0_f32; super::retained::UNIFORM_FLOATS + EXTRA_FLOATS];
        u[..super::retained::UNIFORM_FLOATS].copy_from_slice(uniforms);
        let d = draw.deformation;
        let a = d.linear.transpose();
        let n = d.cofactor.transpose();
        let mut args = d.args;
        args.z = (255. * args.w * if vertex_alpha { args.z / 255. } else { 1. }) as u8 as f32;
        let rows = [
            a.x_axis.extend(d.origin.x),
            a.y_axis.extend(d.origin.y),
            a.z_axis.extend(d.origin.z),
            n.x_axis.extend(0.),
            n.y_axis.extend(0.),
            n.z_axis.extend(0.),
            args,
        ];
        for (dst, row) in u[super::retained::UNIFORM_FLOATS..]
            .chunks_exact_mut(4)
            .zip(rows)
        {
            dst.copy_from_slice(&row.to_array());
        }
        let b = &mut s.static_data[draw.slot];
        b.bindings.images[0] = pass.mesh.texture.as_ref().unwrap().raw_miniquad_id();
        let ctx = unsafe { get_internal_gl() }.quad_context;
        ctx.apply_pipeline(&s.pipelines[pass.blend_index * 2 + usize::from(pass.depth_equal)]);
        unsafe {
            gl::glDepthMask(u8::from(depth_write));
        }
        ctx.apply_bindings(&b.bindings);
        ctx.apply_uniforms(UniformsSource::table(&u));
        ctx.draw(0, draw.owner.indices.len() as i32, 1);
        super::retained::count("gpu_skin_draws", 1);
        super::retained::count("gpu_skin_deformation_bytes", EXTRA_FLOATS as u64 * 4);
    });
    let elapsed = started.elapsed().as_nanos() as u64;
    crate::light_profile::render_add(
        crate::light_profile::RenderPhase::Actors,
        crate::light_profile::RenderMetrics {
            draws: 1,
            pipelines: 1,
            depth_states: 1,
            bindings: 1,
            texture_bind_slots: 3,
            uniform_uploads: 1,
            uniform_bytes: (super::retained::UNIFORM_FLOATS + EXTRA_FLOATS) as u64 * 4,
            cpu_ns: elapsed,
            ..Default::default()
        },
    );
    elapsed
}
impl Drop for State {
    fn drop(&mut self) {
        crate::game_thread::assert_current();
        let ctx = unsafe { get_internal_gl() }.quad_context;
        for b in &self.static_data {
            ctx.delete_buffer(b.vertex);
            ctx.delete_buffer(b.index);
        }
        for p in &self.pipelines {
            ctx.delete_pipeline(*p);
        }
        ctx.delete_shader(self.shader);
    }
}
const VERTEX: &str = r#"#version 100
precision highp float;
attribute vec3 position; attribute vec2 texcoord; attribute vec4 color0; attribute vec4 normal;
uniform mat4 Model; uniform mat4 Projection;
uniform vec4 SkinRow0; uniform vec4 SkinRow1; uniform vec4 SkinRow2;
uniform vec4 NormalRow0; uniform vec4 NormalRow1; uniform vec4 NormalRow2; uniform vec4 SkinArgs;
varying highp vec2 uv; varying highp vec3 lm; varying lowp vec4 color; varying highp vec3 worldPosition;
void main() {
 vec4 p=vec4(position,1.0);
 worldPosition=vec3(dot(SkinRow0,p),dot(SkinRow1,p),dot(SkinRow2,p));
 gl_Position=Projection*vec4(worldPosition,1.0);
 vec3 n=vec3(dot(NormalRow0.xyz,normal.xyz),dot(NormalRow1.xyz,normal.xyz),dot(NormalRow2.xyz,normal.xyz));
 float light=1.0;
 if(SkinArgs.x<0.5) {
   if(SkinArgs.y>0.5) light=217.0/255.0;
   else light=0.65+0.35*max(dot(normalize(n),normalize(vec3(0.3,-0.5,1.0))),0.0);
 }
 color=vec4(vec3(floor(light*255.0)/255.0),SkinArgs.z/255.0);
 uv=texcoord; lm=vec3(0.0);
}
"#;

#[cfg(not(target_os = "vita"))]
fn compare(cpu: &SkinSurface, p: Params<'_>) -> anyhow::Result<Option<(f32, f32, u8)>> {
    let Some(r) = &cpu.rigid else {
        return Ok(None);
    };
    let Some(d) = deformation(r, p) else {
        return Ok(None);
    };
    let mut reference = cpu.bind_vertices.clone();
    crate::character::skinning::kernel(
        &mut crate::character::skinning::Job {
            input: &cpu.vertices,
            vertices: &mut reference,
            indices: &cpu.indices,
        },
        p,
    );
    let a = d.linear.transpose();
    let n = d.cofactor.transpose();
    let mut position_error = 0_f32;
    let mut normal_error = 0_f32;
    let mut rgb_error = 0u8;
    for (v, expected) in r.vertices.iter().zip(reference) {
        let pos = vec3(
            a.x_axis.dot(v.position) + d.origin.x,
            a.y_axis.dot(v.position) + d.origin.y,
            a.z_axis.dot(v.position) + d.origin.z,
        );
        let norm = vec3(
            n.x_axis.dot(v.normal.truncate()),
            n.y_axis.dot(v.normal.truncate()),
            n.z_axis.dot(v.normal.truncate()),
        )
        .normalize_or_zero();
        let norm_ref = expected.normal.truncate().normalize_or_zero();
        let angle = 2.
            * ((norm - norm_ref).length() * 0.5)
                .min(1.)
                .asin()
                .to_degrees();
        let light = if p.fullbright {
            255
        } else if p.flat {
            217
        } else {
            ((0.65 + 0.35 * norm.dot(vec3(0.3, -0.5, 1.).normalize()).max(0.)) * 255.).floor() as u8
        };
        position_error = position_error.max((pos - expected.position).abs().max_element());
        if !p.flat {
            normal_error = normal_error.max(angle);
        }
        rgb_error = rgb_error.max(light.abs_diff(expected.color[0]));
        anyhow::ensure!(expected.color[3] == d.args.z as u8, "alpha mismatch");
    }
    anyhow::ensure!(position_error<=0.001 && normal_error<=0.5 && rgb_error<=1,
        "rigid numeric tolerance failed: position={position_error}, normal={normal_error}, rgb={rgb_error}");
    Ok(Some((position_error, normal_error, rgb_error)))
}
/// Headless comparison with the unmodified CPU kernel on supplied game data.
#[cfg(not(target_os = "vita"))]
pub(crate) fn check(assets: &mut Assets) -> anyhow::Result<()> {
    use crate::skeletal::{Animation, Definition, Skeleton, Transform};
    let mut models = Vec::new();
    let mut total = 0;
    let mut maxima = (0_f32, 0_f32, 0u8);
    for name in [
        "alice",
        "c_cheshire",
        "cardguard_club",
        "c_whiterabbit",
        "c_torchgnome3",
        "c_insanechildnew",
    ] {
        let def = Definition::load(assets, &format!("models/{name}.tik"))?;
        let skeleton = Skeleton::parse(&assets.read(&format!("{}/{}", def.path, def.model))?)?;
        let mut clips = Vec::new();
        for filename in def
            .animations
            .values()
            .filter(|s| s.ends_with(".ska"))
            .take(4)
        {
            clips.push(Animation::parse(
                &assets.read(&format!("{}/{filename}", def.path))?,
                skeleton.bones.len(),
            )?);
        }
        anyhow::ensure!(!clips.is_empty(), "no animation for {name}");
        let mut surfaces = Vec::new();
        for surface in &skeleton.surfaces {
            let mut accepted = 0;
            let mut rejected = 0;
            for clip in &clips {
                for frame in clip
                    .frames
                    .iter()
                    .step_by((clip.frames.len() / 16).max(1))
                    .take(16)
                {
                    let pose = skeleton.global_pose(&frame.pose);
                    for origin in [Vec3::ZERO, vec3(100., -200., 30.), vec3(-4188., 6004., 32.)] {
                        for (scale, qscale, palette) in
                            [(1., 1., false), (-0.75, 1., true), (1.4, 0.96, true)]
                        {
                            let scales = vec![1.2; skeleton.bones.len()];
                            let pal: Vec<_> = pose
                                .iter()
                                .map(|b| {
                                    (
                                        crate::character::skinning::rotation(b.rotation) * 1.2,
                                        b.translation,
                                    )
                                })
                                .collect();
                            for (flat, fullbright) in [(false, false), (false, true), (true, false)]
                            {
                                let p = Params {
                                    pose: &pose,
                                    scales: &scales,
                                    palette: if palette { &pal } else { &[] },
                                    transform: Transform {
                                        translation: origin,
                                        rotation: Quat::from_rotation_z(0.731) * qscale,
                                    },
                                    scale: scale * def.scale,
                                    flat,
                                    fullbright,
                                    alpha: 0.37,
                                };
                                match compare(surface, p)? {
                                    Some((a, b, c)) => {
                                        accepted += 1;
                                        total += 1;
                                        maxima.0 = maxima.0.max(a);
                                        maxima.1 = maxima.1.max(b);
                                        maxima.2 = maxima.2.max(c);
                                    }
                                    None => rejected += 1,
                                }
                            }
                        }
                    }
                }
            }
            surfaces.push(serde_json::json!({"name":surface.name,"vertices":surface.vertices.len(),"rigid_metadata":surface.rigid.is_some(),"accepted":accepted,"exact_cpu_fallback":rejected}));
        }
        models.push(
            serde_json::json!({"model":name,"bones":skeleton.bones.len(),"surfaces":surfaces}),
        );
    }
    anyhow::ensure!(total > 0, "no accepted rigid cases");
    std::fs::create_dir_all("private/gpu-skin")?;
    std::fs::write(
        "private/gpu-skin/numerical.json",
        serde_json::to_vec_pretty(&serde_json::json!({
        "accepted_cases":total,"max_position_error":maxima.0,"max_normal_degrees":maxima.1,"max_rgb_byte_error":maxima.2,"alpha_exact":true,"models":models,"console_access":false}))?,
    )?;
    println!(
        "PASS rigid skin: {total} real surface/pose cases; position={} normal={} RGB={}",
        maxima.0, maxima.1, maxima.2
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skeletal::{Transform, Weight};
    fn input() -> Vec<SkinVertex> {
        [Vec3::ZERO, Vec3::X * 10., Vec3::Y * 10.]
            .into_iter()
            .map(|offset| SkinVertex {
                uv: Vec2::ZERO,
                weights: vec![Weight {
                    bone: 130,
                    amount: 1.,
                    offset,
                }],
            })
            .collect()
    }
    #[test]
    fn eligibility_counts_surface_weights_not_skeleton_bones_and_rejects_truncation() {
        let mut v = input();
        assert!(Rigid::new(&v, &[0, 1, 2]).is_some());
        v[1].weights.push(Weight {
            bone: 0,
            amount: 0.,
            offset: Vec3::ZERO,
        });
        assert!(Rigid::new(&v, &[0, 1, 2]).is_none());
        v[1].weights.pop();
        v[1].weights[0].amount = 0.999;
        assert!(Rigid::new(&v, &[0, 1, 2]).is_none());
        assert!(Rigid::new(&input(), &[0, 0, 0]).is_none());
        assert!(Rigid::new(&input(), &[0, 1, 3]).is_none());
    }
    #[test]
    fn cofactor_matches_cpu_area_normals_for_mirrors_and_nonunit_rotations() {
        let cpu = crate::skeletal::Surface::new("synthetic".into(), input(), vec![0, 1, 2]).cpu();
        let pose = vec![
            Transform {
                translation: vec3(10., 20., 30.),
                rotation: Quat::from_rotation_y(0.53) * 0.94
            };
            131
        ];
        let scales = vec![-1.3; 131];
        for scale in [-2., 0., 0.001, 1., 2.] {
            for alpha in [0., 0.12, 0.37, 1.] {
                for flat in [false, true] {
                    let p = Params {
                        pose: &pose,
                        scales: &scales,
                        palette: &[],
                        transform: Transform {
                            translation: vec3(100., -200., 30.),
                            rotation: Quat::from_rotation_x(-0.7) * 0.96,
                        },
                        scale,
                        flat,
                        fullbright: false,
                        alpha,
                    };
                    let result = compare(&cpu, p).unwrap();
                    if scale == 0. {
                        assert!(result.is_none());
                    } else if scale.abs() >= 1. {
                        assert!(result.is_some());
                    }
                }
            }
        }
        let huge = Params {
            pose: &pose,
            scales: &[],
            palette: &[],
            transform: Transform {
                translation: Vec3::splat(1e8),
                rotation: Quat::IDENTITY,
            },
            scale: 1.,
            flat: false,
            fullbright: false,
            alpha: 1.,
        };
        assert!(compare(&cpu, huge).unwrap().is_none());
    }
    #[test]
    fn bounded_descriptors_own_surfaces_after_model_drop_without_steady_allocations() {
        let surface = crate::skeletal::Surface::new("owned".into(), input(), vec![0, 1, 2]);
        let cpu = surface.cpu();
        let weak = Arc::downgrade(&cpu);
        let pose = vec![
            Transform {
                translation: Vec3::ZERO,
                rotation: Quat::IDENTITY
            };
            131
        ];
        let d = deformation(
            cpu.rigid.as_ref().unwrap(),
            Params {
                pose: &pose,
                scales: &[],
                palette: &[],
                transform: pose[0],
                scale: 1.,
                flat: false,
                fullbright: false,
                alpha: 1.,
            },
        )
        .unwrap();
        let mut draws = Vec::with_capacity(DRAW_LIMIT);
        let ptr = draws.as_ptr();
        drop(surface);
        let scope = crate::kernel_alloc::Scope::begin();
        for _ in 0..20 {
            draws.clear();
            for _ in 0..DRAW_LIMIT {
                assert!(descriptor_available(&draws));
                draws.push(Draw {
                    owner: cpu.clone(),
                    slot: 0,
                    deformation: d,
                    center: Vec3::ZERO,
                });
            }
            assert_eq!(draws.as_ptr(), ptr);
            assert!(!descriptor_available(&draws));
        }
        assert_eq!(scope.finish(), 0);
        drop(cpu);
        assert!(weak.upgrade().is_some());
        drop(draws);
        assert!(weak.upgrade().is_none());
        assert!(static_available(0, 0, BYTE_LIMIT));
        assert!(!static_available(STATIC_LIMIT, 0, 1));
        assert!(!static_available(0, BYTE_LIMIT, 1));
        assert!(!static_available(0, usize::MAX, 1));
    }
    #[test]
    fn metadata_is_numeric_send_sync_but_renderer_is_confined() {
        fn numeric<T: Send + Sync>() {}
        numeric::<Rigid>();
        numeric::<SkinSurface>();
        trait AmbiguousIfSend<A> {
            fn check() {}
        }
        impl<T: ?Sized> AmbiguousIfSend<()> for T {}
        struct Invalid;
        impl<T: ?Sized + Send> AmbiguousIfSend<Invalid> for T {}
        let _ = <State as AmbiguousIfSend<_>>::check;
    }
}

#[cfg(target_os = "vita")]
unsafe fn compile_pair(vertex: &str, fragment: &str) -> bool {
    extern "C" {
        fn glCreateShader(kind: u32) -> u32;
        fn glShaderSource(
            shader: u32,
            count: i32,
            source: *const *const std::ffi::c_char,
            length: *const i32,
        );
        fn glCompileShader(shader: u32);
        fn glGetShaderiv(shader: u32, name: u32, value: *mut i32);
        fn glDeleteShader(shader: u32);
    }
    let mut shaders = [0; 2];
    let mut ok = true;
    for (index, (kind, source)) in [(0x8B31, vertex), (0x8B30, fragment)]
        .into_iter()
        .enumerate()
    {
        shaders[index] = glCreateShader(kind);
        if shaders[index] == 0 {
            ok = false;
            break;
        }
        let source = std::ffi::CString::new(source).unwrap();
        let ptr = source.as_ptr();
        glShaderSource(shaders[index], 1, &ptr, std::ptr::null());
        glCompileShader(shaders[index]);
        let mut status = 0;
        glGetShaderiv(shaders[index], 0x8B81, &mut status);
        ok &= status != 0;
    }
    for shader in shaders {
        if shader != 0 {
            glDeleteShader(shader);
        }
    }
    ok
}

#[cfg(not(target_os = "vita"))]
thread_local! {static SURFACE_COUNTS:RefCell<Vec<(ArcWeak<SkinSurface>,String,[u64;2])>>=RefCell::new(Vec::with_capacity(512));}
pub(crate) fn record(cpu: &Arc<SkinSurface>, gpu: bool) {
    #[cfg(not(target_os = "vita"))]
    SURFACE_COUNTS.with(|s| {
        let mut s = s.borrow_mut();
        let owner = Arc::downgrade(cpu);
        if let Some(c) = s.iter_mut().find(|c| c.0.ptr_eq(&owner)) {
            c.2[usize::from(!gpu)] += 1;
        } else if s.len() < 512 {
            s.push((
                owner,
                format!(
                    "bones={}/{}:vertices={}",
                    cpu.skeleton_bones,
                    cpu.source_name,
                    cpu.vertices.len()
                ),
                if gpu { [1, 0] } else { [0, 1] },
            ));
        }
    });
    #[cfg(target_os = "vita")]
    let _ = (cpu, gpu);
}
#[cfg(not(target_os = "vita"))]
pub(crate) fn surface_report() -> serde_json::Value {
    SURFACE_COUNTS.with(|s| {
        serde_json::Value::Array(
            s.borrow()
                .iter()
                .filter(|c| c.2 != [0; 2])
                .map(|c| serde_json::json!({"surface":c.1,"gpu":c.2[0],"cpu":c.2[1]}))
                .collect(),
        )
    })
}
