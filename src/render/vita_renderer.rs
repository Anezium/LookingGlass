//! Direct, retained world draws. CPU geometry and PVS compilation happen at load time.
#[cfg(feature="frame_total_only")]
use crate::light_profile::InactiveInstant as MetricInstant;
#[cfg(not(feature="frame_total_only"))]
use std::time::Instant as MetricInstant;
mod shader;
mod scale;
mod depth_sentinel;
mod pvs_cache;
mod state_cache;
mod submit;
mod world_list;
mod gxm;
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
#[serde(remote = "Self", default, deny_unknown_fields)]
pub struct Config {
    /// Miniquad timing report cadence in frames; omitted keeps the historical 10, null or 0
    /// disables it. Measurement runs set 0: the report adds 3-5 ms to p95 (r15b).
    pub log_frame_time_every: Option<u32>,
    /// Build trigger facts once per trigger pass and rebuild after each fire.
    /// Default on since r21 (console A/B r16b: wchess2 -8.6/-9.1 ms, hatter1 -2.2/-2.4 ms).
    pub sim_trigger_facts_reuse: bool,
    /// Exact NPC sight predicates, publication-scoped trace reuse and stack targets.
    pub sim_npc_sight_candidates: bool,
    pub sim_npc_sight_cache: bool,
    pub sim_npc_target_stack: bool,
    /// Defer Clockwork door traces until their exact range/facing predicates pass.
    /// Default on since r22 (console ABBA, hatter1: -6.29 to -7.31 ms).
    pub sim_hatter_door_candidates: bool,
    /// Reuse bit-identical Ant sight traces within one immutable Ant update.
    /// Default on since r21 (console A/B r16d: centipede1 -14.4 to -15.2 ms).
    pub sim_sight_query_reuse: bool,
    /// Answer a repeated NPC collision sweep from its first answer inside one update.
    pub sim_sweep_memo: bool,
    /// Microseconds the skin workers and the game thread poll for each other before sleeping. Zero is off.
    pub worker_spin_us: u32,
    /// Skin workers claim draws as the gather captures them, instead of starting after it.
    pub skin_stream: bool,
    /// Exact static occlusion certificates for unchanged eye/light positions.
    pub lights_static_cache: bool,
    /// Spatial AABB buckets; preserve original dynamic contact order.
    pub sim_dynamic_buckets: bool,
    /// Load an ordered static SAH hierarchy. Default on since RC1, as the console measured it.
    pub collision_sah: bool,
    /// Exact round-two candidates; console acceptance is still pending.
    pub fx_merge_local_reuse: bool,
    /// Keep certified unchanged source ranges in a partially changing merge.
    pub fx_merge_ranges: bool,
    /// Run small FX numeric jobs locally instead of dispatching and joining.
    pub fx_cpu_inline: bool,
    pub sim_triangle_candidates: bool,
    pub sim_fan_candidates: bool,
    pub sim_school_mesh: bool,
    pub sim_tower_templates: bool,
    pub sim_npc_queries: bool,
    pub sim_mechanism_state: bool,
    pub renderer: String,
    /// Exact campaign names, or "*" for every map (the default).
    #[serde(deserialize_with = "deserialize_maps")]
    pub retained_maps: Vec<String>,
    /// Explicit legacy exceptions take precedence over retained_maps.
    #[serde(deserialize_with = "deserialize_maps")]
    pub legacy_maps: Vec<String>,
    /// Vita startup policy; stock preserves the clocks inherited from the system.
    pub clocks: Clocks,
    /// Clean thumbnails during play; diagnostic modes retain their own policy.
    pub save_preview: String,
    /// auto/on correct Vita's two-readback orientation; off restores J10.
    pub preview_flip: String,
    /// Experimental exact submission levers; independently default off.
    pub world_material_sort: bool,
    pub world_static_merge: bool,
    pub world_uniform_persist: bool,
    pub world_binding_cache: bool,
    pub world_gl_state_cache: bool,
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
    /// Prepare actor packets once using bounded owned publication banks.
    pub actor_single_prepare: bool,
    /// Give each repeated draw of one actor surface in a frame its own publication
    /// bank, up to four per frame. Default on since r22 (console ABBA, skool2: -1.84 / -2.16 ms).
    pub actor_publication_occurrences: bool,
    /// Skin non-simple actor layer recipes on the workers (raw kernel bytes) and build
    /// their layers on the game thread after collect. Default on since r22
    /// (console ABBA, wchess2: -4.47 to -5.01 ms).
    pub actor_deferred_layers: bool,
    /// Skip the per-vertex UV compare and copy of a publication slot that already holds
    /// the UVs of this live mesh. Exact while nothing edits a live skeletal UV, which no
    /// production code does (tools/check_source.py fails on a new writer). Default on since r25;
    /// with prop_occurrence_reuse, console ABBA: hatter1 -10.16/-10.17, skool2 -4.11/-3.99, wchess2 -3.11/-3.04 ms.
    pub actor_publication_uv_reuse: bool,
    /// Keep one animated copy per repeated Prop draw occurrence in a frame and skip the
    /// animation and the geometry compare while its exact inputs repeat. Default on since r25,
    /// measured with actor_publication_uv_reuse above.
    pub prop_occurrence_reuse: bool,
    /// Observed FX publications take a changed array from the pass that owns it instead of
    /// copying it. Exact on the host oracle. Default on since r25
    /// (console ABBA: wchess2 -2.51/-1.96, skool2 -1.32/-1.66 ms).
    pub fx_observe_move: bool,
    /// Attached emitters keep each birth's constants, and an Alpha set takes the raw quad path
    /// additive sets already use. Same quads, blend and order. Default off.
    pub fx_attached_persist: bool,
    /// School two publishes the other movers' colliders only when a rider's body trace will read
    /// them, and builds a mover's collider only when something can read it. Default off.
    pub mover_advance_reuse: bool,
    /// Two clock reads per world pass inside the survey window, one printed line after it. Diagnostic.
    pub world_pass_clock: bool,
    /// Opaque world pass from dense records. Same GL calls in the same order. Default off.
    pub world_list_submit: bool,
    /// All world groups in a few shared buffers, not one VBO and IBO per group. Draw order and
    /// vertex bytes stay. Default off.
    pub world_shared_buffers: bool,
    /// Opaque world draws straight to sceGxm: off, shadow, live, or a proof mode. Requires
    /// world_list_submit. Only the Vita build acts on it; a host treats it as off. Default off.
    pub world_gxm: String,
    /// Remember each thread-local key's list node on the game thread. off, on, shadow or
    /// shadow_sabotage. The host has no such list: there every value behaves like off.
    pub tls_fast: String,
    /// Attached emitters that declare `sphere` send each particle along its own random direction,
    /// as the original client does, instead of along the tag's X axis. Changes pixels. Default off.
    pub fx_attached_sphere: bool,
    /// The `bubble` sprite is as wide as its 16-pixel image times the scale, and its scalerate
    /// compounds, as in the original. Changes pixels. Default off.
    pub fx_bubble_true_size: bool,
    /// GPU timing through a survey window: off, backlog, poll or wait. See `gpu_timing`.
    pub gpu_timing: String,
    /// Diagnostic only, the picture changes: draw classes whose draw call is skipped, CPU kept.
    pub gpu_mask: Vec<String>,
    /// Scenes per frame for the display and framebuffer render targets, before vglInit.
    /// None keeps vitaGL's 1 and 1. Diagnostic.
    pub rt_scenes: Option<[u8; 2]>,
    /// GXM parameter buffer in MiB, before vglInit. None keeps vitaGL's 16. Diagnostic.
    pub param_buffer_mib: Option<u32>,
    /// With a scaled 3D target, skip the display clear that opens the frame. The upscale covers
    /// every display pixel, so its scene draws nothing that stays. Default off.
    pub display_clear_skip: bool,
    /// None selects off in normal play and full in diagnostic modes.
    pub profiler: Option<crate::light_profile::Level>,
    /// Retain all GL observers under light profiling for matched comparisons.
    pub light_gl_observers: bool,
    /// Arm bounded capture and profile-mode polling; reports only at pause/end.
    pub profile_capture: bool,
    pub skin_worker_bench: bool,
    pub fx_prepare_cache: bool,
    pub particle_producer_reuse: bool,
    /// Publish world puffs once per final joint-alpha material batch.
    pub world_puff_batches: bool,
    /// Send additive attached quads to the joint-alpha batch bank directly.
    pub attached_puff_batches: bool,
    pub fx_merge_in_place: bool,
    pub fx_worker_prep: bool,
    pub frame_phase_probe: bool,
    /// Fixed-storage elapsed census; opt in only for a bounded benchmark.
    pub phase_census: bool,
    /// Select mutable skin from actual frame consumers; keep inherited visual policy.
    pub skin_actual_consumers: bool,
    pub mover_far_lazy: bool,
    pub mover_lazy_publish: bool,
    pub pickup_fx_cache: bool,
    #[serde(alias = "renderer_world_only")]
    pub world_only: bool,
    pub max_layers: usize,
    pub sort_alpha: bool,
    pub sky: bool,
    /// Diagnostic authored sky bridge; no native/picture acceptance yet. Default off.
    pub authored_sky: bool,
    /// Diagnostic direct-effect routing candidate; picture/native cost unqualified.
    pub defer_immediate_fx: bool,
    /// Present a Loading frame with the destination title and a step bar at the existing load
    /// stage boundaries of chapter, console, natural-exit, save-load and death-retry transitions,
    /// instead of a black screen. Frames only between stages. Default off.
    pub loading_screen: bool,
    /// A collision world built from the same map with a hull set bit-identical to a live world's
    /// shares that world's immutable hulls, tree and SAH instead of building them again.
    /// Exact by construction; saves one tree and SAH build per such world. Default off.
    pub collision_geometry_share: bool,
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
    /// Human fallback only; preserves normal game/renderer policy when off.
    pub story_film_bypass: bool,
    /// Explicit console candidate: move DSP/decoding off the device callback.
    pub audio_pcm_worker: bool,
    /// Exact static acoustic traces on the PCM producer, only while its cushion is full.
    pub audio_offmain: bool,
    /// Exact immutable acoustic traces on main; live geometry is always completed.
    pub audio_static_cache: bool,
    pub audio_block_mix: bool,
    pub audio_preresample: bool,
    pub audio_block_parameters: bool,
    /// Only ambience is capped; music, speech, player and gameplay effects survive.
    pub audio_ambient_voices: usize,
    pub audio_cull_gain: f32,
    pub audio_producer_priority: i32,
    pub audio_producer_affinity: i32,
    pub audio_cushion_blocks: usize,
    /// The producer never prints: per-second windows go through a lock-free
    /// queue to a lowest-priority reporter thread.
    pub audio_log_thread: bool,
    /// Music decodes on its own lower-priority thread into a PCM ring; the
    /// producer only copies decoded samples.
    pub audio_music_thread: bool,
    pub audio_music_priority: i32,
    /// Persistent binary pairs; process deduplication remains active when off.
    pub shader_cache: String,
    /// Optional cache of exact device-compressed immutable pixels.
    pub texture_cache: String,
    pub route_heading_lock:bool,
    /// Input-only diagnostic: recover recorded world wishes without camera waits.
    pub route_world_wish: bool,
    /// Record a play into this tape file (src/replay). Empty: off. Needs replay_start.
    pub replay_record: String,
    /// Replay this tape file instead of the pad. Empty: off.
    pub replay_play: String,
    /// Where a recording starts: "save:<slot>" or "chapter:<map>[$<entrance>][@<difficulty>]".
    pub replay_start: String,
    /// Steps between check values in a recording.
    pub replay_check_every: u32,
    /// Steps between full state hashes in a recording; 0 never.
    pub replay_state_every: u32,
    /// End a recording and the game after this many steps; 0 never (a person ends it with Start).
    pub replay_stop_after: u32,
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
    /// How the scaled 3D target is built. `resolve` (default, historical): a one-sample
    /// Macroquad target with a resolve copy, so every end of a pass on it runs
    /// `glBlitFramebuffer`. The pinned vitaGL ends the GXM scene at each blit and keeps no
    /// depth between scenes, so a Macroquad flush inside the frame (one pass per queued
    /// draw) leaves later world alpha, FX and actors without world depth.
    /// `direct`: no resolve copy; the upscale samples the drawn texture.
    /// `direct_flip`: as `direct`, with the upscale V axis inverted relative to `resolve`.
    pub scaled_target: String,
    pub msaa_samples:i32,
    pub quality:String,
    pub opaque_order:String,
    pub world_alpha:bool,
}
// The remote derive keeps one strict field schema while letting us distinguish
// omitted actor preparation from an explicit request before applying defaults.
impl serde::Serialize for Config {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        Config::serialize(self, serializer)
    }
}
impl<'de> serde::Deserialize<'de> for Config {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        let value = <serde_json::Value as serde::Deserialize>::deserialize(deserializer)?;
        let actor_explicit = value.get("actor_single_prepare").is_some();
        let overlap_explicit = value.get("skin_overlap").is_some();
        let mut c = Config::deserialize(value).map_err(serde::de::Error::custom)?;
        // A worker-count kill switch also disables implicitly enabled overlap.
        // Explicit overlap=true with zero workers remains a validation error.
        if c.skin_workers == 0 && !overlap_explicit { c.skin_overlap = false; }
        if !actor_explicit && c.actor_single_prepare && !c.actor_single_prepare_active() {
            c.actor_single_prepare = false;
            eprintln!("[lg/vita-renderer] warning default actor_single_prepare inactive: prerequisite disabled; preserving configuration");
        }
        Ok(c)
    }
}
// A malformed name is local to its entry; serde still rejects unknown keys/types.
fn deserialize_maps<'de, D: serde::Deserializer<'de>>(deserializer: D) -> std::result::Result<Vec<String>, D::Error> {
    let mut maps = <Vec<String> as serde::Deserialize>::deserialize(deserializer)?;
    maps.retain(|name| {
        let valid = name == "*" || (!name.is_empty() && name.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_'));
        if !valid { eprintln!("[lg/vita-renderer] warning invalid map name {name:?}; dropping entry only"); }
        valid
    });
    Ok(maps)
}
impl Default for Config {
    fn default() -> Self {
        Self {
            log_frame_time_every: Some(10),
            sim_trigger_facts_reuse: true,
            sim_hatter_door_candidates: true,
            sim_sight_query_reuse: true,
            sim_sweep_memo: false,
            worker_spin_us: 0,
            skin_stream: false,
            sim_npc_sight_candidates: true,
            sim_npc_sight_cache: true,
            sim_npc_target_stack: true,
            lights_static_cache: true,
            sim_dynamic_buckets: false,
            collision_sah: true,
            fx_merge_local_reuse: true,
            fx_merge_ranges: false,
            fx_cpu_inline: false,
            sim_triangle_candidates: true,
            sim_fan_candidates: true,
            sim_school_mesh: true,
            sim_tower_templates: true,
            sim_npc_queries: true,
            sim_mechanism_state: true,
            renderer: "vita".into(),
            retained_maps: vec!["*".into()],
            legacy_maps: Vec::new(),
            clocks: Clocks::Max,
            save_preview: "throttled".into(),
            preview_flip: "auto".into(),
            world_material_sort: false,
            world_static_merge: false,
            world_uniform_persist: false,
            world_binding_cache: false,
            world_gl_state_cache: false,
            world_state_cache: true,
            world_view_reuse: true,
            world_parameter_reuse: true,
            world_parameter_time_split: true,
            uniform_field_cache: true,
            render_draw_timing: false,
            worker_probe: false, skin_workers: 2, skin_overlap: true,
            lights_early_out: true, fx_submit_state_cache: true, actor_layer_reuse: false, actor_single_prepare: true,
            actor_publication_occurrences: true,
            actor_deferred_layers: true,
            actor_publication_uv_reuse: true,
            prop_occurrence_reuse: true,
            fx_observe_move: true,
            fx_attached_persist: false,
            mover_advance_reuse: false,
            world_pass_clock: false,
            world_list_submit: false,
            world_shared_buffers: false,
            world_gxm: "off".into(),
            tls_fast: "off".into(),
            gpu_timing: "off".into(), gpu_mask: Vec::new(), rt_scenes: None, param_buffer_mib: None, display_clear_skip: false,
            fx_attached_sphere: false, fx_bubble_true_size: false,
            light_gl_observers: false,
            profiler: Some(crate::light_profile::Level::Off), profile_capture: false, skin_worker_bench: false, fx_prepare_cache: true,
            particle_producer_reuse: true, world_puff_batches: true, attached_puff_batches: true, fx_merge_in_place: true, fx_worker_prep: true,
            frame_phase_probe: false,
            phase_census: false,
            skin_actual_consumers: true,
            mover_far_lazy: true,
            mover_lazy_publish: true,
            pickup_fx_cache: true,
            world_only: false,
            max_layers: 2,
            sort_alpha: true,
            sky: true,
            authored_sky: false,
            defer_immediate_fx: false,
            loading_screen: false,
            collision_geometry_share: false,
            cell_size: 256.,
            alpha_interop: true,
            actor_max_layers: 2,
            merge_fx: true,
            fx_batching:"material".into(),fx_streaming:"ring".into(),fx_merge_cache:true,fx_upload_cache:true,fx_max_layers:1,actor_fast_path:true,
            fx_ring_safe: true,
            actor_layer_cache:true,actor_shared_geometry:true,actor_batching:true,actor_frustum_cull:true,
            bone_palette: false,
            skin_frustum_cull: true,
            fast_frustum: true,
            flat_actor_light: false, max_texture_dimension: None,
            draw_distance_cap: None, simple_fog: false, simple_dynamic_lights: false,
            gpu_state_probe: false,
            gpu_index_dedupe: false, gpu_uniform_dedupe: false,
            startup_splash:false,
            story_film_bypass: false,
            audio_pcm_worker: false,
            audio_offmain: false,
            audio_static_cache: true,
            audio_block_mix: false,
            audio_preresample: false,
            audio_block_parameters: false,
            audio_ambient_voices: 0,
            audio_cull_gain: 0.,
            audio_producer_priority: 161,
            audio_producer_affinity: 0x40000,
            audio_cushion_blocks: 2,
            audio_log_thread: false,
            audio_music_thread: false,
            audio_music_priority: 170,
            shader_cache:"on".into(),
            texture_cache:"off".into(),
            route_heading_lock:true,
            route_world_wish: false,
            replay_record: String::new(),
            replay_play: String::new(),
            replay_start: String::new(),
            replay_check_every: 30,
            replay_state_every: 0,
            replay_stop_after: 0,
            pvs_cache: false,
            pvs_method: "sources".into(),
            pvs_verify: false,
            gpu_experiment: "gpu222".into(),
            world_viewport_scale: 1.,
            depth_sentinel: false, scaled_depth_fix: true, scaled_target: "resolve".into(),
            render_scale:0.75,msaa_samples:1,quality:"low".into(),opaque_order:"front_to_back".into(),world_alpha:true,
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
    pub(super) fn selection(&self, name: &str) -> (bool, &'static str) {
        if self.renderer != "vita" { return (false, "configuration"); }
        if self.legacy_maps.iter().any(|m| m == "*" || m == name) {
            return (false, "excluded");
        }
        if self.retained_maps.iter().any(|m| m == "*" || m == name) {
            (true, "listed")
        } else { (false, "unlisted") }
    }
    fn unknown_maps<'a>(&'a self, exists: impl Fn(&str) -> bool) -> Vec<(&'static str, &'a str)> {
        [("retained_maps", &self.retained_maps), ("legacy_maps", &self.legacy_maps)].into_iter()
            .flat_map(|(field, maps)| maps.iter().map(move |name| (field, name.as_str())))
            .filter(|(_, name)| *name != "*" && !exists(name)).collect()
    }
    pub(crate) fn warn_unknown_maps(&self, assets: &crate::assets::Assets) {
        for (field, name) in self.unknown_maps(|name| assets.contains(&format!("maps/{name}.bsp"))) {
            eprintln!("[lg/vita-renderer] warning {field} map {name:?} matches no map in mounted game data");
        }
    }
    pub(crate) fn actor_single_prepare_active(&self)->bool {
        self.actor_single_prepare && self.skin_overlap && self.skin_workers > 0 && self.actor_fast_path && self.actor_layer_cache
            && self.fx_prepare_cache && !self.actor_layer_reuse && self.renderer=="vita"
    }
    fn validate(&self) -> Result<()> {
        crate::replay::validate_settings(&crate::replay::Settings { record: self.replay_record.clone(),
            play: self.replay_play.clone(), start: self.replay_start.clone(), check_every: self.replay_check_every,
            state_every: self.replay_state_every, stop_after: self.replay_stop_after })?;
        ensure!(!self.audio_offmain || self.audio_pcm_worker, "audio_offmain requires audio_pcm_worker");
        ensure!((160..=191).contains(&self.audio_producer_priority), "audio_producer_priority must be 160..191 (below skin workers)");
        ensure!(matches!(self.audio_producer_affinity,0x10000|0x20000|0x40000), "audio_producer_affinity must select CPU0, CPU1 or CPU2");
        ensure!((1..=8).contains(&self.audio_cushion_blocks), "audio_cushion_blocks must be 1..8");
        ensure!(self.audio_music_priority > self.audio_producer_priority && self.audio_music_priority <= 191, "audio_music_priority must be below the producer's urgency (greater than audio_producer_priority, at most 191)");
        ensure!(self.audio_ambient_voices<=128 && self.audio_cull_gain.is_finite() && (0. ..=0.1).contains(&self.audio_cull_gain), "invalid audio voice budget");
        ensure!(!self.actor_single_prepare || self.actor_single_prepare_active(),
            "actor_single_prepare requires renderer=vita, skin_overlap=true, skin_workers>0, actor_fast_path=true, actor_layer_cache=true, fx_prepare_cache=true and actor_layer_reuse=false");
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
        ensure!(matches!(self.scaled_target.as_str(),"resolve"|"direct"|"direct_flip"),"scaled_target must be resolve, direct or direct_flip");
        ensure!(matches!(self.quality.as_str(),"full"|"low"),"quality must be full or low");
        ensure!(matches!(self.opaque_order.as_str(),"material"|"front_to_back"),"invalid opaque_order");
        ensure!(!self.world_list_submit || (self.world_state_cache && self.world_view_reuse && self.world_parameter_reuse && self.uniform_field_cache && !self.world_static_merge && !self.world_binding_cache && !self.world_gl_state_cache),
            "world_list_submit requires world_state_cache, world_view_reuse, world_parameter_reuse and uniform_field_cache, and excludes world_static_merge, world_binding_cache and world_gl_state_cache");
        ensure!(!self.world_shared_buffers || !self.world_static_merge,"world_shared_buffers excludes world_static_merge: its packing replaces the merge's keys and loses merges");
        ensure!(gxm::Mode::parse(&self.world_gxm).is_some(),"world_gxm must be off, shadow, live, proof, proof_control or proof_sabotage");
        ensure!(self.world_gxm=="off" || self.world_list_submit,"world_gxm requires world_list_submit");
        ensure!(self.world_gxm=="off" || self.gpu_mask.is_empty(),"world_gxm excludes gpu_mask: a masked draw captures nothing");
        ensure!(!self.skin_stream || (self.skin_overlap && self.actor_single_prepare && !self.actor_layer_reuse),"skin_stream requires skin_overlap and actor_single_prepare, and excludes actor_layer_reuse");
        ensure!(self.worker_spin_us <= 1000,"worker_spin_us must be at most 1000: worker 1 shares CPU2 with the audio producer");
        ensure!(crate::tls_count::FastMode::parse(&self.tls_fast).is_some(),"tls_fast must be off, on, shadow or shadow_sabotage");
        ensure!(crate::gpu_timing::Mode::parse(&self.gpu_timing).is_some(),"gpu_timing must be off, backlog, poll or wait");
        for name in &self.gpu_mask { ensure!(crate::gpu_timing::mask_bit(name).is_some(),"unknown gpu_mask class {name}"); }
        if let Some(scenes)=self.rt_scenes { ensure!(scenes.iter().all(|&n|(1..=8).contains(&n)),"rt_scenes must be 1 to 8"); }
        if let Some(mib)=self.param_buffer_mib { ensure!((16..=64).contains(&mib),"param_buffer_mib must be 16 to 64"); }
        ensure!(matches!(self.gpu_experiment.as_str(),"unchanged"|"gpu111"|"gpu222"),"gpu_experiment must be unchanged, gpu111 or gpu222");
        ensure!(matches!(self.pvs_method.as_str(),"sources"|"legacy"),"pvs_method must be sources or legacy");
        ensure!(
            self.cell_size.is_finite() && (64. ..=2048.).contains(&self.cell_size),
            "invalid cell_size"
        );
        Ok(())
    }
}
/// The effective miniquad report cadence, as printed: `off` or a frame count.
fn report_cadence(c: &Config) -> String {
    c.log_frame_time_every.filter(|&n| n > 0).map_or_else(|| "off".into(), |n| n.to_string())
}
/// The two startup records. The config record stays one JSON document after its tag, because
/// the console auditors `json.loads` everything after `[lg/vita-renderer] config `.
fn config_log_lines(c: &Config) -> [String; 2] {
    [
        format!("[lg/vita-renderer] config {}", serde_json::to_string(c).unwrap_or_else(|e| format!("serialization_error={e}"))),
        format!("[lg/vita-renderer] report_cadence effective_log_frame_time_every={}", report_cadence(c)),
    ]
}
/// Reject a malformed report cadence without discarding unrelated flags.
fn parse_runtime_config(bytes: &[u8]) -> Result<(Config, Option<String>)> {
    let mut value: serde_json::Value = serde_json::from_slice(bytes)?;
    let rejected = if let Some(object) = value.as_object_mut() {
        if let Some(raw) = object.get("log_frame_time_every") {
            if serde_json::from_value::<Option<u32>>(raw.clone()).is_err() {
                let message = format!("log_frame_time_every={raw}: expected null or u32; rejecting key, effective value={}; preserving other config keys", report_cadence(&Config::default()));
                object.remove("log_frame_time_every");
                Some(message)
            } else { None }
        } else { None }
    } else { None };
    let config: Config = serde_json::from_value(value)?;
    config.validate()?;
    Ok((config, rejected))
}
#[cfg(target_os = "vita")]
const CONFIG_PATH: &str = "ux0:data/alice/private/vita_renderer.json";
#[cfg(not(target_os = "vita"))]
const CONFIG_PATH: &str = "private/vita_renderer.json";
pub(crate) fn fx_cpu_options() -> (bool, bool) {
    #[cfg(test)]
    {
        static ON: OnceLock<bool> = OnceLock::new();
        if *ON.get_or_init(||std::env::var("LOOKING_GLASS_TEST_FX_CPU").as_deref() == Ok("on")) { return (true,true); }
    }
    let c = config();(c.fx_merge_ranges,c.fx_cpu_inline)
}
#[cfg(test)]
pub fn config() -> &'static Config {
    static CONFIG: OnceLock<Config> = OnceLock::new();
    CONFIG.get_or_init(Config::default)
}
#[cfg(not(test))]
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
            let (c, rejected) = parse_runtime_config(&bytes)?;
            if let Some(message) = rejected {
                eprintln!("[lg/vita-renderer] invalid config key {CONFIG_PATH}: {message}");
            }
            eprintln!("[lg/startup] renderer_config parse done elapsed_ms={:.3}; validate begin",start.elapsed().as_secs_f64()*1000.);
            c.validate()?;
            eprintln!("[lg/startup] renderer_config validate done elapsed_ms={:.3}",start.elapsed().as_secs_f64()*1000.);
            Ok(c)
        })();
        let c = parsed.unwrap_or_else(|e| {
            eprintln!("[lg/vita-renderer] invalid config {CONFIG_PATH}: {e:#}; using defaults");
            Config::default()
        });
        for line in config_log_lines(&c) {
            println!("{line}");
        }
        println!("[lg/vita-renderer] actor_single_prepare active={}", c.actor_single_prepare_active());
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
/// The one line `world_gxm` prints after the survey window, if it is on.
pub(crate) fn world_gxm_summary(map: &str) -> Option<String> { gxm::summary_line(map) }
pub(crate) fn world_viewport() -> Option<(i32,i32,i32,i32)> {
    if !world_only() || config().world_viewport_scale==1. { return None; }
    Some((0,0,(screen_width()*config().render_scale*config().world_viewport_scale).round() as i32,
              (screen_height()*config().render_scale*config().world_viewport_scale).round() as i32))
}
pub(crate) fn apply_world_viewport() {
    if let Some((x,y,w,h))=world_viewport() { unsafe { get_internal_gl().quad_context.apply_viewport(x,y,w,h); } }
}
pub(crate) fn prepare_3d(bg:Color){scale::prepare(bg);}
pub(crate) fn display_clear_skipped() -> bool { config().display_clear_skip && config().render_scale < 1. }
pub(crate) fn begin_3d_pass(action:PassAction){scale::begin(action);}
pub(crate) fn set_3d_camera(camera:&Camera3D){scale::camera(camera);}
pub(crate) fn present_3d(){scale::present();}
pub(crate) fn diagnostic_target() -> Option<[u32; 2]> { scale::diagnostic_size() }
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
static CONSTRUCTION_FAILURES: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
pub(super) fn construction_failed(name: &str, error: &anyhow::Error) {
    let count = CONSTRUCTION_FAILURES.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
    *INVENTORY.lock().unwrap() = None;
    eprintln!("[lg/vita-renderer] retained construction failed map={name} error={error:#}; fallback=legacy construction_failures={count}");
}
#[cfg(not(target_os = "vita"))]
pub(crate) mod allmaps;
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
        "construction_failures":CONSTRUCTION_FAILURES.load(std::sync::atomic::Ordering::Relaxed), "world_only_active":world_only(), "inventory":*INVENTORY.lock().unwrap(),"view":*VIEW.lock().unwrap()});
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
    base_element: i32,
    material_order: usize,
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
    indices: i32,
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
    /// The `world_list_submit` pass that last checked this parameter, and its payload's class there.
    list_serial: u32,
    class: u32,
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
    world_buffers: Vec<(BufferId,BufferId)>,
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
    schedule: submit::Schedule,
    list: Option<world_list::WorldList>,
    gxm: Box<gxm::State>,
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
// Own every direct GPU allocation until construction commits to Renderer.
// A shader/PVS error must not leak partial retained resources into legacy fallback.
#[derive(Default)]
struct ConstructionResources {
    shaders: Vec<ShaderId>,
    pipelines: Vec<Pipeline>,
    buffers: Vec<BufferId>,
    committed: bool,
}
impl Drop for ConstructionResources {
    fn drop(&mut self) {
        if self.committed { return; }
        let ctx = unsafe { get_internal_gl() }.quad_context;
        for id in self.buffers.drain(..) { ctx.delete_buffer(id); }
        for id in self.pipelines.drain(..) { ctx.delete_pipeline(id); }
        for id in self.shaders.drain(..) { ctx.delete_shader(id); }
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
        // Buffer payload accounting, not allocator/driver/process high-water RAM.
        let old_scene_geometry_capacity_bytes: usize = scene.batches.iter().chain(&scene.sky_props).map(|b| {
            (b.base.capacity() + b.mesh.vertices.capacity()) * std::mem::size_of::<Vertex>()
            + (b.full_indices.capacity() + b.mesh.indices.capacity()) * 2
            + b.normals.capacity() * std::mem::size_of::<Vec3>()
            + b.detail.iter().map(|d| d.indices.capacity() * 2).sum::<usize>()
            + b.animation.as_ref().map_or(0, |a| a.positions.iter().map(|f| f.capacity() * std::mem::size_of::<Vec3>()).sum::<usize>())
        }).sum();
        let pending_geometry_capacity_bytes: usize = pending.iter().map(|p|
            p.geometry.vertices.capacity() * 52 + p.geometry.indices.capacity() * 2).sum();
        let pending_geometry_initial_capacity_bytes = pending_geometry_capacity_bytes;
        let mut uploaded_geometry_bytes = 0;
        let mut construction_geometry_peak_bytes = old_scene_geometry_capacity_bytes + pending_geometry_capacity_bytes;
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
        let material_keys_by_group = pending.iter().map(|p| format!("{:?}", p.recipe.textures.iter()
            .map(Texture2D::raw_miniquad_id).collect::<Vec<_>>())).collect::<Vec<_>>();
        let material_keys = material_keys_by_group.iter().cloned().collect::<std::collections::BTreeSet<_>>();
        let material_order = material_keys.into_iter().enumerate().map(|(i,k)|(k,i)).collect::<BTreeMap<_,_>>();
        let groups_ms=started.elapsed().as_secs_f64()*1000.;
        println!("[lg/vita-renderer] load groups {:.3} ms recipes={recipes_ms:.3} chunks={chunks_ms:.3} source_chunks={source_chunks} groups={} vertices={} layer_triangles={}",groups_ms,pending.len(),vertices,triangles);
        let upload_start=std::time::Instant::now();
        let mut startup_timing = crate::startup::Phase::new("retained_upload");
        let gl = unsafe { get_internal_gl() };
        let ctx = gl.quad_context;
        crate::frame_profile::shader_creation();
        let shader_timing = crate::startup::WorkTimer::new(crate::startup::Work::ShaderCreation);
        let _shader_name = crate::shader_cache::Name::new("retained-world");
        let mut resources = ConstructionResources::default();
        let shader = ctx
            .new_shader(
                ShaderSource::Glsl {
                    vertex: shader::VERTEX,
                    fragment: &crate::environment::fragment(shader::FRAGMENT),
                },
                shader::meta(),
            )
            .context("retained world shader")?;
        resources.shaders.push(shader);
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
        resources.pipelines.extend(pipelines.iter().copied());
        let mut low_pipelines=Vec::new();
        let low_shader=if c.quality=="low" {
            crate::frame_profile::shader_creation();
            let shader_timing = crate::startup::WorkTimer::new(crate::startup::Work::ShaderCreation);
            let _shader_name = crate::shader_cache::Name::new("retained-world-low");
            let low=ctx.new_shader(ShaderSource::Glsl{vertex:shader::VERTEX,fragment:&shader::low_fragment()},shader::meta())?;
            resources.shaders.push(low);
            drop(shader_timing);
            for cull in [CullFace::Nothing,CullFace::Back,CullFace::Front] {
                crate::frame_profile::pipeline_creation();
                low_pipelines.push(ctx.new_pipeline(&[BufferLayout{stride:52,..Default::default()}],&attributes,low,PipelineParams{depth_test:Comparison::LessOrEqual,depth_write:true,cull_face:cull,front_face_order:FrontFaceOrder::Clockwise,..Default::default()}));
            }
            Some(low)
        }else{None};
        resources.pipelines.extend(low_pipelines.iter().copied());
        #[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
        if crate::sim_maps::probe::world_packets_active() {
            use sha2::{Digest, Sha256};
            crate::sim_maps::probe::world_geometry(pending.iter().map(|p| {
                let mut h = Sha256::new();
                for &index in &p.geometry.indices {
                    let v = &p.geometry.vertices[index as usize];
                    h.update(unsafe { std::slice::from_raw_parts((v as *const WorldVertex).cast::<u8>(), std::mem::size_of::<WorldVertex>()) });
                }
                format!("{:x}", h.finalize())
            }).collect());
        }
        let (packed, allocations) = world_list::pack(pending.iter_mut().map(|p| {
            let key = (c.world_static_merge && submit::static_opaque(&p.recipe)).then_some((p.parameter,p.recipe.rank));
            (key, &mut p.geometry.vertices, &mut p.geometry.indices)
        }), c.world_shared_buffers);
        // Host proof: each draw's range in its shared buffer resolves to the bytes its group had.
        #[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
        if crate::sim_maps::probe::world_packets_active() && !world_list::negative("world-pack-wrong-vertex-offset") {
            use sha2::{Digest, Sha256};
            for (group,&(slot,base,count)) in allocations.iter().enumerate() {
                let (_,v,i)=&packed[slot];
                let mut h = Sha256::new();
                for &index in &i[base as usize..(base+count) as usize] {
                    let vertex = &v[index as usize];
                    h.update(unsafe { std::slice::from_raw_parts((vertex as *const WorldVertex).cast::<u8>(), std::mem::size_of::<WorldVertex>()) });
                }
                assert_eq!(format!("{:x}", h.finalize()), crate::sim_maps::probe::world_geometry_hash(group), "group {group} resolves to other bytes in its buffer");
            }
        }
        #[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
        crate::sim_maps::probe::world_ranges(allocations.iter().map(|&(_,base,count)|(base,count)).collect());
        let packing_capacity_bytes: usize = packed.iter().map(|(_,v,i)|v.capacity()*52+i.capacity()*2).sum();
        let buffers = packed.into_iter().map(|(_,v,i)| {
            let vertex = ctx.new_buffer(BufferType::VertexBuffer, BufferUsage::Immutable, BufferSource::slice(&v));
            let index = ctx.new_buffer(BufferType::IndexBuffer, BufferUsage::Immutable, BufferSource::slice(&i));
            resources.buffers.extend([vertex,index]);
            uploaded_geometry_bytes += v.len()*52+i.len()*2;
            (vertex,index)
        }).collect::<Vec<_>>();
        construction_geometry_peak_bytes = construction_geometry_peak_bytes.max(
            old_scene_geometry_capacity_bytes + pending_geometry_capacity_bytes + packing_capacity_bytes + uploaded_geometry_bytes);
        let mut groups = Vec::new();
        for (((p, texture), (slot, base_element, indices)), material_key) in pending.into_iter().zip(first_textures).zip(allocations).zip(material_keys_by_group) {
            let extent = deform_extent(&p.recipe.deform);
            let (vertex,index) = buffers[slot];
            groups.push(Group {
                base_element, material_order: material_order[&material_key],
                parameter: p.parameter,
                bindings: Bindings {
                    vertex_buffers: vec![vertex],
                    index_buffer: index,
                    images: vec![texture, atlas],
                },
                indices,
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
        resources.shaders.push(sky_shader);
        drop(shader_timing);
        crate::frame_profile::pipeline_creation();
        let sky_pipeline = ctx.new_pipeline(
            &[BufferLayout::default()],
            &[VertexAttribute::new("position", VertexFormat::Float2)],
            sky_shader,
            PipelineParams::default(),
        );
        resources.pipelines.push(sky_pipeline);
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
        resources.buffers.extend(sky_bindings.vertex_buffers.iter().copied());
        resources.buffers.push(sky_bindings.index_buffer);
        #[cfg(not(target_os = "vita"))]
        ensure!(std::env::var("LOOKING_GLASS_RETAINED_FAIL").as_deref() != Ok("after-upload"), "host diagnostic construction failure after upload");
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
        let pvs_with_unknown_bytes: usize = pvs.clusters.iter().chain(std::iter::once(&pvs.unknown))
            .fold(BTreeMap::new(), |mut m, a| {
                m.entry(Arc::as_ptr(a) as usize).or_insert((a.opaque.capacity() + a.alpha.capacity()) * std::mem::size_of::<usize>());
                m
            }).values().sum();
        let staging_bytes = groups.len() * std::mem::size_of::<Prepared>();
        if crate::allocation_census::enabled() {crate::allocation_census::payload("retained_world",serde_json::json!({
            "gpu_geometry_payload_bytes":bytes,"unique_pvs_capacity_bytes":pvs_with_unknown_bytes,
            "uniform_staging_capacity_bytes":staging_bytes,"construction_geometry_capacity_bound":construction_geometry_peak_bytes,
            "scope":"GPU payload is separate from newlib; CPU capacities exclude metadata"}));}
        let construction_buffer_peak_bytes = construction_geometry_peak_bytes.max(
            old_scene_geometry_capacity_bytes + bytes + pvs_with_unknown_bytes + staging_bytes);
        let inventory = serde_json::json!({"groups":groups.len(),"vertices":vertices,"layer_triangles":triangles,"gpu_geometry_bytes":bytes,"cpu_cluster_list_payload_bytes":pvs_bytes,
            "old_scene_geometry_capacity_bytes":old_scene_geometry_capacity_bytes,
            "pending_geometry_initial_capacity_bytes":pending_geometry_initial_capacity_bytes,
            "packing_capacity_bytes":packing_capacity_bytes,
            "shared_geometry_buffers":buffers.len(),
            "construction_geometry_peak_bytes":construction_geometry_peak_bytes,
            "construction_buffer_peak_bytes":construction_buffer_peak_bytes,
            "construction_peak_scope":"conservative packing bound: old scene geometry arrays + initial pending capacity + packed capacity + uploaded geometry payload; final phase includes unique PVS lists and uniform staging. Excludes allocator metadata, textures, BSP/collision, remap trees, recipes and driver copies; not process RAM or a Vita OOM bound",
            "host_pointer_bytes":std::mem::size_of::<usize>(),
            "cpu_cluster_list_payload_including_unknown_bytes":pvs_with_unknown_bytes,
            "vita_cluster_list_payload_including_unknown_bytes":pvs_with_unknown_bytes / std::mem::size_of::<usize>() * 4,
            "skipped_sky_batches":scene.batches.iter().filter(|b| b.sky).count() + scene.sky_props.len(),
            "portals_disabled":true,"frozen_ropes":scene.rope_shapes.len(),
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
        let parameter_count = parameter_recipes.len();
        let parameters = parameter_recipes.into_iter().map(|recipe| {
            let u = uniforms(&recipe, 0., Vec3::ZERO, Mat4::IDENTITY, Mat4::IDENTITY, false, scene.atlas_grid);
            let time_uv = recipe.stage.mods.iter().any(|m| matches!(m,
                TcMod::Scroll(_) | TcMod::Rotate(_) | TcMod::Stretch(_) | TcMod::Turb(_)));
            let time_gain = recipe.stage.rgb.is_some() || recipe.stage.alpha.is_some();
            Parameter { recipe, bits: u.bits(), uniforms: u, generation: 0, model: [0;16],
                time: 0_f32.to_bits(), time_uv, time_gain, list_serial: 0, class: 0 }
        }).collect();
        resources.committed = true;
        let list = c.world_list_submit.then(|| world_list::WorldList::new(&groups, &pipelines, &low_pipelines, parameter_count));
        Ok(Self {
            list,
            gxm: gxm::State::published(gxm::Mode::parse(&c.world_gxm).unwrap_or(gxm::Mode::Off), groups.len(), &shader::meta()),
            _game_thread: crate::game_thread::GameThread::new(),
            parameters, parameter_key: None, parameter_generation: 0,
            field_uniforms: std::array::from_fn(|_| super::uniform_fields::Cache::new(shader::meta())),
            alpha_order: Vec::with_capacity(groups.len()),
            prepared: Vec::with_capacity(groups.len()),
            fx_gpu: None,
            visibility_key: None, visibility_generation: 0,
            visibility: vec![(0, None); groups.len()],
            models: vec![None; groups.len()],
            world_buffers: buffers,
            schedule: submit::Schedule::default(),
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
        let _pass_clock = crate::pass_clock::scope(if alpha { crate::pass_clock::ALPHA_PREPARE } else { crate::pass_clock::WORLD_OPAQUE });
        count("vita_world_geometry_upload_bytes", 0);
        count("vita_world_retransformed_vertices", 0);
        count("vita_world_rebuilt_indices", 0);
        count("vita_world_sorted_triangles", 0);
        let _census_world = crate::phase_census::span(if alpha {crate::phase_census::Region::AlphaPrepare} else {crate::phase_census::Region::WorldOpaque});
        let census_visibility = crate::phase_census::span(crate::phase_census::Region::WorldVisibility);
        let visibility_started = MetricInstant::now();
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
                if let Some(list) = &mut self.list { list.forget_visibility(); }
                self.visibility_generation = 1;
            }
        }
        let mut tests = 0u64;
        let mut hits = 0u64;
        let list_submit = !alpha && self.list.is_some();
        if list_submit {
            let list = self.list.as_mut().unwrap();
            let generation = self.visibility_generation;
            for &id in ids {
                let entry = &mut list.visible[id];
                // Valid models were written by the test that set this generation.
                if entry.static_group && entry.generation == generation {
                    hits += 1;
                    if entry.visible { self.alpha_order.push((entry.depth, id)); }
                    continue;
                }
                let g = &self.groups[id];
                let result = model_matrix(&g.recipe, time, transforms).and_then(|model| {
                    tests += 1;
                    in_frustum(projection * model, g.min, g.max).then(|| {
                        let depth = if crate::render_fx::retained_active() {
                            crate::render_fx::group_depth(model.transform_point3(g.center))
                        } else { (model.transform_point3(g.center) - eye).length_squared() };
                        (depth, model)
                    })
                });
                if entry.static_group {
                    entry.generation = generation;
                    entry.visible = result.is_some();
                    entry.depth = result.map_or(0., |r| r.0);
                }
                self.models[id] = result.map(|(_, model)| model);
                if let Some((depth, _)) = result { self.alpha_order.push((depth, id)); }
            }
        } else {
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
        }
        crate::light_profile::render_add(phase, RenderMetrics { visibility_tests: tests,
            visibility_cpu_ns: visibility_started.elapsed().as_nanos() as u64, ..Default::default() });
        count("vita_world_visibility_cache_hits", hits);
        drop(visibility_span);
        drop(census_visibility);
        if alpha && !config().world_alpha {self.alpha_order.clear();}
        if !alpha && config().opaque_order=="front_to_back" {
            if list_submit { self.list.as_mut().unwrap().sort.apply(&mut self.alpha_order); }
            else { self.alpha_order.sort_unstable_by(|a,b|a.0.total_cmp(&b.0).then_with(||a.1.cmp(&b.1))); }
        }
        if alpha && config().sort_alpha && !crate::render_fx::retained_active() {
            let _sort = crate::frame_profile::span("vita_alpha_group_sort");
            self.alpha_order
                .sort_unstable_by(|a, b| b.0.total_cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
        }
        if !alpha && config().world_material_sort {
            self.schedule.apply(&mut self.alpha_order, &self.groups, projection, self.visibility_generation);
        }
        self.prepared.clear();
        {
            let _census = crate::phase_census::span(crate::phase_census::Region::WorldParameters);
            let parameters_started = MetricInstant::now();
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
            if list_submit {
                let list = self.list.as_mut().unwrap();
                list.begin_parameter_pass(&mut self.parameters);
                let time_bits = time.to_bits();
                for &(_, id) in &self.alpha_order {
                    let draw = &list.draws[id];
                    let index = draw.parameter as usize;
                    let p = &mut self.parameters[index];
                    // Groups sharing a parameter share a model, motion and time: the signature
                    // names the model and gives a moving batch its own. One check a pass answers all.
                    if p.list_serial != list.serial {
                        p.list_serial = list.serial;
                        let model = self.models[id].unwrap();
                        let model_bits = model.to_cols_array().map(f32::to_bits);
                        if p.generation != self.parameter_generation || p.model != model_bits {
                            p.uniforms = uniforms(&p.recipe, time, eye, model, projection, fullbright, scene.atlas_grid);
                            p.uniforms.atmosphere(&scene.atmosphere, eye);
                            p.bits = p.uniforms.bits();
                            p.generation = self.parameter_generation;
                            p.model = model_bits;
                            p.time = time_bits;
                            builds += 1;
                        } else if split && p.time != time_bits {
                            p.update_time(time);
                            time_updates += 1;
                        }
                        // Parameters in the table had their last write of this pass already.
                        let parameters = &self.parameters;
                        let class = list.classes.class(draw.parameter, &parameters[index].bits, |q| &parameters[q as usize].bits);
                        self.parameters[index].class = class;
                    }
                    let p = &self.parameters[index];
                    #[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
                    assert_eq!(p.model, self.models[id].unwrap().to_cols_array().map(f32::to_bits), "groups sharing a parameter must share a model");
                    let texture = if draw.animated {
                        let g = &self.groups[id];
                        g.recipe.textures[g.recipe.stage.frame(time)].raw_miniquad_id()
                    } else { draw.texture };
                    list.items.push(world_list::Item { group: id as u32, texture, uniforms: p.class });
                }
                #[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
                if world_list::negative("world-list-swap-order") && list.items.len() > 12 { list.items.swap(5, 11); }
            } else {
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
                self.prepared.push(Prepared { depth, group: id, texture, uniforms: u, indices: g.indices });
            }
            }
            crate::light_profile::render_add(phase, RenderMetrics {
                parameter_cpu_ns: parameters_started.elapsed().as_nanos() as u64, ..Default::default() });
            count("vita_world_parameter_builds", builds);
            count("vita_world_parameter_time_updates", time_updates);
            count("vita_world_parameter_reuses", self.alpha_order.len() as u64 - builds);
        }
        // Uniform values are program storage. These two programs are private to
        // this Renderer; sky, FX and HUD use other programs. Alpha uses this same
        // cache, including interleaved callbacks, so every write stays observed.
        if !config().world_uniform_persist {
            for cache in &mut self.field_uniforms { cache.interrupt(); }
        }
        if !alpha && config().world_static_merge {
            submit::merge(&mut self.prepared, &self.groups, &self.parameters, projection);
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
        #[cfg(target_os = "vita")]
        if list_submit && self.gxm.proof_due() { self.proof_frame(scene, time); }
        if !alpha && config().sky && !scene.authored_sky {
            let _sky_span=crate::frame_profile::span("vita_world_sky_submit");
            let mut totals = crate::frame_profile::Totals::<3>::profile_only();
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
            #[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
            crate::sim_maps::probe::draw_query::mark("world_opaque","sky",0,None,0,None);
            if !crate::gpu_timing::masked(crate::gpu_timing::MASK_SKY) { ctx.draw(0, 3, 1); }
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
        let _census_submit = crate::phase_census::span(crate::phase_census::Region::WorldSubmit);
        #[cfg(target_os="vita")]
        let _commands=crate::phase_census::world_commands();
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
        let _submission = crate::render_tail::submission(config().world_binding_cache, config().world_gl_state_cache);
        // Schema 7: draw i's last lap to draw i+1's first read, from reads Totals already takes.
        let (mut between_ns, mut between_calls, mut previous_end) = (0u64, 0u32, None::<std::time::Instant>);
        let _pass_submit = (!alpha).then(|| crate::pass_clock::scope(crate::pass_clock::WORLD_SUBMIT));
        if list_submit {
            let mut counts = world_list::Counts::default();
            self.submit_list(time, gxm::Pass::Frame, &mut totals, &mut counts, &mut previous_end, &mut between_ns, &mut between_calls);
            low_draws = counts.low_draws; pipeline_changes = counts.pipeline_changes; texture_changes = counts.texture_changes;
            applications = counts.applications; uniform_traffic = counts.uniform_traffic;
            stats.batches += counts.batches; stats.triangles += counts.triangles;
            #[cfg(target_os = "vita")]
            { static_batches = counts.static_batches; }
        } else {
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
            let mut start = totals.start();
            if let (Some(previous), Some(now)) = (previous_end, start) {
                between_ns += now.duration_since(previous).as_nanos() as u64; between_calls += 1;
            }
            if plan.pipeline { ctx.apply_pipeline(&pipeline); applications[0] += 1; }
            if plan.depth { unsafe{macroquad::miniquad::gl::glDepthMask(if alpha {0}else{1});} applications[1] += 1; }
            scale::log_depth(!alpha);
            if low { low_draws += 1; if !cache_enabled {count("vita_world_low_draws",1);} }
            if plan.bindings { ctx.apply_bindings(&g.bindings); applications[2] += 1; }
            totals.lap(0, &mut start);
            if plan.uniforms {
                let traffic = self.field_uniforms[usize::from(low)].apply(ctx, u.values(), config().uniform_field_cache);
                applications[3] += 1;
                uniform_traffic[0] += u64::from(traffic.calls > 0);
                uniform_traffic[1] += traffic.bytes;
                uniform_traffic[2] += traffic.calls;
            }
            totals.lap(1, &mut start);
            #[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
            if alpha || g.recipe.stage.blend != Blend::Opaque {
                crate::sim_maps::probe::world_order(prepared.group, alpha);
            }
            #[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
            crate::sim_maps::probe::draw_query::mark(
                if alpha {"world_alpha"} else {"world_opaque"},
                if low {"world_low"} else {"world_full"},
                g.recipe.blend, Some(prepared.group), g.recipe.stage.alpha_test,
                g.recipe.stage.images.get(g.recipe.stage.frame(time)).map(String::as_str));
            #[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
            crate::sim_maps::probe::world_packet(prepared.group, g.base_element, prepared.indices, &texture, u.bits_ref(), low, pi as u8);
            let class = if alpha { crate::gpu_timing::MASK_WORLD_ALPHA } else if low { crate::gpu_timing::MASK_WORLD_LOW } else { crate::gpu_timing::MASK_WORLD_FULL };
            if !crate::gpu_timing::masked(class) { ctx.draw(g.base_element, prepared.indices, 1); }
            totals.lap(2, &mut start);
            previous_end = start;
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
                    prepared.indices as u64 / 3,
                );
            }
            stats.batches += 1;
            stats.triangles += prepared.indices as usize / 3;
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
                    prepared.indices as usize / 3,
                    0,
                    g.recipe.model.is_none(),
                );
            }
        }
        }
        if !alpha { crate::pass_clock::draws(stats.batches); }
        drop(_pass_submit);
        if let Some((ns, calls)) = totals.census_values() {
            crate::phase_census::world_publish(ns, calls, totals.profile_active());
            if between_calls > 0 { crate::phase_census::world_between_publish(between_ns, between_calls); }
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
    /// The opaque pass from dense records. `pass` says how: as the frame's own pass, per the key, or
    /// as one trial of a proof frame.
    fn submit_list(&mut self, time: f32, pass: gxm::Pass, totals: &mut crate::frame_profile::Totals<3>, c: &mut world_list::Counts,
                   previous_end: &mut Option<std::time::Instant>, between_ns: &mut u64, between_calls: &mut u32) {
        let Self { list, parameters, field_uniforms, groups, gxm: state, .. } = self;
        let list = list.as_ref().unwrap();
        let _ = &groups;
        // One lookup a pass. Nothing below changes what the context points at.
        let gl = unsafe { get_internal_gl() };
        let ctx = gl.quad_context;
        let mut planner = world_list::Planner::new();
        let (mut previous_pipeline, mut previous_texture) = (None, None);
        let way = state.way(pass);
        let mut drawn = 0u64;
        #[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
        let mut last_written = [None::<usize>; 2];
        #[cfg(target_os = "vita")]
        let mut real = gxm::vita::Real::new();
        for item in &list.items {
            let draw = &list.draws[item.group as usize];
            let low = draw.low;
            let binding = (draw.vertex, draw.index, item.texture, draw.atlas);
            let mut plan = planner.observe(draw.pipeline, low, binding, item.uniforms);
            // The whole point of a class: a skipped write must be a write of equal bits.
            #[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
            {
                let storage = &mut last_written[usize::from(low)];
                if plan.uniforms { *storage = Some(draw.parameter as usize); }
                else { assert_eq!(parameters[storage.expect("a skip before any write")].bits, parameters[draw.parameter as usize].bits, "a uniform class skipped a different payload"); }
            }
            #[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
            if world_list::negative("world-list-stale-uniform") && drawn % 7 == 3 { plan.uniforms = false; }
            drawn += 1;
            #[cfg(target_os = "vita")]
            state.note_draw(pass);
            let mut start = totals.start();
            if let (Some(previous), Some(now)) = (*previous_end, start) {
                *between_ns += now.duration_since(previous).as_nanos() as u64; *between_calls += 1;
            }
            let bits = &parameters[draw.parameter as usize].bits;
            let class = if low { crate::gpu_timing::MASK_WORLD_LOW } else { crate::gpu_timing::MASK_WORLD_FULL };
            #[cfg(target_os = "vita")]
            let direct = way == gxm::Way::Direct && !plan.pipeline && !draw.animated && !crate::gpu_timing::masked(class)
                && state.direct_draw(&mut real, item.group as usize, draw.indices as u32, item.uniforms, low, bits, pass, drawn - 1);
            #[cfg(not(target_os = "vita"))]
            let direct = false;
            if !direct {
                // vitaGL holds what it last wrote, not what a direct draw left. Take every step through it.
                #[cfg(target_os = "vita")]
                if way == gxm::Way::Direct {
                    gxm::vita::uniforms_are_stale();
                    plan.bindings = true;
                    plan.uniforms = true;
                }
                if plan.pipeline { ctx.apply_pipeline(&draw.pipeline); c.applications[0] += 1; }
                if plan.depth { unsafe{macroquad::miniquad::gl::glDepthMask(1);} c.applications[1] += 1; }
                if plan.pipeline { scale::log_depth(true); }
                if plan.bindings {
                    #[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
                    let texture = if world_list::negative("world-list-wrong-texture") && drawn % 11 == 5 { draw.atlas } else { item.texture };
                    #[cfg(not(all(feature="sim_maps_probe",not(target_os="vita"))))]
                    let texture = item.texture;
                    ctx.apply_bindings_from_slice(&[draw.vertex], draw.index, &[texture, draw.atlas]);
                    c.applications[2] += 1;
                }
            }
            if low { c.low_draws += 1; }
            totals.lap(0, &mut start);
            if !direct && plan.uniforms {
                let values = parameters[draw.parameter as usize].uniforms.values();
                let traffic = field_uniforms[usize::from(low)].apply(ctx, values, true);
                c.applications[3] += 1;
                c.uniform_traffic[0] += u64::from(traffic.calls > 0);
                c.uniform_traffic[1] += traffic.bytes;
                c.uniform_traffic[2] += traffic.calls;
            }
            totals.lap(1, &mut start);
            #[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
            {
                let g = &groups[item.group as usize];
                if g.recipe.stage.blend != Blend::Opaque { crate::sim_maps::probe::world_order(item.group as usize, false); }
                crate::sim_maps::probe::draw_query::mark("world_opaque", if low {"world_low"} else {"world_full"},
                    g.recipe.blend, Some(item.group as usize), g.recipe.stage.alpha_test,
                    g.recipe.stage.images.get(g.recipe.stage.frame(time)).map(String::as_str));
            }
            let _ = (time, bits);
            #[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
            let base_element = draw.base_element + i32::from(world_list::negative("world-pack-wrong-base") && drawn % 17 == 9) * 3;
            #[cfg(not(all(feature="sim_maps_probe",not(target_os="vita"))))]
            let base_element = draw.base_element;
            #[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
            crate::sim_maps::probe::world_packet(item.group as usize, base_element, draw.indices, &item.texture,
                parameters[draw.parameter as usize].uniforms.bits_ref(), low, draw.pipeline_class);
            if !direct {
                #[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
                let dropped = world_list::negative("world-list-drop-draw") && drawn % 13 == 6;
                #[cfg(not(all(feature="sim_maps_probe",not(target_os="vita"))))]
                let dropped = false;
                #[cfg(target_os = "vita")]
                if way != gxm::Way::Vitagl { gxm::vita::arm(); }
                let ran = !dropped && !crate::gpu_timing::masked(class);
                if ran { ctx.draw(base_element, draw.indices, 1); }
                #[cfg(target_os = "vita")]
                if way != gxm::Way::Vitagl {
                    let capture = gxm::vita::take();
                    // A masked draw called nothing, so there is nothing to learn from it.
                    if ran { state.after_vitagl(&capture, item.group as usize, draw.animated, draw.pipeline_class, low, draw.indices as u32, item.uniforms, bits, way); }
                }
            }
            totals.lap(2, &mut start);
            *previous_end = start;
            if previous_pipeline != Some(draw.pipeline_class) {
                c.pipeline_changes += 1;
                previous_pipeline = Some(draw.pipeline_class);
            }
            if previous_texture != Some(item.texture) {
                c.texture_changes += 1;
                previous_texture = Some(item.texture);
            }
            c.batches += 1;
            c.triangles += draw.indices as usize / 3;
            #[cfg(target_os = "vita")]
            { c.static_batches += usize::from(draw.static_group); }
        }
        // The next vitaGL draw follows a pipeline apply, which marks uniforms dirty. This makes it
        // not depend on that.
        #[cfg(target_os = "vita")]
        if way == gxm::Way::Direct { gxm::vita::uniforms_are_stale(); }
    }
    /// Same frame, same uniforms, twice into the target: vitaGL, then the direct path, then bytes
    /// compared. Animation cannot differ, so any difference is the path's. The frame then starts
    /// over on a clear of its own. Diagnostic arms only.
    #[cfg(target_os = "vita")]
    fn proof_frame(&mut self, scene: &Scene, time: f32) {
        let bg = scene.atmosphere.background();
        let (x, y, w, h) = scale::target_rect();
        let clear = || PassAction::Clear { color: Some((bg.r, bg.g, bg.b, 1.)), depth: Some(1.), stencil: None };
        let mut totals = crate::frame_profile::Totals::<3>::profile_only();
        let (mut end, mut ns, mut calls) = (None, 0, 0);
        let read = |target: &mut Vec<u8>| unsafe {
            extern "C" { fn glReadPixels(x: i32, y: i32, w: i32, h: i32, format: u32, kind: u32, data: *mut std::ffi::c_void); }
            target.resize((w * h * 4) as usize, 0);
            glReadPixels(x, y, w, h, 0x1908, 0x1401, target.as_mut_ptr().cast());
        };
        let (mut a, mut b) = self.gxm.take_proof_buffers();
        // Mid-game allocation. Short of memory, skip the proof and say so rather than abort.
        let bytes = (w * h * 4) as usize;
        if a.try_reserve(bytes.saturating_sub(a.len())).is_err() || b.try_reserve(bytes.saturating_sub(b.len())).is_err() {
            self.gxm.give_proof_buffers(a, b);
            self.gxm.proof_skipped();
            return;
        }
        for (pass, target) in [(gxm::Pass::ProofVitagl, &mut a), (gxm::Pass::ProofSecond, &mut b)] {
            begin_3d_pass(clear());
            apply_world_viewport();
            let mut counts = world_list::Counts::default();
            self.submit_list(time, pass, &mut totals, &mut counts, &mut end, &mut ns, &mut calls);
            read(target);
        }
        self.gxm.proof_result(&a, &b);
        self.gxm.give_proof_buffers(a, b);
        begin_3d_pass(clear());
        apply_world_viewport();
    }
    pub(super) fn begin_fx(&mut self) {
        self.prepared.clear();
    }
    pub(super) fn finish_fx(&mut self, interop_flush: bool) -> (usize, usize) {
        let _pass_clock = crate::pass_clock::scope(crate::pass_clock::FX_FINISH);
        let mut gpu = self.fx_gpu.take().unwrap_or_else(|| {
            crate::render_fx::retained::Gpu::new().expect("retained actor shader")
        });
        let world = self
            .prepared
            .iter()
            .enumerate()
            .map(|(id, p)| (p.depth, id))
            .collect::<Vec<_>>();
        let totals = std::cell::RefCell::new(crate::frame_profile::Totals::<3>::profile_only());
        let result = crate::render_fx::retained::finish_world(&mut gpu, &world, interop_flush, |id| {
            let _census = crate::phase_census::span(crate::phase_census::Region::AlphaSubmit);
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
            #[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
            crate::sim_maps::probe::world_order(p.group,true);
            #[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
            crate::sim_maps::probe::draw_query::mark("world_alpha","world_full",
                g.recipe.blend, Some(p.group), g.recipe.stage.alpha_test,
                g.recipe.stage.images.first().map(String::as_str));
            #[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
            crate::sim_maps::probe::world_packet(p.group, g.base_element, g.indices, &p.texture, u.bits_ref(), false, (g.recipe.blend * 3 + g.recipe.cull) as u8);
            if !crate::gpu_timing::masked(crate::gpu_timing::MASK_WORLD_ALPHA) { ctx.draw(g.base_element, g.indices, 1); }
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
        #[cfg(target_os = "vita")]
        if self.gxm.direct_used() { gxm::vita::wait_for_gpu(); }
        gxm::withdraw(&mut self.gxm);
        let gl = unsafe { get_internal_gl() };
        let ctx = gl.quad_context;
        for &(vertex,index) in &self.world_buffers {
            ctx.delete_buffer(vertex); ctx.delete_buffer(index);
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
    fn accepted_light_and_npc_defaults_are_independent_strict_booleans() {
        let fields = ["lights_static_cache", "sim_npc_sight_candidates",
                      "sim_npc_sight_cache", "sim_npc_target_stack"];
        for config in [Config::default(), serde_json::from_str("{}").unwrap()] {
            let defaults = serde_json::to_value(config).unwrap();
            for field in fields { assert_eq!(defaults[field], true); }
            assert_eq!(defaults["sim_dynamic_buckets"], false);
        }
        for name in fields.into_iter().chain(["sim_dynamic_buckets"]) {
            for invalid in [serde_json::json!(1), serde_json::json!("on"), serde_json::Value::Null] {
                assert!(serde_json::from_value::<Config>(serde_json::json!({name:invalid})).is_err());
            }
            for enabled in [false, true] {
                let config: Config = serde_json::from_value(serde_json::json!({name:enabled})).unwrap();
                config.validate().unwrap();
                let actual = serde_json::to_value(config).unwrap();
                for other in fields.into_iter().chain(["sim_dynamic_buckets"]) {
                    assert_eq!(actual[other], if other == name { enabled } else { other != "sim_dynamic_buckets" });
                }
            }
        }
    }
    #[test]
    fn backend_report_interval_defaults_to_ten_and_strict_types() {
        for (raw,want) in [("{}",Some(10)),(r#"{"log_frame_time_every":10}"#,Some(10)),(r#"{"log_frame_time_every":1}"#,Some(1)),(r#"{"log_frame_time_every":0}"#,None),(r#"{"log_frame_time_every":null}"#,None)] {
            let c:Config=serde_json::from_str(raw).unwrap();
            assert_eq!(c.log_frame_time_every.filter(|&n|n>0),want);
        }
        assert_eq!(Config::default().log_frame_time_every, Some(10));
        for value in [serde_json::json!(-1),serde_json::json!(1.5),serde_json::json!(true),serde_json::json!("off"),serde_json::json!(4294967296u64)] {
            assert!(serde_json::from_value::<Config>(serde_json::json!({"log_frame_time_every":value})).is_err());
        }
    }
    #[test]
    fn runtime_report_interval_rejects_only_bad_key_and_preserves_flags() {
        for (raw, want) in [("10", Some(10)), ("1", Some(1)), ("0", None), ("null", None)] {
            let bytes = format!(r#"{{"log_frame_time_every":{raw},"world_state_cache":false,"skin_overlap":true,"audio_static_cache":false,"sim_trigger_facts_reuse":true}}"#);
            let (c, rejected) = parse_runtime_config(bytes.as_bytes()).unwrap();
            assert!(rejected.is_none());
            assert_eq!(c.log_frame_time_every.filter(|&n| n > 0), want);
            assert!(!c.world_state_cache && c.skin_overlap && !c.audio_static_cache && c.sim_trigger_facts_reuse);
        }
        for raw in [r#""0""#, "-1", "1.5", "true", "4294967296"] {
            let bytes = format!(r#"{{"log_frame_time_every":{raw},"world_state_cache":false,"skin_overlap":true,"audio_static_cache":false,"sim_trigger_facts_reuse":true}}"#);
            let (c, rejected) = parse_runtime_config(bytes.as_bytes()).unwrap();
            assert!(rejected.unwrap().contains("rejecting key, effective value=10;"));
            assert_eq!(c.log_frame_time_every, Some(10));
            assert!(!c.world_state_cache && c.skin_overlap && !c.audio_static_cache && c.sim_trigger_facts_reuse);
        }
        let (c, rejected) = parse_runtime_config(b"{}").unwrap();
        assert!(rejected.is_none() && c.log_frame_time_every == Some(10) && c.sim_trigger_facts_reuse);
        assert!(parse_runtime_config(br#"{"log_frame_time_every":"bad","typo":true}"#).is_err());
        assert!(parse_runtime_config(br#"{"log_frame_time_every":"bad","world_state_cache":"bad"}"#).is_err());
    }
    #[test]
    fn g2_keys_default_off_validate_and_are_printed() {
        let defaults = Config::default();
        assert_eq!(defaults.gpu_timing, "off");
        assert!(defaults.gpu_mask.is_empty() && defaults.rt_scenes.is_none() && defaults.param_buffer_mib.is_none() && !defaults.display_clear_skip && !defaults.fx_attached_sphere && !defaults.fx_bubble_true_size);
        defaults.validate().unwrap();
        let record = config_log_lines(&defaults).join("\n");
        for key in ["\"gpu_timing\":\"off\"", "\"gpu_mask\":[]", "\"rt_scenes\":null", "\"param_buffer_mib\":null", "\"display_clear_skip\":false", "\"fx_attached_sphere\":false", "\"fx_bubble_true_size\":false"] {
            assert!(record.contains(key), "{key} missing from {record}");
        }
        for (good, text) in [
            (true, r#"{"gpu_timing":"backlog"}"#), (true, r#"{"gpu_timing":"poll"}"#), (true, r#"{"gpu_timing":"wait"}"#),
            (false, r#"{"gpu_timing":"on"}"#), (false, r#"{"gpu_timing":""}"#),
            (true, r#"{"gpu_mask":["sky","fx_alpha"]}"#), (false, r#"{"gpu_mask":["bubbles"]}"#),
            (true, r#"{"rt_scenes":[2,1]}"#), (false, r#"{"rt_scenes":[0,1]}"#), (false, r#"{"rt_scenes":[9,1]}"#),
            (true, r#"{"param_buffer_mib":32}"#), (false, r#"{"param_buffer_mib":8}"#), (false, r#"{"param_buffer_mib":128}"#),
            (true, r#"{"display_clear_skip":true}"#),
        ] {
            let parsed: Config = serde_json::from_str(text).unwrap();
            assert_eq!(parsed.validate().is_ok(), good, "{text}");
        }
    }
    #[test]
    fn config_record_stays_one_json_document_and_cadence_has_its_own_record() {
        let tag = "[lg/vita-renderer] config ";
        for (raw, cadence) in [("{}", "10"), (r#"{"log_frame_time_every":0}"#, "off"), (r#"{"log_frame_time_every":null}"#, "off"), (r#"{"log_frame_time_every":3,"sim_trigger_facts_reuse":true}"#, "3")] {
            let (c, _) = parse_runtime_config(raw.as_bytes()).unwrap();
            let [config, report] = config_log_lines(&c);
            // The auditors' parser: everything after the tag is one JSON document.
            let (_, rest) = config.split_once(tag).unwrap();
            let printed: serde_json::Value = serde_json::from_str(rest).unwrap();
            assert_eq!(printed, serde_json::to_value(&c).unwrap());
            assert!(!report.contains(tag) && !report.contains('{'));
            assert_eq!(report, format!("[lg/vita-renderer] report_cadence effective_log_frame_time_every={cadence}"));
        }
    }
    #[test]
    fn trigger_facts_reuse_defaults_on_and_requires_a_boolean() {
        for c in [Config::default(), serde_json::from_str::<Config>("{}").unwrap()] {
            assert!(c.sim_trigger_facts_reuse);
        }
        for enabled in [false, true] {
            let c: Config = serde_json::from_value(serde_json::json!({"sim_trigger_facts_reuse": enabled})).unwrap();
            c.validate().unwrap(); assert_eq!(c.sim_trigger_facts_reuse, enabled);
            assert_eq!(serde_json::to_value(&c).unwrap()["sim_trigger_facts_reuse"], enabled);
        }
        for json in [r#"{"sim_trigger_facts_reuse":1}"#, r#"{"sim_trigger_facts_reuse":"on"}"#, r#"{"sim_trigger_facts_reuse":null}"#] {
            assert!(serde_json::from_str::<Config>(json).is_err());
        }
    }
    #[test]
    fn publication_occurrences_default_on_require_a_boolean_and_are_printed() {
        for c in [Config::default(), serde_json::from_str::<Config>("{}").unwrap()] {
            assert!(c.actor_publication_occurrences);
        }
        for enabled in [false, true] {
            let (c, rejected) = parse_runtime_config(serde_json::json!({"actor_publication_occurrences": enabled}).to_string().as_bytes()).unwrap();
            assert!(rejected.is_none());
            c.validate().unwrap(); assert_eq!(c.actor_publication_occurrences, enabled);
            // Prerequisites keep their own defaults; the occurrence switch never activates single preparation.
            assert!(c.actor_single_prepare_active());
            let [config, _] = config_log_lines(&c);
            let printed: serde_json::Value = serde_json::from_str(config.split_once("[lg/vita-renderer] config ").unwrap().1).unwrap();
            assert_eq!(printed["actor_publication_occurrences"], enabled);
        }
        for json in [r#"{"actor_publication_occurrences":1}"#, r#"{"actor_publication_occurrences":"on"}"#, r#"{"actor_publication_occurrences":null}"#] {
            assert!(serde_json::from_str::<Config>(json).is_err());
        }
    }
    #[test]
    fn gather_reuse_levers_default_on_require_a_boolean_and_are_printed() {
        for key in ["actor_publication_uv_reuse", "prop_occurrence_reuse"] {
            for c in [Config::default(), serde_json::from_str::<Config>("{}").unwrap()] {
                assert_eq!(serde_json::to_value(&c).unwrap()[key], true, "{key} default");
            }
            for enabled in [false, true] {
                let (c, rejected) = parse_runtime_config(serde_json::json!({key: enabled}).to_string().as_bytes()).unwrap();
                assert!(rejected.is_none());
                c.validate().unwrap();
                let [config, _] = config_log_lines(&c);
                let printed: serde_json::Value = serde_json::from_str(config.split_once("[lg/vita-renderer] config ").unwrap().1).unwrap();
                assert_eq!(printed[key], enabled);
            }
            for value in ["1", r#""on""#, "null"] {
                assert!(serde_json::from_str::<Config>(&format!("{{\"{key}\":{value}}}")).is_err());
            }
        }
    }
    #[test]
    fn fx_observe_move_defaults_on_requires_a_boolean_and_is_printed() {
        for c in [Config::default(), serde_json::from_str::<Config>("{}").unwrap()] {
            assert!(c.fx_observe_move);
            let [config, _] = config_log_lines(&c);
            assert!(config.contains("\"fx_observe_move\":true"), "{config}");
        }
        for enabled in [false, true] {
            let c: Config = serde_json::from_value(serde_json::json!({"fx_observe_move": enabled})).unwrap();
            c.validate().unwrap(); assert_eq!(c.fx_observe_move, enabled);
            assert!(c.actor_single_prepare_active());
            assert_eq!(serde_json::to_value(&c).unwrap()["fx_observe_move"], enabled);
            let [config, _] = config_log_lines(&c);
            assert!(config.contains(&format!("\"fx_observe_move\":{enabled}")), "{config}");
        }
        for json in [r#"{"fx_observe_move":1}"#, r#"{"fx_observe_move":"on"}"#, r#"{"fx_observe_move":null}"#] {
            assert!(serde_json::from_str::<Config>(json).is_err());
        }
        // The accessor reports the requested state, so a suite arm cannot be vacuous.
        for enabled in [false, true] {
            assert_eq!(crate::sim_maps::with_fx_observe_move(enabled, crate::sim_maps::fx_observe_move), enabled);
        }
    }
    #[test]
    fn actor_deferred_layers_defaults_on_requires_a_boolean_and_is_printed() {
        for c in [Config::default(), serde_json::from_str::<Config>("{}").unwrap()] {
            assert!(c.actor_deferred_layers);
            let [config, _] = config_log_lines(&c);
            assert!(config.contains("\"actor_deferred_layers\":true"), "{config}");
        }
        for enabled in [false, true] {
            let c: Config = serde_json::from_value(serde_json::json!({"actor_deferred_layers": enabled})).unwrap();
            c.validate().unwrap(); assert_eq!(c.actor_deferred_layers, enabled);
            // The flag is independent of the direct-path prerequisites.
            assert!(c.actor_single_prepare_active());
            assert_eq!(serde_json::to_value(&c).unwrap()["actor_deferred_layers"], enabled);
            let [config, _] = config_log_lines(&c);
            assert!(config.contains(&format!("\"actor_deferred_layers\":{enabled}")), "{config}");
        }
        for json in [r#"{"actor_deferred_layers":1}"#, r#"{"actor_deferred_layers":"on"}"#, r#"{"actor_deferred_layers":null}"#] {
            assert!(serde_json::from_str::<Config>(json).is_err());
        }
    }
    #[test]
    fn hatter_door_candidates_default_on_and_require_a_boolean() {
        for c in [Config::default(), serde_json::from_str::<Config>("{}").unwrap()] {
            assert!(c.sim_hatter_door_candidates);
        }
        for enabled in [false, true] {
            let c: Config = serde_json::from_value(serde_json::json!({"sim_hatter_door_candidates": enabled})).unwrap();
            c.validate().unwrap(); assert_eq!(c.sim_hatter_door_candidates, enabled);
        }
        for json in [r#"{"sim_hatter_door_candidates":1}"#, r#"{"sim_hatter_door_candidates":"on"}"#] {
            assert!(serde_json::from_str::<Config>(json).is_err());
        }
    }
    #[test]
    fn sight_query_reuse_defaults_on_and_requires_a_boolean() {
        for c in [Config::default(), serde_json::from_str::<Config>("{}").unwrap()] {
            assert!(c.sim_sight_query_reuse);
        }
        for enabled in [false, true] {
            let c: Config = serde_json::from_value(serde_json::json!({"sim_sight_query_reuse": enabled})).unwrap();
            c.validate().unwrap(); assert_eq!(c.sim_sight_query_reuse, enabled);
            assert_eq!(serde_json::to_value(&c).unwrap()["sim_sight_query_reuse"], enabled);
        }
        for json in [r#"{"sim_sight_query_reuse":1}"#, r#"{"sim_sight_query_reuse":"on"}"#,
                     r#"{"sim_sight_query_reuse":null}"#] {
            assert!(serde_json::from_str::<Config>(json).is_err());
        }
    }
    #[test]
    fn round_two_switches_default_on_after_console_acceptance_and_validate() {
        for c in [Config::default(), serde_json::from_str::<Config>("{}").unwrap()] {
            assert!(c.sim_triangle_candidates && c.sim_fan_candidates && c.fx_merge_local_reuse);
        }
        let c: Config = serde_json::from_str(r#"{"sim_triangle_candidates":false,"sim_fan_candidates":false,"fx_merge_local_reuse":false}"#).unwrap();
        c.validate().unwrap(); assert!(!c.sim_triangle_candidates && !c.sim_fan_candidates && !c.fx_merge_local_reuse);
        for json in [r#"{"sim_triangle_candidates":1}"#, r#"{"sim_fan_candidates":"on"}"#] {
            assert!(serde_json::from_str::<Config>(json).is_err());
        }
    }
    #[test]
    fn simulation_switches_default_on_after_console_acceptance() {
        for c in [Config::default(), serde_json::from_str::<Config>("{}").unwrap()] {
            assert!(c.sim_school_mesh && c.sim_tower_templates
                && c.sim_npc_queries && c.sim_mechanism_state);
        }
        let c: Config = serde_json::from_str(r#"{"sim_school_mesh":true,"sim_tower_templates":true,"sim_npc_queries":true,"sim_mechanism_state":true}"#).unwrap();
        c.validate().unwrap();
        assert!(c.sim_school_mesh && c.sim_tower_templates && c.sim_npc_queries && c.sim_mechanism_state);
    }
    #[test]
    fn map_selection_defaults_and_precedence() {
        let mut c: Config = serde_json::from_str("{}").unwrap();
        assert_eq!(c.selection("gvillage"), (true, "listed"));
        assert_eq!(c.selection("skool2"), (true, "listed"));
        c.retained_maps = vec!["*".into()];
        assert_eq!(c.selection("skool2"), (true, "listed"));
        c.legacy_maps = vec!["skool2".into()];
        assert_eq!(c.selection("skool2"), (false, "excluded"));
        assert_eq!(c.selection("gvillage"), (true, "listed"));
        c.renderer = "legacy".into();
        assert_eq!(c.selection("gvillage"), (false, "configuration"));
        c.renderer = "vita".into(); c.retained_maps.clear();
        assert_eq!(c.selection("gvillage"), (false, "unlisted"));
        c.legacy_maps = vec!["*".into()];
        assert_eq!(c.selection("gvillage"), (false, "excluded"));
        c.validate().unwrap();
    }
    #[test]
    fn invalid_map_entries_preserve_other_entries_and_settings() {
        let c: Config = serde_json::from_str(r#"{"retained_maps":["gvillage","../base","skool2","","Upper"],"legacy_maps":["tower2","bad-name"],"render_scale":0.75,"quality":"low"}"#).unwrap();
        c.validate().unwrap();
        assert_eq!(c.retained_maps, ["gvillage", "skool2"]);
        assert_eq!(c.legacy_maps, ["tower2"]);
        assert_eq!(c.render_scale, 0.75);
        assert_eq!(c.quality, "low");
        assert_eq!(c.selection("tower2"), (false, "excluded"));
        assert_eq!(c.selection("hatter1"), (false, "unlisted"));
        assert!(serde_json::from_str::<Config>(r#"{"retained_maps":["bad-name"],"typo":true}"#).is_err());
        assert!(serde_json::from_str::<Config>(r#"{"retained_maps":[3]}"#).is_err());
    }
    #[test]
    fn unknown_map_names_check_the_mounted_inventory_and_skip_wildcards() {
        let c: Config = serde_json::from_str(r#"{"retained_maps":["*","gvillage","typo"],"legacy_maps":["skool2","missing"]}"#).unwrap();
        assert_eq!(c.unknown_maps(|name| ["gvillage", "skool2"].contains(&name)),
            [("retained_maps", "typo"), ("legacy_maps", "missing")]);
    }
    #[test]
    fn world_submission_switches_are_independent_strict_and_default_off() {
        let fields=["world_material_sort","world_static_merge","world_uniform_persist",
                    "world_binding_cache","world_gl_state_cache"];
        let defaults=serde_json::to_value(Config::default()).unwrap();
        for field in fields {
            assert_eq!(defaults[field],false);
            let mut input=serde_json::json!({}); input[field]=true.into();
            let c:Config=serde_json::from_value(input.clone()).unwrap();c.validate().unwrap();
            let actual=serde_json::to_value(c).unwrap();
            for other in fields {assert_eq!(actual[other],other==field);}
            for bad in [serde_json::json!(1),serde_json::json!("on"),serde_json::Value::Null] {
                input[field]=bad;assert!(serde_json::from_value::<Config>(input.clone()).is_err());
            }
        }
    }
    #[test]
    fn r26_levers_default_off_require_a_boolean_and_are_printed() {
        for key in crate::sim_maps::R26_LEVERS {
            let defaults = serde_json::to_value(Config::default()).unwrap();
            assert_eq!(defaults[key], false, "{key}");
            let on: Config = serde_json::from_value(serde_json::json!({key: true})).unwrap();
            on.validate().unwrap();
            assert_eq!(serde_json::to_value(&on).unwrap()[key], true, "{key}");
            for c in [&on, &Config::default()] {
                let line = &config_log_lines(c)[0];
                assert!(line.contains(&format!("\"{key}\":")), "{key} missing from the printed record");
            }
            for bad in [serde_json::json!(1), serde_json::json!("on"), serde_json::Value::Null] {
                assert!(serde_json::from_value::<Config>(serde_json::json!({key: bad})).is_err(), "{key}");
            }
        }
    }
    #[test]
    fn world_submission_keys_default_off_require_a_boolean_and_are_printed() {
        for key in ["world_pass_clock", "world_list_submit", "world_shared_buffers"] {
            let defaults = serde_json::to_value(Config::default()).unwrap();
            assert_eq!(defaults[key], false, "{key}");
            let on: Config = serde_json::from_value(serde_json::json!({key: true})).unwrap();
            on.validate().unwrap();
            assert!(config_log_lines(&on)[0].contains(&format!("\"{key}\":true")), "{key}");
            assert!(config_log_lines(&Config::default())[0].contains(&format!("\"{key}\":false")), "{key}");
            for bad in [serde_json::json!(1), serde_json::json!("on"), serde_json::Value::Null] {
                assert!(serde_json::from_value::<Config>(serde_json::json!({key: bad})).is_err(), "{key}");
            }
        }
    }
    #[test]
    fn skin_stream_defaults_off_and_needs_its_path() {
        assert!(!Config::default().skin_stream);
        let on: Config = serde_json::from_value(serde_json::json!({"skin_stream": true, "skin_overlap": true, "actor_single_prepare": true, "actor_layer_reuse": false})).unwrap();
        on.validate().unwrap();
        for (key, value) in [("skin_overlap", false), ("actor_single_prepare", false), ("actor_layer_reuse", true)] {
            let mut json = serde_json::json!({"skin_stream": true, "skin_overlap": true, "actor_single_prepare": true, "actor_layer_reuse": false});
            json[key] = serde_json::json!(value);
            let bad: Config = serde_json::from_value(json).unwrap();
            assert!(bad.validate().is_err(), "{key}");
        }
    }
    #[test]
    fn worker_spin_defaults_off_and_is_bounded() {
        assert_eq!(Config::default().worker_spin_us, 0);
        for (value, fine) in [(0, true), (500, true), (1000, true), (1001, false)] {
            let config: Config = serde_json::from_value(serde_json::json!({"worker_spin_us": value})).unwrap();
            assert_eq!(config.validate().is_ok(), fine, "{value}");
        }
        assert!(serde_json::from_str::<Config>(r#"{"worker_spin_us":true}"#).is_err());
    }
    #[test]
    fn sim_sweep_memo_defaults_off_and_requires_a_boolean() {
        assert!(!Config::default().sim_sweep_memo);
        let on: Config = serde_json::from_value(serde_json::json!({"sim_sweep_memo": true})).unwrap();
        on.validate().unwrap();
        assert!(on.sim_sweep_memo);
        for bad in [r#"{"sim_sweep_memo":1}"#, r#"{"sim_sweep_memo":"on"}"#, r#"{"sim_sweep_memo":null}"#] {
            assert!(serde_json::from_str::<Config>(bad).is_err(), "{bad}");
        }
    }
    #[test]
    fn tls_fast_defaults_off_and_names_its_modes() {
        assert_eq!(Config::default().tls_fast, "off");
        assert!(config_log_lines(&Config::default())[0].contains("\"tls_fast\":\"off\""));
        for mode in ["off", "on", "shadow", "shadow_sabotage"] {
            let config: Config = serde_json::from_value(serde_json::json!({"tls_fast": mode})).unwrap();
            config.validate().unwrap();
        }
        let bad: Config = serde_json::from_value(serde_json::json!({"tls_fast": "yes"})).unwrap();
        assert!(bad.validate().is_err());
        assert!(serde_json::from_value::<Config>(serde_json::json!({"tls_fast": true})).is_err());
    }
    #[test]
    fn world_gxm_defaults_off_requires_the_list_and_names_its_modes() {
        assert_eq!(Config::default().world_gxm, "off");
        assert!(config_log_lines(&Config::default())[0].contains("\"world_gxm\":\"off\""));
        for mode in ["shadow", "live", "proof", "proof_control", "proof_sabotage"] {
            let without: Config = serde_json::from_value(serde_json::json!({"world_gxm": mode})).unwrap();
            assert!(without.validate().is_err(), "{mode}");
            let with: Config = serde_json::from_value(serde_json::json!({"world_gxm": mode, "world_list_submit": true})).unwrap();
            with.validate().unwrap();
        }
        let masked: Config = serde_json::from_value(serde_json::json!({"world_gxm": "shadow", "world_list_submit": true, "gpu_mask": ["world_low"]})).unwrap();
        assert!(masked.validate().unwrap_err().to_string().contains("gpu_mask"));
        let bad: Config = serde_json::from_value(serde_json::json!({"world_gxm": "on", "world_list_submit": true})).unwrap();
        assert!(bad.validate().is_err());
        assert!(serde_json::from_value::<Config>(serde_json::json!({"world_gxm": true})).is_err());
    }
    #[test]
    fn world_list_submit_rejects_each_setting_it_cannot_honour() {
        let on = serde_json::json!({"world_list_submit": true});
        for (key, value) in [("world_state_cache", false), ("world_view_reuse", false), ("world_parameter_reuse", false),
                             ("uniform_field_cache", false), ("world_static_merge", true), ("world_binding_cache", true), ("world_gl_state_cache", true)] {
            let mut bad = on.clone();
            bad[key] = serde_json::json!(value);
            let config: Config = serde_json::from_value(bad).unwrap();
            assert!(config.validate().is_err(), "{key}");
        }
        serde_json::from_value::<Config>(on).unwrap().validate().unwrap();
        let both: Config = serde_json::from_value(serde_json::json!({"world_shared_buffers": true, "world_static_merge": true})).unwrap();
        assert!(both.validate().is_err());
    }
    #[test]
    fn audio_static_cache_defaults_on_as_measured() {
        assert!(Config::default().audio_static_cache);
        assert!(!serde_json::from_str::<Config>(r#"{"audio_static_cache":false}"#).unwrap().audio_static_cache);
        assert!(serde_json::from_str::<Config>(r#"{"audio_static_cache":true}"#).unwrap().audio_static_cache);
        assert!(serde_json::from_str::<Config>(r#"{"audio_static_cache":"on"}"#).is_err());
    }
    #[test]
    fn audio_offmain_is_opt_in_and_requires_the_pcm_producer() {
        assert!(!Config::default().audio_offmain);
        let c:Config=serde_json::from_str(r#"{"audio_offmain":true}"#).unwrap();
        assert!(c.validate().is_err());
        let c:Config=serde_json::from_str(r#"{"audio_offmain":true,"audio_pcm_worker":true}"#).unwrap();
        c.validate().unwrap();
        assert!(serde_json::from_str::<Config>(r#"{"audio_offmain":"on"}"#).is_err());
    }
    #[test]
    fn a3_loading_config_is_rc1_with_the_two_load_keys_on() {
        let (rc1, rejected) = parse_runtime_config(include_bytes!("../../configs/PL-rc1.json")).unwrap();
        assert!(rejected.is_none());
        let bytes = include_bytes!("../../configs/PL-a3-loading.json");
        let (a3, rejected) = parse_runtime_config(bytes).unwrap();
        assert!(rejected.is_none());
        a3.validate().unwrap();
        let written: serde_json::Value = serde_json::from_slice(bytes).unwrap();
        let all = serde_json::to_value(Config::default()).unwrap();
        let keys = |v: &serde_json::Value| { let mut k: Vec<_> = v.as_object().unwrap().keys().cloned().collect(); k.sort(); k };
        assert_eq!(keys(&written), keys(&all), "the A3 config writes every runtime key");
        assert!(!rc1.loading_screen && !rc1.collision_geometry_share, "RC1 keeps both off");
        assert!(a3.loading_screen && a3.collision_geometry_share);
        let (mut rc1, mut a3) = (serde_json::to_value(&rc1).unwrap(), serde_json::to_value(&a3).unwrap());
        for key in ["loading_screen", "collision_geometry_share"] {
            rc1.as_object_mut().unwrap().remove(key);
            a3.as_object_mut().unwrap().remove(key);
        }
        assert_eq!(a3, rc1, "the A3 file differs from PL-rc1 only in the two load keys");
    }
    #[test]
    fn packaged_r3_config_contains_exactly_all_runtime_defaults() {
        let bytes = include_bytes!("../../configs/PL-def-r3.json");
        let c: Config = serde_json::from_slice(bytes).unwrap();
        c.validate().unwrap();
        assert_eq!(serde_json::from_slice::<serde_json::Value>(bytes).unwrap(), serde_json::to_value(Config::default()).unwrap());
    }
    #[test]
    fn human_play_configs_differ_only_in_the_two_reuse_levers() {
        let on = include_bytes!("../../configs/PL-r21-human.json");
        let off = include_bytes!("../../configs/PL-r21-human-levers-off.json");
        let (c_on, rejected) = parse_runtime_config(on).unwrap();
        assert!(rejected.is_none());
        c_on.validate().unwrap();
        assert!(c_on.sim_trigger_facts_reuse && c_on.sim_sight_query_reuse && c_on.log_frame_time_every.filter(|&n| n > 0).is_none());
        let (c_off, rejected) = parse_runtime_config(off).unwrap();
        assert!(rejected.is_none());
        c_off.validate().unwrap();
        assert!(!c_off.sim_trigger_facts_reuse && !c_off.sim_sight_query_reuse);
        let mut v_on = serde_json::to_value(&c_on).unwrap();
        let mut v_off = serde_json::to_value(&c_off).unwrap();
        for key in ["sim_trigger_facts_reuse", "sim_sight_query_reuse"] {
            v_on.as_object_mut().unwrap().remove(key);
            v_off.as_object_mut().unwrap().remove(key);
        }
        assert_eq!(v_on, v_off);
    }
    #[test]
    fn r22_human_play_configs_differ_only_in_the_five_levers() {
        const LEVERS: [&str; 5] = ["sim_trigger_facts_reuse", "sim_sight_query_reuse", "sim_hatter_door_candidates",
            "actor_publication_occurrences", "actor_deferred_layers"];
        let parse = |bytes: &[u8]| {
            let (c, rejected) = parse_runtime_config(bytes).unwrap();
            assert!(rejected.is_none());
            c.validate().unwrap();
            assert!(c.log_frame_time_every.filter(|&n| n > 0).is_none(), "human play runs without the frame-time log");
            serde_json::to_value(&c).unwrap()
        };
        let mut on = parse(include_bytes!("../../configs/PL-r22-human.json"));
        let mut off = parse(include_bytes!("../../configs/PL-r22-human-levers-off.json"));
        let r21 = parse(include_bytes!("../../configs/PL-r21-human.json"));
        for key in LEVERS {
            assert_eq!(on[key], true, "{key} on");
            assert_eq!(off[key], false, "{key} off");
            assert_eq!(serde_json::to_value(Config::default()).unwrap()[key], true, "{key} shipped default");
        }
        // r22 human = r21 human plus the three new levers.
        let mut r21_plus = r21.clone();
        for key in ["sim_hatter_door_candidates", "actor_publication_occurrences", "actor_deferred_layers"] {
            r21_plus[key] = serde_json::json!(true);
        }
        assert_eq!(on, r21_plus);
        for key in LEVERS {
            on.as_object_mut().unwrap().remove(key);
            off.as_object_mut().unwrap().remove(key);
        }
        assert_eq!(on, off);
    }
    #[test]
    fn rc1_play_configs_are_r25_human_plus_the_rc1_keys() {
        // Default file: PL-rc1.json. The sky and FX routing files each turn one key on for a by-eye A/B.
        const LEVERS: [&str; 7] = ["world_list_submit", "world_shared_buffers", "sim_sweep_memo", "tls_fast", "world_gxm",
            "fx_attached_persist", "mover_advance_reuse"];
        const BUBBLE: [&str; 2] = ["fx_attached_sphere", "fx_bubble_true_size"];
        let parse = |bytes: &[u8]| {
            // Every runtime key is written, so no value comes from a code default.
            let raw: serde_json::Value = serde_json::from_slice(bytes).unwrap();
            let defaults = serde_json::to_value(Config::default()).unwrap();
            let mut written: Vec<_> = raw.as_object().unwrap().keys().cloned().collect();
            // The replay keys came after RC1; its play files keep them at their off defaults.
            let mut all: Vec<_> = defaults.as_object().unwrap().keys().filter(|k| !k.starts_with("replay_")).cloned().collect();
            written.sort(); all.sort();
            assert_eq!(written, all, "an RC1 config must write every runtime key");
            let (c, rejected) = parse_runtime_config(bytes).unwrap();
            assert!(rejected.is_none());
            c.validate().unwrap();
            assert_eq!(c.log_frame_time_every, Some(10), "frame-time rows every ten frames stay on for the play session");
            assert_eq!(c.render_scale, 0.75);
            serde_json::to_value(&c).unwrap()
        };
        let mut rc1 = parse(include_bytes!("../../configs/PL-rc1.json"));
        let mut sky = parse(include_bytes!("../../configs/PL-rc1-sky-on.json"));
        let mut fx = parse(include_bytes!("../../configs/PL-rc1-fx-on.json"));
        let mut off = parse(include_bytes!("../../configs/PL-rc1-levers-off.json"));
        let (human, _) = parse_runtime_config(include_bytes!("../../configs/PL-r25-human.json")).unwrap();
        let mut human = serde_json::to_value(&human).unwrap();
        for (key, on) in [("world_list_submit", serde_json::json!(true)), ("world_shared_buffers", serde_json::json!(true)),
                          ("sim_sweep_memo", serde_json::json!(true)), ("tls_fast", serde_json::json!("on")),
                          ("world_gxm", serde_json::json!("live")), ("fx_attached_persist", serde_json::json!(true)),
                          ("mover_advance_reuse", serde_json::json!(true))] {
            assert_eq!(rc1[key], on, "{key} on in RC1");
            assert_eq!(off[key], serde_json::to_value(Config::default()).unwrap()[key], "{key} at its off default in levers-off");
        }
        for key in BUBBLE { assert_eq!(rc1[key], true, "{key}: bubble fix on"); assert_eq!(off[key], true, "{key}: not a lever"); }
        for key in ["collision_sah", "skin_actual_consumers", "audio_static_cache"] { assert_eq!(rc1[key], true, "{key} as measured"); }
        assert_eq!(rc1["skin_stream"], false);
        assert_eq!(rc1["worker_spin_us"], 0);
        assert_eq!(rc1["authored_sky"], false, "the default RC1 file keeps the gradient sky");
        assert_eq!(sky["authored_sky"], true);
        assert_eq!(rc1["defer_immediate_fx"], false, "the default RC1 file keeps the direct effect draws");
        assert_eq!(fx["defer_immediate_fx"], true);
        // Diagnostics stay off: no census, no exports, no GPU timing or masks, no pass clock, no GL observers.
        assert_eq!(rc1["profiler"], "off");
        for key in ["profile_capture", "phase_census", "world_pass_clock", "light_gl_observers", "render_draw_timing",
                    "worker_probe", "skin_worker_bench", "frame_phase_probe", "gpu_state_probe", "pvs_verify", "depth_sentinel"] {
            assert_eq!(rc1[key], false, "{key} off");
        }
        assert_eq!(rc1["gpu_timing"], "off");
        assert_eq!(rc1["gpu_mask"], serde_json::json!([]));
        sky["authored_sky"] = serde_json::json!(false);
        assert_eq!(sky, rc1, "the sky file differs only in authored_sky");
        fx["defer_immediate_fx"] = serde_json::json!(false);
        assert_eq!(fx, rc1, "the FX routing file differs only in defer_immediate_fx");
        for key in LEVERS { off[key] = rc1[key].clone(); }
        assert_eq!(off, rc1, "the levers-off file differs only in the seven levers");
        // Against what the human played on 2026-11-06: the levers, the bubble fix, the frame log and profiler off.
        for key in LEVERS.iter().chain(&BUBBLE).chain(&["profiler", "log_frame_time_every"]) {
            rc1.as_object_mut().unwrap().remove(*key);
            human.as_object_mut().unwrap().remove(*key);
        }
        assert_eq!(rc1, human);
    }
    #[test]
    fn rc2_play_configs_are_rc1_plus_the_rc2_keys() {
        // Default file: PL-rc2.json. Each variant differs from it only in the keys listed below.
        let parse = |bytes: &[u8]| {
            // Every runtime key is written, so no value comes from a code default.
            let raw: serde_json::Value = serde_json::from_slice(bytes).unwrap();
            let defaults = serde_json::to_value(Config::default()).unwrap();
            let mut written: Vec<_> = raw.as_object().unwrap().keys().cloned().collect();
            let mut all: Vec<_> = defaults.as_object().unwrap().keys().cloned().collect();
            written.sort(); all.sort();
            assert_eq!(written, all, "an RC2 config must write every runtime key");
            let (c, rejected) = parse_runtime_config(bytes).unwrap();
            assert!(rejected.is_none());
            c.validate().unwrap();
            assert_eq!(c.log_frame_time_every, Some(10), "frame-time rows every ten frames stay on");
            assert_eq!(c.render_scale, 0.75);
            serde_json::to_value(&c).unwrap()
        };
        let rc2 = parse(include_bytes!("../../configs/PL-rc2.json"));
        let (rc1, rejected) = parse_runtime_config(include_bytes!("../../configs/PL-rc1.json")).unwrap();
        assert!(rejected.is_none());
        let mut rc1 = serde_json::to_value(&rc1).unwrap();
        assert_eq!(rc2["scaled_target"], "resolve");
        assert_eq!(rc2["authored_sky"], false, "RC2 keeps the gradient sky");
        assert_eq!(rc2["defer_immediate_fx"], false, "RC2 keeps the direct effect draws");
        assert_eq!((rc2["texture_cache"].as_str(), rc2["pvs_cache"].as_bool()), (Some("off"), Some(false)));
        let defaults = serde_json::to_value(Config::default()).unwrap();
        for key in ["replay_record", "replay_play", "replay_start", "replay_check_every", "replay_state_every", "replay_stop_after"] {
            assert_eq!(rc2[key], defaults[key], "{key} off in RC2");
        }
        // RC2 = RC1 plus the two A3 load keys on.
        for key in ["loading_screen", "collision_geometry_share"] {
            assert_eq!(rc2[key], true, "{key} on in RC2");
            rc1[key] = serde_json::json!(true);
        }
        assert_eq!(rc2, rc1, "PL-rc2 differs from PL-rc1 only in the two load keys");
        for (bytes, keys) in [
            (&include_bytes!("../../configs/PL-rc2-direct.json")[..], vec![("scaled_target", serde_json::json!("direct"))]),
            (&include_bytes!("../../configs/PL-rc2-direct-flip.json")[..], vec![("scaled_target", serde_json::json!("direct_flip"))]),
            (&include_bytes!("../../configs/PL-rc2-texcache.json")[..], vec![("texture_cache", serde_json::json!("on")), ("pvs_cache", serde_json::json!(true))]),
            (&include_bytes!("../../configs/PL-rc2-loading-off.json")[..], vec![("loading_screen", serde_json::json!(false)),
                ("collision_geometry_share", serde_json::json!(false))]),
            (&include_bytes!("../../configs/PL-a2-record.json")[..], vec![("replay_record", serde_json::json!("a2-#.lgtape")),
                ("replay_start", serde_json::json!("chapter:potears2"))]),
            (&include_bytes!("../../configs/PL-a2-replay.json")[..], vec![("replay_play", serde_json::json!("a2-01.lgtape"))]),
        ] {
            let mut variant = parse(bytes);
            for (key, value) in keys {
                assert_eq!(variant[key], value, "{key}");
                variant[key] = rc2[key].clone();
            }
            assert_eq!(variant, rc2, "a variant differs from PL-rc2 only in its own keys");
        }
    }
    #[test]
    fn r25_human_play_configs_add_three_levers_to_r22() {
        const NEW: [&str; 3] = ["actor_publication_uv_reuse", "prop_occurrence_reuse", "fx_observe_move"];
        const OLD: [&str; 5] = ["sim_trigger_facts_reuse", "sim_sight_query_reuse", "sim_hatter_door_candidates",
            "actor_publication_occurrences", "actor_deferred_layers"];
        let parse = |bytes: &[u8]| {
            let (c, rejected) = parse_runtime_config(bytes).unwrap();
            assert!(rejected.is_none());
            c.validate().unwrap();
            serde_json::to_value(&c).unwrap()
        };
        let mut on = parse(include_bytes!("../../configs/PL-r25-human.json"));
        let mut off = parse(include_bytes!("../../configs/PL-r25-human-levers-off.json"));
        let mut measure = parse(include_bytes!("../../configs/PL-r25-human-measure.json"));
        let r22 = parse(include_bytes!("../../configs/PL-r22-human.json"));
        // A missing key would silently take the shipped default, so read the files' own keys too.
        for bytes in [&include_bytes!("../../configs/PL-r25-human.json")[..], &include_bytes!("../../configs/PL-r25-human-measure.json")[..]] {
            let raw: serde_json::Value = serde_json::from_slice(bytes).unwrap();
            for key in NEW.iter().chain(&OLD) { assert_eq!(raw[*key], true, "{key} written on"); }
        }
        let raw: serde_json::Value = serde_json::from_slice(include_bytes!("../../configs/PL-r25-human-levers-off.json")).unwrap();
        for key in NEW.iter().chain(&OLD) { assert_eq!(raw[*key], false, "{key} written off"); }
        for key in NEW {
            assert_eq!(on[key], true, "{key} on");
            assert_eq!(off[key], false, "{key} off");
            assert_eq!(serde_json::to_value(Config::default()).unwrap()[key], true, "{key} shipped default");
        }
        // What the console ABBAs measured, minus the diagnostic exports: profile_capture stays
        // off and profiler stays light, because the human session writes no diagnostic exports.
        const MEASURED: [&str; 3] = ["collision_sah", "skin_actual_consumers", "audio_static_cache"];
        let mut r22_plus = r22.clone();
        for key in MEASURED {
            assert_eq!(on[key], true, "{key} on");
            assert_eq!(off[key], true, "{key} is not a lever: on in both arms");
            r22_plus[key] = serde_json::json!(true);
        }
        assert_eq!(on, r22_plus, "r25 human is r22 human plus the three levers and the three measured keys");
        assert_eq!(on["profile_capture"], false);
        assert_eq!(on["profiler"], "light");
        assert!(on["log_frame_time_every"].as_u64().unwrap_or(0) == 0, "human play runs without the frame-time log");
        assert_eq!(measure["log_frame_time_every"], 10);
        measure["log_frame_time_every"] = on["log_frame_time_every"].clone();
        assert_eq!(measure, on, "the measurement variant differs only in the frame log");
        for key in NEW.iter().chain(&OLD) {
            on.as_object_mut().unwrap().remove(*key);
            off.as_object_mut().unwrap().remove(*key);
        }
        assert_eq!(on, off);
    }
    #[test]
    fn fx_cpu_candidates_default_off_and_require_boolean_values() {
        let c = Config::default();assert!(!c.fx_merge_ranges && !c.fx_cpu_inline);
        for name in ["fx_merge_ranges", "fx_cpu_inline"] {
            for value in [serde_json::json!(false),serde_json::json!(true)] {
                let c:Config=serde_json::from_value(serde_json::json!({name:value})).unwrap();c.validate().unwrap();
            }
            for value in [serde_json::json!(1),serde_json::json!("on"),serde_json::Value::Null] {
                assert!(serde_json::from_value::<Config>(serde_json::json!({name:value})).is_err());
            }
        }
    }
    #[test]
    fn scaled_target_defaults_to_resolve_and_names_its_modes() {
        assert_eq!(Config::default().scaled_target, "resolve");
        for mode in ["resolve", "direct", "direct_flip"] {
            let c: Config = serde_json::from_value(serde_json::json!({"scaled_target": mode})).unwrap();
            c.validate().unwrap();
        }
        let bad: Config = serde_json::from_value(serde_json::json!({"scaled_target": "none"})).unwrap();
        assert!(bad.validate().is_err());
        assert!(serde_json::from_value::<Config>(serde_json::json!({"scaled_target": true})).is_err());
    }
    #[test]
    fn a3_variant_configs_differ_from_rc1_only_in_their_keys() {
        let parse = |bytes: &[u8]| {
            let (c, rejected) = parse_runtime_config(bytes).unwrap();
            assert!(rejected.is_none());
            c.validate().unwrap();
            serde_json::to_value(&c).unwrap()
        };
        let rc1 = parse(include_bytes!("../../configs/PL-rc1.json"));
        assert_eq!(rc1["scaled_target"], "resolve");
        for (bytes, keys) in [
            (&include_bytes!("../../configs/PL-rc1-a3-direct.json")[..], vec![("scaled_target", serde_json::json!("direct"))]),
            (&include_bytes!("../../configs/PL-rc1-a3-direct-flip.json")[..], vec![("scaled_target", serde_json::json!("direct_flip"))]),
            (&include_bytes!("../../configs/PL-rc1-a3-direct-fx-on.json")[..], vec![("scaled_target", serde_json::json!("direct")),
                ("defer_immediate_fx", serde_json::json!(true))]),
        ] {
            let mut variant = parse(bytes);
            for (key, value) in keys {
                assert_eq!(variant[key], value, "{key}");
                variant[key] = rc1[key].clone();
            }
            assert_eq!(variant, rc1);
        }
    }
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
                time:0,time_uv:dynamic,time_gain:true, list_serial:0, class:0 };
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
    #[test]
    fn actor_single_preparation_defaults_on_and_is_strictly_boolean() {
        assert!(Config::default().actor_single_prepare);
        for name in ["actor_single_prepare"] {
        for enabled in [true,false] {
            let c:Config=serde_json::from_value(serde_json::json!({(name):enabled})).unwrap();
            assert_eq!(serde_json::to_value(&c).unwrap()[name],enabled);c.validate().unwrap();
        }
        assert!(serde_json::from_value::<Config>(serde_json::json!({(name):"on"})).is_err());
        }
    }
    #[test]
    fn actor_single_preparation_rejects_each_missing_prerequisite() {
        let valid=Config {actor_single_prepare:true,..Default::default()};
        valid.validate().unwrap();assert!(valid.actor_single_prepare_active());
        for (key,value) in [("skin_overlap",serde_json::json!(false)),("actor_fast_path",serde_json::json!(false)),
            ("actor_layer_cache",serde_json::json!(false)),("fx_prepare_cache",serde_json::json!(false)),
            ("actor_layer_reuse",serde_json::json!(true)),("renderer",serde_json::json!("legacy"))] {
            let mut config=serde_json::to_value(&valid).unwrap();config[key]=value;
            let invalid:Config=serde_json::from_value(config).unwrap();
            assert!(!invalid.actor_single_prepare_active());
            assert!(invalid.validate().unwrap_err().to_string().contains("actor_single_prepare"),"{key}");
        }
        assert!(Config::default().actor_single_prepare_active());
    }
    #[test]
    fn implicit_actor_preparation_preserves_kill_switches() {
        for (key, value) in [("renderer", serde_json::json!("legacy")),
            ("skin_overlap", serde_json::json!(false)), ("skin_workers", serde_json::json!(0)),
            ("actor_fast_path", serde_json::json!(false)), ("actor_layer_cache", serde_json::json!(false)),
            ("actor_layer_reuse", serde_json::json!(true)), ("fx_prepare_cache", serde_json::json!(false))] {
            let input = serde_json::json!({key: value, "retained_maps": ["gvillage"], "render_scale": 0.5});
            let c: Config = serde_json::from_value(input.clone()).unwrap();
            c.validate().unwrap();
            assert!(!c.actor_single_prepare && !c.actor_single_prepare_active(), "{key}");
            assert_eq!(c.retained_maps, ["gvillage"]);
            assert_eq!(c.render_scale, 0.5);
            assert_eq!(serde_json::to_value(&c).unwrap()[key], value);
            let mut explicit = input;
            explicit["actor_single_prepare"] = serde_json::json!(true);
            assert!(serde_json::from_value::<Config>(explicit).unwrap().validate().is_err(), "{key}");
        }
    }
    #[test]
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
        assert!(!Config::default().light_gl_observers);
        for enabled in [false,true] {
            let c:Config=serde_json::from_value(serde_json::json!({"profiler":"light","light_gl_observers":enabled})).unwrap();
            c.validate().unwrap(); assert_eq!(c.light_gl_observers,enabled);
        }
        for invalid in [serde_json::json!(0),serde_json::json!("true"),serde_json::Value::Null] {
            assert!(serde_json::from_value::<Config>(serde_json::json!({"light_gl_observers":invalid})).is_err());
        }
        for level in ["off","light","full"] {
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
        assert_eq!(c.skin_workers,2); assert_eq!(c.profiler,Some(crate::light_profile::Level::Off)); assert!(!c.profile_capture); assert!(!c.skin_worker_bench); assert!(c.fx_prepare_cache);
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
        assert_eq!(u.bits_ref(), &u.bits());
        for bits in [0, 0x80000000, 0x7fc00001, 0x7fc00002, 0xff800000] {
            u.fog_b[3] = f32::from_bits(bits);
            assert_eq!(u.bits_ref(), &u.bits());
        }
    }
    #[test]
    fn story_film_bypass_is_explicit_and_default_off() {
        let default: Config = serde_json::from_str("{}").unwrap();
        assert!(!default.story_film_bypass);
        let bypass: Config = serde_json::from_str(r#"{"story_film_bypass":true}"#).unwrap();
        assert!(bypass.story_film_bypass && bypass.validate().is_ok());
        assert!(serde_json::from_str::<Config>(r#"{"story_film_bypass":"true"}"#).is_err());
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
        assert_eq!(lean.gpu_experiment, "gpu222");
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
