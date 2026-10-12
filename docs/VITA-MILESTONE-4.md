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
