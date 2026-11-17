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
mod draw_cuts;
mod alpha_fill;
mod sky_surfaces;
mod water_join;
mod water_probe;
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
    /// Skip a Snark movement step whose delta is exactly zero. Same state, no sweep.
    pub sim_snark_move_skip: bool,
    /// Answer the Snark sight segment as the yes/no blocked predicate instead of a full trace.
    pub sim_snark_sight_blocked: bool,
    /// Match Pool mover names without building one String per candidate.
    pub sim_pool_name_match: bool,
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
    /// Diagnostic: print every gameplay frame slower than this many milliseconds with its main-thread
    /// split, and every load done outside a load phase (`spike_log.rs`). 0 is off.
    pub spike_log_ms: f32,
    /// Preload, with the level's sounds, the combat sounds of the enemy kinds placed in the map, Alice's
    /// pain cries and the weapon flesh hits, so the first hit or death does not read and decode a clip
    /// in a frame (`audio.rs`, `combat_preload`).
    pub audio_combat_preload: bool,
    /// Yes/no collision questions (flying route clearance, the ground walk's horizontal move) stop at the
    /// first hull that blocks instead of finding the nearest one. Same answers (`collision.rs`, `sweep_blocked`).
    pub sim_sweep_blocked: bool,
    /// Answer solid yes/no sweeps during SAH traversal without collecting and sorting candidates.
    pub sim_sweep_any: bool,
    /// Map-load empty-space certificates, re-tested flight blockers and compact route nodes. Default off.
    pub sim_flight_memo: bool,
    /// Draw omission diagnostics only; these never belong in a release config.
    pub diag_skip_ant_corpses: bool,
    /// Reject dead Ant triangles strictly outside the homogeneous view before GPU submission.
    pub ant_corpse_triangle_cull: bool,
    /// Use the same fragment arithmetic without discard for certified opaque dead Ants.
    pub ant_corpse_opaque_shader: bool,
    pub diag_skip_live_actors: bool,
    /// Use the discard-free program for every certified opaque actor/prop packet.
    /// Draw order, geometry and fragment arithmetic stay unchanged. Default off.
    pub actor_opaque_shader: bool,
    /// Remove unreachable discard instructions from certified actor layers, including blends. Default off.
    pub actor_discard_free_shader: bool,
    /// Discard-free program for actor packets whose default cutout (alpha < 0.35) is certified
    /// unreachable: opaque texture, unit gain alpha, opaque vertex colours. Default off.
    pub actor_cutout_discard_free: bool,
    /// Picture-changing experiment: cull GL front-facing actor triangles. Owner approval required. Default off.
    pub actor_frontface_cull: bool,
    /// Cull script-owned bodies when single preparation has no mutable consumer.
    pub actor_consumer_frustum_cull: bool,
    pub diag_skip_fx: bool,
    pub diag_skip_decals: bool,
    pub diag_skip_alpha_world: bool,
    pub diag_skip_water: bool,
    pub diag_skip_sky: bool,
    /// Inclusive spike output window; zero upper bound means no upper limit.
    pub spike_log_first_frame: u32,
    pub spike_log_last_frame: u32,
    /// Do not draw a dead Ant whose body centre is closer to the camera than this many units. A corpse
    /// against the low camera covers the screen several times over (`npc.rs`, `near_culled`). 0 is off.
    pub actor_near_cull: f32,
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
    /// Coalesce certified adjacent opaque ranges and reuse uninterrupted world-alpha state.
    /// Never changes the order of world groups or crosses an actor/FX callback. Default off.
    pub world_adjacent_batch: bool,
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
    /// Animate `deformVertexes wavenormal` on the GPU for world stages that read the normal
    /// (`tcGen environment`, `alphaGen dot`), as the desktop renderer does on the CPU. Off keeps
    /// the RC2 shader, whose normals stay still and freeze the water reflection. Default off.
    pub water_normals: bool,
    /// With `quality: low`, keep the authored fog (map `setfarplane` distance fog, fog volumes,
    /// liquid fog) in the lean opaque world program, as `quality: full` and the desktop renderer
    /// draw it. Off keeps the RC3 low program, which leaves opaque walls and floors unfogged.
    /// No effect in `quality: full`. Default off.
    pub low_fog: bool,
    /// On a map with authored distance fog (`setfarplane`), draw the gradient sky flat in the fog
    /// colour, as the desktop renderer shows potears2 (its sky shader draws nothing over the fog
    /// colour clear). Off keeps the RC3 gradient from the fog colour up to 1.5 times it, which
    /// outlines fully fogged geometry against a brighter sky. No effect with `authored_sky`,
    /// `sky: false` or on a map without distance fog. Default off.
    pub fog_sky: bool,
    /// Draw world layers whose blend the five world blend classes only approximate with the
    /// desktop renderer's exact GL factors: `GL_DST_COLOR GL_ONE` (drawn as additive),
    /// `GL_SRC_ALPHA GL_SRC_COLOR` and every other factor pair (drawn as alpha), such as the
    /// green slime, `flatwater`, `darkflowingwater1`, grates and hanging moss. Each such blend
    /// used by the map gets its own pipeline class from 5 on, so the draws stay the same and
    /// only their pipelines change. Off keeps the RC3 classes. Default off.
    pub exact_blends: bool,
    /// Keep world `detail` stages (the fine texture tiled over walls and floors, such as the
    /// garden4 ice, keep stone and skool2 wood) as a layer instead of dropping them. They still
    /// count against `max_layers`; on most surfaces the detail stage is the second layer, since
    /// the world program applies the lightmap itself. Each kept stage is one more draw of its
    /// surface. Off keeps the RC3 world, which drops them. Default off.
    pub detail_layers: bool,
    /// Draw the authored sky from a cube map captured once from the sky camera, instead of the
    /// gradient: the sky room (portal maps) and the sky shader layers are drawn by the legacy path
    /// into six faces of one render target on the first frame, and the map's sky surfaces sample it
    /// by view direction in one opaque, depth-writing draw before the world. The legacy sky geometry
    /// is released after the capture unless the map moves its sky camera or `sky_cube_refresh` is on.
    /// Animated sky content stays as captured. Takes precedence over `sky_depth`. No effect with
    /// `authored_sky` or `sky: false`. Default off.
    pub sky_cube: bool,
    /// Face size in pixels of the `sky_cube` capture, 32 to 1024. The target is 3 by 2 faces of RGBA8.
    pub sky_cube_size: u32,
    /// With `sky_cube`: redraw one face every this many frames, in turn, so animated sky content
    /// moves. Keeps the legacy sky geometry for the whole map. 0 never redraws. Default 0.
    pub sky_cube_refresh: u32,
    /// Diagnostic for the sky cube's first-frame GPU crash: keep the legacy resources after
    /// capture. This does not enable animated refresh. Default off.
    pub sky_cube_retain: bool,
    /// Complete the offscreen capture before sampling its atlas or retiring its materials.
    /// On vitaGL, glDeleteProgram waits for submitted scenes but does not close the current one;
    /// glFinish closes it first. This wait runs only when a capture is made. Default off.
    pub sky_cube_finish: bool,
    /// Diagnostic: capture the cube normally but draw the gradient and sky depth instead of
    /// sampling it. Used to separate capture/resource lifetime from atlas sampling. Default off.
    pub sky_cube_capture_only: bool,
    /// Diagnostic: clear the cube atlas without drawing the legacy sky into it, then sample it
    /// normally (unless capture_only is on). Used to isolate atlas sampling. Default off.
    pub sky_cube_clear_only: bool,
    /// With the gradient sky, draw the map's sky surfaces into depth before the world (colour writes
    /// off), so scenery placed behind the sky stays hidden, as on the desktop renderer.
    /// No effect with `authored_sky` or `sky: false`. Default off.
    pub sky_depth: bool,
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
    /// Actor, prop and FX packets whose vertex bounds lie wholly outside the view frustum are
    /// neither uploaded nor drawn. Their vertices are in world space and the actor program's
    /// model matrix is the identity, so no pixel changes. Default off.
    pub packet_frustum_cull: bool,
    /// A Prop draw (weapon, attachment, level object) whose model box, placed at its attachment,
    /// lies outside the view frustum is neither animated nor drawn. Saves the animation as well
    /// as the draw; no pixel changes. Default off.
    pub prop_frustum_cull: bool,
    /// Actor, prop and FX packets whose screen rectangle is shorter than this many pixels of the
    /// 960x544 screen on its longer side are neither uploaded nor drawn. Changes pixels. 0 is off.
    pub packet_min_pixels: f32,
    /// The same size test for world groups, opaque and alpha. Changes pixels. 0 is off.
    pub world_min_pixels: f32,
    /// Opaque world draws in rank order: draws without alpha test first, grouped by pipeline and
    /// texture and front to back inside one texture, then alpha-tested draws front to back.
    /// Only coplanar draws at equal depth can change the picture. Default off.
    pub world_texture_sort: bool,
    /// One world surface whose filter stage (dst*src) is followed by an alpha-blended stage, both
    /// unlit, untested and unclamped (the potears2 river and waterfall): "merge" draws both in
    /// one pass, blending one,src_alpha with the filter's luma in place of its colour, so the
    /// colour of the filter is lost where the second stage is transparent; "drop_filter" skips
    /// the filter stage. "off" draws both. Halves that surface's blended pixels.
    pub water_stage_merge: String,
    /// World alpha draws that are unlit, unclamped and not alpha-tested use a fragment program
    /// without the alpha-test chain, the lightmap branch and the clamp; same arithmetic, so no
    /// pixel should change. Default off.
    pub world_alpha_lite: bool,
    /// Consecutive liquid world alpha draws that share pipeline, vertex buffer, atlas, texture and
    /// uniform parameter, with no actor or puff between them, submit as one indexed draw whose
    /// dynamic index buffer holds their indices in the exact submitted order. The alpha pass
    /// writes no depth, so only blending order matters and it is preserved; fragments, depth
    /// tests and uniforms are unchanged. Needs `world_shared_buffers`. Default off.
    pub water_draw_join: bool,
    /// A model's world alpha groups (an entity brush or a script object with no vertex deform
    /// and no sky-sequence motion) use the map's PVS over their transformed bounds instead of
    /// being drawn in every cluster. The bounds test is conservative and fails open, so no
    /// visible fragment is dropped. Default off.
    pub alpha_model_pvs: bool,
    /// Effect packets that are unlit, unclamped, uncut, with no dissolve, ghost, power tint or
    /// filter fade use a fragment program without those paths, the dynamic lights (all zero when
    /// unlit) and the sky fade (never set here); host rounding can differ. Default off.
    pub fx_lite: bool,
    /// Eligible unlit effects keep the full fragment arithmetic and replace only the
    /// known-zero dynamic light result. Takes precedence over fx_lite. Default off.
    pub fx_unlit_exact: bool,
    /// Additive effects (drawn with source alpha) whose texture has alpha 0 in every texel add
    /// nothing; their puffs and effect groups are not drawn. No pixel changes. Default off.
    pub fx_add_zero_alpha_cut: bool,
    /// Additive world puffs (emitter quads such as waterfall spray) whose screen area is more than
    /// this fraction of the screen shrink about their centre to that area. Changes pixels. 0 is off.
    pub fx_add_max_area: f32,
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
    /// Retain the decoded samples of a looping cue when its loop is reissued, so the
    /// same file is not read and decoded again on a later frame. Sample-exact, off by default.
    pub audio_loop_resident: bool,
    /// Decode a bounded set of the level's own cue clips during load, before their first
    /// request, so no fight frame pays a first-use read and decode. Sample-exact, off by default.
    pub audio_cue_prefetch: bool,
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
    /// Periodic one-second audio statistics (`audio-realtime`, `audio-stage`,
    /// `audio-voices`, `audio-music`). Default on; the release config turns it off.
    pub audio_stats_log: bool,
    /// Music decodes on its own lower-priority thread into a PCM ring; the
    /// producer only copies decoded samples.
    pub audio_music_thread: bool,
    pub audio_music_priority: i32,
    /// Persistent binary pairs; process deduplication remains active when off.
    pub shader_cache: String,
    /// Empty preserves the release cache path; diagnostics use a fresh child directory.
    pub shader_cache_namespace: String,
    /// Optional cache of exact device-compressed immutable pixels.
    pub texture_cache: String,
    /// One upload per resolved asset path for the duration of a load.
    pub load_texture_share: bool,
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
    /// Comma-separated replay steps at which to save a device PNG under private/. Empty: off.
    pub capture_frames: String,
    /// Filename tag for those PNGs (lowercase letters, digits and '-'). Empty with capture_frames: off.
    pub capture_tag: String,
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
            spike_log_ms: 0.,
            audio_combat_preload: false,
            sim_sweep_blocked: false,
            sim_sweep_any: false,
            diag_skip_ant_corpses: false,
            ant_corpse_triangle_cull: false,
            ant_corpse_opaque_shader: false,
            diag_skip_live_actors: false,
            actor_opaque_shader: false,
            actor_discard_free_shader: false,
            actor_cutout_discard_free: false,
            actor_frontface_cull: false,
            actor_consumer_frustum_cull: false,
            diag_skip_fx: false,
            diag_skip_decals: false,
            diag_skip_alpha_world: false,
            diag_skip_water: false,
            diag_skip_sky: false,
            sim_flight_memo: false,
            spike_log_first_frame: 0,
            spike_log_last_frame: 0,
            actor_near_cull: 0.,
            sim_npc_sight_candidates: true,
            sim_npc_sight_cache: true,
            sim_npc_target_stack: true,
            sim_snark_move_skip: false,
            sim_snark_sight_blocked: false,
            sim_pool_name_match: false,
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
            world_adjacent_batch: false,
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
            water_normals: false,
            low_fog: false,
            fog_sky: false,
            exact_blends: false,
            detail_layers: false,
            sky_cube: false,
            sky_cube_size: 256,
            sky_cube_refresh: 0,
            sky_cube_retain: false,
            sky_cube_finish: false,
            sky_cube_capture_only: false,
            sky_cube_clear_only: false,
            sky_depth: false,
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
            packet_frustum_cull: false, prop_frustum_cull: false, packet_min_pixels: 0., world_min_pixels: 0., world_texture_sort: false,
            water_stage_merge: "off".into(), world_alpha_lite: false, water_draw_join: false, alpha_model_pvs: false, fx_lite: false, fx_unlit_exact: false, fx_add_zero_alpha_cut: false, fx_add_max_area: 0.,
            flat_actor_light: false, max_texture_dimension: None,
            draw_distance_cap: None, simple_fog: false, simple_dynamic_lights: false,
            gpu_state_probe: false,
            gpu_index_dedupe: false, gpu_uniform_dedupe: false,
            startup_splash:false,
            story_film_bypass: false,
            audio_pcm_worker: false,
            audio_offmain: false,
            audio_static_cache: true,
            audio_loop_resident: false,
            audio_cue_prefetch: false,
            audio_block_mix: false,
            audio_preresample: false,
            audio_block_parameters: false,
            audio_ambient_voices: 0,
            audio_cull_gain: 0.,
            audio_producer_priority: 161,
            audio_producer_affinity: 0x40000,
            audio_cushion_blocks: 2,
            audio_log_thread: false,
            audio_stats_log: true,
            audio_music_thread: false,
            audio_music_priority: 170,
            shader_cache:"on".into(),
            shader_cache_namespace:String::new(),
            texture_cache:"off".into(),
            load_texture_share: false,
            route_heading_lock:true,
            route_world_wish: false,
            replay_record: String::new(),
            replay_play: String::new(),
            replay_start: String::new(),
            replay_check_every: 30,
            replay_state_every: 0,
            replay_stop_after: 0,
            capture_frames: String::new(),
            capture_tag: String::new(),
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
        ensure!(self.spike_log_last_frame == 0 || self.spike_log_last_frame >= self.spike_log_first_frame, "invalid spike frame window");
        crate::replay::validate_settings(&crate::replay::Settings { record: self.replay_record.clone(),
            play: self.replay_play.clone(), start: self.replay_start.clone(), check_every: self.replay_check_every,
            state_every: self.replay_state_every, stop_after: self.replay_stop_after })?;
        crate::replay_capture::validate(&self.capture_frames, &self.capture_tag)?;
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
        ensure!(self.packet_min_pixels.is_finite() && (0. ..=64.).contains(&self.packet_min_pixels), "packet_min_pixels must be 0 to 64");
        ensure!(self.world_min_pixels.is_finite() && (0. ..=64.).contains(&self.world_min_pixels), "world_min_pixels must be 0 to 64");
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
        ensure!(matches!(self.water_stage_merge.as_str(),"off"|"merge"|"drop_filter"),"water_stage_merge must be off, merge or drop_filter");
        ensure!(!self.water_draw_join || self.world_shared_buffers, "water_draw_join needs world_shared_buffers");
        ensure!(!self.water_draw_join || self.world_parameter_reuse,
            "water_draw_join needs world_parameter_reuse: with reuse off the parameter table keeps its construction-time uniforms, so a joined run would submit a stale payload");
        ensure!(!self.water_draw_join || self.water_stage_merge == "off", "water_draw_join and water_stage_merge are exclusive");
        ensure!(self.fx_add_max_area.is_finite() && (0. ..=1.).contains(&self.fx_add_max_area), "fx_add_max_area must be 0 to 1");
        ensure!(matches!(self.texture_cache.as_str(),"on"|"off"),"texture_cache must be on or off");
        ensure!(self.shader_cache_namespace.len() <= 64 && self.shader_cache_namespace.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_'), "invalid shader_cache_namespace");
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
        ensure!(!self.world_adjacent_batch || (self.world_list_submit && self.world_shared_buffers),
            "world_adjacent_batch requires world_list_submit and world_shared_buffers");
        ensure!(self.world_gxm=="off" || self.gpu_mask.is_empty(),"world_gxm excludes gpu_mask: a masked draw captures nothing");
        ensure!(!self.skin_stream || (self.skin_overlap && self.actor_single_prepare && !self.actor_layer_reuse),"skin_stream requires skin_overlap and actor_single_prepare, and excludes actor_layer_reuse");
        ensure!(self.spike_log_ms.is_finite() && (0. ..=10000.).contains(&self.spike_log_ms),"spike_log_ms must be 0 (off) to 10000");
        ensure!(self.actor_near_cull.is_finite() && (0. ..=1000.).contains(&self.actor_near_cull),"actor_near_cull must be 0 (off) to 1000");
        ensure!(self.worker_spin_us <= 1000,"worker_spin_us must be at most 1000: worker 1 shares CPU2 with the audio producer");
        ensure!(crate::tls_count::FastMode::parse(&self.tls_fast).is_some(),"tls_fast must be off, on, shadow or shadow_sabotage");
        ensure!(crate::gpu_timing::Mode::parse(&self.gpu_timing).is_some(),"gpu_timing must be off, backlog, poll or wait");
        for name in &self.gpu_mask { ensure!(crate::gpu_timing::mask_bit(name).is_some(),"unknown gpu_mask class {name}"); }
        if let Some(scenes)=self.rt_scenes { ensure!(scenes.iter().all(|&n|(1..=8).contains(&n)),"rt_scenes must be 1 to 8"); }
        if let Some(mib)=self.param_buffer_mib { ensure!((16..=64).contains(&mib),"param_buffer_mib must be 16 to 64"); }
        ensure!(matches!(self.gpu_experiment.as_str(),"unchanged"|"gpu111"|"gpu222"),"gpu_experiment must be unchanged, gpu111 or gpu222");
        ensure!(matches!(self.pvs_method.as_str(),"sources"|"legacy"),"pvs_method must be sources or legacy");
        ensure!((32..=1024).contains(&self.sky_cube_size), "sky_cube_size must be 32..1024");
        ensure!(
            self.cell_size.is_finite() && (64. ..=2048.).contains(&self.cell_size),
            "invalid cell_size"
        );
        Ok(())
    }
}
/// Bottom and top colours of the gradient sky. `flat` (`fog_sky` on a fogged map) keeps the
/// background colour at the top too.
fn sky_gradient(bg: Color, flat: bool) -> [[f32; 4]; 2] {
    let top = if flat { 1. } else { 1.5 };
    [[bg.r, bg.g, bg.b, 1.], [bg.r * top, bg.g * top, bg.b * top, 1.]]
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
/// Reject a malformed report cadence, or a join without a live parameter table, without
/// discarding unrelated flags.
fn parse_runtime_config(bytes: &[u8]) -> Result<(Config, Option<String>)> {
    let mut value: serde_json::Value = serde_json::from_slice(bytes)?;
    let mut rejected = if let Some(object) = value.as_object_mut() {
        if let Some(raw) = object.get("log_frame_time_every") {
            if serde_json::from_value::<Option<u32>>(raw.clone()).is_err() {
                let message = format!("log_frame_time_every={raw}: expected null or u32; rejecting key, effective value={}; preserving other config keys", report_cadence(&Config::default()));
                object.remove("log_frame_time_every");
                Some(message)
            } else { None }
        } else { None }
    } else { None };
    let mut config: Config = serde_json::from_value(value)?;
    // The join submits the parameter table's payload. Without reuse that table keeps its
    // construction-time uniforms, so the key cannot take effect; refuse it alone rather
    // than discard the rest of the configuration.
    if config.water_draw_join && !config.world_parameter_reuse {
        let message = "water_draw_join requires world_parameter_reuse: with reuse off the parameter table keeps its construction-time uniforms and a joined run would submit a stale payload; rejecting key, effective value=false; preserving other config keys";
        config.water_draw_join = false;
        rejected = Some(match rejected {
            Some(previous) => format!("{previous}; {message}"),
            None => message.into(),
        });
    }
    config.validate()?;
    Ok((config, rejected))
}
/// The production read/parse path. Only NotFound may select the compiled release
/// input; serde defaults for supplied partial files and error recovery stay original.
fn read_runtime_config(path: &std::path::Path, missing: crate::config_input::MissingFile) -> Result<Config> {
    let start = std::time::Instant::now();
    eprintln!("[lg/startup] renderer_config read begin path={}", path.display());
    let Some(bytes) = crate::config_input::read(path, missing, include_bytes!("../../configs/PL-release.json"))?
        else { return Ok(Config::default()) };
    if matches!(bytes, std::borrow::Cow::Borrowed(_)) {
        eprintln!("[lg/vita-renderer] missing config {}; source=compiled-release", path.display());
    }
    eprintln!("[lg/startup] renderer_config read done bytes={} elapsed_ms={:.3}; parse begin", bytes.len(), start.elapsed().as_secs_f64()*1000.);
    let (c, rejected) = parse_runtime_config(&bytes)?;
    if let Some(message) = rejected {
        eprintln!("[lg/vita-renderer] invalid config key {}: {message}", path.display());
    }
    eprintln!("[lg/startup] renderer_config parse done elapsed_ms={:.3}; validate begin", start.elapsed().as_secs_f64()*1000.);
    c.validate()?;
    eprintln!("[lg/startup] renderer_config validate done elapsed_ms={:.3}", start.elapsed().as_secs_f64()*1000.);
    Ok(c)
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
thread_local! {
    /// Scoped test override, per thread: parallel tests must not see each other's keys.
    static TEST_CONFIG: std::cell::RefCell<Option<&'static Config>> = const { std::cell::RefCell::new(None) };
}
/// Tests cannot edit the process-wide OnceLock. Run one body with a modified copy of the
/// shipped defaults instead; the copy is leaked (tests only) so `config()` can keep
/// returning a stable reference, and the previous override is restored on unwind.
#[cfg(test)]
pub(crate) fn with_config<T>(change: impl FnOnce(&mut Config), run: impl FnOnce() -> T) -> T {
    struct Reset(Option<&'static Config>);
    impl Drop for Reset { fn drop(&mut self) { TEST_CONFIG.with(|c| *c.borrow_mut() = self.0); } }
    let mut config = Config::default();
    change(&mut config);
    let config: &'static Config = Box::leak(Box::new(config));
    let _reset = Reset(TEST_CONFIG.with(|c| c.borrow_mut().replace(config)));
    run()
}
#[cfg(test)]
pub fn config() -> &'static Config {
    if let Some(config) = TEST_CONFIG.with(|c| *c.borrow()) { return config; }
    static CONFIG: OnceLock<Config> = OnceLock::new();
    CONFIG.get_or_init(Config::default)
}
#[cfg(not(test))]
pub fn config() -> &'static Config {
    static CONFIG: OnceLock<Config> = OnceLock::new();
    CONFIG.get_or_init(|| {
        let parsed = read_runtime_config(std::path::Path::new(CONFIG_PATH), crate::config_input::MissingFile::for_platform());
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
    /// A `wavenormal` deform the shader applies to the normal. Kept apart from `deform`, which
    /// moves vertices and so decides culling, PVS sources and scheduling; a normal ripple does not.
    wave_normal: Option<Deform>,
}
struct Pending {
    parameter: usize,
    geometry: Geometry,
    recipe: Recipe,
    /// (batch, chunk of that batch's layer) per append, recorded only for `water_stage_merge`.
    origin: Vec<(u32, u32)>,
    /// The part's shader carries a liquid contents bit, as `diag_skip_water` tests it.
    liquid: bool,
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
    liquid: bool,
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
    water_join: Option<water_join::State>,
    pipelines: Vec<Pipeline>,
    shader: ShaderId,
    low_shader:Option<ShaderId>,
    low_pipelines:Vec<Pipeline>,
    sky_pipeline: Pipeline,
    sky_shader: ShaderId,
    sky_bindings: Bindings,
    /// `sky_depth` and `sky_cube`: the map's static sky surfaces.
    sky_surfaces: Option<sky_surfaces::SkySurfaces>,
    alpha_order: Vec<(f32, usize)>,
    prepared: Vec<Prepared>,
    fx_gpu: Option<crate::render_fx::retained::Gpu>,
    visibility_key: Option<[u32; 26]>,
    visibility_generation: u64,
    visibility: Vec<(u64, Option<(f32, Mat4)>)>,
    models: Vec<Option<Mat4>>,
    /// `alpha_model_pvs`: the last PVS answer per group, keyed by camera cluster and model matrix.
    model_pvs: Vec<ModelPvs>,
    schedule: submit::Schedule,
    texture_sort: draw_cuts::TextureSort,
    alpha_fill: alpha_fill::State,
    list: Option<world_list::WorldList>,
    gxm: Box<gxm::State>,
}
/// Blend classes every world renderer has; `exact_blends` adds classes from here on.
const WORLD_BLEND_CLASSES: usize = 5;
/// Blends the five world classes draw with other factors (see `blend_class`).
fn approximated_blend(blend: Blend) -> bool {
    matches!(blend, Blend::ColorAdd | Blend::AlphaColor | Blend::Factors(..))
}
/// The fog mode of an `exact_blends` class, as the desktop renderer picks it, except
/// `AlphaColor`, whose desktop fade toward white has no counterpart in the world program: it
/// fades toward black like the additive layers.
fn exact_fog_mode(blend: Blend) -> f32 {
    match blend {
        Blend::ColorAdd | Blend::AlphaColor => 1.,
        _ => 0.,
    }
}
/// The world pipelines' blend, per blend class.
fn world_blend(class: usize) -> Option<BlendState> {
    match class {
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
    }
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
fn diagnostic_batch_skip(b: &super::Batch, scene: &Scene, c: &Config) -> bool {
    if c.diag_skip_alpha_world && b.transparent() { return true; }
    if !c.diag_skip_water && !c.diag_skip_decals { return false; }
    b.parts.iter().any(|part| {
        let shader=&scene.map.shaders[scene.map.surfaces[part.face].shader];
        (c.diag_skip_water && shader.contents & crate::collision::LIQUID_MASK != 0)
            || (c.diag_skip_decals && shader.name.to_ascii_lowercase().contains("decal"))
    })
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
        let mut diagnostic_batches = 0;
        // `exact_blends`: the blends of pipeline classes WORLD_BLEND_CLASSES and up, in first-seen order.
        let mut exact_blends: Vec<Blend> = Vec::new();
        let mut frozen_deforms = 0;
        let mut frozen_models = 0;
        let (mut wave_normal_layers, mut wave_normal_triangles, mut wave_normal_skipped) = (0, 0, 0);
        for (bi, b) in scene.batches.iter().enumerate() {
            if diagnostic_batch_skip(b, scene, c) { diagnostic_batches+=1; continue; }
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
            let batch_liquid = b.parts.iter().any(|part| {
                scene.map.shaders[scene.map.surfaces[part.face].shader].contents & crate::collision::LIQUID_MASK != 0
            });
            let mut rank = 0;
            for layer in &b.layers {
                if layer.spec.blend == Blend::Invisible || (layer.spec.detail && !c.detail_layers) {
                    dropped_layers += 1;
                    continue;
                }
                if rank == c.max_layers {
                    dropped_layers += 1;
                    continue;
                }
                let recipe_started=std::time::Instant::now();
                let deform = b
                    .deforms
                    .iter()
                    .find(|d| matches!(d, Deform::Wave(..) | Deform::Move(..) | Deform::Bulge(..)))
                    .cloned();
                let reads_normal = layer.spec.environment || layer.spec.dot_alpha.is_some();
                let wave_normal = b.deforms.iter().find(|d| matches!(d, Deform::WaveNormal(..)));
                // Both ride the same Deform/Wave uniforms: a stage that also moves its vertices
                // keeps still normals, as before.
                let wave_normal = if c.water_normals && reads_normal && wave_normal.is_some() {
                    if deform.is_none() {
                        wave_normal_layers += 1;
                        wave_normal_triangles += b.full_indices.len() / 3;
                        wave_normal.cloned()
                    } else {
                        wave_normal_skipped += 1;
                        None
                    }
                } else {
                    None
                };
                let recipe = Recipe {
                    stage: layer.spec.clone(),
                    textures: layer.textures.clone(),
                    lit: layer.lit,
                    rank,
                    blend: if c.exact_blends && approximated_blend(layer.spec.blend) {
                        WORLD_BLEND_CLASSES + exact_blends.iter().position(|&b| b == layer.spec.blend).unwrap_or_else(|| {
                            exact_blends.push(layer.spec.blend);
                            exact_blends.len() - 1
                        })
                    } else {
                        blend_class(layer.spec.blend)
                    },
                    opaque: opaque_stage(&layer.spec),
                    cull: match b.face_side {
                        FaceSide::Both => 0,
                        FaceSide::Front => 1,
                        FaceSide::Back => 2,
                    },
                    model: b.model,
                    motion: b.motion.clone(),
                    deform,
                    wave_normal,
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
                let mut chunk = 0u32;
                for (face, range, center) in parts {
                    let cell = (center / c.cell_size).floor();
                    let key = format!("{signature}|{}|{cell:?}", recipe.rank);
                    let source = &b.full_indices[range];
                    let liquid = face.map_or(batch_liquid, |face| {
                        scene.map.shaders[scene.map.surfaces[face].shader].contents & crate::collision::LIQUID_MASK != 0
                    });
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
                                    origin: Vec::new(),
                                    liquid,
                                });
                                candidates.push(id);
                                id
                            });
                        pending[id].geometry.append(bi, b, source, face);
                        if c.water_stage_merge != "off" { pending[id].origin.push((bi as u32, chunk)); }
                        chunk += 1;
                    }
                }
                chunks_ms+=chunks_started.elapsed().as_secs_f64()*1000.;
            }
        }
        ensure!(
            !pending.is_empty(),
            "retained world has no drawable geometry"
        );
        if c.diag_skip_water || c.diag_skip_decals || c.diag_skip_alpha_world { println!("[lg/draw-diag] omitted_world_batches={diagnostic_batches} water={} decals={} alpha={}",c.diag_skip_water,c.diag_skip_decals,c.diag_skip_alpha_world); }
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
        if c.water_normals {
            println!("[lg/vita-renderer] water_normals=on layers={wave_normal_layers} triangles={wave_normal_triangles} skipped_vertex_deform={wave_normal_skipped}");
        }
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
                    vertex: &shader::vertex(c.water_normals),
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
        if !exact_blends.is_empty() {
            println!("[lg/vita-renderer] exact_blends={:?}", exact_blends);
        }
        let blend_states: Vec<Option<BlendState>> = (0..WORLD_BLEND_CLASSES).map(world_blend)
            .chain(exact_blends.iter().map(|&b| crate::render::blend(crate::render::blend_index(b))))
            .collect();
        for &state in &blend_states {
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
            let low=ctx.new_shader(ShaderSource::Glsl{vertex:&shader::vertex(c.water_normals),fragment:&shader::low_fragment(c.low_fog)},shader::meta())?;
            resources.shaders.push(low);
            if c.low_fog { println!("[lg/vita-renderer] low_fog=on"); }
            drop(shader_timing);
            for cull in [CullFace::Nothing,CullFace::Back,CullFace::Front] {
                crate::frame_profile::pipeline_creation();
                low_pipelines.push(ctx.new_pipeline(&[BufferLayout{stride:52,..Default::default()}],&attributes,low,PipelineParams{depth_test:Comparison::LessOrEqual,depth_write:true,cull_face:cull,front_face_order:FrontFaceOrder::Clockwise,..Default::default()}));
            }
            Some(low)
        }else{None};
        resources.pipelines.extend(low_pipelines.iter().copied());
        // A key whose program does not build is turned off with a warning; the frame stays whole.
        let mut alpha_fill = alpha_fill::State::off();
        let lite = c.world_alpha_lite.then(|| {
            crate::frame_profile::shader_creation();
            let _shader_name = crate::shader_cache::Name::new("retained-world-alpha-lite");
            // The world's vertex shader, so a stage with a wave normal ripples here too.
            ctx.new_shader(ShaderSource::Glsl { vertex: &shader::vertex(c.water_normals),
                fragment: &crate::environment::fragment(alpha_fill::ALPHA_LITE_FRAGMENT) }, shader::meta())
                .map_err(|e| eprintln!("[lg/vita-renderer] warning world_alpha_lite program failed, key off: {e:?}")).ok()
        }).flatten();
        if let Some(lite) = lite {
            resources.shaders.push(lite);
            // Same blend classes as the world pipelines, in the same order.
            let mut lite_pipelines = Vec::new();
            for &state in &blend_states {
                lite_pipelines.extend(alpha_fill::pipelines(ctx, lite, &attributes, state, state));
            }
            resources.pipelines.extend(lite_pipelines.iter().copied());
            alpha_fill.lite = Some((lite, lite_pipelines, super::uniform_fields::Cache::new(shader::meta())));
        }
        if c.water_stage_merge == "merge" {
            crate::frame_profile::shader_creation();
            let _shader_name = crate::shader_cache::Name::new("retained-world-water-merge");
            match ctx.new_shader(ShaderSource::Glsl { vertex: alpha_fill::MERGE_VERTEX,
                fragment: &crate::environment::fragment(alpha_fill::MERGE_FRAGMENT) }, alpha_fill::merge_meta()) {
                Ok(merge) => {
                    resources.shaders.push(merge);
                    let (color, alpha) = alpha_fill::merge_blend();
                    let merge_pipelines = alpha_fill::pipelines(ctx, merge, &attributes, color, alpha);
                    resources.pipelines.extend(merge_pipelines.iter().copied());
                    alpha_fill.merge = Some((merge, merge_pipelines, super::uniform_fields::Cache::new(alpha_fill::merge_meta())));
                }
                Err(e) => eprintln!("[lg/vita-renderer] warning water_stage_merge program failed, key off: {e:?}"),
            }
        }
        if c.water_stage_merge == "drop_filter" || alpha_fill.merge.is_some() {
            let recipes = pending.iter().map(|p| &p.recipe).collect::<Vec<_>>();
            let origins = pending.iter().map(|p| p.origin.clone()).collect::<Vec<_>>();
            alpha_fill.partner = alpha_fill::partners(&recipes, &origins);
            alpha_fill.drop_filter = c.water_stage_merge == "drop_filter";
            println!("[lg/vita-renderer] water_stage_merge={} pairs={}", c.water_stage_merge,
                alpha_fill.partner.iter().filter(|p| p.is_some()).count());
        }
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
        let water_indices: Vec<Option<Vec<u16>>> = if c.water_draw_join {
            pending.iter().zip(&allocations).map(|(p, &(slot, base, count))| {
                (p.liquid && !p.recipe.opaque).then(|| {
                    let (_, _, indices) = &packed[slot];
                    indices[base as usize..(base + count) as usize].to_vec()
                })
            }).collect()
        } else { Vec::new() };
        let water_join = if c.water_draw_join {
            let total: usize = water_indices.iter().flatten().map(Vec::len).sum();
            if total * 2 > 4 * 1024 * 1024 {
                println!("[lg/vita-renderer] water_draw_join key off: {total} indices exceed the 4 MiB stream budget");
                None
            } else {
                let state = water_join::State::new(ctx, water_indices);
                resources.buffers.push(state.buffer);
                println!("[lg/vita-renderer] water_draw_join groups={} indices={} stream_bytes={}",
                    state.indices.iter().filter(|i| i.is_some()).count(), total, total * 2);
                Some(state)
            }
        } else { None };
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
                liquid: p.liquid,
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
        let sky_surfaces = sky_surfaces::SkySurfaces::new(scene, ctx, &mut resources)?;
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
            model_pvs: model_pvs_cache(),
            world_buffers: buffers,
            schedule: submit::Schedule::default(),
            texture_sort: draw_cuts::TextureSort::default(),
            alpha_fill,
            groups,
            pvs,
            water_join,
            pipelines,
            shader,low_shader,low_pipelines,
            sky_pipeline,
            sky_shader,
            sky_bindings,
            sky_surfaces,
        })
    }
    /// `water_draw_join`: plan this frame's runs from the joint alpha list, then upload their
    /// concatenated indices once. A run holds the list entries between two non-members in list
    /// order, so drawing it at its first member's place submits exactly those primitives in order.
    fn plan_water_runs(&mut self, list: &[(f32, crate::render_fx::retained::Source)]) {
        use crate::render_fx::retained::Source;
        let Some(state) = self.water_join.as_mut() else { return; };
        state.begin_frame(self.prepared.len());
        if !state.enabled { return; }
        for (_, source) in list {
            let entry = match *source {
                Source::Actor(_) => water_join::Entry::Fence(water_join::Fence::Actor),
                Source::Puff(_) => water_join::Entry::Fence(water_join::Fence::Puff),
                Source::World(id) => {
                    let group = self.prepared[id].group;
                    let g = &self.groups[group];
                    let fence = if !g.liquid { Some(water_join::Fence::NonLiquid) }
                        else if state.indices[group].is_none() { Some(water_join::Fence::NoIndices) }
                        else if self.alpha_fill.lite.is_some() && alpha_fill::lite_recipe(&g.recipe) {
                            Some(water_join::Fence::AlphaLite)
                        } else { None };
                    match fence {
                        Some(fence) => water_join::Entry::Fence(fence),
                        None => water_join::Entry::Join {
                            key: water_join::Key {
                                pipeline: self.pipelines[g.recipe.blend * 3 + g.recipe.cull],
                                vertex: g.bindings.vertex_buffers[0], atlas: g.bindings.images[1],
                                texture: self.prepared[id].texture, parameter: g.parameter,
                            },
                            first: id,
                            items: state.indices[group].as_ref().expect("a join key has an index copy"),
                        },
                    }
                }
            };
            state.plan.entry(entry, state.capacity);
        }
        if state.plan.scratch.is_empty() { return; }
        let ctx = unsafe { get_internal_gl() }.quad_context;
        let upload = water_probe::tick();
        if !state.plan.upload(&mut state.enabled, |scratch| {
            ctx.buffer_update_checked(state.buffer, BufferSource::slice(scratch))
        }) {
            println!("[lg/vita-renderer] water_draw_join disabled for this map: index upload failed; using original draws");
            return;
        }
        let scratch = &state.plan.scratch;
        water_probe::add(|f| &mut f.water_upload_time, upload);
        water_probe::add_n(|f| &mut f.water_upload_bytes, (scratch.len() * 2) as u64);
        count("vita_world_alpha_join_runs", state.plan.runs.len() as u64);
        count("vita_world_alpha_join_upload_bytes", (scratch.len() * 2) as u64);
        state.upload_bytes += (scratch.len() * 2) as u64;
    }
    /// Submit one planned `water_draw_join` run as a single indexed draw. The host probe still
    /// sees one canonical packet per original draw, in the same order, so the oracle compares
    /// logical submissions and the joined physical command separately.
    fn draw_water_run(&mut self, index: usize) {
        let Some(run) = self.water_join.as_ref().map(|state| state.plan.runs[index]) else { return; };
        #[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
        {
            let members = self.water_join.as_ref().expect("join state").plan.members[index].clone();
            for member in &members {
                let group = self.prepared[*member].group;
                let g = &self.groups[group];
                crate::sim_maps::probe::world_order(group, true);
                crate::sim_maps::probe::draw_query::mark("world_alpha", "world_joined",
                    g.recipe.blend, Some(group), g.recipe.stage.alpha_test,
                    g.recipe.stage.images.first().map(String::as_str));
                crate::sim_maps::probe::world_packet(group, g.base_element, g.indices,
                    &self.prepared[*member].texture,
                    self.parameters[g.parameter].uniforms.bits_ref(), false,
                    world_list::pipeline_class(g.recipe.blend, g.recipe.cull));
            }
        }
        let ctx = unsafe { get_internal_gl() }.quad_context;
        let uniforms = &self.parameters[run.key.parameter].uniforms;
        let buffer = self.water_join.as_ref().expect("join state").buffer;
        let probe_submit = if water_probe::in_window() { water_probe::tick() } else { None };
        let probe_pipeline = water_probe::tick();
        ctx.apply_pipeline(&run.key.pipeline);
        unsafe { macroquad::miniquad::gl::glDepthMask(0); }
        scale::log_depth(false);
        water_probe::add(|f| &mut f.water_pipeline_time, probe_pipeline);
        let probe_bindings = water_probe::tick();
        ctx.apply_bindings(&Bindings { vertex_buffers: vec![run.key.vertex], index_buffer: buffer,
            images: vec![run.key.texture, run.key.atlas] });
        water_probe::add(|f| &mut f.water_binding_time, probe_bindings);
        let probe_uniform = water_probe::tick();
        let traffic = self.field_uniforms[0].apply(ctx, uniforms.values(), config().uniform_field_cache);
        water_probe::add(|f| &mut f.water_uniform_time, probe_uniform);
        let probe_draw = water_probe::tick();
        if !crate::gpu_timing::masked(crate::gpu_timing::MASK_WORLD_ALPHA) {
            ctx.draw(run.offset as i32, run.count as i32, 1);
        }
        water_probe::add(|f| &mut f.water_draw_time, probe_draw);
        water_probe::add(|f| &mut f.water_submit_time, probe_submit);
        water_probe::add_n(|f| &mut f.water_draw_calls, 1);
        water_probe::add_n(|f| &mut f.water_pipeline_applications, 1);
        water_probe::add_n(|f| &mut f.water_binding_applications, 1);
        water_probe::add_n(|f| &mut f.water_uniform_applications, 1);
        water_probe::add_n(|f| &mut f.water_uniform_calls, traffic.calls);
        water_probe::add_n(|f| &mut f.water_uniform_bytes, traffic.bytes);
        count("vita_world_gpu_draws", 1);
        count("vita_world_alpha_triangles", run.count as u64 / 3);
        count("vita_world_alpha_state_applications", 1);
        count("vita_world_alpha_joined_draws", 1);
        count("vita_world_alpha_joined_groups", run.groups as u64);
        count("vita_world_alpha_joined_indices", run.count as u64);
        crate::light_profile::render_add(crate::light_profile::RenderPhase::WorldAlpha,
            crate::light_profile::RenderMetrics { draws: 1, pipelines: 1, depth_states: 1, bindings: 1,
                texture_bind_slots: 2, uniform_uploads: 1, uniform_bytes: traffic.bytes,
                uniform_gl_calls: traffic.calls, ..Default::default() });
        #[cfg(target_os = "vita")]
        {
            crate::frame_profile::record_world_submission();
            crate::frame_profile::record_geometry(run.count / 3, 0, true);
        }
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
        water_probe::begin(alpha);
        let probe_draw = water_probe::tick();
        let census_visibility = crate::phase_census::span(crate::phase_census::Region::WorldVisibility);
        let visibility_started = MetricInstant::now();
        let visibility_span = crate::frame_profile::span("vita_cluster_frustum");
        let projection = crate::render_fx::projection().expect("world camera projection");
        let cluster = scene.map.visibility.cluster(eye, &scene.map.planes);
        let lists = self.pvs.get(cluster);
        let ids = if alpha { &lists.alpha } else { &lists.opaque };
        #[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
        if alpha && crate::sim_maps::probe::draw_query::armed() {
            println!("WORLD_ALPHA_LIST {}", serde_json::json!({
                "frame": crate::sim_maps::probe::frame(),
                "cluster": cluster, "visible_alpha": ids.len(),
                "map_alpha": self.groups.iter().filter(|g| !g.recipe.opaque).count(),
                "unknown_alpha": self.pvs.unknown.alpha.len(),
            }));
        }
        // `alpha_model_pvs`: the per-group answers are taken out of the renderer so the loop below
        // can hold a group borrow and the cache at once.
        let mut model_pvs = (alpha && config().alpha_model_pvs).then(|| std::mem::take(&mut self.model_pvs));
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
        let world_min = config().world_min_pixels;
        let list_submit = !alpha && self.list.is_some();
        if list_submit {
            let list = self.list.as_mut().unwrap();
            let generation = self.visibility_generation;
            for &id in ids {
                let entry = &mut list.visible[id];
                // Valid models were written by the test that set this generation.
                if entry.static_group && entry.generation == generation {
                    hits += 1;
                    if entry.visible {
                        self.alpha_order.push((entry.depth, id));
                        if !alpha && self.groups[id].liquid {
                            water_probe::add_n(|f| &mut f.opaque_water, 1);
                            water_probe::add_n(|f| &mut f.opaque_water_triangles, self.groups[id].indices as u64 / 3);
                        }
                    }
                    continue;
                }
                let g = &self.groups[id];
                let result = model_matrix(&g.recipe, time, transforms).and_then(|model| {
                    tests += 1;
                    let m = projection * model;
                    (in_frustum(m, g.min, g.max) && !draw_cuts::below_pixels(m, g.min, g.max, world_min)).then(|| {
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
                if let Some((depth, _)) = result {
                    self.alpha_order.push((depth, id));
                    if !alpha && self.groups[id].liquid {
                        water_probe::add_n(|f| &mut f.opaque_water, 1);
                        water_probe::add_n(|f| &mut f.opaque_water_triangles, self.groups[id].indices as u64 / 3);
                    }
                }
            }
        } else {
        for &id in ids {
            let g = &self.groups[id];
            let static_group = g.recipe.model.is_none() && g.recipe.motion.is_none();
            let visibility = water_probe::tick();
            let result = if reuse && static_group && self.visibility[id].0 == self.visibility_generation {
                hits += 1;
                self.visibility[id].1
            } else {
                let model = model_matrix(&g.recipe, time, transforms);
                // A model group is always in every cluster's list: its geometry is stored in
                // model space, so the load-time face sources cannot place it. The placed bounds
                // can, and `bounds_visible` answers from the map's own PVS row.
                let hidden = model_pvs.is_some() && g.recipe.model.is_some() && g.recipe.motion.is_none()
                    && g.recipe.deform.is_none() && model.is_some_and(|model| model_bounds_hidden(
                        model_pvs.as_mut().unwrap(), &scene.map.visibility, &scene.map.planes,
                        cluster, id, model, g.min, g.max));
                if hidden { count("vita_world_model_pvs_culled", 1); }
                let result = if hidden { None } else { model.and_then(|model| {
                    tests += 1;
                    let m = projection * model;
                    (in_frustum(m, g.min, g.max) && !draw_cuts::below_pixels(m, g.min, g.max, world_min)).then(|| {
                        let depth = if crate::render_fx::retained_active() {
                            crate::render_fx::group_depth(model.transform_point3(g.center))
                        } else { (model.transform_point3(g.center) - eye).length_squared() };
                        (depth, model)
                    })
                }) };
                if reuse && static_group { self.visibility[id] = (self.visibility_generation, result); }
                result
            };
            water_probe::add(|f| &mut f.vis_time, visibility);
            if g.liquid { water_probe::add(|f| &mut f.water_vis_time, visibility); }
            self.models[id] = result.map(|(_, model)| model);
            if let Some((depth, _)) = result {
                self.alpha_order.push((depth, id));
                if !alpha && g.liquid {
                    water_probe::add_n(|f| &mut f.opaque_water, 1);
                    water_probe::add_n(|f| &mut f.opaque_water_triangles, g.indices as u64 / 3);
                }
            }
        }
        }
        crate::light_profile::render_add(phase, RenderMetrics { visibility_tests: tests,
            visibility_cpu_ns: visibility_started.elapsed().as_nanos() as u64, ..Default::default() });
        if let Some(cache) = model_pvs { self.model_pvs = cache; }
        count("vita_world_visibility_cache_hits", hits);
        drop(visibility_span);
        drop(census_visibility);
        if !alpha { water_probe::add_n(|f| &mut f.opaque_groups, self.alpha_order.len() as u64); }
        let sort_started = water_probe::tick();
        if alpha && !config().world_alpha {self.alpha_order.clear();}
        if !alpha && config().world_texture_sort {
            self.texture_sort.apply(&mut self.alpha_order, &self.groups);
        } else if !alpha && config().opaque_order=="front_to_back" {
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
        water_probe::add(|f| &mut f.sort_time, sort_started);
        let probe_parameters = water_probe::tick();
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
            let mut water_parameters: Vec<usize> = Vec::new();
            for &(depth, id) in &self.alpha_order {
                let g = &self.groups[id];
                let liquid = g.liquid;
                if liquid && !water_parameters.contains(&g.parameter) { water_parameters.push(g.parameter); }
                let model = if reuse { self.models[id].unwrap() }
                    else { model_matrix(&g.recipe, time, transforms).unwrap() };
                let build_started = if liquid { water_probe::tick() } else { None };
                let mut built = false;
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
                        built = true;
                    } else if split && p.time != time.to_bits() {
                        p.update_time(time);
                        time_updates += 1;
                    }
                    None
                } else {
                    let mut u = uniforms(&g.recipe, time, eye, model, projection, fullbright, scene.atlas_grid);
                    u.atmosphere(&scene.atmosphere, eye);
                    builds += 1;
                    built = true;
                    Some(u)
                };
                if liquid {
                    water_probe::add(|f| &mut f.water_parameter_time, build_started);
                    if built { water_probe::add_n(|f| &mut f.water_parameter_builds, 1); }
                    if g.recipe.deform.is_some() { water_probe::add_n(|f| &mut f.water_deform_groups, 1); }
                    if g.recipe.stage.mods.iter().any(|m| matches!(m, TcMod::Turb(_))) {
                        water_probe::add_n(|f| &mut f.water_turb_groups, 1);
                    }
                    if g.recipe.stage.environment { water_probe::add_n(|f| &mut f.water_environment_groups, 1); }
                    if g.recipe.stage.dot_alpha.is_some() { water_probe::add_n(|f| &mut f.water_dot_alpha_groups, 1); }
                    if g.recipe.wave_normal.is_some() { water_probe::add_n(|f| &mut f.water_wave_normal_groups, 1); }
                    water_probe::add_n(|f| &mut f.water_groups, 1);
                    water_probe::add_n(|f| &mut f.water_triangles, g.indices as u64 / 3);
                }
                let texture = g.recipe.textures[g.recipe.stage.frame(time)].raw_miniquad_id();
                self.prepared.push(Prepared { depth, group: id, texture, uniforms: u, indices: g.indices });
            }
            water_probe::add_n(|f| &mut f.water_parameters, water_parameters.len() as u64);
            }
            crate::light_profile::render_add(phase, RenderMetrics {
                parameter_cpu_ns: parameters_started.elapsed().as_nanos() as u64, ..Default::default() });
            count("vita_world_parameter_builds", builds);
            count("vita_world_parameter_time_updates", time_updates);
            count("vita_world_parameter_reuses", self.alpha_order.len() as u64 - builds);
        }
        water_probe::add(|f| &mut f.parameter_time, probe_parameters);
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
            water_probe::add_n(|f| &mut f.alpha_groups, self.prepared.len() as u64);
            water_probe::add(|f| &mut f.alpha_draw_time, probe_draw);
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
        if !alpha && config().sky && !scene.authored_sky && !config().diag_skip_sky {
            #[cfg(not(target_os = "vita"))]
            let probe = super::sky_stats::begin(false);
            let _sky_span=crate::frame_profile::span("vita_world_sky_submit");
            let mut totals = crate::frame_profile::Totals::<3>::profile_only();
            let bg = scene.atmosphere.background();
            let sky = sky_gradient(bg, config().fog_sky && scene.atmosphere.distance.w > 0.);
            let gl = unsafe { get_internal_gl() };
            let ctx = gl.quad_context;
            let cube = scene.sky_cube.as_ref().filter(|c| c.ready())
                .filter(|_| !config().sky_cube_capture_only && self.sky_surfaces.is_some()).map(|c| {
                let rotation = if scene.sky_origin.is_some() {
                    scene.sky_rotation * scene.sky_motion.as_ref().map_or(Quat::IDENTITY, |m| m.transform(time).1)
                } else { Quat::IDENTITY };
                use super::sky_cube::Set;
                sky_surfaces::CubeView {
                    texture: c.target.texture.raw_miniquad_id(),
                    rotation: Mat4::from_quat(rotation),
                    inset: 0.5 / c.size as f32,
                    // The Vita copy of a target is stored top row first, as `scale.rs` upscales it.
                    flip: cfg!(target_os = "vita") != (config().scaled_target == "direct_flip"),
                    rows: c.rows(), room_row: c.row(Set::Room), layers_row: c.row(Set::Layers),
                }
            });
            let background = self.sky_surfaces.as_ref().is_some_and(|s| s.has_background(cube.as_ref()));
            let start = totals.start();
            if !background {
                ctx.apply_pipeline(&self.sky_pipeline);
                ctx.apply_bindings(&self.sky_bindings);
            }
            totals.add(0, start);
            let start = totals.start();
            if !background { ctx.apply_uniforms(UniformsSource::table(&sky)); }
            totals.add(1, start);
            let start = totals.start();
            #[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
            crate::sim_maps::probe::draw_query::mark("world_opaque","sky",0,None,0,None);
            if !crate::gpu_timing::masked(crate::gpu_timing::MASK_SKY) {
                match (&self.sky_surfaces, &cube) {
                    (Some(s), Some(c)) if background => s.draw_background(ctx, &self.sky_bindings, projection, eye, c),
                    _ => ctx.draw(0, 3, 1),
                }
            }
            totals.add(2, start);
            totals.publish(["vita_world_sky_state_bindings", "vita_world_sky_uniforms", "vita_world_sky_draws_time"]);
            count("vita_world_gpu_draws", 1);
            count("vita_world_sky_draws", 1);
            crate::light_profile::render_add(phase, RenderMetrics { draws: 1, pipelines: 1,
                bindings: 1, uniform_uploads: 1, uniform_bytes: 32, uniform_gl_calls: 2, ..Default::default() });
            let mode = match (&self.sky_surfaces, &cube) {
                (Some(_), Some(_)) => "cube",
                (Some(_), None) if config().sky_depth => "depth",
                _ => "gradient",
            };
            if let (Some(sky), true) = (&self.sky_surfaces, mode != "gradient") {
                let _span = crate::frame_profile::span("vita_world_sky_surfaces");
                #[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
                crate::sim_maps::probe::draw_query::mark("world_opaque","sky_surfaces",0,None,0,None);
                let draws = sky.draw(ctx, projection, eye, cube.as_ref());
                count("vita_world_gpu_draws", draws);
                count("vita_world_sky_surface_draws", draws);
                crate::light_profile::render_add(phase, RenderMetrics { draws, pipelines: 1,
                    bindings: draws, uniform_uploads: 1, ..Default::default() });
            }
            #[cfg(not(target_os = "vita"))]
            super::sky_stats::end(probe, mode, &format!("sky_surface_triangles={}",
                self.sky_surfaces.as_ref().map_or(0, |s| s.triangles)));
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
            let submit_started = water_probe::tick();
            self.submit_list(time, gxm::Pass::Frame, &mut totals, &mut counts, &mut previous_end, &mut between_ns, &mut between_calls);
            water_probe::add(|f| &mut f.opaque_submit_time, submit_started);
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
            crate::sim_maps::probe::world_packet(prepared.group, g.base_element, prepared.indices, &texture, u.bits_ref(), low, pi);
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
        let batch = config().world_adjacent_batch;
        let projection = crate::render_fx::projection().expect("world camera projection");
        let mut position = 0;
        while position < list.items.len() {
            let first = position;
            let item = &list.items[first];
            let draw = &list.draws[item.group as usize];
            let (end, indices) = if batch { list.adjacent_run(first, groups, projection) }
                else { (first + 1, draw.indices) };
            position = end;
            if end > first + 1 { count("vita_world_adjacent_draws_saved", (end - first - 1) as u64); }
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
                && state.direct_draw(&mut real, item.group as usize, indices as u32, item.uniforms, low, bits, pass, drawn - 1);
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
            for member in &list.items[first..end] {
                let part = &list.draws[member.group as usize];
                // Canonical logical packets keep every original range. The run certificate checked
                // that the one actual draw covers exactly their ordered concatenation.
                crate::sim_maps::probe::world_packet(member.group as usize,
                    part.base_element + (base_element - draw.base_element), part.indices, &member.texture,
                    parameters[part.parameter as usize].uniforms.bits_ref(), part.low, part.pipeline_class);
            }
            if !direct {
                #[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
                let dropped = world_list::negative("world-list-drop-draw") && drawn % 13 == 6;
                #[cfg(not(all(feature="sim_maps_probe",not(target_os="vita"))))]
                let dropped = false;
                #[cfg(target_os = "vita")]
                if way != gxm::Way::Vitagl { gxm::vita::arm(); }
                let ran = !dropped && !crate::gpu_timing::masked(class);
                #[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
                let (actual_base, actual_indices) = if end > first + 1 {
                    (base_element + i32::from(world_list::negative("world-adjacent-wrong-base")) * 3,
                        indices - i32::from(world_list::negative("world-adjacent-short-range")) * 3)
                } else { (base_element, indices) };
                #[cfg(not(all(feature="sim_maps_probe",not(target_os="vita"))))]
                let (actual_base, actual_indices) = (base_element, indices);
                if ran {
                    #[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
                    list.verify_emission(first, end, actual_base, actual_indices, parameters);
                    ctx.draw(actual_base, actual_indices, 1);
                }
                #[cfg(target_os = "vita")]
                if way != gxm::Way::Vitagl {
                    let capture = gxm::vita::take();
                    // A masked draw called nothing, so there is nothing to learn from it.
                    if ran { state.after_vitagl(&capture, item.group as usize, draw.animated, draw.pipeline_class, low, indices as u32, item.uniforms, bits, way); }
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
            c.triangles += indices as usize / 3;
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
        let probe_finish = water_probe::tick();
        let _pass_clock = crate::pass_clock::scope(crate::pass_clock::FX_FINISH);
        let mut gpu = self.fx_gpu.take().unwrap_or_else(|| {
            crate::render_fx::retained::Gpu::new().expect("retained actor shader")
        });
        // water_stage_merge: a pair draws once, at the filter stage's place ("merge"), or as
        // the alpha stage alone ("drop_filter"). See alpha_fill::frame_pairs for the order.
        if self.alpha_fill.partner.is_empty() { self.alpha_fill.pairs.clear(); }
        else { alpha_fill::frame_pairs(&self.prepared, |p| (p.depth, p.group), &self.alpha_fill.partner, &mut self.alpha_fill.pairs); }
        let drop_filter = self.alpha_fill.drop_filter;
        let world = self
            .prepared
            .iter()
            .enumerate()
            .filter(|&(id, _)| !self.alpha_fill.pairs.iter().any(|&(f, a)| id == if drop_filter { f } else { a }))
            .map(|(id, p)| (p.depth, id))
            .collect::<Vec<_>>();
        if !self.alpha_fill.partner.is_empty() { count("vita_world_alpha_merged_pairs", self.alpha_fill.pairs.len() as u64); }
        if self.alpha_fill.active() { self.alpha_fill.interrupt(); }
        let totals = std::cell::RefCell::new(crate::frame_profile::Totals::<3>::profile_only());
        let mut adjacent_state = state_cache::Cache::new();
        let adjacent = config().world_adjacent_batch;
        let result = crate::render_fx::retained::finish_world(&mut gpu, &world, interop_flush, |event| {
            use crate::render_fx::retained::WorldEvent;
            let WorldEvent::Draw(id, continuation) = event else {
                if let WorldEvent::Plan(list) = event { self.plan_water_runs(list); }
                return;
            };
            // water_draw_join: the run is drawn at its first member's place, in primitive order.
            if self.water_join.is_some() {
                let run = self.water_join.as_ref()
                    .and_then(|state| state.plan.member.get(id).copied().filter(|&m| m != u32::MAX));
                if let Some(run) = run {
                    // The joined command changes pipeline, bindings and uniforms outside the
                    // adjacent state cache; the next draw must observe a fresh one.
                    adjacent_state = state_cache::Cache::new();
                    if self.water_join.as_ref().unwrap().plan.runs[run as usize].first == id {
                        self.draw_water_run(run as usize);
                    }
                    return;
                }
            }
            let _census = crate::phase_census::span(crate::phase_census::Region::AlphaSubmit);
            let _owner = crate::render_tail::owner(crate::light_profile::RenderPhase::WorldAlpha);
            let _timer = crate::light_profile::render_timer(crate::light_profile::RenderPhase::WorldAlpha);
            let probe_world = water_probe::tick();
            if !adjacent || !continuation { adjacent_state = state_cache::Cache::new(); }
            let mut totals = totals.borrow_mut();
            let merged = if drop_filter { None } else {
                self.alpha_fill.pairs.iter().find(|&&(f, _)| f == id).map(|&(_, a)| a) };
            if let Some(a) = merged {
                adjacent_state = state_cache::Cache::new();
                crate::light_profile::render_add(crate::light_profile::RenderPhase::WorldAlpha,
                    crate::light_profile::RenderMetrics { draws: 1, pipelines: 1, depth_states: 1,
                        bindings: 1, texture_bind_slots: 2, uniform_uploads: 1, ..Default::default() });
                // The filter stage and the alpha stage in one draw, over the filter's range.
                let (pf, pa) = (&self.prepared[id], &self.prepared[a]);
                let (gf, ga) = (&self.groups[pf.group], &self.groups[pa.group]);
                let uf = pf.uniforms.as_ref().unwrap_or(&self.parameters[gf.parameter].uniforms);
                let ua = pa.uniforms.as_ref().unwrap_or(&self.parameters[ga.parameter].uniforms);
                alpha_fill::merge_payload(uf, ua, &mut self.alpha_fill.payload);
                let (_, pipelines, fields) = self.alpha_fill.merge.as_mut().expect("merge program");
                let ctx = unsafe { get_internal_gl() }.quad_context;
                let start = totals.start();
                ctx.apply_pipeline(&pipelines[gf.recipe.cull]);
                unsafe{macroquad::miniquad::gl::glDepthMask(0);}
                scale::log_depth(false);
                ctx.apply_bindings(&Bindings { vertex_buffers: gf.bindings.vertex_buffers.clone(),
                    index_buffer: gf.bindings.index_buffer, images: vec![pf.texture, pa.texture] });
                totals.add(0, start);
                let start = totals.start();
                let traffic = fields.apply(ctx, &self.alpha_fill.payload[..], config().uniform_field_cache);
                crate::light_profile::render_add(crate::light_profile::RenderPhase::WorldAlpha,
                    crate::light_profile::RenderMetrics { uniform_bytes: traffic.bytes,
                        uniform_gl_calls: traffic.calls, ..Default::default() });
                totals.add(1, start);
                let start = totals.start();
                #[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
                { crate::sim_maps::probe::world_order(pf.group,true); crate::sim_maps::probe::world_order(pa.group,true);
                  crate::sim_maps::probe::draw_query::mark("world_alpha","world_merge",
                    gf.recipe.blend, Some(pf.group), 0, gf.recipe.stage.images.first().map(String::as_str)); }
                if !crate::gpu_timing::masked(crate::gpu_timing::MASK_WORLD_ALPHA) { ctx.draw(gf.base_element, gf.indices, 1); }
                totals.add(2, start);
                count("vita_world_gpu_draws", 1);
                count("vita_world_alpha_triangles", gf.indices as u64 / 3);
                count("vita_world_alpha_state_applications", 1);
                return;
            }
            let p = &self.prepared[id];
            let g = &mut self.groups[p.group];
            g.bindings.images[0] = p.texture;
            let liquid = g.liquid;
            let u = p.uniforms.as_ref().unwrap_or(&self.parameters[g.parameter].uniforms);
            let ctx = unsafe { get_internal_gl() }.quad_context;
            let probe_draw = if liquid { water_probe::tick() } else { None };
            #[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
            let census_started = crate::sim_maps::probe::draw_query::armed().then(std::time::Instant::now);
            let start = totals.start();
            // world_alpha_lite: the same draw through the lean program when its stage allows.
            let lite = self.alpha_fill.lite.as_mut().filter(|_| alpha_fill::lite_recipe(&g.recipe));
            let lite_draw = lite.is_some();
            if lite_draw { count("vita_world_alpha_lite_draws", 1); }
            let (pipeline, fields) = match lite {
                Some((_, pipelines, fields)) => (&pipelines[g.recipe.blend * 3 + g.recipe.cull], fields),
                None => (&self.pipelines[g.recipe.blend * 3 + g.recipe.cull], &mut self.field_uniforms[0]),
            };
            let mut plan = if adjacent {
                adjacent_state.observe(*pipeline, lite_draw, false,
                    (g.bindings.vertex_buffers[0], g.bindings.index_buffer, p.texture, g.bindings.images[1]), u.bits())
            } else { state_cache::Applications { pipeline: true, depth: true, bindings: true, uniforms: true } };
            // Conservatively treat every pipeline transition as a program transition, including
            // future blend-specific variants. A matching pipeline always names the same program.
            plan.uniforms |= plan.pipeline;
            let probe_pipeline = if liquid { water_probe::tick() } else { None };
            if plan.pipeline { ctx.apply_pipeline(pipeline); }
            if plan.depth { unsafe{macroquad::miniquad::gl::glDepthMask(0);} }
            if plan.pipeline || plan.depth { scale::log_depth(false); }
            water_probe::add(|f| &mut f.water_pipeline_time, probe_pipeline);
            let probe_bindings = if liquid { water_probe::tick() } else { None };
            if plan.bindings { ctx.apply_bindings(&g.bindings); }
            water_probe::add(|f| &mut f.water_binding_time, probe_bindings);
            if liquid {
                water_probe::add_n(|f| &mut f.water_pipeline_applications, u64::from(plan.pipeline));
                water_probe::add_n(|f| &mut f.water_binding_applications, u64::from(plan.bindings));
                water_probe::add_n(|f| &mut f.water_uniform_applications, u64::from(plan.uniforms));
            }
            if adjacent { count("vita_world_alpha_adjacent_state_skips", u64::from(!plan.pipeline) + u64::from(!plan.depth)
                + u64::from(!plan.bindings) + u64::from(!plan.uniforms)); }
            crate::light_profile::render_add(crate::light_profile::RenderPhase::WorldAlpha,
                crate::light_profile::RenderMetrics { draws: 1, pipelines: u64::from(plan.pipeline),
                    depth_states: u64::from(plan.depth), bindings: u64::from(plan.bindings),
                    texture_bind_slots: 2 * u64::from(plan.bindings), uniform_uploads: u64::from(plan.uniforms),
                    ..Default::default() });
            totals.add(0, start);
            let start = totals.start();
            let probe_uniform = if liquid { water_probe::tick() } else { None };
            let traffic = if plan.uniforms { fields.apply(ctx, u.values(), config().uniform_field_cache) }
                else { super::uniform_fields::Traffic::default() };
            water_probe::add(|f| &mut f.water_uniform_time, probe_uniform);
            if liquid {
                water_probe::add_n(|f| &mut f.water_uniform_calls, traffic.calls);
                water_probe::add_n(|f| &mut f.water_uniform_bytes, traffic.bytes);
            }
            crate::light_profile::render_add(crate::light_profile::RenderPhase::WorldAlpha,
                crate::light_profile::RenderMetrics { uniform_bytes: traffic.bytes,
                    uniform_gl_calls: traffic.calls, ..Default::default() });
            totals.add(1, start);
            let start = totals.start();
            #[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
            crate::sim_maps::probe::world_order(p.group,true);
            #[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
            crate::sim_maps::probe::draw_query::mark("world_alpha",if lite_draw {"world_lite"} else {"world_full"},
                g.recipe.blend, Some(p.group), g.recipe.stage.alpha_test,
                g.recipe.stage.images.first().map(String::as_str));
            #[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
            crate::sim_maps::probe::world_packet(p.group, g.base_element, g.indices, &p.texture, u.bits_ref(), false, world_list::pipeline_class(g.recipe.blend, g.recipe.cull));
            let probe_gpu_draw = if liquid { water_probe::tick() } else { None };
            if !crate::gpu_timing::masked(crate::gpu_timing::MASK_WORLD_ALPHA) { ctx.draw(g.base_element, g.indices, 1); }
            water_probe::add(|f| &mut f.water_draw_time, probe_gpu_draw);
            if liquid {
                water_probe::add_n(|f| &mut f.water_draw_calls, 1);
                water_probe::add(|f| &mut f.water_submit_time, probe_draw);
            }
            totals.add(2, start);
            count("vita_world_gpu_draws", 1);
            count("vita_world_alpha_triangles", g.indices as u64 / 3);
            count(
                "vita_world_uniform_bytes",
                std::mem::size_of::<shader::Uniforms>() as u64,
            );
            // Every alternation can change state; this counter is conservative.
            count("vita_world_alpha_state_applications", 1);
            // Host census: one line per alpha world draw on the sampled frame, for the GL
            // samples table of the same frame. Times the state apply, uniform upload and submit.
            #[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
            if let Some(started) = census_started {
                let stage = &g.recipe.stage;
                println!("WORLD_ALPHA_DRAW {}", serde_json::json!({
                    "frame": crate::sim_maps::probe::frame(),
                    "group": p.group, "depth_bits": p.depth.to_bits(),
                    "parameter": g.parameter,
                    "min": g.min.to_array(), "max": g.max.to_array(),
                    "triangles": g.indices / 3, "indices": g.indices,
                    "blend": format!("{:?}", stage.blend), "blend_class": g.recipe.blend,
                    "lit": g.recipe.lit, "alpha_test": stage.alpha_test,
                    "clamp": stage.clamp, "environment": stage.environment,
                    "vector_uv": stage.vector_uv.is_some(), "dot_alpha": stage.dot_alpha.is_some(),
                    "depth_equal": stage.depth_equal, "liquid": liquid,
                    "model": g.recipe.model.is_some(), "model_id": g.recipe.model,
                    "model_identity": self.models[p.group].map(|m| m == Mat4::IDENTITY),
                    "model_translation": self.models[p.group].map(|m| m.to_cols_array()[12..15].to_vec()),
                    "deform": g.recipe.deform.is_some(),
                    "motion": g.recipe.motion.is_some(), "images": stage.images,
                    "texture": format!("{:?}", p.texture),
                    "lite_recipe": alpha_fill::lite_recipe(&g.recipe),
                    "cpu_ns": started.elapsed().as_nanos() as u64,
                    "query_overhead_included": true,
                }));
            }
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
            water_probe::add(|f| &mut f.world_submit_time, probe_world);
        }, Some(&totals));
        self.prepared.clear();
        self.fx_gpu = Some(gpu);
        water_probe::finish(probe_finish);
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
        if let Some(join) = self.water_join.take() { ctx.delete_buffer(join.buffer); }
        for p in &self.pipelines {
            ctx.delete_pipeline(*p);
        }
        ctx.delete_pipeline(self.sky_pipeline);
        for p in &self.low_pipelines {ctx.delete_pipeline(*p);}
        if let Some(shader)=self.low_shader {ctx.delete_shader(shader);}
        ctx.delete_shader(self.shader);
        ctx.delete_shader(self.sky_shader);
        self.alpha_fill.delete(ctx);
        ctx.delete_buffer(self.sky_bindings.vertex_buffers[0]);
        ctx.delete_buffer(self.sky_bindings.index_buffer);
        if let Some(sky) = &self.sky_surfaces { sky.delete(ctx); }
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
/// `alpha_model_pvs`: one group's last answer. The key is the camera cluster and the model matrix
/// the tested bounds were placed with; a moving model or a new cluster recomputes.
#[derive(Clone, Copy, Default)]
struct ModelPvs {
    group: u32,
    /// The camera cluster, or `u32::MAX` for an unknown one.
    cluster: u32,
    model: [u32; 16],
    hidden: bool,
}
/// The box a model matrix places a group's own bounds in. Enlarging an axis-aligned box by
/// enclosing the eight transformed corners can only add space, so a PVS test over it is
/// conservative. The supported matrices are the rigid rotation+translation this pass builds in
/// `model_matrix` (an eligible group has no motion and no deform), so every exact vertex lies
/// inside the exact corner hull. The f32 corner computation is inflated outward by 32·2^-24 of
/// its largest coordinate (0.031 units at the corpus extreme of 8192), which covers the
/// rounding in the matrix product and in the corner sums without moving a real edge inward.
/// This allowance is separate from, and additive to, the plane-support margin inside
/// `Visibility::bounds_visible`, which covers the plane and box f32 storage in that test.
fn placed_bounds(model: Mat4, min: Vec3, max: Vec3) -> (Vec3, Vec3) {
    let mut low = Vec3::splat(f32::INFINITY);
    let mut high = Vec3::splat(f32::NEG_INFINITY);
    for corner in 0..8 {
        let p = vec3(
            if corner & 1 == 0 { min.x } else { max.x },
            if corner & 2 == 0 { min.y } else { max.y },
            if corner & 4 == 0 { min.z } else { max.z },
        );
        let q = model.transform_point3(p);
        low = low.min(q);
        high = high.max(q);
    }
    let magnitude = low.abs().max(high.abs()).max(min.abs()).max(max.abs())
        .max_element().max(1.);
    let margin = magnitude * 32. * f32::EPSILON;
    (low - Vec3::splat(margin), high + Vec3::splat(margin))
}
/// `alpha_model_pvs`: a fresh cache for a renderer, and therefore for a new map.
fn model_pvs_cache() -> Vec<ModelPvs> { Vec::new() }
/// `alpha_model_pvs`: a model group whose placed box the BSP descent proves cannot reach any
/// cluster the camera's PVS row shows is not submitted. Unknown data always answers visible,
/// so the cut can only remove groups that `bounds_visible` proves hidden.
fn model_bounds_hidden(cache: &mut Vec<ModelPvs>, visibility: &crate::visibility::Visibility,
    planes: &[crate::bsp::Plane], cluster: Option<usize>, id: usize, model: Mat4, min: Vec3, max: Vec3) -> bool {
    // Host sabotage: hide every eligible model group, so the pixel oracle must report a wrong cut.
    #[cfg(all(feature = "sim_maps_probe", not(target_os = "vita")))]
    if crate::sim_maps::probe::negative_control() == Some("alpha-model-pvs") { return true; }
    let key = cluster.map_or(u32::MAX, |c| c as u32);
    let bits = model.to_cols_array().map(f32::to_bits);
    if let Some(entry) = cache.iter().find(|e| e.group == id as u32) {
        if entry.cluster == key && entry.model == bits { return entry.hidden; }
    }
    let (low, high) = placed_bounds(model, min, max);
    let hidden = visibility.bounds_visible(cluster, planes, low, high) == Some(false);
    let entry = ModelPvs { group: id as u32, cluster: key, model: bits, hidden };
    match cache.iter_mut().find(|e| e.group == id as u32) {
        Some(slot) => *slot = entry,
        None => cache.push(entry),
    }
    hidden
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
                b if b >= WORLD_BLEND_CLASSES => exact_fog_mode(s.blend),
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
    if let (None, Some(Deform::WaveNormal(spread, direction, w))) = (&recipe.deform, &recipe.wave_normal) {
        u.deform[0] = 4.;
        u.deform[1] = *spread;
        u.move_dir = direction.extend(0.).to_array();
        u.wave = w.values;
        u.deform[3] = wave_kind(&w.kind);
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
    fn shader_cache_namespace_preserves_defaults_and_rejects_paths() {
        let parsed = parse_runtime_config(include_bytes!("../../configs/PL-rc13-release-play04.json")).unwrap().0;
        assert!(parsed.shader_cache_namespace.is_empty());
        for name in ["np-old-a2-04", "np_new_gvillage"] {
            assert!(Config { shader_cache_namespace:name.into(), ..Config::default() }.validate().is_ok());
        }
        for name in ["../old", "a/b", "ux0:foo", ".", "a b", &"a".repeat(65)] {
            assert!(Config { shader_cache_namespace:name.into(), ..Config::default() }.validate().is_err());
        }
    }
    #[test]
    fn release_fallback_renderer_missing_only_and_read_only() {
        use crate::config_input::{MissingFile, TestDir};
        let dir = TestDir::new("renderer");
        let missing = dir.0.join("absent-parent/vita_renderer.json");
        let value = |config: Config| serde_json::to_value(config).unwrap();
        let release = include_bytes!("../../configs/PL-release.json");
        let expected = value(parse_runtime_config(release).unwrap().0);
        assert_eq!(value(read_runtime_config(&missing, MissingFile::CompiledRelease).unwrap()), expected);
        assert_eq!(value(read_runtime_config(&missing, MissingFile::OriginalDefaults).unwrap()), value(Config::default()));
        #[cfg(not(target_os = "vita"))]
        assert_eq!(value(read_runtime_config(&missing, MissingFile::for_platform()).unwrap()), value(Config::default()));
        assert!(!missing.parent().unwrap().exists(), "the fallback must not create private directories or files");

        let supplied = dir.0.join("supplied.json");
        for bytes in [b"{}".as_slice(), br#"{"world_list_submit":false,"audio_music_thread":false,"water_normals":true,"log_frame_time_every":10}"#, release] {
            std::fs::write(&supplied, bytes).unwrap();
            let expected = value(parse_runtime_config(bytes).unwrap().0);
            for policy in [MissingFile::OriginalDefaults, MissingFile::CompiledRelease] {
                assert_eq!(value(read_runtime_config(&supplied, policy).unwrap()), expected);
                assert_eq!(std::fs::read(&supplied).unwrap(), bytes);
            }
        }
        // Existing cadence salvage uses the original serde defaults, even on Vita.
        let bad_cadence = br#"{"log_frame_time_every":"bad","world_list_submit":true}"#;
        std::fs::write(&supplied, bad_cadence).unwrap();
        let recovered = read_runtime_config(&supplied, MissingFile::CompiledRelease).unwrap();
        assert_eq!(recovered.log_frame_time_every, Config::default().log_frame_time_every);
        assert!(recovered.world_list_submit);
        assert_eq!(std::fs::read(&supplied).unwrap(), bad_cadence);
        for bytes in [b"{".as_slice(), br#"{"unknown_release_key":true}"#, br#"{"render_scale":0}"#] {
            std::fs::write(&supplied, bytes).unwrap();
            for policy in [MissingFile::OriginalDefaults, MissingFile::CompiledRelease] {
                assert!(read_runtime_config(&supplied, policy).is_err());
                assert_eq!(value(read_runtime_config(&supplied, policy).unwrap_or_default()), value(Config::default()));
                assert_eq!(std::fs::read(&supplied).unwrap(), bytes);
            }
        }
        for policy in [MissingFile::OriginalDefaults, MissingFile::CompiledRelease] {
            assert!(read_runtime_config(&dir.0, policy).is_err(), "non-NotFound I/O errors must propagate");
        }
    }
    #[test]
    fn release_fallback_renderer_is_explicit_played_baseline_without_recording() {
        use sha2::{Digest, Sha256};
        let played_bytes = include_bytes!("../../configs/PL-played-rc3.json");
        assert_eq!(format!("{:x}", Sha256::digest(played_bytes)), "def605c0bd92a38281cb86fd9c20e36a794946999569e3a1f4131e1d0bba7c33");
        let bytes = include_bytes!("../../configs/PL-release.json");
        let raw: serde_json::Value = serde_json::from_slice(bytes).unwrap();
        let defaults = serde_json::to_value(Config::default()).unwrap();
        assert!(!parse_runtime_config(bytes).unwrap().0.sim_sweep_any);
        assert_eq!(historical_keys(&raw), historical_keys(&defaults), "every original release input remains explicit");
        let (parsed, rejected) = parse_runtime_config(bytes).unwrap();
        assert!(rejected.is_none());
        let played_raw: serde_json::Value = serde_json::from_slice(played_bytes).unwrap();
        for (key, value) in played_raw.as_object().unwrap() {
            let expected = match key.as_str() {
                "log_frame_time_every" => serde_json::json!(0),
                "load_texture_share" => serde_json::json!(true),
                _ => value.clone(),
            };
            assert_eq!(raw[key], expected, "{key}");
        }
        let mut played = serde_json::to_value(parse_runtime_config(played_bytes).unwrap().0).unwrap();
        assert_eq!(played["log_frame_time_every"], 10);
        played["log_frame_time_every"] = 0.into();
        // RC8, RC9 and RC10 deliberately enable the defaults measured on 2026-11-14 and 2026-11-15;
        // RC12 enables the shared upload. RC13 keeps the model alpha PVS cull off. RC15 turns
        // the three potears1 levers on and the periodic audio statistics off (the release
        // fallback is what a fresh install without a private config runs).
        for key in ["sim_flight_memo", "ant_corpse_triangle_cull", "ant_corpse_opaque_shader", "water_draw_join",
                    "actor_consumer_frustum_cull", "fx_unlit_exact", "load_texture_share",
                    "sim_snark_move_skip", "sim_snark_sight_blocked", "sim_pool_name_match",
                    "audio_loop_resident", "audio_cue_prefetch", "actor_discard_free_shader",
                    "actor_cutout_discard_free"] { played[key] = true.into(); }
        played["audio_stats_log"] = false.into();
        // Compare typed values on both sides: to_value widens f32 (e.g. the
        // played audio gain 0.001), unlike its original JSON decimal spelling.
        assert_eq!(serde_json::to_value(parsed).unwrap(), played, "only the frame-report shutdown, the seven measured defaults, the four RC15 release keys and the RC17 ant key may change parsed played values");
        let added = raw.as_object().unwrap().iter().filter(|(key, _)| played_raw.get(*key).is_none()).map(|(key, value)| (key.clone(), value.clone())).collect::<serde_json::Map<_, _>>();
        assert_eq!(serde_json::Value::Object(added), serde_json::json!({
            "world_adjacent_batch":false,"exact_blends":false,"sky_cube_finish":false,
            "fog_sky":false,"sky_cube_size":256,"low_fog":false,"actor_near_cull":0.0,
            "sky_cube_clear_only":false,"sim_sweep_blocked":false,"sky_cube_retain":false,
            "spike_log_ms":0.0,"detail_layers":false,"sky_depth":false,
            "sky_cube_capture_only":false,"sky_cube_refresh":0,"audio_combat_preload":false,"sky_cube":false,
            "sim_flight_memo":true,"ant_corpse_triangle_cull":true,"ant_corpse_opaque_shader":true,
            "water_draw_join":true,"actor_consumer_frustum_cull":true,"fx_unlit_exact":true,
            "alpha_model_pvs":false,"sim_snark_move_skip":true,"sim_snark_sight_blocked":true,
            "sim_pool_name_match":true,"audio_stats_log":false,
            "audio_loop_resident":true,"audio_cue_prefetch":true,
            "actor_discard_free_shader":true,"actor_cutout_discard_free":true,"actor_frontface_cull":false
        }));
        assert_eq!(raw["water_draw_join"], true, "RC9 enables the measured water draw join");
        assert_eq!(raw["actor_consumer_frustum_cull"], true, "RC10 enables the measured consumer cull");
        assert_eq!(raw["fx_unlit_exact"], true, "RC10 enables the measured unlit exact FX path");
        assert_eq!(raw["world_shared_buffers"], true);
        assert_eq!(raw["world_parameter_reuse"], true);
        assert_eq!(raw["water_stage_merge"], "off");
        for key in ["render_draw_timing", "worker_probe", "light_gl_observers", "skin_worker_bench", "frame_phase_probe", "phase_census", "profile_capture", "gpu_state_probe", "depth_sentinel", "pvs_verify", "world_pass_clock", "water_normals", "world_adjacent_batch"] {
            assert_eq!(raw[key], false, "{key}");
        }
        assert_eq!(raw["profiler"], "off");
        assert_eq!(raw["gpu_timing"], "off");
        assert_eq!(raw["gpu_mask"], serde_json::json!([]));
        assert_eq!(raw["spike_log_ms"], 0.0);
        for key in ["replay_record", "replay_play", "replay_start"] { assert_eq!(raw[key], "", "{key}"); }
        for key in ["replay_state_every", "replay_stop_after"] { assert_eq!(raw[key], 0, "{key}"); }
        // This chooses the proven asynchronous reporting route; it is not a log enable.
        assert_eq!(raw["audio_log_thread"], true);
        assert_eq!(raw["audio_music_thread"], true);
        assert_eq!(raw["world_list_submit"], true);
        assert_eq!(raw["world_shared_buffers"], true);
        assert_eq!(raw["world_gxm"], "live");
        assert_eq!(raw["tls_fast"], "on");
    }
    #[test]
    fn round5_keys_are_strict_and_off_in_historical_configs() {
        let defaults=serde_json::to_value(Config::default()).unwrap();
        for (key,value) in defaults.as_object().unwrap().iter().filter(|(k,_)| round5_key(k)) {
            if key.starts_with("diag_skip_") || key.starts_with("ant_corpse_") || key == "sim_flight_memo"
                || key == "actor_cutout_discard_free" || key == "actor_frontface_cull" || key == "actor_discard_free_shader" || key == "actor_opaque_shader" || key == "actor_consumer_frustum_cull" {
                assert_eq!(value, false);
                assert!(serde_json::from_value::<Config>(serde_json::json!({key:true})).is_ok());
                assert!(serde_json::from_value::<Config>(serde_json::json!({key:1})).is_err());
            } else { assert_eq!(value, 0); }
        }
        assert!(serde_json::from_value::<Config>(serde_json::json!({"spike_log_first_frame":20,"spike_log_last_frame":10})).unwrap().validate().is_err());
    }
    #[test]
    fn sweep_any_is_default_off_and_strict() {
        assert!(!Config::default().sim_sweep_any);
        assert!(!serde_json::from_str::<Config>("{}").unwrap().sim_sweep_any);
        assert!(serde_json::from_str::<Config>(r#"{"sim_sweep_any":true}"#).unwrap().sim_sweep_any);
        assert!(serde_json::from_str::<Config>(r#"{"sim_sweep_any":1}"#).is_err());
    }
    // The three potears1 arms are exact work removals, not visual choices: every one
    // ships off, accepts only a boolean, and survives a round trip with the others off.
    #[test]
    fn potears1_arms_ship_off_and_are_strict_booleans() {
        const FIELDS: [&str; 3] = ["sim_snark_move_skip", "sim_snark_sight_blocked",
                                   "sim_pool_name_match"];
        for field in FIELDS {
            let defaults = serde_json::to_value(Config::default()).unwrap();
            assert_eq!(defaults[field], false, "{field}");
            let empty = serde_json::to_value(serde_json::from_str::<Config>("{}").unwrap()).unwrap();
            assert_eq!(empty[field], false, "{field}");
            for invalid in [serde_json::json!(1), serde_json::json!("on"), serde_json::Value::Null] {
                assert!(serde_json::from_value::<Config>(serde_json::json!({field: invalid})).is_err());
            }
            for enabled in [false, true] {
                let config: Config = serde_json::from_value(
                    serde_json::json!({field: enabled})).unwrap();
                config.validate().unwrap();
                let actual = serde_json::to_value(config).unwrap();
                for other in FIELDS {
                    assert_eq!(actual[other], if other == field { enabled } else { false });
                }
            }
        }
    }
    // RC15 is RC14's shipped play config with the three potears1 switches on and the
    // periodic audio statistics off, and nothing else changed. The levers stay off in
    // the code defaults and in every earlier config.
    #[test]
    fn rc15_release_play_is_rc14_plus_the_levers_and_release_logging_off() {
        const LEVERS: [&str; 3] = ["sim_snark_move_skip", "sim_snark_sight_blocked",
                                   "sim_pool_name_match"];
        let rc14 = parse_runtime_config(include_bytes!("../../configs/PL-rc13-release-play04.json"))
            .unwrap().0;
        let rc15 = parse_runtime_config(include_bytes!("../../configs/PL-rc15-release-play.json"))
            .unwrap().0;
        rc15.validate().unwrap();
        let mut expected = serde_json::to_value(rc14).unwrap();
        for lever in LEVERS {
            assert_eq!(expected[lever], false, "RC14 ships {lever} off");
            expected[lever] = true.into();
        }
        assert_eq!(expected["audio_stats_log"], true,
                   "RC14 keeps the one-second audio statistics on");
        expected["audio_stats_log"] = false.into();
        assert_eq!(serde_json::to_value(rc15).unwrap(), expected,
                   "RC15 is RC14 with the three potears1 levers on and periodic audio stats off");
    }
    // RC16 is the RC15 shipped play config with the three device-measured fight fixes on
    // and the picture-changing front-face cull explicitly off. Nothing else changed.
    #[test]
    fn rc16_release_play_is_rc15_plus_the_three_exact_fixes() {
        let rc15 = parse_runtime_config(include_bytes!("../../configs/PL-rc15-release-play.json"))
            .unwrap().0;
        let rc16 = parse_runtime_config(include_bytes!("../../configs/PL-rc16-release-play.json"))
            .unwrap().0;
        rc16.validate().unwrap();
        let mut expected = serde_json::to_value(rc15).unwrap();
        assert_eq!(expected["audio_loop_resident"], false, "RC15 ships the loop cache off");
        assert_eq!(expected["audio_cue_prefetch"], false, "RC15 ships the cue prefetch off");
        assert_eq!(expected["actor_discard_free_shader"], false, "RC15 ships the actor shader off");
        assert_eq!(expected["actor_frontface_cull"], false, "RC15 ships the front-face cull off");
        for key in ["audio_loop_resident", "audio_cue_prefetch", "actor_discard_free_shader"] {
            expected[key] = true.into();
        }
        expected["actor_frontface_cull"] = false.into();
        assert_eq!(serde_json::to_value(rc16).unwrap(), expected,
                   "RC16 is RC15 with the three exact fight fixes and the cull explicitly off");
    }
    #[test]
    fn rc17_play_config_is_the_release_fallback_plus_the_measurement_keys() {
        let release = parse_runtime_config(include_bytes!("../../configs/PL-release.json"))
            .unwrap().0;
        let play = parse_runtime_config(include_bytes!("../../configs/PL-rc17-release-play.json"))
            .unwrap().0;
        play.validate().unwrap();
        let release_value = serde_json::to_value(release).unwrap();
        let play_value = serde_json::to_value(play).unwrap();
        for (key, expected) in [("sim_snark_move_skip", true),
                                ("sim_snark_sight_blocked", true),
                                ("sim_pool_name_match", true),
                                ("audio_stats_log", false),
                                ("audio_loop_resident", true),
                                ("audio_cue_prefetch", true),
                                ("actor_discard_free_shader", true),
                                ("actor_cutout_discard_free", true),
                                ("actor_frontface_cull", false)] {
            assert_eq!(release_value[key], expected,
                       "a fresh install runs the compiled fallback, which must ship {key}={expected}");
            assert_eq!(play_value[key], expected, "{key} in the play config");
        }
        let mut expected = release_value;
        expected["replay_play"] = "a2-04.lgtape".into();
        expected["log_frame_time_every"] = 10.into();
        assert_eq!(play_value, expected,
                   "the RC17 play config is the release fallback plus only the two measurement keys");
    }
    // RC17 is RC16 with exactly the ant cutout discard-free key turned on: same shipped
    // value for every other key, including the explicit front-face cull off.
    #[test]
    fn rc17_release_play_is_rc16_plus_the_exact_ant_key() {
        let rc16 = parse_runtime_config(include_bytes!("../../configs/PL-rc16-release-play.json"))
            .unwrap().0;
        let rc17 = parse_runtime_config(include_bytes!("../../configs/PL-rc17-release-play.json"))
            .unwrap().0;
        rc17.validate().unwrap();
        let mut expected = serde_json::to_value(rc16).unwrap();
        assert_eq!(expected["actor_cutout_discard_free"], false,
                   "RC16 predates the ant key and reads it off");
        assert_eq!(expected["actor_frontface_cull"], false, "RC16 ships the cull off");
        expected["actor_cutout_discard_free"] = true.into();
        assert_eq!(serde_json::to_value(rc17).unwrap(), expected,
                   "RC17 changes only actor_cutout_discard_free against RC16");
    }
    // The picture-changing cull is off in every shipped input; the four exact fixes
    // are on in both RC17 inputs. RC16 keeps its own frozen polarity, and every
    // earlier config keeps all five off/absent.
    #[test]
    fn actor_frontface_cull_is_false_in_every_shipped_config() {
        const KEYS: [&str; 5] = ["audio_loop_resident", "audio_cue_prefetch",
                                 "actor_discard_free_shader", "actor_cutout_discard_free",
                                 "actor_frontface_cull"];
        for bytes in [&include_bytes!("../../configs/PL-release.json")[..],
                      &include_bytes!("../../configs/PL-rc17-release-play.json")[..]] {
            let (c, rejected) = parse_runtime_config(bytes).unwrap();
            assert!(rejected.is_none());
            for key in KEYS {
                let value = serde_json::to_value(&c).unwrap()[key].clone();
                let want = key != "actor_frontface_cull";
                assert_eq!(value, want, "a shipped RC17 input has the wrong {key} polarity");
            }
        }
        let (rc16, rejected) =
            parse_runtime_config(include_bytes!("../../configs/PL-rc16-release-play.json")).unwrap();
        assert!(rejected.is_none());
        let rc16 = serde_json::to_value(rc16).unwrap();
        for key in ["audio_loop_resident", "audio_cue_prefetch", "actor_discard_free_shader"] {
            assert_eq!(rc16[key], true, "the frozen RC16 input keeps {key} on");
        }
        assert_eq!(rc16["actor_cutout_discard_free"], false,
                   "the frozen RC16 input predates the ant key");
        assert_eq!(rc16["actor_frontface_cull"], false);
        for bytes in [&include_bytes!("../../configs/PL-rc15-release-play.json")[..],
                      &include_bytes!("../../configs/PL-rc13-release-play04.json")[..],
                      &include_bytes!("../../configs/PL-rc1.json")[..]] {
            let (c, rejected) = parse_runtime_config(bytes).unwrap();
            assert!(rejected.is_none());
            for key in KEYS {
                let value = serde_json::to_value(&c).unwrap()[key].clone();
                assert_eq!(value, false, "a pre-RC16 input ships {key}=true");
            }
        }
        assert!(!Config::default().actor_frontface_cull);
        assert!(serde_json::from_str::<Config>(r#"{"actor_frontface_cull":true}"#)
            .unwrap().actor_frontface_cull);
        for invalid in [serde_json::json!(0), serde_json::json!("on"), serde_json::Value::Null] {
            assert!(serde_json::from_value::<Config>(serde_json::json!({"actor_frontface_cull": invalid}))
                .is_err());
        }
    }
    #[test]
    fn audio_stats_log_defaults_on_and_is_strict() {
        assert!(Config::default().audio_stats_log);
        assert!(serde_json::from_str::<Config>("{}").unwrap().audio_stats_log);
        assert!(!serde_json::from_str::<Config>(r#"{"audio_stats_log":false}"#)
            .unwrap().audio_stats_log);
        for invalid in [serde_json::json!(0), serde_json::json!("off"), serde_json::Value::Null] {
            assert!(serde_json::from_value::<Config>(serde_json::json!({"audio_stats_log": invalid}))
                .is_err());
        }
        assert!(parse_runtime_config(include_bytes!("../../configs/PL-rc13-release-play04.json"))
            .unwrap().0.audio_stats_log, "the RC14 config keeps today's behaviour");
        assert!(!parse_runtime_config(include_bytes!("../../configs/PL-rc15-release-play.json"))
            .unwrap().0.audio_stats_log, "the RC15 release config turns it off");
    }
    // Historical console configs must stay usable by their original SELF. Their omitted
    // new opt-in is checked as off; every earlier runtime key remains pinned by these tests.
    #[test]
    fn audio_clip_candidates_default_off_and_are_strict() {
        assert!(!Config::default().audio_loop_resident);
        assert!(!Config::default().audio_cue_prefetch);
        assert!(!serde_json::from_str::<Config>("{}").unwrap().audio_loop_resident);
        assert!(!serde_json::from_str::<Config>("{}").unwrap().audio_cue_prefetch);
        for key in ["audio_loop_resident", "audio_cue_prefetch"] {
            for invalid in [serde_json::json!(0), serde_json::json!("on"), serde_json::Value::Null] {
                assert!(serde_json::from_value::<Config>(serde_json::json!({key: invalid})).is_err());
            }
            let on: Config = serde_json::from_value(serde_json::json!({key: true})).unwrap();
            on.validate().unwrap();
            assert_eq!(serde_json::to_value(on).unwrap()[key], true);
        }
        // Every frozen configuration predates both keys and reads both as off.
        for bytes in [&include_bytes!("../../configs/PL-rc1.json")[..],
                      &include_bytes!("../../configs/PL-rc13-release-play04.json")[..],
                      &include_bytes!("../../configs/PL-rc15-release-play.json")[..]] {
            let (c, rejected) = parse_runtime_config(bytes).unwrap();
            assert!(rejected.is_none());
            assert!(!c.audio_loop_resident && !c.audio_cue_prefetch);
        }
        // RC16 ships both measured audio fixes on in the fallback and the play config.
        for bytes in [&include_bytes!("../../configs/PL-release.json")[..],
                      &include_bytes!("../../configs/PL-rc16-release-play.json")[..]] {
            let (c, rejected) = parse_runtime_config(bytes).unwrap();
            assert!(rejected.is_none());
            assert!(c.audio_loop_resident && c.audio_cue_prefetch);
        }
    }
    // Historical console configs must stay usable by their original SELF. Their omitted
    // new opt-in is checked as off; every earlier runtime key remains pinned by these tests.
    // The two actor keys arrived after every frozen era file, so those files stay valid
    // unchanged and their omitted opt-in must read as an explicit off.
    fn round5_key(k: &str) -> bool { k.starts_with("diag_skip_") || matches!(k, "sim_flight_memo" | "spike_log_first_frame" | "spike_log_last_frame" | "ant_corpse_triangle_cull" | "ant_corpse_opaque_shader" | "actor_cutout_discard_free" | "actor_frontface_cull" | "actor_discard_free_shader" | "actor_opaque_shader" | "actor_consumer_frustum_cull") }
    /// Keys added after every frozen era file. Those files stay valid unchanged; an
    /// omitted opt-in reads as an explicit off, and `audio_stats_log` keeps its
    /// default-on release behaviour until a config turns it off.
    fn later_opt_in_key(k: &str) -> bool { matches!(k, "shader_cache_namespace" | "water_draw_join" | "alpha_model_pvs" | "fx_unlit_exact" | "sim_snark_move_skip" | "sim_snark_sight_blocked" | "sim_pool_name_match" | "audio_stats_log" | "audio_loop_resident" | "audio_cue_prefetch" | "capture_frames" | "capture_tag") }
    fn historical_keys(v: &serde_json::Value) -> Vec<String> {
        assert!(v.get("world_adjacent_batch").is_none_or(|on| on == false));
        assert!(v.get("sim_sweep_any").is_none_or(|on| on == false));
        let mut keys = v.as_object().unwrap().keys().filter(|k| *k != "world_adjacent_batch" && *k != "sim_sweep_any"
            && !later_opt_in_key(k) && !round5_key(k)).cloned().collect::<Vec<_>>();
        keys.sort();
        keys
    }
    // Every config built before RC9 leaves the join off, including the frozen RC8 release.
    #[test]
    fn water_draw_join_is_off_before_rc9_and_on_only_from_the_rc9_release() {
        for bytes in [&include_bytes!("../../configs/PL-rc1.json")[..], &include_bytes!("../../configs/PL-rc4.json")[..],
                      &include_bytes!("../../configs/PL-rc7-release.json")[..], &include_bytes!("../../configs/PL-rc8-release.json")[..]] {
            let raw: serde_json::Value = serde_json::from_slice(bytes).unwrap();
            assert!(raw.get("water_draw_join").is_none_or(|on| on == false), "a pre-RC9 config never joins water draws");
        }
        for bytes in [&include_bytes!("../../configs/PL-rc9-release.json")[..],
                      &include_bytes!("../../configs/PL-release.json")[..]] {
            let raw: serde_json::Value = serde_json::from_slice(bytes).unwrap();
            assert_eq!(raw["water_draw_join"], true);
        }
    }
    // Every config built before RC10 leaves the actor cull off, including the frozen RC9 release.
    #[test]
    fn actor_consumer_frustum_cull_is_off_before_rc10_and_on_only_from_the_rc10_release() {
        for bytes in [&include_bytes!("../../configs/PL-rc1.json")[..],
                      &include_bytes!("../../configs/PL-rc8-release.json")[..],
                      &include_bytes!("../../configs/PL-rc9-release.json")[..],
                      &include_bytes!("../../configs/PL-rc9-release-play04.json")[..]] {
            let (c, rejected) = parse_runtime_config(bytes).unwrap();
            assert!(rejected.is_none());
            assert!(!c.actor_consumer_frustum_cull, "a pre-RC10 config culls actors");
        }
        let (c, rejected) = parse_runtime_config(include_bytes!("../../configs/PL-release.json")).unwrap();
        assert!(rejected.is_none());
        assert!(c.actor_consumer_frustum_cull, "RC10 enables the measured consumer cull");
    }
    #[test]
    fn fx_unlit_exact_is_strict_and_on_only_from_the_rc10_release() {
        assert!(!Config::default().fx_unlit_exact);
        assert!(!serde_json::from_str::<Config>("{}").unwrap().fx_unlit_exact);
        assert!(serde_json::from_str::<Config>(r#"{"fx_unlit_exact":true}"#).unwrap().fx_unlit_exact);
        assert!(serde_json::from_str::<Config>(r#"{"fx_unlit_exact":1}"#).is_err());
        // Every input built before RC10 leaves the opt-in off. The frozen files are
        // not rewritten to add the key.
        for bytes in [&include_bytes!("../../configs/PL-rc9-release-play04.json")[..],
                      &include_bytes!("../../configs/PL-rc8-release.json")[..],
                      &include_bytes!("../../configs/PL-rc1.json")[..]] {
            let (c, rejected) = parse_runtime_config(bytes).unwrap();
            assert!(rejected.is_none());
            assert!(!c.fx_unlit_exact, "a pre-RC10 input enabled fx_unlit_exact");
        }
        let (release, rejected) = parse_runtime_config(include_bytes!("../../configs/PL-release.json")).unwrap();
        assert!(rejected.is_none());
        assert!(release.fx_unlit_exact, "RC10 enables the measured unlit exact path");
        let control_bytes = include_bytes!("../../configs/PL-a5-hatter2-control.json");
        let exact_bytes = include_bytes!("../../configs/PL-a5-hatter2-unlit-exact.json");
        let (control, rejected) = parse_runtime_config(control_bytes).unwrap();
        assert!(rejected.is_none());
        control.validate().unwrap();
        let (exact, rejected) = parse_runtime_config(exact_bytes).unwrap();
        assert!(rejected.is_none());
        exact.validate().unwrap();
        assert!(!control.fx_unlit_exact && exact.fx_unlit_exact);
        let mut off = serde_json::to_value(control).unwrap();
        let mut on = serde_json::to_value(exact).unwrap();
        off["fx_unlit_exact"] = on["fx_unlit_exact"].clone();
        assert_eq!(off, on, "the hatter2 pair changes only fx_unlit_exact");
        assert_eq!(crate::render_fx::retained::specialized_program(true, true),
            Some(crate::render_fx::retained::Specialized::UnlitExact),
            "fx_unlit_exact takes precedence over fx_lite");
    }
    #[test]
    fn alpha_model_pvs_is_default_off_and_stays_off_in_the_release_after_the_hatter2_finding() {
        assert!(!Config::default().alpha_model_pvs);
        assert!(!serde_json::from_str::<Config>("{}").unwrap().alpha_model_pvs);
        assert!(serde_json::from_str::<Config>(r#"{"alpha_model_pvs":true}"#).unwrap().alpha_model_pvs);
        for invalid in [serde_json::json!(1), serde_json::json!("on"), serde_json::Value::Null] {
            assert!(serde_json::from_value::<Config>(serde_json::json!({"alpha_model_pvs": invalid})).is_err());
        }
        Config::default().validate().unwrap();
        for bytes in [&include_bytes!("../../configs/PL-release.json")[..],
                      &include_bytes!("../../configs/PL-rc12-release.json")[..],
                      &include_bytes!("../../configs/PL-rc9-release-play04.json")[..]] {
            let raw: serde_json::Value = serde_json::from_slice(bytes).unwrap();
            assert!(raw.get("alpha_model_pvs").is_none_or(|on| on == false), "a release input keeps the cut off");
            assert!(!serde_json::from_slice::<Config>(bytes).unwrap().alpha_model_pvs);
        }
        let control = include_bytes!("../../configs/PL-a5-wchess1-control.json");
        let on = include_bytes!("../../configs/PL-a5-wchess1-alpha-model-pvs.json");
        let (control_parsed, rejected) = parse_runtime_config(control).unwrap();
        assert!(rejected.is_none());
        control_parsed.validate().unwrap();
        let (on_parsed, rejected) = parse_runtime_config(on).unwrap();
        assert!(rejected.is_none());
        on_parsed.validate().unwrap();
        assert!(!control_parsed.alpha_model_pvs && on_parsed.alpha_model_pvs);
        assert_eq!(control_parsed.replay_play, "a4-wchess1-01.lgtape");
        for (config, expected) in [(&control_parsed, false), (&on_parsed, true)] {
            assert_eq!(config.replay_start, "chapter:wchess1");
            assert_eq!(config.replay_stop_after, 1200);
            assert_eq!(config.replay_check_every, 600);
            assert_eq!(config.log_frame_time_every, Some(10));
            assert_eq!(config.alpha_model_pvs, expected);
        }
        let mut a = serde_json::to_value(control_parsed).unwrap();
        let mut b = serde_json::to_value(on_parsed).unwrap();
        a.as_object_mut().unwrap().remove("alpha_model_pvs");
        b.as_object_mut().unwrap().remove("alpha_model_pvs");
        assert_eq!(a, b, "the wchess1 pair differs only in alpha_model_pvs");
    }
    #[test]
    fn alpha_model_pvs_bounds_enclose_every_placed_corner_with_an_outward_margin() {
        let min = vec3(-1., -2., -3.);
        let max = vec3(4., 5., 6.);
        let (low, high) = placed_bounds(Mat4::IDENTITY, min, max);
        assert!(low.cmplt(min).all() && high.cmpgt(max).all(), "the margin must point outward");
        let model = Mat4::from_rotation_translation(Quat::from_rotation_z(0.5), vec3(10., -4., 2.));
        let (low, high) = placed_bounds(model, min, max);
        for corner in 0..8 {
            let p = vec3(if corner & 1 == 0 { min.x } else { max.x },
                         if corner & 2 == 0 { min.y } else { max.y },
                         if corner & 4 == 0 { min.z } else { max.z });
            let q = model.transform_point3(p);
            assert!(q.cmpge(low).all() && q.cmple(high).all(), "corner {corner}");
        }
        let inner = Mat4::from_translation(vec3(100., 0., 0.));
        let (low, high) = placed_bounds(inner, min, max);
        assert!(low.cmplt(min + Vec3::X * 100.).all() && high.cmpgt(max + Vec3::X * 100.).all());
        // A rotated thin box at the largest corpus coordinate stays strictly inside the
        // inflated result; the f64 corner is the exact placement of the same f32 inputs.
        let extent = 8192.;
        let far = Mat4::from_rotation_translation(
            Quat::from_axis_angle(vec3(0.3, 0.8, 0.5).normalize(), 0.7),
            vec3(extent, -extent, extent));
        let (thin_min, thin_max) = (vec3(-0.02, -0.02, -0.02), vec3(0.02, 0.02, 0.02));
        let (low, high) = placed_bounds(far, thin_min, thin_max);
        let m = far.to_cols_array().map(f64::from);
        for corner in 0..8 {
            let p = [
                if corner & 1 == 0 { thin_min.x } else { thin_max.x },
                if corner & 2 == 0 { thin_min.y } else { thin_max.y },
                if corner & 4 == 0 { thin_min.z } else { thin_max.z },
            ].map(f64::from);
            let exact = [
                m[0] * p[0] + m[4] * p[1] + m[8] * p[2] + m[12],
                m[1] * p[0] + m[5] * p[1] + m[9] * p[2] + m[13],
                m[2] * p[0] + m[6] * p[1] + m[10] * p[2] + m[14],
            ];
            assert!(exact[0] > low.x as f64 && exact[0] < high.x as f64
                && exact[1] > low.y as f64 && exact[1] < high.y as f64
                && exact[2] > low.z as f64 && exact[2] < high.z as f64, "corner {corner}");
        }
    }
    #[test]
    fn alpha_model_pvs_cache_handles_cluster_matrix_round_trips_and_a_fresh_map() {
        let v = crate::visibility::tests::two_rooms();
        let planes = [crate::bsp::Plane { normal: Vec3::X, distance: 0. }];
        let (min, max) = (vec3(2., -1., -1.), vec3(3., 1., 1.));
        let mut cache = model_pvs_cache();
        assert!(cache.is_empty());
        // A -> B -> A for the camera cluster, then None -> B: each change recomputes.
        assert!(!model_bounds_hidden(&mut cache, &v, &planes, Some(0), 7, Mat4::IDENTITY, min, max));
        assert!(model_bounds_hidden(&mut cache, &v, &planes, Some(1), 7, Mat4::IDENTITY, min, max));
        assert!(!model_bounds_hidden(&mut cache, &v, &planes, Some(0), 7, Mat4::IDENTITY, min, max));
        assert!(!model_bounds_hidden(&mut cache, &v, &planes, None, 7, Mat4::IDENTITY, min, max));
        assert!(model_bounds_hidden(&mut cache, &v, &planes, Some(1), 7, Mat4::IDENTITY, min, max));
        // A changed matrix moves the group into the other room, then back.
        let moved = Mat4::from_translation(vec3(-5., 0., 0.));
        assert!(!model_bounds_hidden(&mut cache, &v, &planes, Some(1), 7, moved, min, max));
        assert!(model_bounds_hidden(&mut cache, &v, &planes, Some(1), 7, Mat4::IDENTITY, min, max));
        assert_eq!(cache.len(), 1);
        assert_eq!((cache[0].group, cache[0].cluster, cache[0].hidden), (7, 1, true));
        // A new renderer for a new map starts with its own empty cache; the identical
        // group, cluster and matrix key must not reuse map A's hidden answer.
        let mut fresh = model_pvs_cache();
        assert!(fresh.is_empty());
        let other = crate::visibility::tests::two_rooms_swapped();
        assert!(!model_bounds_hidden(&mut fresh, &other, &planes, Some(1), 7, Mat4::IDENTITY, min, max));
        assert_eq!(fresh.len(), 1);
    }
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
    fn adjacent_batch_requires_the_shipped_list_and_shared_geometry() {
        assert!(!Config::default().world_adjacent_batch);
        for list in [false, true] { for buffers in [false, true] {
            let c = Config { world_adjacent_batch: true, world_list_submit: list,
                world_shared_buffers: buffers, world_gxm: "live".into(), ..Config::default() };
            assert_eq!(c.validate().is_ok(), list && buffers);
        } }
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
    fn a6_load_pair_differs_only_in_the_four_load_keys() {
        let (release, rejected) = parse_runtime_config(include_bytes!("../../configs/PL-release.json")).unwrap();
        assert!(rejected.is_none());
        assert!(release.load_texture_share, "RC12 enables the shared upload in the release input");
        let off_bytes = include_bytes!("../../configs/PL-a6-load-off.json");
        let on_bytes = include_bytes!("../../configs/PL-a6-load-on.json");
        let (off, rejected) = parse_runtime_config(off_bytes).unwrap();
        assert!(rejected.is_none());
        off.validate().unwrap();
        let (on, rejected) = parse_runtime_config(on_bytes).unwrap();
        assert!(rejected.is_none());
        on.validate().unwrap();
        for bytes in [&off_bytes[..], &on_bytes[..]] {
            let written: serde_json::Value = serde_json::from_slice(bytes).unwrap();
            let all = serde_json::to_value(Config::default()).unwrap();
            assert_eq!(historical_keys(&written), historical_keys(&all), "the A6 config writes every runtime key");
        }
        assert!(!off.loading_screen && !off.load_texture_share && off.texture_cache == "off" && !off.pvs_cache);
        assert!(on.loading_screen && on.load_texture_share && on.texture_cache == "on" && on.pvs_cache);
        let (mut off, mut on) = (serde_json::to_value(&off).unwrap(), serde_json::to_value(&on).unwrap());
        for key in ["loading_screen", "load_texture_share", "texture_cache", "pvs_cache"] {
            off.as_object_mut().unwrap().remove(key);
            on.as_object_mut().unwrap().remove(key);
        }
        assert_eq!(on, off, "the A6 pair differs only in the four load keys");
    }
    #[test]
    fn load_texture_share_is_off_by_default_and_a_strict_boolean() {
        assert!(!Config::default().load_texture_share);
        assert!(!serde_json::from_str::<Config>("{}").unwrap().load_texture_share);
        assert!(serde_json::from_str::<Config>(r#"{"load_texture_share":true}"#).unwrap().load_texture_share);
        assert!(serde_json::from_str::<Config>(r#"{"load_texture_share":"on"}"#).is_err());
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
        let keys = historical_keys;
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
    fn rc4_baseline_and_isolated_variants_write_every_key_without_optional_activation() {
        let parse = |bytes: &[u8]| {
            let raw: serde_json::Value = serde_json::from_slice(bytes).unwrap();
            let defaults = serde_json::to_value(Config::default()).unwrap();
            assert!(!parse_runtime_config(bytes).unwrap().0.sim_sweep_any);
            let keys = |v: &serde_json::Value| v.as_object().unwrap().keys()
                .filter(|k| *k != "sim_sweep_any" && !later_opt_in_key(k) && !round5_key(k)).cloned().collect::<Vec<_>>();
            assert_eq!(keys(&raw), keys(&defaults));
            let (c, rejected) = parse_runtime_config(bytes).unwrap();
            assert!(rejected.is_none()); c.validate().unwrap();
            // Preserve authored decimal spelling (e.g. 0.001) while comparing the
            // actual typed runtime value after the parser's f32 conversion.
            serde_json::to_value(c).unwrap()
        };
        let baseline = parse(include_bytes!("../../configs/PL-rc4.json"));
        let (rc3, rejected) = parse_runtime_config(include_bytes!("../../configs/PL-rc3.json")).unwrap();
        assert!(rejected.is_none());
        assert_eq!(baseline, serde_json::to_value(rc3).unwrap(), "RC4 baseline preserves played RC3 plus later off defaults");
        for key in ["low_fog", "fog_sky", "exact_blends", "detail_layers", "water_normals", "world_alpha_lite", "fx_lite",
                    "authored_sky", "sky_cube", "sky_depth", "sky_cube_finish", "sky_cube_retain", "sky_cube_capture_only", "sky_cube_clear_only",
                    "audio_combat_preload", "sim_sweep_blocked", "world_adjacent_batch"] { assert_eq!(baseline[key], false, "{key}"); }
        for key in ["actor_near_cull", "spike_log_ms"] { assert_eq!(baseline[key], 0.); }
        assert_eq!(baseline["sky_cube_refresh"], 0);
        assert_eq!(baseline["water_stage_merge"], "off");
        assert_eq!(baseline["scaled_target"], "direct");
        for key in ["sky", "packet_frustum_cull", "prop_frustum_cull", "fx_add_zero_alpha_cut", "world_list_submit", "world_shared_buffers"] {
            assert_eq!(baseline[key], true, "proven RC3 key {key} remains enabled");
        }
        let variants: &[(&[u8], &[(&str, serde_json::Value)])] = &[
            (include_bytes!("../../configs/PL-rc4-fog.json"), &[("low_fog", true.into()), ("fog_sky", true.into())]),
            (include_bytes!("../../configs/PL-rc4-exact-fog.json"), &[("low_fog", true.into()), ("fog_sky", true.into()), ("exact_blends", true.into())]),
            (include_bytes!("../../configs/PL-rc4-water.json"), &[("water_normals", true.into())]),
            (include_bytes!("../../configs/PL-rc4-fill.json"), &[("water_stage_merge", "merge".into()), ("world_alpha_lite", true.into()), ("fx_lite", true.into())]),
            (include_bytes!("../../configs/PL-rc4-sky-finish-fog.json"), &[("low_fog", true.into()), ("fog_sky", true.into()), ("sky_cube", true.into()), ("sky_cube_finish", true.into())]),
            (include_bytes!("../../configs/PL-rc4-river.json"), &[("world_adjacent_batch", true.into())]),
            (include_bytes!("../../configs/PL-rc4-record.json"), &[("replay_record", "a2-#.lgtape".into()), ("replay_start", "chapter:potears2".into())]),
            (include_bytes!("../../configs/PL-rc4-replay.json"), &[("replay_play", "a2-04.lgtape".into())]),
        ];
        for &(bytes, changes) in variants {
            let mut variant = parse(bytes);
            for (key, expected) in changes { assert_eq!(&variant[*key], expected, "{key}"); variant[*key] = baseline[*key].clone(); }
            assert_eq!(variant, baseline, "an isolated RC4 variant changes only its declared keys");
        }
    }
    #[test]
    fn release_inputs_add_only_their_declared_keys_and_packages_add_their_replay_keys() {
        use sha2::{Digest, Sha256};
        let release_bytes = include_bytes!("../../configs/PL-release.json");
        let rc12_bytes = include_bytes!("../../configs/PL-rc12-release.json");
        let rc11_bytes = include_bytes!("../../configs/PL-rc11-release.json");
        let rc10_bytes = include_bytes!("../../configs/PL-rc10-release.json");
        let rc9_bytes = include_bytes!("../../configs/PL-rc9-release.json");
        let rc8_bytes = include_bytes!("../../configs/PL-rc8-release.json");
        let rc7_bytes = include_bytes!("../../configs/PL-rc7-release.json");
        let sim_bytes = include_bytes!("../../configs/vita-sim/vita-sim-release.json");
        assert_eq!(format!("{:x}", Sha256::digest(release_bytes)), "9464c92d98893879eff802225e16eae9cd29f3be64820860014b84d0dd8e64f8");
        assert_eq!(format!("{:x}", Sha256::digest(rc12_bytes)), "4b1a1503b00b7826258a54fff0e69e5bdbd3a609f440bc13aaa2926aae9cb9f5");
        assert_eq!(format!("{:x}", Sha256::digest(rc11_bytes)), "00d1de6a0e8a692c4a32b7a5289e2990dcab91e422b70d76925bc8657fa6a24a");
        assert_eq!(format!("{:x}", Sha256::digest(rc10_bytes)), "78547291fe3455184564a2437addec154ba8d056d4cd4dc4e4cead80f86624c6");
        assert_eq!(format!("{:x}", Sha256::digest(rc9_bytes)), "9dbf482cf40ff440dd74ecc5c6be45aaf5c3cb0b06da3bfc0fe47a4b76466cac");
        assert_eq!(format!("{:x}", Sha256::digest(rc8_bytes)), "a5d21beec0d15648cc94f209bbe0adf1fa387430c0df18a1a5e10ad2756f6f2b");
        assert_eq!(format!("{:x}", Sha256::digest(rc7_bytes)), "593778fbe24338cbb2c8d00a8504286d5c26018dbc2bc663500221095bfd2b80");
        assert_eq!(format!("{:x}", Sha256::digest(sim_bytes)), "fa64b0c5b79c52d8e028f7fc9d0d3dd2eec57b7641dfc8f76b34212bdcb3334b");
        let (release, rejected) = parse_runtime_config(release_bytes).unwrap();
        assert!(rejected.is_none());
        release.validate().unwrap();
        let baseline = serde_json::to_value(release).unwrap();
        let (rc12_release, rejected) = parse_runtime_config(rc12_bytes).unwrap();
        assert!(rejected.is_none());
        let rc12 = serde_json::to_value(rc12_release).unwrap();
        let (rc11, rejected) = parse_runtime_config(rc11_bytes).unwrap();
        assert!(rejected.is_none());
        let rc11 = serde_json::to_value(rc11).unwrap();
        let (rc10, rejected) = parse_runtime_config(rc10_bytes).unwrap();
        assert!(rejected.is_none());
        let rc10 = serde_json::to_value(rc10).unwrap();
        let (rc9, rejected) = parse_runtime_config(rc9_bytes).unwrap();
        assert!(rejected.is_none());
        let rc9 = serde_json::to_value(rc9).unwrap();
        let (rc8, rejected) = parse_runtime_config(rc8_bytes).unwrap();
        assert!(rejected.is_none());
        let rc8 = serde_json::to_value(rc8).unwrap();
        assert_eq!(baseline["water_draw_join"], true, "the join remains a measured release default");
        assert_eq!(baseline["actor_consumer_frustum_cull"], true, "the consumer cull is a measured RC10 release default");
        assert_eq!(baseline["fx_unlit_exact"], true, "the unlit exact path is a measured RC10 release default");
        assert_eq!(baseline["load_texture_share"], true, "RC12 enables the shared upload in the release input");
        assert_eq!(baseline["alpha_model_pvs"], false, "RC13 keeps the model alpha cull off after the hatter2 finding");
        assert_eq!(rc11["load_texture_share"], false, "RC11 kept sharing off in the release input");
        assert_eq!(rc12["alpha_model_pvs"], false, "RC12 kept the model alpha cull off in the release input");
        assert_eq!(baseline["loading_screen"], true, "the boot loading screen stays on");
        assert_eq!(baseline["texture_cache"], "off", "the disk texture cache stays off");
        assert_eq!(baseline["pvs_cache"], false, "the disk PVS cache stays off");
        assert_eq!(baseline["world_shared_buffers"], true);
        assert_eq!(baseline["world_parameter_reuse"], true);
        assert_eq!(baseline["water_stage_merge"], "off");
        let mut rc12_with_rc17 = rc12.clone();
        for key in ["sim_snark_move_skip", "sim_snark_sight_blocked", "sim_pool_name_match", "audio_stats_log",
                    "audio_loop_resident", "audio_cue_prefetch", "actor_discard_free_shader",
                    "actor_cutout_discard_free", "actor_frontface_cull"] {
            rc12_with_rc17[key] = baseline[key].clone();
        }
        assert_eq!(baseline, rc12_with_rc17, "RC17 release keeps every RC12 value except the nine RC15/RC16/RC17 keys");
        assert_eq!(baseline["actor_cutout_discard_free"], true,
                   "RC17 ships the exact ant cutout program on");
        // The RC13 package predates the four RC15 keys, the four RC16 keys and the RC17
        // ant key; check it against the RC17 release with those keys reverted, so its
        // frozen bytes stay the RC13 baseline.
        let mut rc13_baseline = baseline.clone();
        for key in ["sim_snark_move_skip", "sim_snark_sight_blocked", "sim_pool_name_match",
                    "audio_loop_resident", "audio_cue_prefetch", "actor_discard_free_shader",
                    "actor_cutout_discard_free", "actor_frontface_cull"] {
            rc13_baseline[key] = false.into();
        }
        rc13_baseline["audio_stats_log"] = true.into();
        let mut variant = rc12.clone();
        variant["load_texture_share"] = rc11["load_texture_share"].clone();
        assert_eq!(variant, rc11, "RC12 release keeps every RC11 value, with sharing on");
        assert_eq!(rc11, rc10, "RC11 release keeps every RC10 value, with the shared upload off by default");
        let mut variant = rc10.clone();
        for key in ["actor_consumer_frustum_cull", "fx_unlit_exact"] { variant[key] = rc9[key].clone(); }
        assert_eq!(variant, rc9, "RC10 release changes only the two declared keys relative to RC9");
        let mut variant = rc9.clone();
        variant["water_draw_join"] = rc8["water_draw_join"].clone();
        assert_eq!(variant, rc8, "the frozen RC9 release changes only the water draw join relative to RC8");
        // The frozen RC8 release keeps the three measured defaults relative to RC7.
        let (rc7, rejected) = parse_runtime_config(rc7_bytes).unwrap();
        assert!(rejected.is_none());
        let rc7 = serde_json::to_value(rc7).unwrap();
        let mut variant = rc8.clone();
        for key in ["sim_flight_memo", "ant_corpse_triangle_cull", "ant_corpse_opaque_shader"] {
            assert_eq!(rc8[key], true, "{key} remains a measured release default");
            variant[key] = rc7[key].clone();
        }
        assert_eq!(variant, rc7, "the RC8 release changes only its three declared keys relative to RC7");
        let defaults = historical_keys(&serde_json::to_value(Config::default()).unwrap());
        // A package adds only its replay keys to the release it is built on.
        let check_package = |name: &str, bytes: &[u8], map: Option<&str>, keys: &[String], against: &serde_json::Value| {
            let raw: serde_json::Value = serde_json::from_slice(bytes).unwrap();
            assert_eq!(&historical_keys(&raw), keys, "{name}");
            let (play, rejected) = parse_runtime_config(bytes).unwrap();
            assert!(rejected.is_none());
            play.validate().unwrap();
            let mut variant = serde_json::to_value(play).unwrap();
            let mut declared = vec![("log_frame_time_every", serde_json::json!(10))];
            match map {
                None => declared.push(("replay_play", serde_json::json!("a2-04.lgtape"))),
                Some(map) => declared.extend([
                    ("replay_play", serde_json::json!(format!("a4-{map}-01.lgtape"))),
                    ("replay_start", serde_json::json!(format!("chapter:{map}"))),
                    ("replay_stop_after", serde_json::json!(1200)),
                    ("replay_check_every", serde_json::json!(600)),
                ]),
            }
            for (key, expected) in declared {
                assert_eq!(variant[key], expected, "{name} {key}");
                variant[key] = against[key].clone();
            }
            assert_eq!(&variant, against, "{name} changes only its declared keys against its release");
        };
        // The RC17 play package adds only its replay keys to the RC17 release.
        check_package("PL-rc17-release-play.json", include_bytes!("../../configs/PL-rc17-release-play.json"), None, &defaults, &baseline);
        // The frozen RC16 play package adds only its replay keys to the frozen RC16 release,
        // which is the RC17 release with the ant key off.
        let mut rc16_baseline = baseline.clone();
        rc16_baseline["actor_cutout_discard_free"] = false.into();
        check_package("PL-rc16-release-play.json", include_bytes!("../../configs/PL-rc16-release-play.json"), None, &defaults, &rc16_baseline);
        // The RC13 packages add only their replay keys to the RC13 release.
        check_package("PL-rc13-release-play04.json", include_bytes!("../../configs/PL-rc13-release-play04.json"), None, &defaults, &rc13_baseline);
        check_package("PL-rc13-wchess1.json", include_bytes!("../../configs/PL-rc13-wchess1.json"), Some("wchess1"), &defaults, &rc13_baseline);
        // The RC12 packages keep their frozen bytes and add only their replay keys
        // to the frozen RC12 release.
        check_package("PL-rc12-release-play04.json", include_bytes!("../../configs/PL-rc12-release-play04.json"), None, &defaults, &rc12);
        check_package("PL-rc12-load-gvillage.json", include_bytes!("../../configs/PL-rc12-load-gvillage.json"), Some("gvillage"), &defaults, &rc12);
        check_package("PL-rc12-load-funhouse.json", include_bytes!("../../configs/PL-rc12-load-funhouse.json"), Some("funhouse"), &defaults, &rc12);
        // The RC11 packages keep their frozen bytes and add only their replay keys
        // to the frozen RC11 release.
        check_package("PL-rc11-release-play04.json", include_bytes!("../../configs/PL-rc11-release-play04.json"), None, &defaults, &rc11);
        check_package("PL-rc11-load-gvillage.json", include_bytes!("../../configs/PL-rc11-load-gvillage.json"), Some("gvillage"), &defaults, &rc11);
        check_package("PL-rc11-load-funhouse.json", include_bytes!("../../configs/PL-rc11-load-funhouse.json"), Some("funhouse"), &defaults, &rc11);
        // The RC10 packages keep their frozen bytes, so they predate the shared key
        // and add only their replay keys to the frozen RC10 release.
        let frozen_keys: Vec<String> = defaults.iter().filter(|key| *key != "load_texture_share").cloned().collect();
        check_package("PL-rc10-release-play04.json", include_bytes!("../../configs/PL-rc10-release-play04.json"), None, &frozen_keys, &rc10);
        check_package("PL-rc10-skool1.json", include_bytes!("../../configs/PL-rc10-skool1.json"), Some("skool1"), &frozen_keys, &rc10);
        check_package("PL-rc10-hatter2.json", include_bytes!("../../configs/PL-rc10-hatter2.json"), Some("hatter2"), &frozen_keys, &rc10);
        // The RC9 package is the frozen RC9 release plus the two play keys.
        let bytes = include_bytes!("../../configs/PL-rc9-release-play04.json");
        let raw: serde_json::Value = serde_json::from_slice(bytes).unwrap();
        assert_eq!(historical_keys(&raw), defaults);
        assert!(raw.get("sim_sweep_any").is_none());
        assert_eq!(raw["water_draw_join"], true);
        assert!(!raw.as_object().unwrap().keys().any(|key| key.starts_with("diag_skip_")
            || matches!(key.as_str(), "spike_log_first_frame" | "spike_log_last_frame")));
        let (play, rejected) = parse_runtime_config(bytes).unwrap();
        assert!(rejected.is_none());
        play.validate().unwrap();
        let mut variant = serde_json::to_value(play).unwrap();
        assert_eq!(variant["replay_play"], "a2-04.lgtape");
        assert_eq!(variant["log_frame_time_every"], 10);
        variant["replay_play"] = rc9["replay_play"].clone();
        variant["log_frame_time_every"] = rc9["log_frame_time_every"].clone();
        assert_eq!(variant, rc9, "the RC9 play config changes only its two declared keys");
        // The RC8 package keeps its frozen bytes, its measured defaults and no join key.
        let bytes = include_bytes!("../../configs/PL-rc8-release-play04.json");
        let raw: serde_json::Value = serde_json::from_slice(bytes).unwrap();
        assert_eq!(historical_keys(&raw), defaults);
        assert!(raw.get("sim_sweep_any").is_none() && raw.get("water_draw_join").is_none());
        assert_eq!(raw["sim_flight_memo"], true);
        let (play, rejected) = parse_runtime_config(bytes).unwrap();
        assert!(rejected.is_none());
        let mut variant = serde_json::to_value(play).unwrap();
        assert_eq!(variant["replay_play"], "a2-04.lgtape");
        assert_eq!(variant["log_frame_time_every"], 10);
        variant["replay_play"] = rc8["replay_play"].clone();
        variant["log_frame_time_every"] = rc8["log_frame_time_every"].clone();
        assert_eq!(variant, rc8, "the RC8 play config changes only its two declared keys");
        let bytes = include_bytes!("../../configs/PL-rc8-spike-play04.json");
        let raw: serde_json::Value = serde_json::from_slice(bytes).unwrap();
        assert_eq!(historical_keys(&raw), defaults);
        let (spike, rejected) = parse_runtime_config(bytes).unwrap();
        assert!(rejected.is_none());
        let mut variant = serde_json::to_value(spike).unwrap();
        assert_eq!(variant["replay_play"], "a2-04.lgtape");
        assert_eq!(variant["log_frame_time_every"], 10);
        assert_eq!(variant["spike_log_ms"], serde_json::json!(0.01_f32));
        assert_eq!(variant["spike_log_first_frame"], 1980);
        assert_eq!(variant["spike_log_last_frame"], 2140);
        for key in ["spike_log_ms", "spike_log_first_frame", "spike_log_last_frame"] { variant[key] = rc8[key].clone(); }
        variant["replay_play"] = rc8["replay_play"].clone();
        variant["log_frame_time_every"] = rc8["log_frame_time_every"].clone();
        assert_eq!(variant, rc8, "the RC8 spike config adds a spike window to the RC8 play config");
        // The RC7 package keeps its frozen bytes and remains release plus its two declared keys.
        let bytes = include_bytes!("../../configs/PL-rc7-release-play04.json");
        let raw: serde_json::Value = serde_json::from_slice(bytes).unwrap();
        assert!(raw.get("sim_sweep_any").is_none() && raw.get("sim_flight_memo").is_none());
        let (play, rejected) = parse_runtime_config(bytes).unwrap();
        assert!(rejected.is_none());
        let mut variant = serde_json::to_value(play).unwrap();
        assert_eq!(variant["replay_play"], "a2-04.lgtape");
        assert_eq!(variant["log_frame_time_every"], 10);
        variant["replay_play"] = rc7["replay_play"].clone();
        variant["log_frame_time_every"] = rc7["log_frame_time_every"].clone();
        assert_eq!(variant, rc7, "the RC7 play config changes only its two declared keys");
    }
    #[test]
    fn packaged_r3_config_contains_exactly_all_runtime_defaults() {
        let bytes = include_bytes!("../../configs/PL-def-r3.json");
        let c: Config = serde_json::from_slice(bytes).unwrap();
        c.validate().unwrap();
        let mut written = serde_json::from_slice::<serde_json::Value>(bytes).unwrap();
        assert!(!c.sim_sweep_any);
        written["sim_sweep_any"] = false.into();
        for (key,value) in serde_json::to_value(Config::default()).unwrap().as_object().unwrap() {
            if round5_key(key) || later_opt_in_key(key) { written[key] = value.clone(); }
        }
        assert_eq!(written, serde_json::to_value(Config::default()).unwrap());
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
            // Earlier runtime keys are written; the later adjacent opt-in remains off.
            let raw: serde_json::Value = serde_json::from_slice(bytes).unwrap();
            let defaults = serde_json::to_value(Config::default()).unwrap();
            let mut written = historical_keys(&raw);
            // All later optional families remain at off defaults in the frozen RC1 files.
            // at their off defaults.
            const A5: [&str; 10] = ["packet_frustum_cull", "prop_frustum_cull", "packet_min_pixels", "world_min_pixels", "world_texture_sort",
                "water_stage_merge", "world_alpha_lite", "fx_lite", "fx_add_zero_alpha_cut", "fx_add_max_area"];
            const A5_FIGHT: [&str; 4] = ["spike_log_ms", "audio_combat_preload", "sim_sweep_blocked", "actor_near_cull"];
            let mut all: Vec<_> = defaults.as_object().unwrap().keys()
                .filter(|k| !k.starts_with("replay_") && !A5.contains(&k.as_str()) && *k != "water_normals" && *k != "low_fog" && *k != "fog_sky" && *k != "exact_blends" && *k != "detail_layers"
                    && !k.starts_with("sky_cube") && *k != "sky_depth" && !A5_FIGHT.contains(&k.as_str()) && *k != "world_adjacent_batch" && *k != "sim_sweep_any"
                    && !later_opt_in_key(k) && !round5_key(k)).cloned().collect();
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
            // Earlier runtime keys are written; the later adjacent opt-in remains off.
            let raw: serde_json::Value = serde_json::from_slice(bytes).unwrap();
            let defaults = serde_json::to_value(Config::default()).unwrap();
            let mut written = historical_keys(&raw);
            let mut all = historical_keys(&defaults);
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
    fn rc3_play_configs_are_rc2_plus_the_rc3_keys() {
        // Default file: PL-rc3.json. Each variant differs from it only in the keys listed below.
        let parse = |bytes: &[u8]| {
            let raw: serde_json::Value = serde_json::from_slice(bytes).unwrap();
            let defaults = serde_json::to_value(Config::default()).unwrap();
            let mut written = historical_keys(&raw);
            let mut all = historical_keys(&defaults);
            written.sort(); all.sort();
            assert_eq!(written, all, "an RC3 config must write every runtime key");
            let (c, rejected) = parse_runtime_config(bytes).unwrap();
            assert!(rejected.is_none());
            c.validate().unwrap();
            assert_eq!(c.log_frame_time_every, Some(10), "frame-time rows every ten frames stay on");
            assert_eq!(c.render_scale, 0.75);
            serde_json::to_value(&c).unwrap()
        };
        let rc3 = parse(include_bytes!("../../configs/PL-rc3.json"));
        let mut rc2 = parse(include_bytes!("../../configs/PL-rc2.json"));
        let defaults = serde_json::to_value(Config::default()).unwrap();
        // Keys added after RC2 that RC3 leaves at their off defaults.
        for key in ["packet_min_pixels", "world_min_pixels", "world_texture_sort", "water_stage_merge", "world_alpha_lite",
            "fx_lite", "fx_add_max_area", "water_normals", "low_fog", "fog_sky", "exact_blends", "detail_layers",
            "sky_cube", "sky_cube_size", "sky_cube_refresh", "sky_cube_retain", "sky_cube_finish",
            "sky_cube_capture_only", "sky_cube_clear_only", "sky_depth"] {
            assert_eq!(rc3[key], defaults[key], "{key} off in RC3");
        }
        for key in ["replay_record", "replay_play", "replay_start", "replay_check_every", "replay_state_every", "replay_stop_after"] {
            assert_eq!(rc3[key], defaults[key], "{key} off in RC3");
        }
        // RC3 = RC2 plus the three reviewed cuts and the direct scaled target.
        for (key, value) in [("packet_frustum_cull", serde_json::json!(true)), ("prop_frustum_cull", serde_json::json!(true)),
            ("fx_add_zero_alpha_cut", serde_json::json!(true)), ("scaled_target", serde_json::json!("direct"))] {
            assert_eq!(rc3[key], value, "{key} in RC3");
            rc2[key] = value;
        }
        assert_eq!(rc3, rc2, "PL-rc3 differs from PL-rc2 only in the RC3 keys");
        for (bytes, keys) in [
            (&include_bytes!("../../configs/PL-rc3-fill.json")[..], vec![("water_stage_merge", serde_json::json!("merge")),
                ("world_alpha_lite", serde_json::json!(true)), ("fx_lite", serde_json::json!(true))]),
            (&include_bytes!("../../configs/PL-rc3-water.json")[..], vec![("water_normals", serde_json::json!(true))]),
            (&include_bytes!("../../configs/PL-rc3-fog.json")[..], vec![("low_fog", serde_json::json!(true)), ("fog_sky", serde_json::json!(true))]),
            (&include_bytes!("../../configs/PL-rc3-fidelity.json")[..], vec![("low_fog", serde_json::json!(true)), ("fog_sky", serde_json::json!(true)),
                ("exact_blends", serde_json::json!(true))]),
            (&include_bytes!("../../configs/PL-rc3-detail.json")[..], vec![("low_fog", serde_json::json!(true)), ("fog_sky", serde_json::json!(true)),
                ("exact_blends", serde_json::json!(true)), ("detail_layers", serde_json::json!(true))]),
            (&include_bytes!("../../configs/PL-rc3-sky.json")[..], vec![("low_fog", serde_json::json!(true)), ("fog_sky", serde_json::json!(true)),
                ("sky_cube", serde_json::json!(true))]),
            (&include_bytes!("../../configs/PL-rc3-sky-finish.json")[..], vec![("low_fog", serde_json::json!(true)), ("fog_sky", serde_json::json!(true)),
                ("sky_cube", serde_json::json!(true)), ("sky_cube_finish", serde_json::json!(true))]),
            (&include_bytes!("../../configs/PL-rc3-sky-depth.json")[..], vec![("low_fog", serde_json::json!(true)), ("fog_sky", serde_json::json!(true)),
                ("sky_depth", serde_json::json!(true))]),
            (&include_bytes!("../../configs/PL-rc3-resolve.json")[..], vec![("scaled_target", serde_json::json!("resolve"))]),
            (&include_bytes!("../../configs/PL-rc3-record.json")[..], vec![("replay_record", serde_json::json!("a2-#.lgtape")),
                ("replay_start", serde_json::json!("chapter:potears2"))]),
            (&include_bytes!("../../configs/PL-rc3-replay.json")[..], vec![("replay_play", serde_json::json!("a2-02-marked.lgtape"))]),
        ] {
            let mut variant = parse(bytes);
            for (key, value) in keys {
                assert_eq!(variant[key], value, "{key}");
                variant[key] = rc3[key].clone();
            }
            assert_eq!(variant, rc3, "a variant differs from PL-rc3 only in its own keys");
        }
    }
    #[test]
    fn a5_draw_cut_keys_default_off_and_validate() {
        let c = Config::default();
        assert!(!c.packet_frustum_cull && !c.prop_frustum_cull && !c.world_texture_sort);
        assert_eq!((c.packet_min_pixels, c.world_min_pixels), (0., 0.));
        for key in ["packet_min_pixels", "world_min_pixels"] {
            for good in [0., 6., 64.] {
                let c: Config = serde_json::from_value(serde_json::json!({key: good})).unwrap();
                c.validate().unwrap();
            }
            for bad in [-1., 64.5, 1e9] {
                let c: Config = serde_json::from_value(serde_json::json!({key: bad})).unwrap();
                assert!(c.validate().is_err(), "{key}={bad}");
            }
            assert!(serde_json::from_value::<Config>(serde_json::json!({key: true})).is_err());
        }
        for key in ["packet_frustum_cull", "prop_frustum_cull", "world_texture_sort"] {
            assert!(serde_json::from_value::<Config>(serde_json::json!({key: 1})).is_err(), "{key}");
            let c: Config = serde_json::from_value(serde_json::json!({key: true})).unwrap();
            c.validate().unwrap();
        }
        let printed = config_log_lines(&Config::default())[0].clone();
        for key in ["packet_frustum_cull", "prop_frustum_cull", "packet_min_pixels", "world_min_pixels", "world_texture_sort"] {
            assert!(printed.contains(&format!("\"{key}\"")), "{key} is printed");
        }
    }
    #[test]
    fn a5_draw_configs_differ_from_rc2_only_in_their_keys() {
        let parse = |bytes: &[u8]| {
            let raw: serde_json::Value = serde_json::from_slice(bytes).unwrap();
            let defaults = serde_json::to_value(Config::default()).unwrap();
            let mut written = historical_keys(&raw);
            let mut all = historical_keys(&defaults);
            written.sort(); all.sort();
            assert_eq!(written, all, "an A5 config must write every runtime key");
            let (c, rejected) = parse_runtime_config(bytes).unwrap();
            assert!(rejected.is_none());
            c.validate().unwrap();
            serde_json::to_value(&c).unwrap()
        };
        let rc2 = parse(include_bytes!("../../configs/PL-rc2.json"));
        for key in ["packet_frustum_cull", "prop_frustum_cull", "world_texture_sort"] { assert_eq!(rc2[key], false, "{key} off in RC2"); }
        for key in ["packet_min_pixels", "world_min_pixels"] { assert_eq!(rc2[key], 0., "{key} off in RC2"); }
        let all = vec![("packet_frustum_cull", serde_json::json!(true)), ("packet_min_pixels", serde_json::json!(6.)),
            ("world_min_pixels", serde_json::json!(6.)), ("world_texture_sort", serde_json::json!(true)),
            ("prop_frustum_cull", serde_json::json!(true))];
        for (bytes, keys) in [
            (&include_bytes!("../../configs/PL-rc2-a5-packet-frustum.json")[..], vec![all[0].clone()]),
            (&include_bytes!("../../configs/PL-rc2-a5-packet-min6.json")[..], vec![all[1].clone()]),
            (&include_bytes!("../../configs/PL-rc2-a5-world-min6.json")[..], vec![all[2].clone()]),
            (&include_bytes!("../../configs/PL-rc2-a5-texture-sort.json")[..], vec![all[3].clone()]),
            (&include_bytes!("../../configs/PL-rc2-a5-prop-frustum.json")[..], vec![all[4].clone()]),
            (&include_bytes!("../../configs/PL-rc2-a5-all.json")[..], all.clone()),
            (&include_bytes!("../../configs/PL-rc2-a5-exact.json")[..], vec![all[0].clone(), all[4].clone()]),
        ] {
            let mut variant = parse(bytes);
            for (key, value) in keys {
                assert_eq!(variant[key], value, "{key}");
                variant[key] = rc2[key].clone();
            }
            assert_eq!(variant, rc2, "an A5 file differs from PL-rc2 only in its own keys");
        }
    }
    #[test]
    fn a5_fight_keys_default_off_validate_and_print() {
        let c = Config::default();
        assert_eq!(c.spike_log_ms, 0.);
        assert!(!c.audio_combat_preload);
        for good in [0., 34., 10000.] {
            let c: Config = serde_json::from_value(serde_json::json!({"spike_log_ms": good})).unwrap();
            c.validate().unwrap();
        }
        for bad in [-1., 10001., 1e30] {
            let c: Config = serde_json::from_value(serde_json::json!({"spike_log_ms": bad})).unwrap();
            assert!(c.validate().is_err(), "{bad}");
        }
        assert!(serde_json::from_value::<Config>(serde_json::json!({"audio_combat_preload": 1})).is_err());
        assert!(!c.sim_sweep_blocked);
        assert_eq!(c.actor_near_cull, 0.);
        for good in [0., 80., 1000.] {
            let c: Config = serde_json::from_value(serde_json::json!({"actor_near_cull": good})).unwrap();
            c.validate().unwrap();
        }
        for bad in [-1., 1001., f32::MAX] {
            let c: Config = serde_json::from_value(serde_json::json!({"actor_near_cull": bad})).unwrap();
            assert!(c.validate().is_err(), "{bad}");
        }
        assert!(serde_json::from_value::<Config>(serde_json::json!({"sim_sweep_blocked": 1})).is_err());
        let printed = config_log_lines(&Config::default())[0].clone();
        for key in ["spike_log_ms", "audio_combat_preload", "sim_sweep_blocked", "actor_near_cull"] {
            assert!(printed.contains(&format!("\"{key}\"")), "{key} is printed");
        }
    }
    #[test]
    fn a5_fight_configs_differ_from_rc3_only_in_their_keys() {
        let parse = |bytes: &[u8]| {
            let raw: serde_json::Value = serde_json::from_slice(bytes).unwrap();
            let defaults = serde_json::to_value(Config::default()).unwrap();
            let mut written = historical_keys(&raw);
            let mut all = historical_keys(&defaults);
            written.sort(); all.sort();
            assert_eq!(written, all, "an A5 fight config must write every runtime key");
            let (c, rejected) = parse_runtime_config(bytes).unwrap();
            assert!(rejected.is_none());
            c.validate().unwrap();
            serde_json::to_value(&c).unwrap()
        };
        let rc3 = parse(include_bytes!("../../configs/PL-rc3.json"));
        assert_eq!(rc3["spike_log_ms"], 0.);
        assert_eq!(rc3["audio_combat_preload"], false);
        assert_eq!(rc3["sim_sweep_blocked"], false);
        assert_eq!(rc3["actor_near_cull"], 0.);
        let spike = ("spike_log_ms", serde_json::json!(40.));
        let audio = ("audio_combat_preload", serde_json::json!(true));
        let blocked = ("sim_sweep_blocked", serde_json::json!(true));
        let near = ("actor_near_cull", serde_json::json!(80.));
        for (bytes, keys) in [
            (&include_bytes!("../../configs/PL-rc3-spike.json")[..], vec![spike.clone()]),
            (&include_bytes!("../../configs/PL-rc3-audio.json")[..], vec![audio.clone()]),
            (&include_bytes!("../../configs/PL-rc3-blocked.json")[..], vec![blocked.clone()]),
            (&include_bytes!("../../configs/PL-rc3-near.json")[..], vec![near.clone()]),
            (&include_bytes!("../../configs/PL-rc3-fight.json")[..], vec![spike, audio, blocked, near]),
        ] {
            let mut variant = parse(bytes);
            for (key, value) in keys {
                assert_eq!(variant[key], value, "{key}");
                variant[key] = rc3[key].clone();
            }
            assert_eq!(variant, rc3, "an A5 fight file differs from PL-rc3 only in its own keys");
        }
    }
    #[test]
    fn a5_alpha_fill_keys_default_off_and_validate() {
        let c = Config::default();
        assert_eq!(c.water_stage_merge, "off");
        assert!(!c.world_alpha_lite && !c.fx_lite && !c.fx_add_zero_alpha_cut);
        assert_eq!(c.fx_add_max_area, 0.);
        for good in ["off", "merge", "drop_filter"] {
            let c: Config = serde_json::from_value(serde_json::json!({"water_stage_merge": good})).unwrap();
            c.validate().unwrap();
        }
        for bad in ["on", "", "Merge"] {
            let c: Config = serde_json::from_value(serde_json::json!({"water_stage_merge": bad})).unwrap();
            assert!(c.validate().is_err(), "{bad}");
        }
        assert!(serde_json::from_value::<Config>(serde_json::json!({"water_stage_merge": true})).is_err());
        for good in [0., 0.1, 1.] {
            let c: Config = serde_json::from_value(serde_json::json!({"fx_add_max_area": good})).unwrap();
            c.validate().unwrap();
        }
        for bad in [-0.1, 1.5, 1e9] {
            let c: Config = serde_json::from_value(serde_json::json!({"fx_add_max_area": bad})).unwrap();
            assert!(c.validate().is_err(), "{bad}");
        }
        for key in ["world_alpha_lite", "fx_lite", "fx_add_zero_alpha_cut"] {
            assert!(serde_json::from_value::<Config>(serde_json::json!({key: 1})).is_err(), "{key}");
            let c: Config = serde_json::from_value(serde_json::json!({key: true})).unwrap();
            c.validate().unwrap();
        }
        let printed = config_log_lines(&Config::default())[0].clone();
        for key in ["water_stage_merge", "world_alpha_lite", "fx_lite", "fx_add_zero_alpha_cut", "fx_add_max_area"] {
            assert!(printed.contains(&format!("\"{key}\"")), "{key} is printed");
        }
    }
    #[test]
    fn a5_water_draw_join_defaults_off_and_requires_shared_buffers() {
        let c = Config::default();
        assert!(!c.water_draw_join);
        assert!(serde_json::from_value::<Config>(serde_json::json!({"water_draw_join": 1})).is_err());
        let c: Config = serde_json::from_value(serde_json::json!({"water_draw_join": true, "world_shared_buffers": false})).unwrap();
        assert!(c.validate().is_err(), "the join copies indices of shared buffers");
        let c: Config = serde_json::from_value(serde_json::json!({"water_draw_join": true, "world_shared_buffers": true,
            "water_stage_merge": "merge"})).unwrap();
        assert!(c.validate().is_err(), "the stage merge owns the pair's order");
        let c: Config = serde_json::from_value(serde_json::json!({"water_draw_join": true, "world_shared_buffers": true})).unwrap();
        c.validate().unwrap();
        let printed = config_log_lines(&Config::default())[0].clone();
        assert!(printed.contains("\"water_draw_join\""), "the key is printed");
    }
    #[test]
    fn a5_water_draw_join_refuses_a_stale_parameter_table() {
        let bytes = serde_json::to_vec(&serde_json::json!({"water_draw_join": true,
            "world_shared_buffers": true, "world_parameter_reuse": false, "world_list_submit": false,
            "world_gxm": "off"})).unwrap();
        let c: Config = serde_json::from_slice(&bytes).unwrap();
        let error = c.validate().unwrap_err().to_string();
        assert!(error.contains("world_parameter_reuse"), "a clear rejection names the missing prerequisite: {error}");
        assert!(error.contains("stale"), "the rejection says why reuse is required: {error}");
        let (refused, rejected) = parse_runtime_config(&bytes).unwrap();
        let message = rejected.expect("the join key is refused");
        assert!(message.contains("water_draw_join requires world_parameter_reuse"), "{message}");
        assert!(!refused.water_draw_join, "the key does not take effect");
        assert!(!refused.world_parameter_reuse, "unrelated keys are preserved");
        assert!(refused.world_shared_buffers && !refused.world_list_submit && refused.world_gxm == "off",
            "unrelated keys are preserved");
        refused.validate().unwrap();
        let c: Config = serde_json::from_value(serde_json::json!({"water_draw_join": true,
            "world_shared_buffers": true, "world_parameter_reuse": true})).unwrap();
        c.validate().unwrap();
        let packaged = parse_runtime_config(include_bytes!("../../configs/PL-a5-water-join.json")).unwrap().0;
        assert!(packaged.water_draw_join && packaged.world_parameter_reuse,
            "the packaged join key is valid only with the prerequisite it requires");
    }
    #[test]
    fn a5_alpha_fill_configs_differ_from_a5_exact_only_in_their_keys() {
        let parse = |bytes: &[u8]| {
            let raw: serde_json::Value = serde_json::from_slice(bytes).unwrap();
            let defaults = serde_json::to_value(Config::default()).unwrap();
            let mut written = historical_keys(&raw);
            let mut all = historical_keys(&defaults);
            written.sort(); all.sort();
            assert_eq!(written, all, "an A5 config must write every runtime key");
            let (c, rejected) = parse_runtime_config(bytes).unwrap();
            assert!(rejected.is_none());
            c.validate().unwrap();
            serde_json::to_value(&c).unwrap()
        };
        let exact = parse(include_bytes!("../../configs/PL-rc2-a5-exact.json"));
        let rc2 = parse(include_bytes!("../../configs/PL-rc2.json"));
        for config in [&exact, &rc2] {
            assert_eq!(config["water_stage_merge"], "off");
            for key in ["world_alpha_lite", "fx_lite", "fx_add_zero_alpha_cut"] { assert_eq!(config[key], false, "{key}"); }
            assert_eq!(config["fx_add_max_area"], 0.);
        }
        let on = |key: &'static str, value: serde_json::Value| (key, value);
        let merge = on("water_stage_merge", serde_json::json!("merge"));
        let alpha = on("world_alpha_lite", serde_json::json!(true));
        let fx = on("fx_lite", serde_json::json!(true));
        let zero = on("fx_add_zero_alpha_cut", serde_json::json!(true));
        for (bytes, keys) in [
            (&include_bytes!("../../configs/PL-rc2-a5-water-merge.json")[..], vec![merge.clone()]),
            (&include_bytes!("../../configs/PL-rc2-a5-water-drop.json")[..], vec![on("water_stage_merge", serde_json::json!("drop_filter"))]),
            (&include_bytes!("../../configs/PL-rc2-a5-alpha-lite.json")[..], vec![alpha.clone()]),
            (&include_bytes!("../../configs/PL-rc2-a5-fx-lite.json")[..], vec![fx.clone()]),
            (&include_bytes!("../../configs/PL-rc2-a5-zero-alpha.json")[..], vec![zero.clone()]),
            (&include_bytes!("../../configs/PL-rc2-a5-add-cap10.json")[..], vec![on("fx_add_max_area", serde_json::json!(0.1_f32))]),
            (&include_bytes!("../../configs/PL-rc2-a5-fill.json")[..], vec![merge, alpha, fx, zero]),
        ] {
            let mut variant = parse(bytes);
            for (key, value) in keys {
                assert_eq!(variant[key], value, "{key}");
                variant[key] = exact[key].clone();
            }
            assert_eq!(variant, exact, "a fill file differs from PL-rc2-a5-exact only in its own keys");
        }
    }
    #[test]
    fn a3_water_config_is_rc2_with_water_normals_on() {
        assert!(!Config::default().water_normals, "water_normals defaults off");
        assert!(serde_json::from_str::<Config>(r#"{"water_normals":"on"}"#).is_err());
        let bytes = include_bytes!("../../configs/PL-a3-water.json");
        let (water, rejected) = parse_runtime_config(bytes).unwrap();
        assert!(rejected.is_none());
        water.validate().unwrap();
        let written: serde_json::Value = serde_json::from_slice(bytes).unwrap();
        let keys = historical_keys;
        assert_eq!(keys(&written), keys(&serde_json::to_value(Config::default()).unwrap()), "the A3 water config writes every runtime key");
        let (rc2, rejected) = parse_runtime_config(include_bytes!("../../configs/PL-rc2.json")).unwrap();
        assert!(rejected.is_none());
        assert!(!rc2.water_normals && water.water_normals);
        let (mut rc2, mut water) = (serde_json::to_value(&rc2).unwrap(), serde_json::to_value(&water).unwrap());
        rc2.as_object_mut().unwrap().remove("water_normals");
        water.as_object_mut().unwrap().remove("water_normals");
        assert_eq!(water, rc2, "PL-a3-water differs from PL-rc2 only in water_normals");
    }
    #[test]
    fn water_normals_off_keeps_the_rc2_vertex_shader() {
        assert_eq!(shader::vertex(false), shader::VERTEX);
        let on = shader::vertex(true);
        assert_ne!(on, shader::VERTEX);
        assert!(on.contains("}else if(Deform.x>2.5&&Deform.x<3.5){") && on.contains("if(Deform.x>3.5){"));
        assert!(on.contains("vec3 n=(Model*vec4(rippled,0.0)).xyz;"), "the environment and dot terms read the rippled normal");
    }
    #[test]
    fn detail_layers_defaults_off_and_parses_as_a_bool() {
        assert!(!Config::default().detail_layers, "detail_layers defaults off");
        assert!(serde_json::from_str::<Config>(r#"{"detail_layers":"on"}"#).is_err());
        assert!(serde_json::from_str::<Config>(r#"{"detail_layers":true}"#).unwrap().detail_layers);
    }
    #[test]
    fn exact_blends_off_keeps_the_five_classes_and_on_draws_the_desktop_factors() {
        assert!(!Config::default().exact_blends, "exact_blends defaults off");
        assert!(serde_json::from_str::<Config>(r#"{"exact_blends":"on"}"#).is_err());
        // Off: the classes RC3 draws these blends with.
        assert_eq!(blend_class(Blend::ColorAdd), 2);
        assert_eq!(blend_class(Blend::AlphaColor), 1);
        assert_eq!(blend_class(Blend::Factors(1, 2)), 1);
        for b in [Blend::Opaque, Blend::Alpha, Blend::Add, Blend::AlphaAdd, Blend::Filter, Blend::DoubleFilter, Blend::Invisible] {
            assert!(!approximated_blend(b), "{b:?} keeps its class");
        }
        for b in [Blend::ColorAdd, Blend::AlphaColor, Blend::Factors(1, 2), Blend::Factors(4, 7), Blend::Factors(7, 1)] {
            assert!(approximated_blend(b), "{b:?} gets an exact class");
        }
        // On: the desktop renderer's factors.
        let state = |b| format!("{:?}", crate::render::blend(crate::render::blend_index(b)).unwrap());
        let want = |s, d| format!("{:?}", BlendState::new(Equation::Add, s, d));
        assert_eq!(state(Blend::ColorAdd), want(BlendFactor::Value(BlendValue::DestinationColor), BlendFactor::One));
        assert_eq!(state(Blend::AlphaColor), want(BlendFactor::Value(BlendValue::SourceAlpha), BlendFactor::Value(BlendValue::SourceColor)));
        assert_eq!(state(Blend::Factors(1, 2)), want(BlendFactor::One, BlendFactor::Value(BlendValue::SourceColor)));
        assert_eq!(state(Blend::Factors(4, 7)),
            want(BlendFactor::Value(BlendValue::DestinationColor), BlendFactor::OneMinusValue(BlendValue::SourceAlpha)));
        assert_eq!((exact_fog_mode(Blend::ColorAdd), exact_fog_mode(Blend::AlphaColor), exact_fog_mode(Blend::Factors(1, 2))), (1., 1., 0.));
    }
    #[test]
    fn low_fog_off_keeps_the_rc3_low_program_and_on_keeps_the_full_fog() {
        assert!(!Config::default().low_fog, "low_fog defaults off");
        assert!(serde_json::from_str::<Config>(r#"{"low_fog":"on"}"#).is_err());
        // Off: the RC3 program, byte for byte, built the way RC3 built it.
        let mut rc3 = shader::FRAGMENT.to_string();
        let start = rc3.find(" float cut=Surface.z;").unwrap();
        let end = rc3.find(" vec3 lighting=").unwrap();
        rc3.replace_range(start..end, "");
        let off = shader::low_fragment(false);
        assert_eq!(off, rc3.replace("fogged(rgb,Alpha.z)", "rgb"));
        assert!(!off.contains("fogged(") && !off.contains("discard"));
        // On: the full program's fog code and call, still without the alpha-test discards.
        let on = shader::low_fragment(true);
        assert!(on.contains("gl_FragColor=vec4(fogged(rgb,Alpha.z),a);") && !on.contains("discard"));
        assert!(on.contains("uniform vec4 DistanceFog;") && !on.contains("// FOG"));
        let full = crate::environment::fragment(shader::FRAGMENT);
        let fog_code = |s: &str| s[s.find("uniform highp vec3 FogEye;").unwrap()..s.find("void main()").unwrap()].to_string();
        assert_eq!(fog_code(&on), fog_code(&full), "the same fog code as quality full");
    }
    #[test]
    fn sky_cube_and_sky_depth_default_off_and_bound_the_face_size() {
        let c = Config::default();
        assert!(!c.sky_cube && !c.sky_depth && c.sky_cube_refresh == 0, "both sky keys default off");
        assert_eq!(c.sky_cube_size, 256);
        assert!(!c.sky_cube_retain && !c.sky_cube_finish && !c.sky_cube_capture_only && !c.sky_cube_clear_only,
            "capture diagnostics must not affect existing configurations");
        let (diagnostic, rejected) = parse_runtime_config(br#"{"sky_cube_retain":true,"sky_cube_finish":true,"sky_cube_capture_only":true,"sky_cube_clear_only":true}"#).unwrap();
        assert!(rejected.is_none() && diagnostic.sky_cube_retain && diagnostic.sky_cube_finish
            && diagnostic.sky_cube_capture_only && diagnostic.sky_cube_clear_only);
        let (c, rejected) = parse_runtime_config(br#"{"sky_cube":true,"sky_cube_size":128,"sky_depth":true}"#).unwrap();
        assert!(rejected.is_none() && c.sky_cube && c.sky_depth && c.sky_cube_size == 128);
        c.validate().unwrap();
        for size in [16, 2048] {
            let c = Config { sky_cube_size: size, ..Config::default() };
            assert!(c.validate().is_err(), "face size {size} rejected");
        }
    }
    #[test]
    fn fog_sky_keeps_the_rc3_gradient_off_and_draws_the_fog_colour_flat_on() {
        assert!(!Config::default().fog_sky, "fog_sky defaults off");
        let bg = Color::new(0.5, 0.5, 0.5, 1.);
        assert_eq!(sky_gradient(bg, false), [[0.5, 0.5, 0.5, 1.], [0.75, 0.75, 0.75, 1.]], "the RC3 gradient");
        assert_eq!(sky_gradient(bg, true), [[0.5, 0.5, 0.5, 1.], [0.5, 0.5, 0.5, 1.]]);
    }
    #[test]
    fn wave_normal_rides_the_deform_uniforms_and_never_displaces_vertices() {
        let wave = crate::materials::Wave { kind: "sin".into(), values: [0., 1., 0., 0.3] };
        let normal = Deform::WaveNormal(512., vec3(0.04, 0.03, 0.), wave.clone());
        let stage = Stage { environment: true, ..Default::default() };
        let mut recipe = Recipe {
            stage, textures: vec![], lit: false, rank: 1, blend: 1, opaque: false,
            cull: 0, model: None, motion: None, deform: None, wave_normal: None,
        };
        let still = uniforms(&recipe, 1.5, Vec3::ZERO, Mat4::IDENTITY, Mat4::IDENTITY, false, Vec2::ONE);
        assert_eq!((still.deform, still.wave, still.move_dir), ([0., 0., 1.5, 0.], [0.; 4], [0.; 4]));
        recipe.wave_normal = Some(normal);
        let u = uniforms(&recipe, 1.5, Vec3::ZERO, Mat4::IDENTITY, Mat4::IDENTITY, false, Vec2::ONE);
        assert_eq!(u.deform, [4., 512., 1.5, 0.]);
        assert_eq!(u.move_dir, [0.04, 0.03, 0., 0.]);
        assert_eq!(u.wave, wave.values);
        // Every other uniform word is the frozen recipe's.
        let (a, b) = (still.bits(), u.bits());
        let changed: Vec<_> = (0..a.len()).filter(|&i| a[i] != b[i]).collect();
        assert!(changed.iter().all(|&i| (96..108).contains(&i)), "{changed:?}");
        // A vertex deform owns the shared uniforms: the normal stays still.
        recipe.deform = Some(Deform::Move(Vec3::Z, wave.clone()));
        let moved = uniforms(&recipe, 1.5, Vec3::ZERO, Mat4::IDENTITY, Mat4::IDENTITY, false, Vec2::ONE);
        assert_eq!((moved.deform[0], moved.move_dir), (2., [0., 0., 1., 0.]));
        assert!(deform_extent(&None) == Vec3::ZERO && !Deform::WaveNormal(1., Vec3::X, wave).moves_vertices());
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
                deform:Some(Deform::Move(Vec3::Z,wave.clone())), wave_normal: None };
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
            cull: 0, model: None, motion: None, deform: None, wave_normal: None,
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
