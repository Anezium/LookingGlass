//! Direct, retained world draws. CPU geometry and PVS compilation happen at load time.
mod shader;
mod pvs_cache;
#[cfg(not(target_os = "vita"))]
pub(crate) mod interop_check;
use super::Scene;
use crate::materials::{Blend, Deform, FaceSide, Stage, TcMod};
use anyhow::{ensure, Context, Result};
use macroquad::{miniquad::*, prelude::*};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex, OnceLock},
};

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub renderer: String,
    #[serde(alias = "renderer_world_only")]
    pub world_only: bool,
    pub max_layers: usize,
    pub sort_alpha: bool,
    pub sky: bool,
    pub cell_size: f32,
    /// Compare grouped actor/world alpha to the milestone-2 bridge.
    pub alpha_interop: bool,
    pub actor_max_layers: usize,
    pub merge_fx: bool,
    pub pvs_cache: bool,
    pub pvs_method: String,
    pub pvs_verify: bool,
    pub gpu_experiment: String,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            renderer: "vita".into(),
            world_only: false,
            max_layers: 2,
            sort_alpha: true,
            sky: true,
            cell_size: 256.,
            alpha_interop: true,
            actor_max_layers: 2,
            merge_fx: true,
            pvs_cache: false,
            pvs_method: "sources".into(),
            pvs_verify: false,
            gpu_experiment: "unchanged".into(),
        }
    }
}
impl Config {
    fn validate(&self) -> Result<()> {
        ensure!(
            matches!(self.renderer.as_str(), "vita" | "legacy"),
            "renderer must be vita or legacy"
        );
        ensure!(
            (1..=2).contains(&self.max_layers),
            "max_layers must be 1 or 2"
        );
        ensure!(
            (1..=2).contains(&self.actor_max_layers),
            "actor_max_layers must be 1 or 2"
        );
        ensure!(matches!(self.gpu_experiment.as_str(),"unchanged"|"gpu111"|"gpu222"),"gpu_experiment must be unchanged, gpu111 or gpu222");
        ensure!(matches!(self.pvs_method.as_str(),"sources"|"legacy"),"pvs_method must be sources or legacy");
        ensure!(
            self.cell_size.is_finite() && (64. ..=2048.).contains(&self.cell_size),
            "invalid cell_size"
        );
        Ok(())
    }
}
#[cfg(target_os = "vita")]
const CONFIG_PATH: &str = "ux0:data/alice/private/vita_renderer.json";
#[cfg(not(target_os = "vita"))]
const CONFIG_PATH: &str = "private/vita_renderer.json";
pub fn config() -> &'static Config {
    static CONFIG: OnceLock<Config> = OnceLock::new();
    CONFIG.get_or_init(|| {
        let parsed = (|| -> Result<Config> {
            let bytes = match std::fs::read(CONFIG_PATH) {
                Ok(bytes) => bytes,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Config::default()),
                Err(e) => return Err(e.into()),
            };
            let c: Config = serde_json::from_slice(&bytes)?;
            c.validate()?;
            Ok(c)
        })();
        let c = parsed.unwrap_or_else(|e| {
            eprintln!("[lg/vita-renderer] invalid config {CONFIG_PATH}: {e:#}; using defaults");
            Config::default()
        });
        println!(
            "[lg/vita-renderer] config {}",
            serde_json::to_string(&c).unwrap()
        );
        apply_gpu_experiment(&c);
        c
    })
}
/// Named GPU-only experiment; default does not call a setter or alter ARM/bus/xbar.
fn apply_gpu_experiment(c:&Config) {
    if c.gpu_experiment=="unchanged" {return;}
    #[cfg(target_os="vita")]
    unsafe {
        extern "C" {
            fn scePowerSetGpuClockFrequency(freq:i32)->i32;
            fn scePowerGetArmClockFrequency()->i32;
            fn scePowerGetBusClockFrequency()->i32;
            fn scePowerGetGpuClockFrequency()->i32;
            fn scePowerGetGpuXbarClockFrequency()->i32;
        }
        let read=||[scePowerGetArmClockFrequency(),scePowerGetBusClockFrequency(),scePowerGetGpuClockFrequency(),scePowerGetGpuXbarClockFrequency()];
        let before=read();let requested=if c.gpu_experiment=="gpu222" {222}else{111};
        let rc=scePowerSetGpuClockFrequency(requested);let after=read();
        println!("[lg/vita-renderer] gpu experiment {} rc={rc} before={before:?} after={after:?}",c.gpu_experiment);
        if rc<0 || after[2]!=requested || before[0]!=after[0] || before[1]!=after[1] || before[3]!=after[3] {
            eprintln!("[lg/vita-renderer] GPU experiment did not satisfy its clock contract; inspect actual window clocks");
        }
    }
    #[cfg(not(target_os="vita"))]
    println!("[lg/vita-renderer] gpu experiment {} ignored on host",c.gpu_experiment);
}
/// Diagnostic draw mask only: simulation, accepted frames and save policy stay intact.
pub fn world_only() -> bool {
    #[cfg(target_os = "vita")]
    {
        mask_in_mode(config().world_only, crate::frame_profile::mode())
    }
    #[cfg(not(target_os = "vita"))]
    {
        false
    }
}
#[cfg(any(target_os = "vita", test))]
fn mask_in_mode(requested: bool, mode: crate::frame_profile::Mode) -> bool {
    requested && mode == crate::frame_profile::Mode::RenderBench
}
static INVENTORY: Mutex<Option<serde_json::Value>> = Mutex::new(None);
#[derive(Clone, Copy, serde::Serialize)]
struct View {
    eye: [f32; 3],
    target: [f32; 3],
    up: [f32; 3],
    fovy_radians: f32,
    near: f32,
    far: f32,
    width: f32,
    height: f32,
}
static VIEW: Mutex<Option<View>> = Mutex::new(None);
pub(crate) fn record_view(camera: &Camera3D) {
    *VIEW.lock().unwrap() = Some(View {
        eye: camera.position.to_array(),
        target: camera.target.to_array(),
        up: camera.up.to_array(),
        fovy_radians: camera.fovy,
        near: camera.z_near,
        far: camera.z_far,
        width: screen_width(),
        height: screen_height(),
    });
}
pub fn report() -> serde_json::Value {
    let result=serde_json::json!({"feature":true, "configuration":config(), "config_path":CONFIG_PATH,
        "world_only_active":world_only(), "inventory":*INVENTORY.lock().unwrap(),"view":*VIEW.lock().unwrap()});
    #[cfg(not(target_os="vita"))]
    { let mut result=result; result["host_frame_counts"]=crate::render_fx::retained::host_report(); return result; }
    #[cfg(target_os="vita")]
    result
}

#[repr(C)]
#[derive(Clone, Copy)]
struct WorldVertex {
    position: [f32; 3],
    uv: [f32; 2],
    color: [u8; 4],
    lightmap: [f32; 4],
    normal: [f32; 3],
}
struct Geometry {
    vertices: Vec<WorldVertex>,
    indices: Vec<u16>,
    remap: BTreeMap<(usize, u16), u16>,
    faces: Vec<usize>,
    min: Vec3,
    max: Vec3,
}
impl Default for Geometry {
    fn default() -> Self {
        Self {
            vertices: Vec::new(),
            indices: Vec::new(),
            remap: BTreeMap::new(),
            faces: Vec::new(),
            min: Vec3::splat(f32::INFINITY),
            max: Vec3::splat(f32::NEG_INFINITY),
        }
    }
}
impl Geometry {
    fn fits(&self, indices: &[u16]) -> bool {
        // Conservative reservation keeps a whole source face together, without u16 overflow.
        self.vertices.len() + indices.len() <= 60000 && self.indices.len() + indices.len() <= 180000
    }
    fn append(
        &mut self,
        batch_id: usize,
        batch: &super::Batch,
        source: &[u16],
        face: Option<usize>,
    ) {
        if let Some(face) = face {
            self.faces.push(face);
        }
        for &index in source {
            let id = *self.remap.entry((batch_id, index)).or_insert_with(|| {
                let v = batch.base[index as usize];
                self.min = self.min.min(v.position);
                self.max = self.max.max(v.position);
                let id = self.vertices.len() as u16;
                self.vertices.push(WorldVertex {
                    position: v.position.to_array(),
                    uv: v.uv.to_array(),
                    color: v.color,
                    lightmap: v.normal.to_array(),
                    normal: batch.normals[index as usize].to_array(),
                });
                id
            });
            self.indices.push(id);
        }
    }
}
#[derive(Clone)]
struct Recipe {
    stage: Stage,
    textures: Vec<Texture2D>,
    lit: bool,
    rank: usize,
    blend: usize,
    opaque: bool,
    cull: usize,
    model: Option<usize>,
    motion: Option<crate::sky_sequence::Motion>,
    deform: Option<Deform>,
}
struct Pending {
    geometry: Geometry,
    recipe: Recipe,
}
struct Group {
    bindings: Bindings,
    indices: i32,
    min: Vec3,
    max: Vec3,
    center: Vec3,
    recipe: Recipe,
    faces: Vec<usize>,
}
#[derive(Default, PartialEq, Eq, PartialOrd, Ord)]
struct Lists {
    opaque: Vec<usize>,
    alpha: Vec<usize>,
}
struct Pvs {
    clusters: Vec<Arc<Lists>>,
    unknown: Arc<Lists>,
}
impl Pvs {
    fn new(groups: &[Group], visibility: &crate::visibility::Visibility) -> Self {
        if config().pvs_method=="legacy" {return Self::legacy(groups,visibility);}
        Self::sources(groups,visibility)
    }
    fn sources(groups:&[Group],visibility:&crate::visibility::Visibility)->Self {
        let source_start=std::time::Instant::now();
        let faces=visibility.face_clusters();
        let faces_ms=source_start.elapsed().as_secs_f64()*1000.;
        let sources=groups.iter().map(|g| {
            if g.recipe.model.is_some() || g.recipe.motion.is_some() {return None;}
            if g.faces.is_empty() {return visibility.bounds_clusters(g.min,g.max);}
            let mut clusters=g.faces.iter().flat_map(|&f|faces[f].iter().copied()).collect::<Vec<_>>();
            if g.recipe.deform.is_some() {
                let bounds=visibility.bounds_clusters(g.min,g.max)?;
                clusters.extend(bounds);
            }
            clusters.sort_unstable();clusters.dedup();Some(clusters)
        }).collect::<Vec<_>>();
        println!("[lg/vita-renderer] PVS sources groups={} source_cluster_ids={} leaf_bounds_queries={}",groups.len(),sources.iter().filter_map(|s|s.as_ref()).map(Vec::len).sum::<usize>(),groups.iter().filter(|g|g.recipe.model.is_none() && g.recipe.motion.is_none() && (g.faces.is_empty() || g.recipe.deform.is_some())).count());
        let sources_ms=source_start.elapsed().as_secs_f64()*1000.;
        let lists_start=std::time::Instant::now();
        let result=Self::compile(visibility.cluster_count(),|cluster| {
            let mut out=Lists::default();
            for (id,source) in sources.iter().enumerate() {
                if cluster.is_none() || source.as_ref().is_none_or(|s|visibility.sees_clusters(cluster.unwrap(),s)) {
                    if groups[id].recipe.opaque {out.opaque.push(id);} else {out.alpha.push(id);}
                }
            }
            out.opaque.sort_by_key(|&id|(groups[id].recipe.rank,groups[id].recipe.blend,groups[id].recipe.cull,id));
            out
        });
        println!("[lg/vita-renderer] PVS phases face_sources={faces_ms:.3} ms sources_total={sources_ms:.3} ms lists={:.3} ms",lists_start.elapsed().as_secs_f64()*1000.);
        result
    }
    fn legacy(groups: &[Group], visibility: &crate::visibility::Visibility) -> Self {
        let list = |cluster| {
            let mask = visibility.faces(cluster);
            let mut out = Lists::default();
            for (id, g) in groups.iter().enumerate() {
                let visible = g.recipe.model.is_some()
                    || g.recipe.motion.is_some()
                    || if g.faces.is_empty() {
                        visibility.bounds(cluster, g.min, g.max)
                    } else {
                        !mask
                            .as_ref()
                            .is_some_and(|mask| !g.faces.iter().any(|&f| mask[f]))
                            || (g.recipe.deform.is_some()
                                && visibility.bounds(cluster, g.min, g.max))
                    };
                if visible {
                    if g.recipe.opaque {
                        out.opaque.push(id);
                    } else {
                        out.alpha.push(id);
                    }
                }
            }
            out.opaque.sort_by_key(|&id| {
                (
                    groups[id].recipe.rank,
                    groups[id].recipe.blend,
                    groups[id].recipe.cull,
                    id,
                )
            });
            out
        };
        Self::compile(visibility.cluster_count(), list)
    }
    fn compile(count: usize, list: impl Fn(Option<usize>) -> Lists) -> Self {
        let unknown = Arc::new(list(None));
        let mut intern = BTreeMap::new();
        let clusters = (0..count)
            .map(|c| {
                let l = list(Some(c));
                intern
                    .entry(l)
                    .or_insert_with_key(|l| {
                        Arc::new(Lists {
                            opaque: l.opaque.clone(),
                            alpha: l.alpha.clone(),
                        })
                    })
                    .clone()
            })
            .collect();
        Self { clusters, unknown }
    }
    fn get(&self, cluster: Option<usize>) -> &Lists {
        cluster
            .and_then(|c| self.clusters.get(c))
            .unwrap_or(&self.unknown)
    }
}
struct Prepared {
    depth: f32,
    group: usize,
    texture: TextureId,
    uniforms: shader::Uniforms,
}
pub struct Renderer {
    groups: Vec<Group>,
    pvs: Pvs,
    pipelines: Vec<Pipeline>,
    shader: ShaderId,
    sky_pipeline: Pipeline,
    sky_shader: ShaderId,
    sky_bindings: Bindings,
    alpha_order: Vec<(f32, usize)>,
    prepared: Vec<Prepared>,
    fx_gpu: Option<crate::render_fx::retained::Gpu>,
}
fn blend_class(blend: Blend) -> usize {
    match blend {
        Blend::Opaque => 0,
        Blend::Add | Blend::AlphaAdd | Blend::ColorAdd => 2,
        Blend::Filter => 3,
        Blend::DoubleFilter => 4,
        _ => 1,
    }
}
fn opaque_stage(stage: &Stage) -> bool {
    stage.blend == Blend::Opaque || (stage.depth_write && stage.alpha_test > 0)
}
fn deform_extent(deform: &Option<Deform>) -> Vec3 {
    match deform {
        Some(Deform::Wave(_, w)) => Vec3::splat(w.values[0].abs() + w.values[1].abs()),
        Some(Deform::Move(v, w)) => v.abs() * (w.values[0].abs() + w.values[1].abs()),
        Some(Deform::Bulge(v)) => Vec3::splat(v[1].abs()),
        _ => Vec3::ZERO,
    }
}
impl Renderer {
    pub(super) fn new(scene: &Scene) -> Result<Self> {
        let started=std::time::Instant::now();
        let c = config();
        let mut pending: Vec<Pending> = Vec::new();
        let mut keys: BTreeMap<String, Vec<usize>> = BTreeMap::new();
        let mut recipes_ms=0.;let mut chunks_ms=0.;let mut source_chunks=0;
        let mut dropped_layers = 0;
        let mut frozen_deforms = 0;
        let mut frozen_models = 0;
        for (bi, b) in scene.batches.iter().enumerate() {
            if b.sky {
                continue;
            }
            frozen_models += usize::from(b.animation.is_some());
            frozen_deforms += b
                .deforms
                .iter()
                .filter(|d| matches!(d, Deform::Sprite(_) | Deform::WaveNormal(..)))
                .count()
                + b.deforms.len().saturating_sub(1);
            let mut rank = 0;
            for layer in &b.layers {
                if layer.spec.blend == Blend::Invisible || layer.spec.detail {
                    dropped_layers += 1;
                    continue;
                }
                if rank == c.max_layers {
                    dropped_layers += 1;
                    continue;
                }
                let recipe_started=std::time::Instant::now();
                let recipe = Recipe {
                    stage: layer.spec.clone(),
                    textures: layer.textures.clone(),
                    lit: layer.lit,
                    rank,
                    blend: blend_class(layer.spec.blend),
                    opaque: opaque_stage(&layer.spec),
                    cull: match b.face_side {
                        FaceSide::Both => 0,
                        FaceSide::Front => 1,
                        FaceSide::Back => 2,
                    },
                    model: b.model,
                    motion: b.motion.clone(),
                    deform: b
                        .deforms
                        .iter()
                        .find(|d| {
                            matches!(d, Deform::Wave(..) | Deform::Move(..) | Deform::Bulge(..))
                        })
                        .cloned(),
                };
                rank += 1;
                let texture_keys = layer
                    .textures
                    .iter()
                    .map(Texture2D::raw_miniquad_id)
                    .collect::<Vec<_>>();
                let signature = format!(
                    "{:?}|{:?}|{}|{}|{:?}|{:?}|{}",
                    layer.spec,
                    texture_keys,
                    layer.lit,
                    recipe.cull,
                    b.model,
                    b.deforms,
                    if b.motion.is_some() { bi + 1 } else { 0 }
                );
                let parts = if b.parts.is_empty() {
                    vec![(None, 0..b.full_indices.len(), b.center)]
                } else {
                    b.parts
                        .iter()
                        .map(|p| (Some(p.face), p.indices.clone(), (p.min + p.max) * 0.5))
                        .collect()
                };
                recipes_ms+=recipe_started.elapsed().as_secs_f64()*1000.;
                let chunks_started=std::time::Instant::now();
                for (face, range, center) in parts {
                    let cell = (center / c.cell_size).floor();
                    let key = format!("{signature}|{}|{cell:?}", recipe.rank);
                    let source = &b.full_indices[range];
                    // Split unusually large parts on triangle boundaries as well as between faces.
                    for source in source.chunks(59997) {
                        source_chunks+=1;
                        let candidates = keys.entry(key.clone()).or_default();
                        let id = candidates
                            .iter()
                            .copied()
                            .find(|&id| pending[id].geometry.fits(source))
                            .unwrap_or_else(|| {
                                let id = pending.len();
                                pending.push(Pending {
                                    geometry: Geometry::default(),
                                    recipe: recipe.clone(),
                                });
                                candidates.push(id);
                                id
                            });
                        pending[id].geometry.append(bi, b, source, face);
                    }
                }
                chunks_ms+=chunks_started.elapsed().as_secs_f64()*1000.;
            }
        }
        ensure!(
            !pending.is_empty(),
            "retained world has no drawable geometry"
        );
        let atlas = scene.atlas.raw_miniquad_id();
        let bytes: usize = pending
            .iter()
            .map(|p| p.geometry.vertices.len() * 52 + p.geometry.indices.len() * 2)
            .sum();
        let vertices: usize = pending.iter().map(|p| p.geometry.vertices.len()).sum();
        let triangles: usize = pending.iter().map(|p| p.geometry.indices.len() / 3).sum();
        // Resolve Macroquad's managed handles before borrowing its internal GL context.
        let first_textures = pending
            .iter()
            .map(|p| p.recipe.textures[0].raw_miniquad_id())
            .collect::<Vec<_>>();
        let groups_ms=started.elapsed().as_secs_f64()*1000.;
        println!("[lg/vita-renderer] load groups {:.3} ms recipes={recipes_ms:.3} chunks={chunks_ms:.3} source_chunks={source_chunks} groups={} vertices={} layer_triangles={}",groups_ms,pending.len(),vertices,triangles);
        let upload_start=std::time::Instant::now();
        let gl = unsafe { get_internal_gl() };
        let ctx = gl.quad_context;
        let shader = ctx
            .new_shader(
                ShaderSource::Glsl {
                    vertex: shader::VERTEX,
                    fragment: &crate::environment::fragment(shader::FRAGMENT),
                },
                shader::meta(),
            )
            .context("retained world shader")?;
        let attributes = [
            VertexAttribute::new("position", VertexFormat::Float3),
            VertexAttribute::new("texcoord", VertexFormat::Float2),
            VertexAttribute::new("color0", VertexFormat::Byte4),
            VertexAttribute::new("lightmap", VertexFormat::Float4),
            VertexAttribute::new("normal", VertexFormat::Float3),
        ];
        let mut pipelines = Vec::new();
        for blend in 0..5 {
            let state = match blend {
                0 => None,
                1 => Some(BlendState::new(
                    Equation::Add,
                    BlendFactor::Value(BlendValue::SourceAlpha),
                    BlendFactor::OneMinusValue(BlendValue::SourceAlpha),
                )),
                2 => Some(BlendState::new(
                    Equation::Add,
                    BlendFactor::One,
                    BlendFactor::One,
                )),
                3 => Some(BlendState::new(
                    Equation::Add,
                    BlendFactor::Value(BlendValue::DestinationColor),
                    BlendFactor::Zero,
                )),
                _ => Some(BlendState::new(
                    Equation::Add,
                    BlendFactor::Value(BlendValue::DestinationColor),
                    BlendFactor::Value(BlendValue::SourceColor),
                )),
            };
            for cull in [CullFace::Nothing, CullFace::Back, CullFace::Front] {
                pipelines.push(ctx.new_pipeline(
                    &[BufferLayout {
                        stride: 52,
                        ..Default::default()
                    }],
                    &attributes,
                    shader,
                    PipelineParams {
                        depth_test: Comparison::LessOrEqual,
                        depth_write: true,
                        cull_face: cull,
                        front_face_order: FrontFaceOrder::Clockwise,
                        color_blend: state,
                        alpha_blend: state,
                        ..Default::default()
                    },
                ));
            }
        }
        let mut groups = Vec::new();
        for (p, texture) in pending.into_iter().zip(first_textures) {
            let extent = deform_extent(&p.recipe.deform);
            let vertex = ctx.new_buffer(
                BufferType::VertexBuffer,
                BufferUsage::Immutable,
                BufferSource::slice(&p.geometry.vertices),
            );
            let index = ctx.new_buffer(
                BufferType::IndexBuffer,
                BufferUsage::Immutable,
                BufferSource::slice(&p.geometry.indices),
            );
            groups.push(Group {
                bindings: Bindings {
                    vertex_buffers: vec![vertex],
                    index_buffer: index,
                    images: vec![texture, atlas],
                },
                indices: p.geometry.indices.len() as i32,
                min: p.geometry.min - extent,
                max: p.geometry.max + extent,
                center: (p.geometry.min + p.geometry.max) * 0.5,
                recipe: p.recipe,
                faces: p.geometry.faces,
            });
        }
        let sky_shader = ctx.new_shader(
            ShaderSource::Glsl {
                vertex: shader::SKY_VERTEX,
                fragment: shader::SKY_FRAGMENT,
            },
            ShaderMeta {
                images: vec![],
                uniforms: UniformBlockLayout {
                    uniforms: vec![
                        UniformDesc::new("Bottom", UniformType::Float4),
                        UniformDesc::new("Top", UniformType::Float4),
                    ],
                },
            },
        )?;
        let sky_pipeline = ctx.new_pipeline(
            &[BufferLayout::default()],
            &[VertexAttribute::new("position", VertexFormat::Float2)],
            sky_shader,
            PipelineParams::default(),
        );
        let sky_bindings = Bindings {
            vertex_buffers: vec![ctx.new_buffer(
                BufferType::VertexBuffer,
                BufferUsage::Immutable,
                BufferSource::slice(&[[-1_f32, -1.], [3., -1.], [-1., 3.]]),
            )],
            index_buffer: ctx.new_buffer(
                BufferType::IndexBuffer,
                BufferUsage::Immutable,
                BufferSource::slice(&[0_u16, 1, 2]),
            ),
            images: vec![],
        };
        let upload_ms=upload_start.elapsed().as_secs_f64()*1000.;
        println!("[lg/vita-renderer] load pipelines/uploads {:.3} ms payload_bytes={bytes}",upload_ms);
        let pvs_start=std::time::Instant::now();
        let (pvs,cache_status)=pvs_cache::load_or_build(&groups,&scene.map.visibility);
        if c.pvs_verify {
            let original=Pvs::legacy(&groups,&scene.map.visibility);
            ensure!(original.unknown.as_ref()==pvs.unknown.as_ref() && original.clusters.iter().zip(&pvs.clusters).all(|(a,b)|a.as_ref()==b.as_ref()),"retained PVS differs from legacy compilation");
            println!("PASS retained PVS matches legacy compiler for None and all {} clusters",pvs.clusters.len());
        }
        let pvs_ms=pvs_start.elapsed().as_secs_f64()*1000.;
        println!("[lg/vita-renderer] load PVS {:.3} ms method={} cache={cache_status}",pvs_ms,c.pvs_method);
        let unique = pvs
            .clusters
            .iter()
            .map(|a| Arc::as_ptr(a) as usize)
            .collect::<std::collections::BTreeSet<_>>()
            .len();
        let pvs_bytes: usize = pvs
            .clusters
            .iter()
            .fold(BTreeMap::new(), |mut m, a| {
                m.entry(Arc::as_ptr(a) as usize).or_insert(
                    (a.opaque.capacity() + a.alpha.capacity()) * std::mem::size_of::<usize>(),
                );
                m
            })
            .values()
            .sum();
        let inventory = serde_json::json!({"groups":groups.len(),"vertices":vertices,"layer_triangles":triangles,"gpu_geometry_bytes":bytes,"cpu_cluster_list_payload_bytes":pvs_bytes,
            "clusters":pvs.clusters.len(),"unique_cluster_lists":unique,"pipelines":16,"uniform_staging_capacity_bytes":groups.len()*std::mem::size_of::<Prepared>(),"dropped_layers":dropped_layers,
            "frozen_ambient_models":frozen_models,"frozen_deforms":frozen_deforms,"dynamic_world_lights":0,"geometry_uploads_per_frame":0,"load_groups_ms":groups_ms,"load_recipes_ms":recipes_ms,"load_chunks_ms":chunks_ms,"source_chunks":source_chunks,"load_pipelines_upload_ms":upload_ms,"load_pvs_ms":pvs_ms,"load_total_ms":started.elapsed().as_secs_f64()*1000.,"pvs_cache_status":cache_status,"pvs_method":c.pvs_method});
        println!("[lg/vita-renderer] loaded {inventory}");
        *INVENTORY.lock().unwrap() = Some(inventory);
        // PVS provenance is no longer needed after compilation.
        for g in &mut groups {
            g.faces = Vec::new();
        }
        Ok(Self {
            alpha_order: Vec::with_capacity(groups.len()),
            prepared: Vec::with_capacity(groups.len()),
            fx_gpu: None,
            groups,
            pvs,
            pipelines,
            shader,
            sky_pipeline,
            sky_shader,
            sky_bindings,
        })
    }
    pub(super) fn draw(
        &mut self,
        scene: &Scene,
        eye: Vec3,
        time: f32,
        fullbright: bool,
        alpha: bool,
        transforms: &[(usize, Vec3, Quat)],
    ) -> super::DrawStats {
        let _span = crate::frame_profile::span("vita_world_total");
        count("vita_world_geometry_upload_bytes", 0);
        count("vita_world_retransformed_vertices", 0);
        count("vita_world_rebuilt_indices", 0);
        count("vita_world_sorted_triangles", 0);
        let visibility_span = crate::frame_profile::span("vita_cluster_frustum");
        let projection = crate::render_fx::projection().expect("world camera projection");
        let cluster = scene.map.visibility.cluster(eye, &scene.map.planes);
        let lists = self.pvs.get(cluster);
        let ids = if alpha { &lists.alpha } else { &lists.opaque };
        self.alpha_order.clear();
        for &id in ids {
            let g = &self.groups[id];
            let Some(model) = model_matrix(&g.recipe, time, transforms) else {
                continue;
            };
            if in_frustum(projection * model, g.min, g.max) {
                self.alpha_order.push((
                    if crate::render_fx::retained_active() {
                        crate::render_fx::group_depth(model.transform_point3(g.center))
                    } else {
                        (model.transform_point3(g.center) - eye).length_squared()
                    },
                    id,
                ));
            }
        }
        drop(visibility_span);
        if alpha && config().sort_alpha && !crate::render_fx::retained_active() {
            let _sort = crate::frame_profile::span("vita_alpha_group_sort");
            self.alpha_order
                .sort_unstable_by(|a, b| b.0.total_cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
        }
        self.prepared.clear();
        {
            let _parameters = crate::frame_profile::span("vita_world_parameters");
            for &(depth, id) in &self.alpha_order {
                let g = &self.groups[id];
                let model = model_matrix(&g.recipe, time, transforms).unwrap();
                let mut u = uniforms(
                    &g.recipe,
                    time,
                    eye,
                    model,
                    projection,
                    fullbright,
                    scene.atlas_grid,
                );
                u.atmosphere(&scene.atmosphere, eye);
                let texture = g.recipe.textures[g.recipe.stage.frame(time)].raw_miniquad_id();
                self.prepared.push(Prepared {
                    depth,
                    group: id,
                    texture,
                    uniforms: u,
                });
            }
        }
        if alpha && crate::render_fx::retained_active() {
            // Keep immutable bindings and uniforms until all actor/effect groups are known.
            let stats = super::DrawStats {
                culled: self
                    .pvs
                    .unknown
                    .alpha
                    .len()
                    .saturating_sub(self.prepared.len()),
                batches: self.prepared.len(),
                triangles: self
                    .prepared
                    .iter()
                    .map(|p| self.groups[p.group].indices as usize / 3)
                    .sum(),
                ..Default::default()
            };
            count("vita_world_culled_groups", stats.culled as u64);
            return stats;
        }
        // Direct actor packets never queue Macroquad geometry.
        if !world_only() && !crate::render_fx::retained_active() {
            let _flush = crate::frame_profile::span("gl_flush");
            let _site = crate::frame_profile::span("flush_vita_world_interop");
            #[cfg(target_os = "vita")]
            crate::frame_profile::flush_boundary("flush_vita_world_interop");
            unsafe {
                get_internal_gl().flush();
            }
            count("vita_world_interop_flushes", 1);
        }
        unsafe {
            get_internal_gl()
                .quad_context
                .begin_default_pass(PassAction::Nothing);
        }
        let mut stats = super::DrawStats {
            culled: (if alpha {
                self.pvs.unknown.alpha.len()
            } else {
                self.pvs.unknown.opaque.len()
            })
            .saturating_sub(self.alpha_order.len()),
            ..Default::default()
        };
        if !alpha && config().sky {
            let bg = scene.atmosphere.background();
            let sky = [
                [bg.r, bg.g, bg.b, 1.],
                [bg.r * 1.5, bg.g * 1.5, bg.b * 1.5, 1.],
            ];
            let gl = unsafe { get_internal_gl() };
            let ctx = gl.quad_context;
            ctx.apply_pipeline(&self.sky_pipeline);
            ctx.apply_bindings(&self.sky_bindings);
            ctx.apply_uniforms(UniformsSource::table(&sky));
            ctx.draw(0, 3, 1);
            count("vita_world_gpu_draws", 1);
            count("vita_world_sky_draws", 1);
        }
        unsafe {
            macroquad::miniquad::gl::glDepthMask(if alpha { 0 } else { 1 });
        }
        let mut previous_pipeline = None;
        let mut previous_texture = None;
        let submit_span = crate::frame_profile::span("vita_world_submit");
        for prepared in &self.prepared {
            let g = &mut self.groups[prepared.group];
            let texture = prepared.texture;
            g.bindings.images[0] = texture;
            let pi = g.recipe.blend * 3 + g.recipe.cull;
            let gl = unsafe { get_internal_gl() };
            let ctx = gl.quad_context;
            ctx.apply_pipeline(&self.pipelines[pi]);
            ctx.apply_bindings(&g.bindings);
            ctx.apply_uniforms(UniformsSource::table(&prepared.uniforms));
            ctx.draw(0, g.indices, 1);
            if previous_pipeline != Some(pi) {
                count("vita_world_pipeline_changes", 1);
                previous_pipeline = Some(pi);
            }
            if previous_texture != Some(texture) {
                count("vita_world_texture_changes", 1);
                previous_texture = Some(texture);
            }
            count("vita_world_gpu_draws", 1);
            count(
                "vita_world_uniform_bytes",
                std::mem::size_of::<shader::Uniforms>() as u64,
            );
            count(
                if alpha {
                    "vita_world_alpha_triangles"
                } else {
                    "vita_world_opaque_triangles"
                },
                g.indices as u64 / 3,
            );
            stats.batches += 1;
            stats.triangles += g.indices as usize / 3;
            #[cfg(target_os = "vita")]
            {
                crate::frame_profile::record_batch(
                    g.recipe.model.is_none() && g.recipe.motion.is_none(),
                    false,
                );
                crate::frame_profile::record_world_submission();
                // Submitted here means drawn from an existing VBO; no CPU geometry upload.
                crate::frame_profile::record_geometry(
                    g.indices as usize / 3,
                    0,
                    g.recipe.model.is_none(),
                );
            }
        }
        drop(submit_span);
        count("vita_world_culled_groups", stats.culled as u64);
        unsafe {
            macroquad::miniquad::gl::glDepthMask(1);
        }
        unsafe {
            get_internal_gl().quad_context.end_render_pass();
        }
        // No flush_depth_begin/end: draws have already reached miniquad/vitaGL.
        stats
    }
    pub(super) fn begin_fx(&mut self) {
        self.prepared.clear();
    }
    pub(super) fn finish_fx(&mut self, interop_flush: bool) -> (usize, usize) {
        let mut gpu = self.fx_gpu.take().unwrap_or_else(|| {
            crate::render_fx::retained::Gpu::new().expect("retained actor shader")
        });
        let world = self
            .prepared
            .iter()
            .enumerate()
            .map(|(id, p)| (p.depth, id))
            .collect::<Vec<_>>();
        let result = crate::render_fx::retained::finish(&mut gpu, &world, interop_flush, |id| {
            let _span = crate::frame_profile::span("vita_world_alpha_submit");
            let p = &self.prepared[id];
            let g = &mut self.groups[p.group];
            g.bindings.images[0] = p.texture;
            let ctx = unsafe { get_internal_gl() }.quad_context;
            ctx.apply_pipeline(&self.pipelines[g.recipe.blend * 3 + g.recipe.cull]);
            ctx.apply_bindings(&g.bindings);
            ctx.apply_uniforms(UniformsSource::table(&p.uniforms));
            ctx.draw(0, g.indices, 1);
            count("vita_world_gpu_draws", 1);
            count("vita_world_alpha_triangles", g.indices as u64 / 3);
            count(
                "vita_world_uniform_bytes",
                std::mem::size_of::<shader::Uniforms>() as u64,
            );
            // Every alternation can change state; this counter is conservative.
            count("vita_world_alpha_state_applications", 1);
            #[cfg(target_os = "vita")]
            {
                crate::frame_profile::record_batch(
                    g.recipe.model.is_none() && g.recipe.motion.is_none(),
                    false,
                );
                crate::frame_profile::record_world_submission();
                crate::frame_profile::record_geometry(
                    g.indices as usize / 3,
                    0,
                    g.recipe.model.is_none(),
                );
            }
        });
        self.prepared.clear();
        self.fx_gpu = Some(gpu);
        result
    }
}
impl Drop for Renderer {
    fn drop(&mut self) {
        let gl = unsafe { get_internal_gl() };
        let ctx = gl.quad_context;
        for g in &self.groups {
            ctx.delete_buffer(g.bindings.vertex_buffers[0]);
            ctx.delete_buffer(g.bindings.index_buffer);
        }
        for p in &self.pipelines {
            ctx.delete_pipeline(*p);
        }
        ctx.delete_pipeline(self.sky_pipeline);
        ctx.delete_shader(self.shader);
        ctx.delete_shader(self.sky_shader);
        ctx.delete_buffer(self.sky_bindings.vertex_buffers[0]);
        ctx.delete_buffer(self.sky_bindings.index_buffer);
    }
}
fn count(name: &'static str, n: u64) {
    #[cfg(target_os = "vita")]
    crate::frame_profile::counts(&[(name, n)]);
    #[cfg(not(target_os = "vita"))]
    crate::render_fx::retained::count(name,n);
}
fn model_matrix(recipe: &Recipe, time: f32, transforms: &[(usize, Vec3, Quat)]) -> Option<Mat4> {
    let rigid = if let Some(id) = recipe.model {
        let (_, p, r) = transforms.iter().find(|t| t.0 == id)?;
        Mat4::from_rotation_translation(*r, *p)
    } else {
        Mat4::IDENTITY
    };
    Some(
        rigid
            * recipe
                .motion
                .as_ref()
                .map_or(Mat4::IDENTITY, |m| m.matrix(time)),
    )
}
fn in_frustum(m: Mat4, min: Vec3, max: Vec3) -> bool {
    let mut outside = [true; 6];
    for x in [min.x, max.x] {
        for y in [min.y, max.y] {
            for z in [min.z, max.z] {
                let p = m * vec4(x, y, z, 1.);
                for (all, out) in outside.iter_mut().zip([
                    p.x < -p.w,
                    p.x > p.w,
                    p.y < -p.w,
                    p.y > p.w,
                    p.z < -p.w,
                    p.z > p.w,
                ]) {
                    *all &= out;
                }
            }
        }
    }
    !outside.into_iter().any(|v| v)
}
fn uv_matrices(stage: &Stage, time: f32) -> (Mat4, Mat4, [f32; 4]) {
    let mut m = Mat4::IDENTITY;
    let mut after = Mat4::IDENTITY;
    let mut turb = [0.; 4];
    let translate = |v: Vec2| Mat4::from_translation(v.extend(0.));
    for modifier in &stage.mods {
        let next = match modifier {
            TcMod::Transform([a, b, c, d, x, y]) => Mat4::from_cols(
                vec4(*a, *c, 0., 0.),
                vec4(*b, *d, 0., 0.),
                Vec4::Z,
                vec4(*x, *y, 0., 1.),
            ),
            TcMod::Scale(v) => Mat4::from_scale(v.extend(1.)),
            TcMod::Scroll(v) => translate(*v * time),
            TcMod::Rotate(deg) => {
                translate(Vec2::splat(0.5))
                    * Mat4::from_rotation_z((-deg * time).to_radians())
                    * translate(Vec2::splat(-0.5))
            }
            TcMod::Stretch(w) => {
                translate(Vec2::splat(0.5))
                    * Mat4::from_scale(vec3(
                        1. / w.sample(time).abs().max(0.01),
                        1. / w.sample(time).abs().max(0.01),
                        1.,
                    ))
                    * translate(Vec2::splat(-0.5))
            }
            TcMod::Turb([_, amp, phase, freq]) => {
                turb = [*amp, *phase, time, *freq];
                after = Mat4::IDENTITY;
                continue;
            }
        };
        m = next * m;
        after = next * after;
    }
    (m, after, turb)
}
fn uniforms(
    recipe: &Recipe,
    time: f32,
    eye: Vec3,
    model: Mat4,
    projection: Mat4,
    fullbright: bool,
    atlas: Vec2,
) -> shader::Uniforms {
    let s = &recipe.stage;
    let (uv, uv_after, turb) = uv_matrices(s, time);
    let gain = s.constant_rgb.unwrap_or(Vec3::ONE)
        * s.rgb.as_ref().map_or(1., |w| w.sample(time)).clamp(0., 2.);
    let mut u = shader::Uniforms {
        mvp: (projection * model).to_cols_array(),
        model: model.to_cols_array(),
        uv: uv.to_cols_array(),
        uv_after: uv_after.to_cols_array(),
        eye: eye.extend(1.).to_array(),
        surface: [
            f32::from(s.clamp),
            f32::from(recipe.lit),
            s.alpha_test as f32,
            f32::from(fullbright),
        ],
        gain: gain
            .extend(
                s.alpha
                    .as_ref()
                    .map(|w| w.sample(time))
                    .or(s.constant_alpha)
                    .unwrap_or(1.)
                    .clamp(0., 1.),
            )
            .to_array(),
        alpha: [
            s.alpha_reference,
            f32::from(s.vertex_alpha),
            match recipe.blend {
                2 => 1.,
                3 => 2.,
                4 => 3.,
                _ => 0.,
            },
            0.,
        ],
        dot_alpha: s
            .dot_alpha
            .map_or([0.; 4], |r| [r.x, r.y, f32::from(s.inverse_dot), 1.]),
        vector_s: s
            .vector_uv
            .map_or([0., 0., 0., if s.environment { 2. } else { 0. }], |v| {
                v.0.extend(1.).to_array()
            }),
        vector_t: s.vector_uv.map_or([0.; 4], |v| v.1.extend(0.).to_array()),
        turb,
        deform: [0., 0., time, 0.],
        wave: [0.; 4],
        move_dir: [0.; 4],
        atlas: [atlas.x, atlas.y, 0., 0.],
        fog_eye: [0.; 3],
        distance_fog: [0.; 4],
        liquid_fog: [0.; 4],
        fog_a_min: [0.; 3],
        fog_a_max: [0.; 3],
        fog_a: [0.; 4],
        fog_b_min: [0.; 3],
        fog_b_max: [0.; 3],
        fog_b: [0.; 4],
    };
    match &recipe.deform {
        Some(Deform::Wave(spread, w)) => {
            u.deform[0] = 1.;
            u.deform[1] = *spread;
            u.wave = w.values;
            u.deform[3] = wave_kind(&w.kind);
        }
        Some(Deform::Move(v, w)) => {
            u.deform[0] = 2.;
            u.move_dir = v.extend(0.).to_array();
            u.wave = w.values;
            u.deform[3] = wave_kind(&w.kind);
        }
        Some(Deform::Bulge(v)) => {
            u.deform[0] = 3.;
            u.wave = [v[0], v[1], v[2], 0.];
        }
        _ => {}
    }
    u
}
fn wave_kind(kind: &str) -> f32 {
    match kind {
        "sin" => 0.,
        "triangle" => 1.,
        "square" => 2.,
        "sawtooth" => 3.,
        "inversesawtooth" => 4.,
        _ => 5.,
    }
}

/// Standalone host viewer for the reference pose. Does not create gameplay/save state.
pub(crate) async fn visualizer(
    mut assets: crate::assets::Assets,
    options: crate::Options,
) -> Result<()> {
    use macroquad::camera::Camera;
    ensure!(
        options.map == "gvillage",
        "jalon 2 visualizer supports gvillage"
    );
    let mut scene = Scene::load_for(&mut assets, &options.map, options.difficulty)?;
    let mut interactions = crate::interaction::Interactions::load(&scene.map)?;
    interactions.set_entry(
        &mut assets,
        &scene.map,
        &options.map,
        options.entry.as_deref(),
    )?;
    let transforms = interactions.transforms();
    let yaw = 4.692389_f32;
    let direction = vec3(yaw.cos(), yaw.sin(), 0.);
    let (eye, target, _) = crate::camera::Follow::default().update(
        &scene.world,
        vec3(-4188., 6004., 32.03125),
        direction,
        132.,
        1. / 60.,
    );
    let eye = options.start_at.unwrap_or(eye);
    let target = if options.start_at.is_some() {
        eye + direction
    } else {
        target
    };
    let mut camera = Camera3D {
        position: eye,
        target,
        up: Vec3::Z,
        fovy: 75_f32.to_radians(),
        z_near: 2.,
        z_far: 30000.,
        ..Default::default()
    };
    record_view(&camera);
    let look = (target - eye).normalize_or_zero();
    let mut free_yaw = look.y.atan2(look.x);
    let mut free_pitch = look.z.asin();
    let mut previous_mouse = mouse_position();
    for frame in 0..options.frames.unwrap_or(u64::MAX) {
        if is_key_down(KeyCode::Escape) {
            break;
        }
        if options.frames.is_none() {
            let dt = get_frame_time().min(0.1);
            let mouse = mouse_position();
            let (dx, dy) = if is_mouse_button_down(MouseButton::Left) {
                (mouse.0 - previous_mouse.0, mouse.1 - previous_mouse.1)
            } else {
                (0., 0.)
            };
            previous_mouse = mouse;
            free_yaw += dt
                * 1.7
                * (f32::from(is_key_down(KeyCode::Right)) - f32::from(is_key_down(KeyCode::Left)))
                + dx * 0.004;
            free_pitch = (free_pitch
                + dt * 1.7
                    * (f32::from(is_key_down(KeyCode::Up))
                        - f32::from(is_key_down(KeyCode::Down)))
                - dy * 0.004)
                .clamp(-1.4, 1.4);
            let direction = vec3(
                free_pitch.cos() * free_yaw.cos(),
                free_pitch.cos() * free_yaw.sin(),
                free_pitch.sin(),
            );
            let right = direction.cross(Vec3::Z).normalize_or_zero();
            let offset = (direction
                * (f32::from(is_key_down(KeyCode::W)) - f32::from(is_key_down(KeyCode::S)))
                + right
                    * (f32::from(is_key_down(KeyCode::D)) - f32::from(is_key_down(KeyCode::A)))
                + Vec3::Z
                    * (f32::from(is_key_down(KeyCode::E)) - f32::from(is_key_down(KeyCode::Q))))
                * dt
                * 350.;
            camera.position += offset;
            camera.target = camera.position + direction;
        }
        clear_background(scene.atmosphere.background());
        set_camera(&camera);
        crate::render_fx::set_projection(camera.matrix());
        #[cfg(not(target_os = "vita"))]
        crate::render_fx::retained::host_reset();
        let time = frame as f32 / 60.;
        scene.draw(camera.position, time, false, false, &transforms);
        if scene.retained_active() {
            scene.draw(camera.position, time, false, true, &transforms);
        } else {
            super::depth_read_only(|| scene.draw(camera.position, time, false, true, &transforms));
        }
        set_default_camera();
        if options.frames.is_some_and(|n| frame + 1 == n) {
            println!("[lg/vita-renderer] visualizer eye={:?} viewport={}x{} layers={} draws={} triangles={}",camera.position,screen_width(),screen_height(),config().max_layers,scene.stats.batches,scene.stats.triangles);
            println!("[lg/vita-renderer] report {}", report());
            if let Some(path) = &options.capture {
                crate::viewer::save_capture(path)?;
            }
        }
        next_frame().await;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn affine_and_turbulence_match_original_stage_order() {
        let stage = Stage {
            mods: vec![
                TcMod::Scroll(vec2(0.2, -0.3)),
                TcMod::Turb([0., 0.15, 0.4, 0.7]),
                TcMod::Rotate(18.),
                TcMod::Scale(vec2(2., 0.8)),
                TcMod::Transform([1., 0.1, -0.2, 1., 0.4, -0.1]),
            ],
            ..Default::default()
        };
        let p = vec3(111., -400., 30.);
        let source = vec2(0.1, 0.7);
        for time in [0., 0.3, 3., 20.] {
            let (m, after, t) = uv_matrices(&stage, time);
            let ripple = vec2(
                (((p.x + p.z) / 1024. + t[1] + t[2] * t[3]) * std::f32::consts::TAU).sin(),
                ((p.y / 1024. + t[1] + t[2] * t[3]) * std::f32::consts::TAU).sin(),
            ) * t[0];
            let actual = m.transform_point3(source.extend(0.)).truncate()
                + after.transform_vector3(ripple.extend(0.)).truncate();
            assert!(actual.distance(stage.uv(source, p, time)) < 0.00001);
        }
    }
    #[test]
    fn packed_vertex_and_uniform_layout_match_shader_metadata() {
        assert_eq!(std::mem::size_of::<WorldVertex>(), 52);
        assert_eq!(std::mem::offset_of!(WorldVertex, color), 20);
        assert_eq!(std::mem::offset_of!(WorldVertex, lightmap), 24);
        assert_eq!(std::mem::offset_of!(WorldVertex, normal), 40);
        let size: usize = shader::meta()
            .uniforms
            .uniforms
            .iter()
            .map(|d| d.uniform_type.size() * d.array_count)
            .sum();
        assert_eq!(std::mem::size_of::<shader::Uniforms>(), size);
    }
    #[test]
    fn settings_bound_gpu_passes_and_reject_bad_values() {
        assert!(Config::default().validate().is_ok());
        for layers in [0, 3, usize::MAX] {
            assert!(Config {
                max_layers: layers,
                ..Config::default()
            }
            .validate()
            .is_err());
        }
        assert!(Config {
            cell_size: f32::NAN,
            ..Config::default()
        }
        .validate()
        .is_err());
        assert!(serde_json::from_str::<Config>(r#"{"unknown":true}"#).is_err());
        for experiment in ["unchanged","gpu111","gpu222"] {
            assert!(Config {gpu_experiment:experiment.into(),..Config::default()}.validate().is_ok());
        }
        assert!(Config {gpu_experiment:"gpu444".into(),..Config::default()}.validate().is_err());
        assert!(Config {pvs_method:"invalid".into(),..Config::default()}.validate().is_err());
    }
    #[test]
    fn frustum_fails_open_for_crossing_boxes() {
        assert!(in_frustum(
            Mat4::IDENTITY,
            Vec3::splat(-2.),
            Vec3::splat(2.)
        ));
        assert!(!in_frustum(
            Mat4::IDENTITY,
            Vec3::splat(2.),
            Vec3::splat(3.)
        ));
    }
    #[test]
    fn diagnostic_mask_only_applies_to_renderbench() {
        use crate::frame_profile::Mode;
        for mode in [
            Mode::Full,
            Mode::NoPreview,
            Mode::NoPrep,
            Mode::NoWorld,
            Mode::Bench,
            Mode::FullBench,
            Mode::Clk444,
            Mode::RenderBench,
        ] {
            assert_eq!(mask_in_mode(true, mode), mode == Mode::RenderBench);
            assert!(!mask_in_mode(false, mode));
        }
    }
    #[test]
    fn pvs_unknown_a_b_a_and_interned_lists() {
        let v = crate::visibility::tests::two_rooms();
        let pvs = Pvs::compile(3, |c| {
            let faces = v.faces(c);
            Lists {
                opaque: (0..2)
                    .filter(|&id| faces.as_ref().is_none_or(|f| f[id]))
                    .collect(),
                alpha: vec![],
            }
        });
        for (c, ids) in [
            (None, vec![0, 1]),
            (Some(0), vec![0]),
            (Some(1), vec![1]),
            (Some(0), vec![0]),
            (Some(100), vec![0, 1]),
        ] {
            assert_eq!(pvs.get(c).opaque, ids);
        }
        let same = Pvs::compile(2, |_| Lists {
            opaque: vec![3, 7],
            alpha: vec![1],
        });
        assert!(Arc::ptr_eq(&same.clusters[0], &same.clusters[1]));
    }
    #[test]
    fn immutable_chunks_keep_u16_indices_in_range() {
        let mut g = Geometry::default();
        let v = WorldVertex {
            position: [0.; 3],
            uv: [0.; 2],
            color: [255; 4],
            lightmap: [0.; 4],
            normal: [0., 0., 1.],
        };
        g.vertices = vec![v; 59997];
        assert!(g.fits(&[0, 1, 2]));
        assert!(!g.fits(&[0, 1, 2, 0, 1, 2]));
        g.vertices.clear();
        g.indices = vec![0; 179997];
        assert!(g.fits(&[0, 1, 2]));
        assert!(!g.fits(&[0, 1, 2, 0, 1, 2]));
    }
    #[test]
    fn cutout_preserves_depth_writes_and_ordinary_alpha_does_not() {
        assert!(opaque_stage(&Stage::default()));
        let mut s = Stage {
            blend: Blend::Alpha,
            depth_write: true,
            alpha_test: 2,
            ..Default::default()
        };
        assert!(opaque_stage(&s));
        s.alpha_test = 0;
        assert!(!opaque_stage(&s));
        s.alpha_test = 2;
        s.depth_write = false;
        assert!(!opaque_stage(&s));
    }
}
