//! Direct, retained world draws. CPU geometry and PVS compilation happen at load time.
mod shader;
mod scale;
mod depth_sentinel;
mod pvs_cache;
mod state_cache;
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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Clocks {
    #[default]
    Max,
    Stock,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub renderer: String,
    /// Vita startup policy; stock preserves the clocks inherited from the system.
    pub clocks: Clocks,
    /// Clean thumbnails during play; diagnostic modes retain their own policy.
    pub save_preview: String,
    /// auto/on correct Vita's two-readback orientation; off restores J10.
    pub preview_flip: String,
    pub world_state_cache: bool,
    pub world_view_reuse: bool,
    pub world_parameter_reuse: bool,
    /// Cache view/model fields independently of the advancing material clock.
    pub world_parameter_time_split: bool,
    pub uniform_field_cache: bool,
    /// Diagnostic per-draw clocks; normal play keeps only phase/tail timings.
    pub render_draw_timing: bool,
    pub worker_probe: bool,
    pub skin_workers: usize,
    /// Gather final poses before world submission and collect before characters.
    pub skin_overlap: bool,
    pub lights_early_out: bool,
    pub fx_submit_state_cache: bool,
    pub actor_layer_reuse: bool,
    /// None selects light in normal play and full in diagnostic modes.
    pub profiler: Option<crate::light_profile::Level>,
    pub skin_worker_bench: bool,
    pub fx_prepare_cache: bool,
    pub particle_producer_reuse: bool,
    /// Publish world puffs once per final joint-alpha material batch.
    pub world_puff_batches: bool,
    pub fx_merge_in_place: bool,
    pub fx_worker_prep: bool,
    pub frame_phase_probe: bool,
    pub mover_far_lazy: bool,
    pub mover_lazy_publish: bool,
    pub pickup_fx_cache: bool,
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
    pub fx_batching:String,
    pub fx_streaming:String,
    /// Five slots exceed the pinned driver four-frame in-flight window.
    pub fx_ring_safe: bool,
    pub fx_merge_cache:bool,
    pub fx_upload_cache:bool,
    pub fx_max_layers:usize,
    pub actor_fast_path:bool,
    pub actor_layer_cache:bool,
    pub actor_shared_geometry:bool,
    pub actor_batching:bool,
    pub actor_frustum_cull:bool,
    /// Matrix skinning changes float rounding: opt in after console comparison.
    pub bone_palette: bool,
    /// Conservative skin bounds; late overlays and normal-driven layers opt out.
    pub skin_frustum_cull: bool,
    /// Conservative center/extent clip-plane test and identity bounds shortcut.
    pub fast_frustum: bool,
    pub flat_actor_light: bool,
    pub max_texture_dimension: Option<u16>,
    pub draw_distance_cap: Option<f32>,
    pub simple_fog: bool,
    pub simple_dynamic_lights: bool,
    pub gpu_state_probe: bool,
    pub gpu_index_dedupe: bool,
    pub gpu_uniform_dedupe: bool,
    /// SDK logo thread is disabled by default; the LG opening film is unaffected.
    pub startup_splash:bool,
    /// Persistent binary pairs; process deduplication remains active when off.
    pub shader_cache: String,
    /// Optional cache of exact device-compressed immutable pixels.
    pub texture_cache: String,
    pub route_heading_lock:bool,
    /// Input-only diagnostic: recover recorded world wishes without camera waits.
    pub route_world_wish: bool,
    pub pvs_cache: bool,
    pub pvs_method: String,
    pub pvs_verify: bool,
    pub gpu_experiment: String,
    pub world_viewport_scale: f32,
    pub render_scale:f32,
    /// Readback diagnostic, excluded from production timing.
    pub depth_sentinel: bool,
    /// Preserve scaled world depth until the direct FX pass completes.
    pub scaled_depth_fix: bool,
    pub msaa_samples:i32,
    pub quality:String,
    pub opaque_order:String,
    pub world_alpha:bool,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            renderer: "vita".into(),
            clocks: Clocks::Max,
            save_preview: "throttled".into(),
            preview_flip: "auto".into(),
            world_state_cache: true,
            world_view_reuse: true,
            world_parameter_reuse: true,
            world_parameter_time_split: true,
            uniform_field_cache: true,
            render_draw_timing: false,
            worker_probe: false, skin_workers: 2, skin_overlap: true,
            lights_early_out: true, fx_submit_state_cache: true, actor_layer_reuse: false,
            profiler: None, skin_worker_bench: false, fx_prepare_cache: true,
            particle_producer_reuse: true, world_puff_batches: true, fx_merge_in_place: true, fx_worker_prep: true,
            frame_phase_probe: false,
            mover_far_lazy: true,
            mover_lazy_publish: true,
            pickup_fx_cache: true,
            world_only: false,
            max_layers: 2,
            sort_alpha: true,
            sky: true,
            cell_size: 256.,
            alpha_interop: true,
            actor_max_layers: 2,
            merge_fx: true,
            fx_batching:"adjacent".into(),fx_streaming:"orphan".into(),fx_merge_cache:true,fx_upload_cache:false,fx_max_layers:2,actor_fast_path:true,
            fx_ring_safe: true,
            actor_layer_cache:true,actor_shared_geometry:true,actor_batching:true,actor_frustum_cull:false,
            bone_palette: false,
            skin_frustum_cull: true,
            fast_frustum: true,
            flat_actor_light: false, max_texture_dimension: None,
            draw_distance_cap: None, simple_fog: false, simple_dynamic_lights: false,
            gpu_state_probe: false,
            gpu_index_dedupe: false, gpu_uniform_dedupe: false,
            startup_splash:false,
            shader_cache:"on".into(),
            texture_cache:"off".into(),
            route_heading_lock:true,
            route_world_wish: false,
            pvs_cache: false,
            pvs_method: "sources".into(),
            pvs_verify: false,
            gpu_experiment: "unchanged".into(),
            world_viewport_scale: 1.,
            depth_sentinel: false, scaled_depth_fix: true,
            render_scale:1.,msaa_samples:1,quality:"full".into(),opaque_order:"front_to_back".into(),world_alpha:true,
        }
    }
}
/// The SDK starts its splash thread in the middle of vglInitWithCustomSizes,
/// while the caller still allocates shader-patcher resources. Suppress that
/// second allocator owner unless explicitly requesting the legacy diagnostic.
/// This linker hook changes no vitaGL archive or dependency pin.
#[cfg(target_os="vita")]
#[no_mangle]
pub extern "C" fn __wrap_invoke_splashscreen() {
    if config().startup_splash {
        eprintln!("[lg/startup] SDK splash legacy thread requested");
        extern "C" {fn __real_invoke_splashscreen();}
        unsafe {__real_invoke_splashscreen();}
    } else {
        eprintln!("[lg/startup] SDK splash suppressed; initialization allocator owner=main");
    }
}
impl Config {
    fn validate(&self) -> Result<()> {
        ensure!(!self.skin_worker_bench || self.skin_workers == 0, "skin_worker_bench requires skin_workers=0 to isolate overlap");
        ensure!(!self.skin_overlap || self.skin_workers > 0, "skin_overlap requires skin_workers=1 or 2");
        ensure!(self.skin_workers <= 2, "skin_workers must be 0, 1 or 2");
        ensure!(matches!(self.save_preview.as_str(), "throttled" | "every_frame"), "save_preview must be throttled or every_frame");
        ensure!(matches!(self.preview_flip.as_str(), "auto" | "on" | "off"), "preview_flip must be auto, on or off");
        ensure!(self.max_texture_dimension.is_none_or(|n| (16..=512).contains(&n)), "max_texture_dimension must be null or 16..512");
        ensure!(self.draw_distance_cap.is_none_or(|n| n.is_finite() && (128. ..=30000.).contains(&n)), "draw_distance_cap must be null or 128..30000");
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
        ensure!(matches!(self.texture_cache.as_str(),"on"|"off"),"texture_cache must be on or off");
        ensure!(matches!(self.shader_cache.as_str(),"on"|"off"),"shader_cache must be on or off");
        ensure!(matches!(self.fx_batching.as_str(),"none"|"adjacent"|"material"),"invalid fx_batching");
        ensure!(matches!(self.fx_streaming.as_str(),"orphan"|"ring"),"invalid fx_streaming");
        ensure!((1..=2).contains(&self.fx_max_layers),"invalid fx_max_layers");
        ensure!(self.world_viewport_scale.is_finite() && (0.25..=1.).contains(&self.world_viewport_scale),"world_viewport_scale must be between 0.25 and 1");
        ensure!(self.render_scale.is_finite()&&(0.25..=1.).contains(&self.render_scale),"invalid render_scale");
        ensure!(matches!(self.msaa_samples,1|2|4),"msaa_samples must be 1, 2 or 4");
        ensure!(self.render_scale==1. || self.msaa_samples==1,"scaled target requires msaa_samples=1");
        ensure!(matches!(self.quality.as_str(),"full"|"low"),"quality must be full or low");
        ensure!(matches!(self.opaque_order.as_str(),"material"|"front_to_back"),"invalid opaque_order");
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
        let start=std::time::Instant::now();
        eprintln!("[lg/startup] renderer_config read begin path={CONFIG_PATH}");
        let parsed = (|| -> Result<Config> {
            let bytes = match std::fs::read(CONFIG_PATH) {
                Ok(bytes) => bytes,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Config::default()),
                Err(e) => return Err(e.into()),
            };
            eprintln!("[lg/startup] renderer_config read done bytes={} elapsed_ms={:.3}; parse begin",bytes.len(),start.elapsed().as_secs_f64()*1000.);
            let c: Config = serde_json::from_slice(&bytes)?;
            eprintln!("[lg/startup] renderer_config parse done elapsed_ms={:.3}; validate begin",start.elapsed().as_secs_f64()*1000.);
            c.validate()?;
            eprintln!("[lg/startup] renderer_config validate done elapsed_ms={:.3}",start.elapsed().as_secs_f64()*1000.);
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
        #[cfg(target_os = "vita")]
        apply_startup_clocks(c.clocks);
        apply_gpu_experiment(&c);
        c
    })
}
/// Called once by the config initializer, before archive mounting on Vita.
#[cfg(target_os = "vita")]
fn apply_startup_clocks(mode: Clocks) {
    extern "C" {
        fn scePowerSetArmClockFrequency(freq: i32) -> i32;
        fn scePowerSetBusClockFrequency(freq: i32) -> i32;
        fn scePowerSetGpuClockFrequency(freq: i32) -> i32;
        fn scePowerSetGpuXbarClockFrequency(freq: i32) -> i32;
        fn scePowerGetArmClockFrequency() -> i32;
        fn scePowerGetBusClockFrequency() -> i32;
        fn scePowerGetGpuClockFrequency() -> i32;
        fn scePowerGetGpuXbarClockFrequency() -> i32;
    }
    unsafe {
        let rc: &[i32] = match mode {
            Clocks::Max => &[
                scePowerSetArmClockFrequency(444),
                scePowerSetBusClockFrequency(222),
                scePowerSetGpuClockFrequency(222),
                scePowerSetGpuXbarClockFrequency(166),
            ],
            Clocks::Stock => &[],
        };
        let actual = [
            scePowerGetArmClockFrequency(),
            scePowerGetBusClockFrequency(),
            scePowerGetGpuClockFrequency(),
            scePowerGetGpuXbarClockFrequency(),
        ];
        let name = match mode {
            Clocks::Max => "max",
            Clocks::Stock => "stock",
        };
        eprintln!("[lg/startup] clocks mode={name} rc={rc:?} actual={actual:?}");
        if mode == Clocks::Max && (rc.iter().any(|&code| code < 0) || actual != [444, 222, 222, 166]) {
            eprintln!("[lg/startup] max clocks request not satisfied; inspect actual clocks");
        }
    }
}
/// Named GPU-only experiment; default does not call a setter or alter ARM/bus/xbar.
fn apply_gpu_experiment(c:&Config) {
    let requested = match c.gpu_experiment.as_str() {
        "gpu111" => 111,
        "gpu222" => 222,
        _ => return,
    };
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
        let before=read();
        let rc=scePowerSetGpuClockFrequency(requested);let after=read();
        println!("[lg/vita-renderer] gpu experiment {} rc={rc} before={before:?} after={after:?}",c.gpu_experiment);
        if rc<0 || after[2]!=requested || before[0]!=after[0] || before[1]!=after[1] || before[3]!=after[3] {
            eprintln!("[lg/vita-renderer] GPU experiment did not satisfy its clock contract; inspect actual window clocks");
        }
    }
    #[cfg(not(target_os="vita"))]
    println!("[lg/vita-renderer] gpu experiment {} ({requested} MHz) ignored on host",c.gpu_experiment);
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
/// Diagnostic world-only probe. Full-frame actor/HUD alignment remains full-size.
pub(crate) fn world_viewport() -> Option<(i32,i32,i32,i32)> {
    if !world_only() || config().world_viewport_scale==1. { return None; }
    Some((0,0,(screen_width()*config().render_scale*config().world_viewport_scale).round() as i32,
              (screen_height()*config().render_scale*config().world_viewport_scale).round() as i32))
}
pub(crate) fn apply_world_viewport() {
    if let Some((x,y,w,h))=world_viewport() { unsafe { get_internal_gl().quad_context.apply_viewport(x,y,w,h); } }
}
pub(crate) fn prepare_3d(bg:Color){scale::prepare(bg);}
pub(crate) fn begin_3d_pass(action:PassAction){scale::begin(action);}
pub(crate) fn set_3d_camera(camera:&Camera3D){scale::camera(camera);}
pub(crate) fn present_3d(){scale::present();}
#[cfg(not(target_os = "vita"))]
pub(crate) fn verification_image() -> Option<Image> { scale::verification_image() }
pub(crate) fn end_world_pass() {
    depth_sentinel::opaque();
    if !(config().scaled_depth_fix && config().render_scale < 1. && crate::render_fx::retained_active()) {
        unsafe { get_internal_gl().quad_context.end_render_pass(); }
    }
}
pub(crate) fn log_world_fx_depth() { depth_sentinel::log_fx_state(); }
pub(crate) fn finish_depth_sentinel(world_view: bool) {
    if world_view { depth_sentinel::finish(); }
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
    viewport: Option<(i32,i32,i32,i32)>,
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
        viewport: camera.viewport,
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
    parameter: usize,
    geometry: Geometry,
    recipe: Recipe,
}
struct Group {
    parameter: usize,
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
        // Sort the immutable material order once rather than once per cluster.
        let mut opaque_order=(0..groups.len()).filter(|&id|groups[id].recipe.opaque).collect::<Vec<_>>();
        opaque_order.sort_by_key(|&id|(groups[id].recipe.rank,groups[id].recipe.blend,groups[id].recipe.cull,id));
        let alpha_order=(0..groups.len()).filter(|&id|!groups[id].recipe.opaque).collect::<Vec<_>>();
        let result=Self::compile(visibility.cluster_count(),|cluster| {
            let mut out=Lists::default();
            let visible=|id:usize| cluster.is_none() || sources[id].as_ref().is_none_or(|s|visibility.sees_clusters(cluster.unwrap(),s));
            out.opaque.extend(opaque_order.iter().copied().filter(|&id|visible(id)));
            out.alpha.extend(alpha_order.iter().copied().filter(|&id|visible(id)));
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
    uniforms: Option<shader::Uniforms>,
}
struct Parameter {
    recipe: Recipe,
    uniforms: shader::Uniforms,
    bits: [u32; 143],
    generation: u64,
    model: [u32; 16],
    time: u32,
    time_uv: bool,
    time_gain: bool,
}
impl Parameter {
    fn update_time(&mut self, time: f32) {
        if self.time_uv {
            let (uv, after, turb) = uv_matrices(&self.recipe.stage, time);
            self.uniforms.uv = uv.to_cols_array();
            self.uniforms.uv_after = after.to_cols_array();
            self.uniforms.turb = turb;
            self.bits[32..48].copy_from_slice(&self.uniforms.uv.map(f32::to_bits));
            self.bits[48..64].copy_from_slice(&self.uniforms.uv_after.map(f32::to_bits));
            self.bits[92..96].copy_from_slice(&turb.map(f32::to_bits));
        }
        if self.time_gain {
            self.uniforms.gain = stage_gain(&self.recipe.stage, time);
            self.bits[72..76].copy_from_slice(&self.uniforms.gain.map(f32::to_bits));
        }
        // Preserve the complete original payload even when deformation is off.
        self.uniforms.deform[2] = time;
        self.bits[98] = time.to_bits();
        self.time = time.to_bits();
    }
}
pub struct Renderer {
    _game_thread: crate::game_thread::GameThread,
    groups: Vec<Group>,
    parameters: Vec<Parameter>,
    parameter_key: Option<[u32; 51]>,
    parameter_generation: u64,
    field_uniforms: [super::uniform_fields::Cache; 2],
    pvs: Pvs,
    pipelines: Vec<Pipeline>,
    shader: ShaderId,
    low_shader:Option<ShaderId>,
    low_pipelines:Vec<Pipeline>,
    sky_pipeline: Pipeline,
    sky_shader: ShaderId,
    sky_bindings: Bindings,
    alpha_order: Vec<(f32, usize)>,
    prepared: Vec<Prepared>,
    fx_gpu: Option<crate::render_fx::retained::Gpu>,
    visibility_key: Option<[u32; 26]>,
    visibility_generation: u64,
    visibility: Vec<(u64, Option<(f32, Mat4)>)>,
    models: Vec<Option<Mat4>>,
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
    pub(super) fn new(scene: &Scene, corpus: &str) -> Result<Self> {
        crate::render_tail::install();
        let started=std::time::Instant::now();
        let c = config();
        let mut pending: Vec<Pending> = Vec::new();
        let mut parameter_keys = BTreeMap::new();
        let mut parameter_recipes = Vec::new();
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
                // Match the existing immutable geometry merge signature. The
                // cell/rank split affects ordering and visibility, not uniforms.
                let parameter = *parameter_keys.entry(signature.clone()).or_insert_with(|| {
                    let id = parameter_recipes.len();
                    parameter_recipes.push(recipe.clone());
                    id
                });
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
                                    parameter,
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
        let mut startup_timing = crate::startup::Phase::new("retained_upload");
        let gl = unsafe { get_internal_gl() };
        let ctx = gl.quad_context;
        crate::frame_profile::shader_creation();
        let shader_timing = crate::startup::WorkTimer::new(crate::startup::Work::ShaderCreation);
        let _shader_name = crate::shader_cache::Name::new("retained-world");
        let shader = ctx
            .new_shader(
                ShaderSource::Glsl {
                    vertex: shader::VERTEX,
                    fragment: &crate::environment::fragment(shader::FRAGMENT),
                },
                shader::meta(),
            )
            .context("retained world shader")?;
        drop(shader_timing);
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
                crate::frame_profile::pipeline_creation();
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
        let mut low_pipelines=Vec::new();
        let low_shader=if c.quality=="low" {
            crate::frame_profile::shader_creation();
            let shader_timing = crate::startup::WorkTimer::new(crate::startup::Work::ShaderCreation);
            let _shader_name = crate::shader_cache::Name::new("retained-world-low");
            let low=ctx.new_shader(ShaderSource::Glsl{vertex:shader::VERTEX,fragment:&shader::low_fragment()},shader::meta())?;
            drop(shader_timing);
            for cull in [CullFace::Nothing,CullFace::Back,CullFace::Front] {
                crate::frame_profile::pipeline_creation();
                low_pipelines.push(ctx.new_pipeline(&[BufferLayout{stride:52,..Default::default()}],&attributes,low,PipelineParams{depth_test:Comparison::LessOrEqual,depth_write:true,cull_face:cull,front_face_order:FrontFaceOrder::Clockwise,..Default::default()}));
            }
            Some(low)
        }else{None};
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
                parameter: p.parameter,
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
        crate::frame_profile::shader_creation();
        let shader_timing = crate::startup::WorkTimer::new(crate::startup::Work::ShaderCreation);
        let _shader_name = crate::shader_cache::Name::new("retained-sky");
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
        drop(shader_timing);
        crate::frame_profile::pipeline_creation();
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
        startup_timing.mark("shaders_pipelines_buffers");
        let pvs_start=std::time::Instant::now();
        let (pvs,cache_status)=pvs_cache::load_or_build(&groups,&scene.map.visibility,corpus);
        if c.pvs_verify {
            let original=Pvs::legacy(&groups,&scene.map.visibility);
            ensure!(original.unknown.as_ref()==pvs.unknown.as_ref() && original.clusters.iter().zip(&pvs.clusters).all(|(a,b)|a.as_ref()==b.as_ref()),"retained PVS differs from legacy compilation");
            println!("PASS retained PVS matches legacy compiler for None and all {} clusters",pvs.clusters.len());
        }
        let pvs_ms=pvs_start.elapsed().as_secs_f64()*1000.;
        println!("[lg/vita-renderer] load PVS {:.3} ms method={} cache={cache_status}",pvs_ms,c.pvs_method);
        startup_timing.mark("pvs_build_cache");
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
            "parameter_recipes": parameter_recipes.len(),
            "world_cache_fixed_payload_bytes": groups.len() * (std::mem::size_of::<(u64,Option<(f32,Mat4)>)>() + std::mem::size_of::<Option<Mat4>>()) + parameter_recipes.len() * std::mem::size_of::<Parameter>(),
            "clusters":pvs.clusters.len(),"unique_cluster_lists":unique,"pipelines":16+low_pipelines.len(),"uniform_staging_capacity_bytes":groups.len()*std::mem::size_of::<Prepared>(),"dropped_layers":dropped_layers,
            "frozen_ambient_models":frozen_models,"frozen_deforms":frozen_deforms,"dynamic_world_lights":0,"geometry_uploads_per_frame":0,"load_groups_ms":groups_ms,"load_recipes_ms":recipes_ms,"load_chunks_ms":chunks_ms,"source_chunks":source_chunks,"load_pipelines_upload_ms":upload_ms,"load_pvs_ms":pvs_ms,"load_total_ms":started.elapsed().as_secs_f64()*1000.,"pvs_cache_status":cache_status,"pvs_method":c.pvs_method});
        println!("[lg/vita-renderer] loaded {inventory}");
        *INVENTORY.lock().unwrap() = Some(inventory);
        // PVS provenance is no longer needed after compilation.
        for g in &mut groups {
            g.faces = Vec::new();
        }
        let parameters = parameter_recipes.into_iter().map(|recipe| {
            let u = uniforms(&recipe, 0., Vec3::ZERO, Mat4::IDENTITY, Mat4::IDENTITY, false, scene.atlas_grid);
            let time_uv = recipe.stage.mods.iter().any(|m| matches!(m,
                TcMod::Scroll(_) | TcMod::Rotate(_) | TcMod::Stretch(_) | TcMod::Turb(_)));
            let time_gain = recipe.stage.rgb.is_some() || recipe.stage.alpha.is_some();
            Parameter { recipe, bits: u.bits(), uniforms: u, generation: 0, model: [0;16],
                time: 0_f32.to_bits(), time_uv, time_gain }
        }).collect();
        Ok(Self {
            _game_thread: crate::game_thread::GameThread::new(),
            parameters, parameter_key: None, parameter_generation: 0,
            field_uniforms: std::array::from_fn(|_| super::uniform_fields::Cache::new(shader::meta())),
            alpha_order: Vec::with_capacity(groups.len()),
            prepared: Vec::with_capacity(groups.len()),
            fx_gpu: None,
            visibility_key: None, visibility_generation: 0,
            visibility: vec![(0, None); groups.len()],
            models: vec![None; groups.len()],
            groups,
            pvs,
            pipelines,
            shader,low_shader,low_pipelines,
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
        use crate::light_profile::{RenderMetrics, RenderPhase};
        let phase = if alpha { RenderPhase::WorldAlpha } else { RenderPhase::WorldOpaque };
        crate::render_tail::end();
        let _owner = crate::render_tail::owner(phase);
        let _timer = crate::light_profile::render_timer(phase);
        let _span = crate::frame_profile::span("vita_world_total");
        count("vita_world_geometry_upload_bytes", 0);
        count("vita_world_retransformed_vertices", 0);
        count("vita_world_rebuilt_indices", 0);
        count("vita_world_sorted_triangles", 0);
        let visibility_started = std::time::Instant::now();
        let visibility_span = crate::frame_profile::span("vita_cluster_frustum");
        let projection = crate::render_fx::projection().expect("world camera projection");
        let cluster = scene.map.visibility.cluster(eye, &scene.map.planes);
        let lists = self.pvs.get(cluster);
        let ids = if alpha { &lists.alpha } else { &lists.opaque };
        self.alpha_order.clear();
        let reuse = config().world_view_reuse;
        let mut key = [0u32; 26];
        key[..16].copy_from_slice(&projection.to_cols_array().map(f32::to_bits));
        key[16..19].copy_from_slice(&eye.to_array().map(f32::to_bits));
        key[19..25].copy_from_slice(&crate::render_fx::depth_view_bits());
        key[25] = u32::from(crate::render_fx::retained_active());
        if self.visibility_key != Some(key) {
            self.visibility_key = Some(key);
            self.visibility_generation = self.visibility_generation.wrapping_add(1);
            if self.visibility_generation == 0 {
                self.visibility.fill((0, None));
                self.visibility_generation = 1;
            }
        }
        crate::coverage::add(16,ids.len() as u64);
        crate::coverage::set(18,cluster.map_or(0,|c|c as u64+1));
        let mut tests = 0u64;
        let mut hits = 0u64;
        for &id in ids {
            let g = &self.groups[id];
            let static_group = g.recipe.model.is_none() && g.recipe.motion.is_none();
            let result = if reuse && static_group && self.visibility[id].0 == self.visibility_generation {
                hits += 1;
                self.visibility[id].1
            } else {
                let result = model_matrix(&g.recipe, time, transforms).and_then(|model| {
                    tests += 1;
                    in_frustum(projection * model, g.min, g.max).then(|| {
                        let depth = if crate::render_fx::retained_active() {
                            crate::render_fx::group_depth(model.transform_point3(g.center))
                        } else { (model.transform_point3(g.center) - eye).length_squared() };
                        (depth, model)
                    })
                });
                if reuse && static_group { self.visibility[id] = (self.visibility_generation, result); }
                result
            };
            self.models[id] = result.map(|(_, model)| model);
            if let Some((depth, _)) = result { self.alpha_order.push((depth, id)); }
        }
        crate::coverage::add(17,self.alpha_order.len() as u64);
        crate::light_profile::render_add(phase, RenderMetrics { visibility_tests: tests,
            visibility_cpu_ns: visibility_started.elapsed().as_nanos() as u64, ..Default::default() });
        count("vita_world_visibility_cache_hits", hits);
        drop(visibility_span);
        if alpha && !config().world_alpha {self.alpha_order.clear();}
        if !alpha && config().opaque_order=="front_to_back" {self.alpha_order.sort_unstable_by(|a,b|a.0.total_cmp(&b.0).then_with(||a.1.cmp(&b.1)));}
        if alpha && config().sort_alpha && !crate::render_fx::retained_active() {
            let _sort = crate::frame_profile::span("vita_alpha_group_sort");
            self.alpha_order
                .sort_unstable_by(|a, b| b.0.total_cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
        }
        self.prepared.clear();
        {
            let parameters_started = std::time::Instant::now();
            let _parameters = crate::frame_profile::span("vita_world_parameters");
            let parameter_reuse = config().world_parameter_reuse;
            let split = config().world_parameter_time_split;
            let key = parameter_view_key(projection, eye, fullbright, scene.atlas_grid,
                if split {0.} else {time}, &scene.atmosphere);
            if self.parameter_key != Some(key) {
                self.parameter_key = Some(key);
                self.parameter_generation = self.parameter_generation.wrapping_add(1);
                if self.parameter_generation == 0 {
                    for p in &mut self.parameters { p.generation = 0; }
                    self.parameter_generation = 1;
                }
            }
            let mut builds = 0;
            let mut time_updates = 0;
            for &(depth, id) in &self.alpha_order {
                let g = &self.groups[id];
                let model = if reuse { self.models[id].unwrap() }
                    else { model_matrix(&g.recipe, time, transforms).unwrap() };
                let u = if parameter_reuse {
                    let p = &mut self.parameters[g.parameter];
                    let model_bits = model.to_cols_array().map(f32::to_bits);
                    if p.generation != self.parameter_generation || p.model != model_bits {
                        p.uniforms = uniforms(&p.recipe, time, eye, model, projection, fullbright, scene.atlas_grid);
                        p.uniforms.atmosphere(&scene.atmosphere, eye);
                        p.bits = p.uniforms.bits();
                        p.generation = self.parameter_generation;
                        p.model = model_bits;
                        p.time = time.to_bits();
                        builds += 1;
                    } else if split && p.time != time.to_bits() {
                        p.update_time(time);
                        time_updates += 1;
                    }
                    None
                } else {
                    let mut u = uniforms(&g.recipe, time, eye, model, projection, fullbright, scene.atlas_grid);
                    u.atmosphere(&scene.atmosphere, eye);
                    builds += 1;
                    Some(u)
                };
                let texture = g.recipe.textures[g.recipe.stage.frame(time)].raw_miniquad_id();
                self.prepared.push(Prepared { depth, group: id, texture, uniforms: u });
            }
            crate::light_profile::render_add(phase, RenderMetrics {
                parameter_cpu_ns: parameters_started.elapsed().as_nanos() as u64, ..Default::default() });
            count("vita_world_parameter_builds", builds);
            count("vita_world_parameter_time_updates", time_updates);
            count("vita_world_parameter_reuses", self.prepared.len() as u64 - builds);
        }
        for cache in &mut self.field_uniforms { cache.interrupt(); }
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
        begin_3d_pass(PassAction::Nothing);
        apply_world_viewport();
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
            let _sky_span=crate::frame_profile::span("vita_world_sky_submit");
            let mut totals = crate::frame_profile::Totals::<3>::new();
            let bg = scene.atmosphere.background();
            let sky = [
                [bg.r, bg.g, bg.b, 1.],
                [bg.r * 1.5, bg.g * 1.5, bg.b * 1.5, 1.],
            ];
            let gl = unsafe { get_internal_gl() };
            let ctx = gl.quad_context;
            let start = totals.start();
            ctx.apply_pipeline(&self.sky_pipeline);
            ctx.apply_bindings(&self.sky_bindings);
            totals.add(0, start);
            let start = totals.start();
            ctx.apply_uniforms(UniformsSource::table(&sky));
            totals.add(1, start);
            let start = totals.start();
            ctx.draw(0, 3, 1);
            totals.add(2, start);
            totals.publish(["vita_world_sky_state_bindings", "vita_world_sky_uniforms", "vita_world_sky_draws_time"]);
            count("vita_world_gpu_draws", 1);
            count("vita_world_sky_draws", 1);
            crate::light_profile::render_add(phase, RenderMetrics { draws: 1, pipelines: 1,
                bindings: 1, uniform_uploads: 1, uniform_bytes: 32, uniform_gl_calls: 2, ..Default::default() });
        }
        if alpha || !config().world_state_cache { unsafe {
            macroquad::miniquad::gl::glDepthMask(if alpha { 0 } else { 1 });
        } }
        let mut previous_pipeline = None;
        let mut previous_texture = None;
        let submit_span = crate::frame_profile::span("vita_world_submit");
        let mut totals = crate::frame_profile::Totals::<3>::new();
        // Lifetime is exactly this owned opaque pass: fresh AFTER sky/interop/
        // target setup, discarded BEFORE any alpha callbacks, actors or HUD.
        let cache_enabled = !alpha && config().world_state_cache;
        let mut cache = state_cache::Cache::new();
        let (mut low_draws, mut pipeline_changes, mut texture_changes) = (0u64, 0u64, 0u64);
        #[cfg(target_os = "vita")]
        let mut static_batches = 0usize;
        let mut applications = [0u64; 4];
        let mut uniform_traffic = [0u64; 3];
        for prepared in &self.prepared {
            let g = &mut self.groups[prepared.group];
            let texture = prepared.texture;
            let u = prepared.uniforms.as_ref().unwrap_or(&self.parameters[g.parameter].uniforms);
            g.bindings.images[0] = texture;
            let pi = g.recipe.blend * 3 + g.recipe.cull;
            let gl = unsafe { get_internal_gl() };
            let ctx = gl.quad_context;
            let low=!alpha && g.recipe.stage.alpha_test==0 && !self.low_pipelines.is_empty();
            let pipeline = if low {self.low_pipelines[g.recipe.cull]}else{self.pipelines[pi]};
            let plan = if cache_enabled {
                // World groups have one immutable VBO, one IBO and two images.
                debug_assert_eq!(g.bindings.vertex_buffers.len(), 1);
                debug_assert_eq!(g.bindings.images.len(), 2);
                cache.observe(pipeline, low, !alpha,
                    (g.bindings.vertex_buffers[0], g.bindings.index_buffer,
                     g.bindings.images[0], g.bindings.images[1]),
                    prepared.uniforms.as_ref().map_or(self.parameters[g.parameter].bits, shader::Uniforms::bits))
            } else {
                state_cache::Applications { pipeline: true, depth: true, bindings: true, uniforms: true }
            };
            let start = totals.start();
            if plan.pipeline { ctx.apply_pipeline(&pipeline); applications[0] += 1; }
            if plan.depth { unsafe{macroquad::miniquad::gl::glDepthMask(if alpha {0}else{1});} applications[1] += 1; }
            scale::log_depth(!alpha);
            if low { low_draws += 1; if !cache_enabled {count("vita_world_low_draws",1);} }
            if plan.bindings { ctx.apply_bindings(&g.bindings); applications[2] += 1; }
            totals.add(0, start);
            let start = totals.start();
            if plan.uniforms {
                let traffic = self.field_uniforms[usize::from(low)].apply(ctx, u.values(), config().uniform_field_cache);
                applications[3] += 1;
                uniform_traffic[0] += u64::from(traffic.calls > 0);
                uniform_traffic[1] += traffic.bytes;
                uniform_traffic[2] += traffic.calls;
            }
            totals.add(1, start);
            let start = totals.start();
            ctx.draw(0, g.indices, 1);
            totals.add(2, start);
            if previous_pipeline != Some(pi) {
                pipeline_changes += 1;
                if !cache_enabled { count("vita_world_pipeline_changes", 1); }
                previous_pipeline = Some(pi);
            }
            if previous_texture != Some(texture) {
                texture_changes += 1;
                if !cache_enabled { count("vita_world_texture_changes", 1); }
                previous_texture = Some(texture);
            }
            if !cache_enabled {
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
            }
            stats.batches += 1;
            stats.triangles += g.indices as usize / 3;
            #[cfg(target_os = "vita")]
            { static_batches += usize::from(g.recipe.model.is_none() && g.recipe.motion.is_none()); }
            #[cfg(target_os = "vita")]
            if !cache_enabled {
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
        if cache_enabled {
            counts(&[
                ("vita_world_low_draws", low_draws),
                ("vita_world_pipeline_changes", pipeline_changes),
                ("vita_world_texture_changes", texture_changes),
                ("vita_world_gpu_draws", stats.batches as u64),
                // Preserve requested payload accounting, including cache hits.
                ("vita_world_uniform_bytes", stats.batches as u64 * std::mem::size_of::<shader::Uniforms>() as u64),
                ("vita_world_opaque_triangles", stats.triangles as u64),
            ]);
            #[cfg(target_os = "vita")]
            crate::frame_profile::record_retained_world_pass(stats.batches, static_batches, stats.triangles);
        }
        crate::light_profile::render_add(phase, RenderMetrics {
            draws: stats.batches as u64, pipelines: applications[0], depth_states: applications[1],
            bindings: applications[2], texture_bind_slots: applications[2] * 2,
            uniform_uploads: uniform_traffic[0], uniform_bytes: uniform_traffic[1],
            uniform_gl_calls: uniform_traffic[2],
            ..Default::default()
        });
        counts(&[
            ("vita_world_pipeline_applications", applications[0]),
            ("vita_world_depth_applications", applications[1]),
            ("vita_world_binding_applications", applications[2]),
            ("vita_world_uniform_applications", applications[3]),
            ("vita_world_uniform_applied_bytes", applications[3] * std::mem::size_of::<shader::Uniforms>() as u64),
        ]);
        totals.publish(if alpha {
            ["vita_world_alpha_state_bindings", "vita_world_alpha_uniforms", "vita_world_alpha_draws"]
        } else {
            ["vita_world_state_bindings", "vita_world_uniforms", "vita_world_draws"]
        });
        drop(submit_span);
        count("vita_world_culled_groups", stats.culled as u64);
        unsafe {
            macroquad::miniquad::gl::glDepthMask(1);
        }
        if !alpha { end_world_pass(); }
        else { unsafe { get_internal_gl().quad_context.end_render_pass(); } }
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
        let totals = std::cell::RefCell::new(crate::frame_profile::Totals::<3>::new());
        let result = crate::render_fx::retained::finish_world(&mut gpu, &world, interop_flush, |id| {
            let _owner = crate::render_tail::owner(crate::light_profile::RenderPhase::WorldAlpha);
            let _timer = crate::light_profile::render_timer(crate::light_profile::RenderPhase::WorldAlpha);
            crate::light_profile::render_add(crate::light_profile::RenderPhase::WorldAlpha,
                crate::light_profile::RenderMetrics { draws: 1, pipelines: 1, depth_states: 1,
                    bindings: 1, texture_bind_slots: 2, uniform_uploads: 1,
                    ..Default::default() });
            let mut totals = totals.borrow_mut();
            let p = &self.prepared[id];
            let g = &mut self.groups[p.group];
            g.bindings.images[0] = p.texture;
            let u = p.uniforms.as_ref().unwrap_or(&self.parameters[g.parameter].uniforms);
            let ctx = unsafe { get_internal_gl() }.quad_context;
            let start = totals.start();
            ctx.apply_pipeline(&self.pipelines[g.recipe.blend * 3 + g.recipe.cull]);
            unsafe{macroquad::miniquad::gl::glDepthMask(0);}
            scale::log_depth(false);
            ctx.apply_bindings(&g.bindings);
            totals.add(0, start);
            let start = totals.start();
            let traffic = self.field_uniforms[0].apply(ctx, u.values(), config().uniform_field_cache);
            crate::light_profile::render_add(crate::light_profile::RenderPhase::WorldAlpha,
                crate::light_profile::RenderMetrics { uniform_bytes: traffic.bytes,
                    uniform_gl_calls: traffic.calls, ..Default::default() });
            totals.add(1, start);
            let start = totals.start();
            ctx.draw(0, g.indices, 1);
            totals.add(2, start);
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
        }, Some(&totals));
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
        for p in &self.low_pipelines {ctx.delete_pipeline(*p);}
        if let Some(shader)=self.low_shader {ctx.delete_shader(shader);}
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
fn counts<const N: usize>(values: &[(&'static str, u64); N]) {
    // Per-draw J10 counters never introduced zero-valued entries. Preserve
    // that published shape too, e.g. no low_draws key in full quality.
    let mut nonzero = [("", 0); N];
    let mut len = 0;
    for &(name, n) in values {
        if n > 0 { nonzero[len] = (name, n); len += 1; }
    }
    let values = &nonzero[..len];
    #[cfg(target_os = "vita")]
    crate::frame_profile::counts(values);
    #[cfg(not(target_os = "vita"))]
    for &(name, n) in values { crate::render_fx::retained::count(name, n); }
}
/// Exact values consumed by the shader, including absent volume zeros. No
/// hashing, pointer identity, tolerance, or hidden dependence on the game clock.
fn parameter_view_key(projection: Mat4, eye: Vec3, fullbright: bool, atlas: Vec2,
                      time: f32, atmosphere: &crate::environment::Atmosphere) -> [u32;51] {
    let mut key = [0;51];
    key[..16].copy_from_slice(&projection.to_cols_array().map(f32::to_bits));
    key[16..19].copy_from_slice(&eye.to_array().map(f32::to_bits));
    key[19..21].copy_from_slice(&atlas.to_array().map(f32::to_bits));
    key[21] = u32::from(fullbright);
    key[22] = time.to_bits();
    key[23..27].copy_from_slice(&atmosphere.distance.to_array().map(f32::to_bits));
    key[27..31].copy_from_slice(&atmosphere.liquid.to_array().map(f32::to_bits));
    for (i,v) in atmosphere.volumes.iter().take(2).enumerate() {
        let offset = 31 + i*10;
        key[offset..offset+3].copy_from_slice(&v.min.to_array().map(f32::to_bits));
        key[offset+3..offset+6].copy_from_slice(&v.max.to_array().map(f32::to_bits));
        key[offset+6..offset+10].copy_from_slice(&v.color.to_array().map(f32::to_bits));
    }
    key
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
    !super::outside_matrix(m, min, max)
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
        gain: stage_gain(s, time),
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
fn stage_gain(s: &Stage, time: f32) -> [f32;4] {
    let gain = s.constant_rgb.unwrap_or(Vec3::ONE)
        * s.rgb.as_ref().map_or(1., |w| w.sample(time)).clamp(0., 2.);
    gain.extend(s.alpha.as_ref().map(|w| w.sample(time)).or(s.constant_alpha)
        .unwrap_or(1.).clamp(0., 1.)).to_array()
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
    fn scaled_depth_fix_defaults_on_and_sentinel_is_opt_in() {
        let c=Config::default(); assert!(c.scaled_depth_fix && !c.depth_sentinel);
        let c:Config=serde_json::from_str(r#"{"scaled_depth_fix":false,"depth_sentinel":true}"#).unwrap();
        c.validate().unwrap(); assert!(!c.scaled_depth_fix && c.depth_sentinel);
        assert!(serde_json::from_str::<Config>(r#"{"depth_sentinel":1}"#).is_err());
        assert!(serde_json::from_str::<Config>(r#"{"scaled_depth_fix":"on"}"#).is_err());
    }
    #[test]
    fn time_only_updates_match_complete_uniforms_without_allocating() {
        let wave = crate::materials::Wave {kind:"sin".into(),values:[0.5,0.3,0.1,0.7]};
        for mods in [vec![], vec![TcMod::Scale(vec2(2.,0.8))], vec![
            TcMod::Scroll(vec2(0.2,-0.3)), TcMod::Turb([0.,0.15,0.4,0.7]),
            TcMod::Rotate(18.), TcMod::Stretch(wave.clone()), TcMod::Scale(vec2(2.,0.8))]] {
            let dynamic = mods.iter().any(|m|matches!(m,TcMod::Scroll(_)|TcMod::Turb(_)|TcMod::Rotate(_)|TcMod::Stretch(_)));
            let recipe = Recipe { stage:Stage {mods,rgb:Some(wave.clone()),alpha:Some(wave.clone()),
                constant_rgb:Some(vec3(0.8,0.7,0.4)),..Default::default()}, textures:vec![],lit:true,
                rank:0,blend:1,opaque:false,cull:0,model:None,motion:None,
                deform:Some(Deform::Move(Vec3::Z,wave.clone())) };
            let model=Mat4::from_translation(Vec3::X*32.);
            let projection=Mat4::from_scale(Vec3::splat(0.3));
            let eye=vec3(3.,5.,7.);
            let u=uniforms(&recipe,0.,eye,model,projection,false,Vec2::ONE);
            let mut p=Parameter {recipe,bits:u.bits(),uniforms:u,generation:1,model:model.to_cols_array().map(f32::to_bits),
                time:0,time_uv:dynamic,time_gain:true};
            for time in [-0.,0.3,3.,20.,0.,f32::from_bits(0x7fc00001)] {
                let expected=uniforms(&p.recipe,time,eye,model,projection,false,Vec2::ONE).bits();
                let scope=crate::kernel_alloc::Scope::begin();
                p.update_time(time);
                assert_eq!(scope.finish(),0);
                assert_eq!(p.uniforms.bits(),expected);
                assert_eq!(p.bits,expected);
            }
        }
    }
    #[test]
    fn world_reuse_switches_and_exact_view_inputs() {
        let c = Config::default();
        assert!(c.world_view_reuse && c.world_parameter_reuse && c.uniform_field_cache);
        for name in ["world_view_reuse", "world_parameter_reuse", "uniform_field_cache"] {
            let mut v = serde_json::to_value(&c).unwrap(); v[name] = serde_json::json!(false);
            serde_json::from_value::<Config>(v.clone()).unwrap().validate().unwrap();
            v[name] = serde_json::json!("off"); assert!(serde_json::from_value::<Config>(v).is_err());
        }
        let mut fog = crate::environment::Atmosphere::default();
        let key = |time, bright, fog: &crate::environment::Atmosphere| parameter_view_key(Mat4::IDENTITY, Vec3::ZERO, bright, Vec2::ONE, time, fog);
        let a = key(0., false, &fog);
        assert_ne!(a, key(-0., false, &fog));
        assert_ne!(a, key(0., true, &fog));
        fog.liquid.x = f32::from_bits(0x7fc00001); let b = key(0., false, &fog);
        assert_ne!(a,b); fog.liquid.x = f32::from_bits(0x7fc00002); assert_ne!(b,key(0.,false,&fog));
        fog.volumes.push(crate::environment::Volume { min: Vec3::ZERO, max: Vec3::ONE, color: Vec4::ONE });
        let b = key(0., false, &fog); fog.volumes[0].min.z = -0.; assert_ne!(b,key(0.,false,&fog));
    }
    #[test]
    fn particle_assembly_switches_default_on_and_reject_non_booleans() {
        let c=Config::default();assert!(c.particle_producer_reuse && c.fx_merge_in_place && c.fx_worker_prep);
        for key in ["particle_producer_reuse","fx_merge_in_place","fx_worker_prep"] {
            let mut value=serde_json::json!({});value[key]=serde_json::json!(false);
            serde_json::from_value::<Config>(value.clone()).unwrap().validate().unwrap();
            value[key]=serde_json::json!("off");assert!(serde_json::from_value::<Config>(value).is_err());
        }
    }
    fn j17_switches_preserve_console_selected_defaults_and_strict_types() {
        let default=serde_json::to_value(Config::default()).unwrap();
        for name in ["lights_early_out","fx_submit_state_cache","actor_layer_reuse"] {
            assert_eq!(default[name],name != "actor_layer_reuse");
            let mut value=default.clone();value[name]=serde_json::json!(false);
            let config:Config=serde_json::from_value(value).unwrap();config.validate().unwrap();
            assert_eq!(serde_json::to_value(config).unwrap()[name],false);
            let mut value=default.clone();value[name]=serde_json::json!("on");
            assert!(serde_json::from_value::<Config>(value).is_err());
        }
    }
    #[test]
    fn overlap_defaults_to_two_workers_and_remains_validated() {
        assert!(Config::default().skin_overlap);
        assert_eq!(Config::default().skin_workers,2);

        for n in [1,2] {
            serde_json::from_value::<Config>(serde_json::json!({"skin_overlap":true,"skin_workers":n})).unwrap().validate().unwrap();
        }
        assert!(serde_json::from_value::<Config>(serde_json::json!({"skin_overlap":true,"skin_workers":0})).unwrap().validate().is_err());
        assert!(serde_json::from_value::<Config>(serde_json::json!({"skin_overlap":"true"})).is_err());
        assert!(serde_json::from_value::<Config>(serde_json::json!({"skin_overlap":true,"skin_workers":1,"skin_worker_bench":true})).unwrap().validate().is_err());
    }
    #[test]
    fn profiler_and_worker_bench_configuration_is_strict() {
        for level in ["light","full"] {
            let c:Config=serde_json::from_value(serde_json::json!({"profiler":level,"skin_worker_bench":true,"skin_overlap":false,"skin_workers":0})).unwrap();
            c.validate().unwrap(); assert!(c.skin_worker_bench); assert_eq!(c.skin_workers,0);
        }
        assert!(serde_json::from_value::<Config>(serde_json::json!({"profiler":"silent"})).is_err());
        assert!(serde_json::from_value::<Config>(serde_json::json!({"skin_worker_bench":1})).is_err());
        assert!(serde_json::from_value::<Config>(serde_json::json!({"skin_worker_bench":true,"skin_workers":1})).unwrap().validate().is_err());
    }
    #[test]
    fn j12_switches_are_strict_and_have_required_defaults() {
        let c=Config::default();
        assert_eq!(c.skin_workers,2); assert_eq!(c.profiler,None); assert!(!c.skin_worker_bench); assert!(c.fx_prepare_cache);
        assert!(!c.worker_probe && !c.frame_phase_probe);
        for workers in 0..=2 {
            let c:Config=serde_json::from_value(serde_json::json!({"skin_workers":workers,"skin_overlap":workers>0,
                "worker_probe":true,"fx_prepare_cache":false,"frame_phase_probe":true})).unwrap();
            assert!(c.validate().is_ok());
        }
        for value in [serde_json::json!({"skin_workers":3}),serde_json::json!({"skin_workers":-1}),
            serde_json::json!({"skin_workers":"1"}),serde_json::json!({"fx_prepare_cache":"on"}),
            serde_json::json!({"worker_probe":1}),serde_json::json!({"frame_phase_probe":1})] {
            assert!(serde_json::from_value::<Config>(value).map_or(true,|c|c.validate().is_err()));
        }
    }
    #[test]
    fn j11_switches_default_on_and_strictly_parse_off() {
        let c: Config = serde_json::from_str("{}").unwrap();
        assert_eq!(c.preview_flip, "auto");
        assert!(c.world_state_cache && c.mover_far_lazy);
        for preview in ["auto", "on", "off"] {
            let c: Config = serde_json::from_value(serde_json::json!({
                "preview_flip": preview, "world_state_cache": false, "mover_far_lazy": false,
            })).unwrap();
            c.validate().unwrap();
            assert!(!c.world_state_cache && !c.mover_far_lazy);
        }
        for value in [serde_json::json!({"preview_flip":true}),
                      serde_json::json!({"world_state_cache":"on"}),
                      serde_json::json!({"mover_far_lazy":1}),
                      serde_json::json!({"unknown_j11":false})] {
            assert!(serde_json::from_value::<Config>(value).is_err());
        }
        assert!(Config { preview_flip: "vertical".into(), ..Default::default() }.validate().is_err());
    }
    #[test]
    fn world_uniform_bits_include_every_metadata_field_in_order() {
        let stage = Stage::default();
        let recipe = Recipe {
            stage, textures: vec![], lit: false, rank: 0, blend: 0, opaque: true,
            cull: 0, model: None, motion: None, deform: None,
        };
        let mut u = uniforms(&recipe,0.,Vec3::ZERO,Mat4::IDENTITY,Mat4::IDENTITY,false,Vec2::ONE);
        let mut next = 1u32;
        for field in [
            u.mvp.as_mut_slice(), u.model.as_mut_slice(), u.uv.as_mut_slice(), u.uv_after.as_mut_slice(),
            u.eye.as_mut_slice(), u.surface.as_mut_slice(), u.gain.as_mut_slice(), u.alpha.as_mut_slice(),
            u.dot_alpha.as_mut_slice(), u.vector_s.as_mut_slice(), u.vector_t.as_mut_slice(), u.turb.as_mut_slice(),
            u.deform.as_mut_slice(), u.wave.as_mut_slice(), u.move_dir.as_mut_slice(), u.atlas.as_mut_slice(),
            u.fog_eye.as_mut_slice(), u.distance_fog.as_mut_slice(), u.liquid_fog.as_mut_slice(),
            u.fog_a_min.as_mut_slice(), u.fog_a_max.as_mut_slice(), u.fog_a.as_mut_slice(),
            u.fog_b_min.as_mut_slice(), u.fog_b_max.as_mut_slice(), u.fog_b.as_mut_slice(),
        ] {
            for value in field { *value = f32::from_bits(next); next += 1; }
        }
        assert_eq!(u.bits().to_vec(), (1..144).collect::<Vec<u32>>());
    }
    #[test]
    fn j10_switches_default_on_and_validate_legacy_settings() {
        let defaults: Config = serde_json::from_str("{}").unwrap();
        assert_eq!(defaults.save_preview, "throttled");
        assert!(defaults.mover_lazy_publish && defaults.pickup_fx_cache);
        let legacy: Config = serde_json::from_str(r#"{"save_preview":"every_frame","mover_lazy_publish":false,"pickup_fx_cache":false}"#).unwrap();
        assert!(legacy.validate().is_ok());
        assert!(!legacy.mover_lazy_publish && !legacy.pickup_fx_cache);
        for json in [r#"{"pickup_fx_cache":"on"}"#, r#"{"mover_lazy_publish":1}"#, r#"{"save_preview":true}"#, r#"{"unexpected_j10_switch":true}"#] {
            assert!(serde_json::from_str::<Config>(json).is_err());
        }
        assert!(Config { save_preview: "off".into(), ..Default::default() }.validate().is_err());
    }
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
    fn clocks_default_to_max_when_absent() {
        let config: Config = serde_json::from_str("{}").unwrap();
        assert_eq!(config.clocks, Clocks::Max);
        assert_eq!(Config::default().clocks, Clocks::Max);
        let lean: Config = serde_json::from_str(include_str!("../../configs/vita_renderer.lean.json")).unwrap();
        assert_eq!(lean.clocks, Clocks::Max);
        assert_eq!(lean.gpu_experiment, "unchanged");
    }
    #[test]
    fn clocks_accept_explicit_max() {
        let config: Config = serde_json::from_str(r#"{"clocks":"max"}"#).unwrap();
        assert_eq!(config.clocks, Clocks::Max);
        assert!(config.validate().is_ok());
    }
    #[test]
    fn clocks_accept_explicit_stock() {
        let config: Config = serde_json::from_str(r#"{"clocks":"stock"}"#).unwrap();
        assert_eq!(config.clocks, Clocks::Stock);
        assert!(config.validate().is_ok());
    }
    #[test]
    fn clocks_reject_invalid_values() {
        for json in [r#"{"clocks":"invalid"}"#, r#"{"clocks":"MAX"}"#, r#"{"clocks":444}"#, r#"{"clocks":null}"#] {
            assert!(serde_json::from_str::<Config>(json).is_err(), "accepted {json}");
        }
    }
    #[test]
    fn settings_bound_gpu_passes_and_reject_bad_values() {
        assert!(Config::default().validate().is_ok());
        assert!(!Config::default().bone_palette);
        assert!(!Config::default().gpu_state_probe && !Config::default().gpu_index_dedupe && !Config::default().gpu_uniform_dedupe);
        assert!(serde_json::from_str::<Config>(r#"{"gpu_uniform_dedupe":"on"}"#).is_err());
        assert!(Config::default().skin_frustum_cull && Config::default().fast_frustum);
        let defaults = Config::default();
        assert!(!defaults.flat_actor_light && !defaults.simple_fog && !defaults.simple_dynamic_lights);
        assert!(defaults.max_texture_dimension.is_none() && defaults.draw_distance_cap.is_none());
        for cap in [0, 15, 513] { assert!(Config {max_texture_dimension:Some(cap), ..Default::default()}.validate().is_err()); }
        for cap in [0., 127., 30001., f32::NAN] { assert!(Config {draw_distance_cap:Some(cap), ..Default::default()}.validate().is_err()); }
        let lean: Config = serde_json::from_str(include_str!("../../configs/vita_renderer.lean.json")).unwrap();
        assert!(lean.validate().is_ok());
        assert!(lean.flat_actor_light && lean.simple_fog && lean.simple_dynamic_lights);

        assert!(serde_json::from_str::<Config>(r#"{"bone_palette":"on"}"#).is_err());
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
        for scale in [0.25,0.5,1.] { assert!(Config {world_viewport_scale:scale,..Config::default()}.validate().is_ok()); }
        for scale in [0.,1.1,f32::NAN] { assert!(Config {world_viewport_scale:scale,..Config::default()}.validate().is_err()); }
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
