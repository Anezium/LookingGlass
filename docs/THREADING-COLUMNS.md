# Threading step 0b / step 1 light columns

Base: r3f with the step-0 probes (da149a6). The instrumentation changes described
here repair that schema and lower FX probe density. The optional world worker is
documented separately in THREADING-STEP1.md. Neither switch changes simulation
order, timestep, RNG or input. Cargo.lock remains pinned. sim_span_probe and
world_prepare_worker both default to false; use profiler="light". On host set
LOOKING_GLASS_RENDER_METRICS=1 to enable telemetry. The same SELF must be used
for both arms of each experiment. Current schema details and changes from the
archived r3f step 0 are listed at the end of this document.

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
- world_opaque_visibility_numeric: cluster/PVS/model/frustum/cache/order work, using snapshotted projection/depth/config; no GL calls. Only opaque
  invocations accumulate here; alpha retains the existing world_alpha counters.
- world_opaque_parameter_numeric: parameter view/generation/model/uniform construction,
  prepared-list publication and existing telemetry, excluding texture resolution.
- world_opaque_texture_resource: stage-frame texture selection and raw_miniquad_id.
- world_opaque_pass_resource: interop flush, begin pass, viewport and sky submission,
  including their small CPU setup. Direct draw loops remain in existing render phases.
- fx_sort_cpu: joint-alpha list construction, bounds/centres and sorting.
- fx_batch_mixed: complete coarse puff batching interval, including numeric packing,
  texture keys, managed packet construction and publication.
- fx_observe_cpu: unchanged geometry observation/validation/cache pass.
- fx_merge_mixed: complete merge stage, including numeric planning/packing,
  resource ownership and any blocking pair work. Not a pure numeric worker budget.
- fx_upload_pack_mixed: complete upload stage minus the existing buffer-update
  clock. Includes validation, texture/context resolution, growth, comparisons,
  copies, uniform packing and packet staging. No per-packet probe scopes remain.
- fx_upload_buffer_gl: existing per-upload buffer-update microseconds credited
  once to the enclosing probe. No additional clock or TLS operation inside the
  upload loop. Includes byte/call bookkeeping; not driver-only time.
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
See the packaged PROTOCOL.md; passing host checks does not enable the worker by default.

## Archived r3f step-0 schema (superseded by step 0b)

Branch `vita-threading-step0-r3f` carries this instrumentation onto
`vita-reference-r3f` 0bd6241. The 43 CSV columns, their boundaries and
nesting are unchanged. r3f's own viewer additions (startup timing marks,
load diagnostics, audio game-stage marks) fall inside the existing scopes:
loading-only probes land in `scene_post_sim` or `other`, stage marks in the
skin-gather/collect `other` regions.

The audio sampling point moved with r3f's output path. With
`audio_pcm_worker` true (the r3f candidate), the CPAL callback only copies
ready PCM blocks; the Monitor, and therefore this probe, runs on the
`audio-producer` thread (priority 161, affinity 0x40000). Each 1024-sample
Monitor block is one producer block. So on r3f:

- `[lg/audio-cpu]` `thread_id` matches the `[lg/audio-producer] id=` line,
  not the CPAL callback thread. `audio_thread_cpu_us` is that producer's whole
  scheduled CPU: mixing, effect decode, resampling, voice DSP, Monitor and
  probe overhead. It excludes music decode on the `audio-music` thread
  (`audio_music_thread`) and the callback's PCM copy.
- `monitor_late_blocks` counts producer block starts more than one block plus
  1 ms after the previous start. The producer runs up to the cushion ahead of
  output, so this is a producer scheduling estimate. r3f's
  `pcm_starved_blocks` remains the authoritative output starvation count.
- With `audio_pcm_worker` false the Monitor runs on the CPAL callback as on
  r3b and the original reading applies.

## Step 0b / step 1 schema

Before the GPU amendment below, the schema had 170 unique fields: 124 inherited fields, 37 exclusive/
parallel probes, sim_blocks_us, probe_scopes and seven world fields. The historical
43-field step-0 schema had colliding upload names and is intentionally rejected
by the new reducer. The inherited fx_upload_cpu_us and fx_upload_gl_us counters
are unchanged; the new upload probes are fx_upload_pack_mixed_us and
fx_upload_buffer_gl_us. Batch/merge are explicitly labelled mixed after removing
the dense per-packet resource/compare/copy scopes. No summation of these fields
with the inclusive inherited FX counters is valid.

`probe_scopes` counts scope openings (not microseconds), resets every frame and
is zero when sim_span_probe is false. Other probe boundaries remain unchanged;
world texture resolution now opens one coarse scope per opaque pass. Numeric
world telemetry is published from measured kernel intervals without opening
per-group scopes. Offloaded numeric durations are not credited to exclusive
main-thread probes; they remain in the inclusive world visibility/parameter
counters and the separate service axis. Main-thread fallback does receive the
exclusive numeric credit. The game-thread exclusive sum must remain <= frame CPU.

World fields are active in both sim_span_probe arms with the light profiler:
- world_prepare_jobs: completed opaque worker jobs (count).
- world_prepare_fallbacks: requested jobs run inline because the pool is unavailable.
- world_prepare_dispatch_us: snapshot, owned-task creation and dispatch; includes
  inline service in a fallback. No double accounting with inline_us.
- world_prepare_wait_us: exposed join and pool restoration on the game thread.
- world_prepare_service_us: worker elapsed service, including preemption, excluding
  queue latency. Parallel axis; do not add it to the game-thread sum.
- world_prepare_inline_us: opaque snapshot+numeric preparation when switch is off.

Both worker and probe experiments use the same SELF. Probe-on/off validation
zeros only the 37 probe fields, sim_blocks_us and probe_scopes. It must not require
world or inherited fields to be zero. Reducer requires the complete ordered schema,
all 120 distinct frames 61..180, nonnegative integer fields, exclusive-sum bound
and nonzero sim_blocks in each on launch. Missing/duplicate headers always fail.
Audio interpretation is the r3f producer-thread interpretation above.

- world_prepare_queue_us: elapsed dispatch-to-callback delay, measured before the
  numeric closure starts. Parallel scheduling diagnostic; zero for inline fallback.

## Existing GPU-call waits and submission CPU (step 0b amendment)

The current schema has **200 unique fields** (the 170 above plus these 30).
These fields are a separate, overlapping axis: do not add them to the exclusive
step-0 sum, render phases, inherited swap_us, or each other indiscriminately.
They are zero with sim_span_probe=false. No extra GL flush, finish, readback,
fence, notification or GXM synchronization call is introduced. Host driver
counters are zero; host parity cannot establish these Vita wait measurements.

The linked, unchanged VitaGL archive references sceGxmBeginScene, EndScene,
Finish, TransferFinish, NotificationWait and DisplayQueueAddEntry. GNU linker
wrappers measure those existing calls plus DisplayQueueFinish, glFlush and
glFinish. Existing vglSwapBuffers wrapping supplies the swap bracket. The
vglGetProcAddress path also returns the glFlush/glFinish wrappers, so function
pointer dispatch cannot bypass them. Prototypes follow the pinned VitaSDK gxm.h.
Zero glFinish/notification counts are valid when those APIs are not called;
nonzero scene, submission and swap coverage is mandatory before interpretation.

Each bracket reads the game thread's sceKernelGetThreadInfo.run_clocks at its
ends, using the same microsecond convention already checked by step 0 on r3f.
Elapsed monotonic time and scheduled thread CPU are measured separately. The
bracket includes the read-only clock-sampling overhead. Clocks are read at coarse
submission/driver-call boundaries, never per retained draw, bind or uniform.

- gpu_submit_wall_us / gpu_submit_cpu_us: wall time / actual scheduled game-thread
  CPU in the union of existing retained world pass submission, scale preparation,
  FX upload/packing and FX draw intervals, explicit bridge flushes and deferred
  framework GL packets. CPU world visibility/parameter preparation, actor gather,
  sorting and merge preparation are outside these regions. Upload includes its
  CPU validation/packing/resource scaffolding. Deferred packets start on the
  first existing GL event and close after the actual draw; residual backend
  cleanup closes before swap. An existing GXM call outside these regions is a
  disjoint submission interval unless it is inside swap. These are engine/backend
  submission costs, not a driver-only CPU timer or total per-frame CPU.
- gpu_submit_nonwait_cpu_us: scheduled CPU in those intervals minus scheduled CPU
  inside their outermost potentially blocking GXM calls. This exposes CPU spent
  preparing/submitting before or between wait APIs. It never subtracts elapsed
  waiting time from a CPU-clock duration.
- gpu_wait_api_wall_us / gpu_wait_api_cpu_us: union of elapsed / scheduled CPU
  inside the existing GXM scene, finish, transfer, notification and display-queue
  calls. Includes calls under swap and readbacks. Nested waits count once in
  this union; the individual API columns remain inclusive. BeginScene/EndScene
  include ordinary scene CPU as well as any backend synchronization.
- gpu_wait_off_cpu_us: per-call max(0, wall minus scheduled CPU) for that union.
  This measures off-CPU time while inside known GPU APIs. It includes scheduler
  preemption and clock-boundary error: **it is not a GPU execution timer or proof
  that every microsecond was a GPU stall**. Report it beside elapsed and CPU,
  rather than labelling all unexplained frame time GPU waiting. Microsecond
  per-call quantization applies; overhead acceptance includes every clock read.
- gpu_gl_flush_us / _calls, gpu_gl_finish_us / _calls, gpu_vgl_swap_us / _calls:
  inclusive elapsed call brackets and invocation counts. A GL flush may submit
  without blocking; only its measured time is claimed. Swap includes display
  queue work and driver housekeeping. inherited swap_us remains independent.
- gpu_gxm_finish_us / _calls, gpu_gxm_transfer_finish_us / _calls,
  gpu_gxm_notification_wait_us / _calls, gpu_gxm_display_queue_add_us / _calls,
  gpu_gxm_display_queue_finish_us / _calls, gpu_gxm_begin_scene_us / _calls,
  gpu_gxm_end_scene_us / _calls: individual existing API elapsed/count pairs.
  Do not add parent GL/swap brackets to their GXM children.
- gpu_submit_scopes: completed disjoint submission regions; gpu_clock_reads:
  run-clock sampling calls; gpu_clock_errors: failed/mismatched/nonmonotonic or
  inconsistent run clocks, or a scope crossing a frame boundary; gpu_probe_frames:
  one when GPU observation was armed for that row. CPU read failures invalidate
  the launch; zero CPU is never silently accepted as evidence of blocking.

GPU rows include the frame tail and swap, and are copied into the preceding
light row at its next begin, matching existing wall/swap ownership. Backend work
before the next viewer begin belongs to that preceding wall interval. Storage is
fixed capacity, and no exports/allocations occur inside the driver wrappers.
The reducer requires the complete ordered 200-column schema, GPU observation on
every retained on row, valid clocks, one swap per row, submission coverage and
observed GXM scene hooks. Probe-off zeros include all 30 GPU fields.
