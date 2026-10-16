# Step 0: attribution on the reference r3b runtime

Base acfd417, runtime 27c4d40. This branch changes instrumentation only. No job
scheduling, ownership, data layout used by rendering, simulation order, timestep,
RNG, input, resource operation, or draw order changes. Cargo.lock stays pinned.
`sim_span_probe` defaults to false; set true in vita_renderer.json, with
`profiler: "light"`. Both arms use the SAME SELF and otherwise identical r3b
settings. Existing light counters remain active in both arms. Additional scope
clocks, nesting writes and audio sampling run only in the on arm. On host, also
set LOOKING_GLASS_RENDER_METRICS=1 to exercise the same attribution code.

## Exact simulation definition

`sim_blocks_us` is the per-frame sum of the INCLUSIVE, nonoverlapping viewer
intervals: input, interaction_search, interactions, movers, movement, story,
particles, combat, alice, audio, camera, listener. It starts immediately after
light cpu_begin and ends immediately before scene preparation. The named switch
sites in viewer.rs define every boundary; conditional blocks retain their
existing guards. Combat's two adjacent intervals accumulate into one column.
Input includes menu/control handling, clocks and early rope queries before
interaction_search. Movement ends at the story boundary, including its intervening
bookkeeping. Story includes transition checks/loading; transition frames are
already excluded by the existing profiler policy. Camera includes view selection,
quake and show_alice. Listener includes audio.prepare, its liquid query and
update_clocks. This intentionally includes input, camera AND listener, unlike the
archived eight-category J24 sum. It measures the serial pre-scene region, not an
assertion that every operation therein can run on a worker. Compare only active,
non-transition gameplay rows with matched input/timestep policy.

The twelve individual columns are EXCLUSIVE of nested step-0 scopes; sim_blocks
includes those children. Do not add sim_blocks to its constituents. All additions
use nanoseconds internally and convert once per CSV field to integer microseconds.

## New CSV attribution fields

All following columns end in `_us` and accumulate per frame. Nested probe scopes
subtract their whole duration from their parent, so game-thread attribution fields
are exclusive and additive; time spent publishing probe bookkeeping remains
unattributed or in an enclosing scope. Existing render phases / FX counters are
inclusive parents and MUST NOT be added to these new columns.

- input, interaction_search, interactions, movers, movement, story, particles,
  combat, alice, audio, camera, listener: exact viewer boundaries above, exclusive.
- scene_post_sim: scene boundary through skin-gather/world boundary, excluding
  nested step-0 work. Includes atmosphere, presentation/reflection, viewport/camera,
  transforms/ropes, second steam publication, lights, portals and FX-view setup.
  Mixed scene CPU/resource work; distinct from sim_blocks.
- hud_queries: active HUD rope, pandemonium, conversation, world prompt and owner
  selection, including prompt construction, stopping before UI label replacement.
- hud_other: HUD region excluding hud_queries and any other nested probe scopes.
- other: remaining viewer regions (gather, render, capture, save), excluding children.
- world_opaque_visibility_numeric: cluster/PVS/model/frustum/cache/order work, including
  numeric TLS projection/depth/config reads; no GL calls. Only opaque
  invocations accumulate here; alpha retains the existing world_alpha counters.
- world_opaque_parameter_numeric: parameter view/generation/model/uniform construction,
  prepared-list publication and existing telemetry, excluding texture resolution.
- world_opaque_texture_resource: stage-frame texture selection and raw_miniquad_id.
- world_opaque_pass_resource: interop flush, begin pass, viewport and sky submission,
  including their small CPU setup. Direct draw loops remain in existing render phases.
- fx_sort_cpu: joint-alpha list construction, bounds/centres and sorting.
- fx_batch_cpu: puff plan/pack/list publication, excluding texture keys and packet
  ownership operations. Includes CPU allocation and generation bookkeeping.
- fx_observe_cpu: geometry observation/validation/comparison/copy cache pass;
  these Geometry publications contain no managed textures.
- fx_merge_cpu: merge signature/plan/compare/packing, excluding resource scopes.
  Includes the existing blocking worker handoff and wait during packing; it is
  elapsed game-thread service, not pure arithmetic service on one core.
- fx_key_resource: texture-ID lookup and managed mesh/packet publication/drop during
  merge (legacy non-in-place path conservatively includes its CPU copying).
- fx_batch_resource: puff packet/material construction, texture ownership/drop and
  group publication. Resource columns include associated CPU scaffolding.
- fx_upload_cpu: packet iteration, shared lookup, uniform packing and staging, minus
  all nested upload scopes.
- fx_upload_compare_cpu: validity checks and upload reuse comparisons/planning.
- fx_upload_copy_cpu: previous vertex/index history copies and version publication.
- fx_upload_resource: managed texture resolution, context access, capacity/growth,
  buffer creation/replacement/deletion.
- fx_upload_gl: buffer_update region, including byte/call bookkeeping; distinct from
  allocation/growth above. It is NOT a driver-only clock.
- fx_recycle_cpu: cache reset, packet/range/scratch recycling and telemetry, excluding
  resource-bearing geometry/pass recycling.
- fx_recycle_resource: geometry/cache retention and destruction, actor-cache recycle,
  group/pass/puff drops; conservatively includes CPU copying/bookkeeping therein.
- pickup_other_mixed: Art::draw residual after children; includes essence/effect
  lifecycle and unextracted numeric/material work. This is NOT a worker budget.
- pickup_select_cpu: pickup filtering/kind selection and collected-effect pruning.
- prop_numeric: Prop::meshes_at animation/interpolation, normals, UV reset and colour
  arithmetic for ALL callers (including pickups), not only powerups::Art.
- prop_layer_numeric: loot-art UV layer arithmetic for ALL loot callers.
- prop_publish_resource: Prop mesh packet submission and loot-art texture/pass
  publication (resource work with CPU packet scaffolding).
- pickup_resource: pickup material setup/bind and final default-material restore.
- attached_sample_cpu: complete Attached::sample, including birth history, callbacks,
  puff generation and deterministic cache pruning, from every sampling caller.
- skin_worker0_service, skin_worker1_service: each worker's already-measured job
  elapsed duration, published by the game thread at drain. Includes preemption;
  excludes queue/start delay. These are a DIFFERENT AXIS, never added to game-thread
  spans. Existing skin_collect_wait_us is exposed drain time (join plus completion
  bookkeeping); existing skin_gather_us is the gather interval.

Resource boundaries are conservative; no remaining mixed residual is advertised
as movable numeric work. Source locations are the Probe enum references. General
GL draw loops retain the existing owning render phases. No second context or
unchecked Send/Sync wrapper is introduced.

## Audio

The existing Monitor is consumed by CPAL on CPU1. In probe-on only, its existing
1024-sample flush boundary (512 stereo frames at 44.1 kHz on Vita) observes callback
start gaps and monitor-consumption duration. Once per >=1-second wall window it
reads sceKernelGetThreadInfo on THAT thread; `[lg/audio-cpu]` reports the cumulative
run_clocks delta as audio_thread_cpu_us, elapsed window_us, thread ID, affinity,
priority, zero-inclusive monitor_late_blocks (>block+1 ms) and
monitor_over_budget_blocks (>block). First window establishes a baseline.
This is whole CPAL-thread scheduled CPU time, including mixer/decoder, conversion,
backend/log overhead; it excludes blocked output time and preemption. It is not
per-sample wall duration or a separately measured decoder-only kernel. Preserve
raw logs and check the kernel run-clock units/policy on console before attribution.
The SDK declaration calls runClocks a SceKernelSysClock; see
https://docs.vitasdk.org/group__SceThreadMgrUser.html .
Unmodified CPAL also logs late_callbacks and underrun_estimates whenever nonzero;
no such warning in a successfully running window means zero reported estimates,
not proof of hardware underrun absence. Monitor counters are explicitly estimates
and are not substituted for CPAL counts. Probe-off preserves CPAL's existing
warnings and performs no added sampling. No CPAL dependency or audio policy change.

## Acceptance

Full default/all-feature suites, fixed-input simulation hashes and frozen PNGs
must match across same-binary off/on arms; host timings are not console claims.
On console preserve stalls, use paired medians/p95 and measure probe overhead.
See the packaged PROTOCOL.md; no threading implementation is authorized by this step.
