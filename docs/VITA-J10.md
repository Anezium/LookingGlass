# Milestone 10: gated frame preparation changes

Build from `vita-night-j10`, based on J9 `b93dcf5`, with the J9 A-stock
compiler/SDK settings and locked dependencies. Rust NEON remains disabled.

The validated `vita_renderer.json` configuration adds three defaults:

- `save_preview: "throttled"`: normal play captures every 120 viewer frames,
  plus a clean frame before pause/menu entry and every thumbnail-bearing save.
  Modal saves, New Game preservation and finale checkpoints wait for that
  clean frame; quit captures before its autosave. The menu never enters the
  thumbnail. `"every_frame"` restores the old capture/write path. Diagnostic,
  benchmark and route preview policies remain unchanged.
- `mover_lazy_publish: true`: publish the temporary collision world without
  the current mover only after a rider validates its feet and needs a world
  query. Final publication, collider versions/order, carry/block decisions,
  simulation substeps and the existing far-mover cadence are unchanged.
- `pickup_fx_cache: true`: retain separate particle instances for placed item
  IDs, drop IDs and collection event IDs. Visit changes, reload, disappeared
  items, replaced origins and rewind invalidate the appropriate state.
  Compile emitter assignments once, preserving enabled-at-birth semantics,
  authored tie order, loop history and exit/entry boundaries. Reuse birth
  storage and particle/order buffers, prepare only accepted essence layers,
  and skip unused legacy particle/essence uniform setters in retained mode.
  Moving scripted rewards keep fresh birth reconstruction.

The profiler records four accumulated leaves under `frame_objects`:
`objects_mesh_layers`, `objects_effect_sampling`, `objects_event_lookup` and
`objects_particle_packets`. Event lookup is subtracted from sampling so the
leaves do not overlap. Opaque, alpha and sky submission each publish totals
for state/bindings, uniforms and draws. The existing world alpha submit label
is now accumulated per pass, with inclusive/exclusive accounting preserved;
no new per-draw spans or per-birth profiler records are emitted.

Host regression coverage adds six tests: strict switch parsing/defaults;
preview cadence/forced captures and unchanged diagnostic policy; persisted
fresh quick/manual/finale thumbnails; village carry/block/interacting-mover
and final collision/save parity; compiled event boundary/loop/tie parity;
and bit-exact particle geometry, colours, counts, birth ages, blend/order and
lifecycle parity across fixed clocks with caching enabled and disabled.
The native save-preview regression also checks that the actual menu drawing
function displays the persisted images. The native loot regression covers
collection, paused clocks, expiry and restoration on the supplied assets.

Package presets preserve every J9 PL-def setting and vary only the three
new switches: `PL-def.json`, `PL-def-off.json`, and the three individual-off
files. Restart after changing the renderer configuration. Artifacts, checks,
logs and revisions are in `~/vita-j0/village-retained-world/jalon-10/`.

Hardware performance, full-mode save/menu timing, visual parity, clocks and
profiler hierarchy on Vita remain unverified. No console or remote server
was contacted. No far-mover cadence, FX topology cache, world state dedupe,
CPU palette or GPU skinning optimization was added in this milestone.
