# Milestone 4 pilot delivery

Native source **0ec6adefa378c569ef0b07a2d5a73b3850ebf06e**, vita-renderer-j4.
One native build; VPK SHA-256
`1814faa163ccf0d1d908a5fccfff158d2bd3905ab7043cc3d0090ab415d0d97c`.
RX ends at 0x81bd3494, RW starts at 0x81c80000: 707,436 bytes of metadata margin,
above 3,348 required. Imports, SELF inside the VPK, 192 MiB heap and 8 MiB stack
are verified. No device/FTP/emulator access by this agent.

The reviewed sim9 merge (15a4f04, including sim8 fc9836b) retains both behaviours.
Conflicts were module declarations and profiler/viewer draw masks; no collision,
progression or save conflict required a gameplay choice. Full host suite:
**739 passed, zero failed, one pre-existing ignored test**. Cargo.lock and dependency
pins remain unchanged. Runtime vita_renderer.json and vita_sim.json are independent.

## Passage and loading

Maps crossed: **gvillage only**. Normal Continue/Load now installs the restored
scene directly. Host normal --load and route Continue show one Loaded gvillage
and one retained loaded line. Other maps explicitly fall back to legacy; their
cost is unmeasured and they are outside the passage.

Pilot 3.1 evidence already reduced retained setup from about 138 s to 6.10 s:
groups 0.46, pipelines/uploads 4.24, PVS 1.39. Base LG loading remains dominant.
This change sorts opaque PVS order once and memoizes the unchanged corpus identity.
Cache v4 validates identity/PVS inputs, SHA-256, counts and ranges; reads are bounded
and writes sync/close/rename a private temporary file. Cache is off by default
pending SD validation. Identity uses resolved PK3 entry name/CRC/size metadata,
not raw archive SHA; loaded PVS inputs are also hashed. No save fingerprint change.

The supplied input tape starts from a normal LG save at village arrival, after
the entry cinematic. It reaches the Hallway Blade, completes knife_cat naturally,
then applies 60 seconds of small symmetric moving-stick pulses on the platform.
No flight, warp, debug scene completion or playback clock override. Host recording
uses LG's physics planner for the approach; host-only planner pauses are excluded
from its input clock. The final tape uses the separate host recorder **e838669**;
only that host-only module changed after the native build.

Host delivery replay passes with one Blade, alive in gvillage, 76.117 seconds of
moving inputs including exactly 60.000 after knife_cat. Separate Continue checks
map/feet/yaw/pitch before physics. Save format and LG Store/restoration stay intact;
diagnostic saves are isolated under private/route-saves. New Game replay exposes
camera-heading drift and is excluded from the delivered grid. The named arrival
save avoids its entry camera history; native input replay still requires pilot proof.

Route reports export raw frame_wall_ms in 300-frame windows with per-launch IDs,
bounded segment histograms, complete configuration, scene startup/total time,
heap start/end/delta and profiler capacity. Native route mode disables cumulative
profiler vectors and periodic preview readbacks. Program/pipeline creations after
60 frames are zero in the host delivery recording/replay.

## Runtime renderer controls

Internal render_scale 1/0.75/0.5 scales all 3D, then performs one opaque linear
upscale before native HUD. Desktop has an explicit Depth32 attachment; vitaGL
owns a hidden DF32M FBO depth surface (gxm.c:631), so no unsupported native depth
texture attachment is issued. Effective MSAA/component/depth state is logged.
Default sample_count=1 maps to GXM NONE, not 4x. Depth clears enable the write mask
before clearing; opaques write depth, alpha does not. Front-to-back opaque order,
sky/world-alpha masks and full/low shader presets remain runtime options.
Low eligible opaque shaders omit discard/fog; cutout and alpha use the full path.
GPU111/GPU222 requests are named and log ARM/bus/GPU/xbar before/after.

FX material batching combines identical particle states inside runs bounded by
world/actor packets. Cross-material alpha order is a declared visual compromise;
adjacent/none modes remain available. FX layer limit is independent of actor
layers and puff_cap. Ring streaming uses persistent Dynamic buffers with separate
world/first-person pages over three frames, retained bindings and optional exact
upload caching. Merge plans and unchanged merged geometry are cached; changed
vertices always stream. This is not persistent mapping. Counters distinguish
merges, uploads, growths, draws and creations; draw counts are emitted once per finish.

Actor fast preparation skips unused normal/view/UV operations, uses texture-ID hash
lookup, borrows Stage metadata and bypasses unused Macroquad uniforms/binding.
Owned packets retain appearance/atmosphere. Legacy/actor_fast_path=false remains
available. All blend/depth pipeline objects share one actor program and prewarm.
Host alpha/depth, lights, ghost/dissolve and scaled moving-depth/native HUD fixtures
pass, with no shader/pipeline creation after startup.

Playable-low combines scale 0.75, GPU222, FX material/ring/cache controls and
vita-sim-all. Actor layers remain two and interop stays on. Native CPU/GPU gains,
scaled visuals, SD cost and long-run heap stability are unmeasured here. The pilot
and automatic Vita3K gate receive the packet through PACKAGE-READY; no host or
emulator timing is presented as console performance.
