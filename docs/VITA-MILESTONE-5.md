# Milestone 5 work log

Branch vita-renderer-j5 starts at 061faaa. Milestone 4 stays unchanged.

## Actor preparation

Runtime actor_layer_cache caches immutable layer state by surface and mesh identity.
Only camera/normal-independent layers with full entity opacity use this path;
animated image/gain state refreshes when its clock changes. Camera-dependent UV,
dot-alpha, fading and TAN effects retain exact preparation. Appearance remains a
live packet input. actor_shared_geometry shares identical layer attributes and
uploads once per shared geometry, with individual texture bindings at draw time.
actor_batching isolates adjacent single-pass actor grouping by complete pipeline,
texture and uniform state. Multi-layer actors and world packets remain barriers;
alpha order is preserved. actor_frustum_cull reuses sim9's safe ambient-actor checks
without changing simulation, contacts, progression or saves.

Host reference-save A/B: 2400x1296, zero differing pixels across the entire image.
The last-frame A/B counters show 45 layer-cache hits, zero dynamic state refreshes
and 64/55 direct dynamic draws with actor batching disabled/enabled. This draw
comparison isolates the knob; adjacent actor grouping already existed in milestone
4, so it is not a measured milestone-4 performance gain. Host tests: 741 passed,
zero failed, one pre-existing ignored. Evidence remains outside Git under
~/vita-j0/village-retained-world/jalon-5/host/.

## Startup timeout and archive audit

4.1 P0 (20:08:58 CEST) reaches pk3_open done / central_directory begin for
pak0.pk3, then stops inside ZipArchive::new(File). The earlier route-tape,
renderer-config and GPU-setup hypotheses are downstream of this marker.
Moving route-inputs.json aside did not change the stall. The known-good 3.1
console health control is pending; degraded storage is not yet proven.

Comparison e6e2d70..061faaa changes no archive opening, scanning/sorting,
central-directory/index construction, buffering, mmap, read sizes or threading.
Only fingerprint() memoization changed after mount completion. Cargo.toml,
Cargo.lock and .cargo are identical, including ZIP/miniquad pins. The sim8/9
integration does not alter archive reading. Startup order is preserved in 4.1
and milestone 5 while the pilot finishes the health control.

Trace markers bracket scan, each open/central index, index progress, window/config,
UI/menu, route read/parse/validation, corpus identity and save load. Host mount:
484.6 ms for 6 packs / 10221 names; renderer JSON: 0.13 ms; tape 3434923 bytes /
9169 samples: 5.55 ms; save: about 3.5 ms. These are host timings only.

The independent splash allocator issue is proven as overlapping ownership in code:
vgl.c starts invoke_splashscreen before later resources; splashscreen.c signals
completion before freeing buffers/context. The shipped archive has separate
vgl.o -> invoke_splashscreen relocation and splashscreen.o entry points, while
mem_utils.o has no LwMutex imports. The 18:14 abort's exact cause is a hypothesis.
The SDK splash starts after renderer config and cannot explain this ZIP stall.

## Splash allocator ownership

startup_splash=false (default) suppresses the SDK logo thread via the linker's
--wrap=invoke_splashscreen hook. vglInit allocations therefore remain on the
caller; no splash buffer/context allocations or cleanup run concurrently. The
original LG opening film is separate and unchanged. startup_splash=true is the
explicit legacy control. The vitaGL archive/pins stay unchanged; the final ELF
must show vgl.o's call reaching __wrap_invoke_splashscreen. This removes the code
proven overlap; it does not prove the 18:14 abort's exact corruption mechanism.
Startup ordering is unchanged while the pilot evaluates the 4.1 trace and health
control. Host feature check passes. RW reservation moves to 0x82080000 for the
larger trace/actor build; final ELF margin remains a required package check.

## New Game route and camera handoff

route_heading_lock=true compensates recorded/live cinematic headings through
ordinary bounded look input (2.5 rad/s). Movement/tape advancement waits during
alignment; a ten-second failure bound prevents an unattended infinite recenter.
No player pose, script facing, scene completion or gameplay clock is overwritten.
reference_yaw is an optional camera target; reference_feet stays diagnostic only.
Frame wall and named alignment segments include that added time.

New Game tapes now enter the normal opening-film path and require its completion.
Named-save and host-record modes retain their previous entry policy. Original LG
controller/progression/save code remains unchanged. Tape reads are capped at
16 MiB and use a 64 KiB buffered reader only after route-mode selection inside the
initialized viewer. Host suite: 742 passed, zero failed, one pre-existing ignored.
The pre-streaming host New Game route passed film, entry, Blade, knife_cat and
60 seconds of input/save; camera alignment required zero corrective ticks at
that run. Streaming-path and second-launch Continue validation follows below.

## Native film support

The base still deliberately skipped native story films. New Game now reuses the
bounded ZIP/movie reader from vita-stream-on-hotfix 962f6e3 (including its 256 KiB
worker stack) without merging its unrelated changes or renaming the mount API.
Only film entry reads use open_stream after window initialization. ZIP workers
send owned 32 KiB chunks through a one-slot queue; drop disconnects before join.
GPU uploads stay on the viewer thread. Normal mount/read paths and benchmark film
suppression stay unchanged. Host/New Game and Continue validation follows below.
