# Milestone 4 progress

Base: e6e2d70, branch vita-renderer-j4. Console packages 3 and 3.1 remain immutable.

## Step 1: normal resume loads once

Menu Continue/Load and explicit --load now build the restored scene before the
fresh-game branch, then install it directly. The bench's prepared scene is reused
as before. Normal resume remains paused; bench resume remains active. Restore
failure keeps the paused fresh-scene/error behavior without retrying restoration.
Gameplay, progression, save data and Restored::build are unchanged.

Host normal --load auto, feature vita_renderer, three rendered frames: exactly
one `Loaded gvillage`, one `[lg/vita-renderer] loaded`, and
`[lg/startup] restored scene reused map=gvillage source=load`.
Evidence: ~/vita-j0/village-retained-world/jalon-4/host/single-load-normal.log.
This exercises the shared startup branch reached by menu Continue/Load; menu
interaction and Vita timing still need the pilot's confirmation.

## Step 2: route coverage

Maps crossed: **gvillage only**. Arrival ends at alice_cat_talk2, the authored
Hallway Blade pickup starts knife_cat, and that scene ends at alice_knife_node.
All are local village nodes (src/village/cinema.rs); the proposed movement ends
before village_route.rs's later Pandemonium exit. The route must reject any map
transition and must not use flight, warp, recovery or debug scene completion.

The feature selects the new renderer for gvillage. Startup now logs the selected
map/backend. Other maps explicitly fall back to legacy; Pandemonium is outside
this delivery. Its cost has not been measured. The old village world-only median
517.1 ms is not a measurement of another map and must not be assigned to it.

Next: renderer loading/cache and unattended route replay. Target package 20:30
CEST; steps 1–3 may be handed off separately if replay is unfinished at 20:00.

## Step 3: loading and cache

The old milestone-2 renderer took 138 s per build on Vita. That total has no
separate phase breakdown; do not invent one. Milestone 3 already logs recipes,
chunks, pipeline/uploads and PVS. Its source-based PVS computes leaf bounds once
per relevant group instead of per group per cluster. Milestone 4 additionally
sorts opaque material order once, instead of sorting every cluster's list.

PVS cache v4 includes the mounted corpus identity and every actual PVS input.
The existing corpus identity is SHA-256 of override-resolved PK3 entry names,
CRC and size metadata, **not raw archive SHA-256**. Visibility/groups are also
hashed from their loaded values. Geometry/chunks are constructed more cheaply,
not persisted. No save fingerprint algorithm or format changes.

Cache reads are bounded to 16 MiB, checked by SHA-256 and validated ranges/counts.
Writes use a private temporary file, sync, close and rename; any failure or
corruption falls back to fresh lists. Paths are under private/renderer-cache,
never player saves. Cache remains off by default pending native SD results;
pvs_cache=true is a runtime option. Assets memoizes its unchanged fingerprint
per read-only mount, avoiding a second archive metadata scan during scene build.

Host proof: source lists match the old compiler for unknown view and all 2,139
clusters; hit loads 8.4 ms; damaged checksum is rejected and rebuilt (42.4 ms).
The cold verification's 1,256 ms includes the deliberately slow old compiler and
is not a cold-build performance result. Native phase timing and SD cost remain
for the pilot; none of these host numbers are Vita measurements.

## Step 4: input replay and bounded route reports

Host recording uses LG's existing physics planner to propose ordinary stick/jump
controls. The viewer records and replays those controls at the original 120 Hz
physics ticks, without changing position, completing scenes or restoring debug
state. Scene gates wait for entry and knife_cat. The recorded passage stays in
gvillage, collects the Hallway Blade and issues at least 60 seconds of moving
inputs. Auto save and Continue use the unchanged LG Store and restored scene.
Diagnostic saves are isolated in private/route-saves; normal player saves remain
outside this test. Continue checks map, feet, yaw and pitch against the route end.

Host recording completed with Blade/knife_cat progression and 60.000 seconds of
moving inputs. Final replay and Continue verification remain required before the
package handshake. Native route modes disable the cumulative frame profiler,
export raw frame wall samples in windows of 300, and keep bounded 1 ms histograms
for whole passage and each segment. SD export and save time count in the wall
measurement. Reports log heap at route start/end and creation totals after 60
frames. Raw windows permit exact quantiles; summary quantiles are rounded.

## Runtime GPU experiments

The pinned backend maps sample_count=1 to GXM MULTISAMPLE_NONE. This remains the
default; MSAA is not a demonstrated explanation of the 150 ms native floor.
Startup logs configured MSAA and queried framebuffer component/depth bits.
render_scale below 1 creates one persistent single-sample RGBA8/Depth32 3D target,
then one opaque linear upscale before the native-resolution HUD. All 3D actors,
effects and first-person geometry share the target. Native validation is pending.
GPU-only gpu111/gpu222 experiments leave ARM/bus/xbar settings unchanged and log
the effective clocks. Opaques default to front-to-back order; alpha never writes
depth. world_alpha and sky independently mask those world costs. quality=low
uses a fragment path without cutout/discard or fog for eligible opaque surfaces;
cutout and alpha retain the full shader. UV wave/turbulence was already evaluated
in the vertex shader, so this preset does not claim to remove fragment waves.

## Combined simulation base

The orchestrator authorized merging vita-sim-9 at 15a4f04, including sim8 fc9836b.
Conflicts were confined to src/main.rs module declarations, frame_profile.rs
instrumentation and viewer.rs character profiling/masks/upscale placement. Both
behaviors were retained: sim9's character spans and simulation counters, the
renderer world-only mask, heap/program diagnostics, route modes and upscale.
No conflicting collision, progression or save logic required a choice. Both
startup JSON configurations remain independent; sim9 defaults preserve sim8.

Merge validation: full host suite with vita_renderer, 738 passed, zero failed,
one pre-existing ignored asset test. The copied 3.1 W1/full.log subsequently
published a native load breakdown: total 6,096.318 ms, groups 456.799 ms
(recipes 49.829/chunks 397.703), pipelines/uploads 4,235.530 ms, PVS 1,391.885 ms.
Thus the earlier 138 s renderer build was already reduced in 3.1; the remaining
native load cost is primarily pipeline creation/uploads, not cluster iteration.
Milestone 4's additional PVS changes still require native timing.

## FX and actor CPU controls

fx_batching=adjacent preserves the previous whole-group order. material additionally
regroups single-layer particle packets with identical state inside each uninterrupted
particle run. Every world or actor packet remains a barrier. It preserves primitive
order within each material; cross-material alpha order is an explicit visual compromise.
none or merge_fx=false disables merging. fx_max_layers is independent of actor_max_layers
and vita_sim.puff_cap. Defaults retain adjacent batching and two FX/actor layers.

fx_streaming=ring uses persistent Dynamic buffers (glBufferSubData) with three frame
pages and separate world/first-person slots. orphan preserves the pinned Stream path.
Bindings are retained with each page. This is ring streaming, not persistent mapping;
animated geometry still copies. fx_upload_cache optionally skips byte-identical geometry
using exact initialized vertex-field/index comparisons. fx_merge_cache caches packet
state/topology plans and reuses unchanged merged geometry after an exact comparison;
changed vertices are always merged and uploaded. No world geometry enters these caches.
Counters separate plan hits/builds, unchanged merged groups, upload hits/bytes/growths,
input particle groups and GPU draws. Draw statistics are accumulated locally and emitted
once per finish, avoiding five profiler calls per draw. Native timing remains unmeasured.

actor_fast_path skips unused normal/view normalization and UV calculations for simple
stages. Environment/dot-alpha stages retain the original path. sim9's independent
anim_cull can bypass an entire proven visual-only ambient actor before layer preparation
and submission; Alice, scripted and attachment-bearing actors remain protected.

The route requirement is 60 seconds of moving inputs **after** knife_cat, not 60 seconds
including the approach. The host tape is recorded with that stronger end condition.

The complete host recording reached authored entry/knife_cat, Blade ownership,
75.567 seconds of moving inputs including exactly 60.000 after knife_cat, then
saved and exited. It showed zero late shader creation and one late pipeline object
at first Blade use (not repeated compilation). Pipeline objects now prewarm all
109 LG blend keys with both depth comparisons against the same single actor
program, keeping creation at startup rather than at the first weapon draw.
Scaled depth/alpha/native HUD checks passed over 120 frames at scale 0.5, with
zero shader/pipeline creation after the first frame. Native validation is pending.

Depth clears explicitly enable the depth write mask before clearing: the pinned
miniquad backend otherwise inherits the previous HUD's disabled write mask.
The scaled fixture alternates the opaque depth each frame to detect stale depth,
not merely repeat a frozen pose. Route diagnostics suppress periodic preview
readbacks as well as cumulative profiler exports; final LG save format is unchanged.

Vita framebuffer inspection: vglInitExtended receives sample_count=1 -> GXM NONE
(miniquad native/vita/mod.rs:121,210). Local vitaGL initializes native color as
A8B8G8R8 and depth as DF32M/S8 (gxm.c:464,506). Its texture attachment helper accepts
only color attachments (shared.h:1326), and gxm.c:631 creates/reuses a hidden DF32M
depth surface for an FBO. The Vita scaled path uses that owned hidden depth;
desktop GL attaches an explicit Depth32 image. It does not issue unsupported
native depth-texture attachment calls. Effective depth enable/write/function and
component queries are logged; hardware early-Z efficiency still needs measurement.

Further direct actor preparation: actor_fast_path uses a texture-ID hash lookup
instead of scanning the registry, computes a managed texture ID once, and borrows
immutable Stage metadata instead of cloning image-name/modifier vectors per layer.
Fade state is applied to the owned direct packet. The legacy/diagnostic path remains
available with actor_fast_path=false. No skinning, controller or save logic changes.

The first strict host replay failed before the Blade while the adaptive host
recording passed. It is not yet an unattended-route proof. Replay failures now
include tick, segment and full map/alive/flight/feet status for diagnosis. This
must be resolved or the packet must be labeled as the authorized partial delivery.

The direct actor fast path also bypasses SkinMaterial's unused Macroquad
atmosphere/appearance uniform updates and binding. Appearance is still captured
in the owned retained packet, and the direct frame owns its atmosphere. Those
updates continue on the legacy path or with actor_fast_path=false.

Host-only automatic recording subtracts its physics-planner CPU pauses from the
next diagnostic input clock. Playback and native clocks do not use that adjustment.
Recorded reference feet/yaw diagnose divergence without setting any game state.
Recording also writes an isolated normal LG entry checkpoint in slot1; scripts
may start from new_game or save:slot1 through the normal single-scene restore.
Raw window directories have per-launch IDs so Continue cannot overwrite replay
samples. Script validation limits distinct segment names to 64, bounding histogram
storage independently of route length.
