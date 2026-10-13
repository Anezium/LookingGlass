# Same-frame skin overlap (milestone 14)

`skin_overlap` defaults to false. With `skin_workers=1` or `2`, it enables
an early CPU snapshot of the gameplay character pass. The benchmark switch is
incompatible with overlap. The existing serial and J13 per-actor persistent
worker paths remain available with overlap disabled.

The viewer prepares the final simulation, dialogue, view, projection, lighting
and portal views first. It visits Alice, the placed NPCs and controller-owned
level art before `scene.draw` and dispatches all accepted skeletal body draws as
one batch. The game thread submits the opaque world while the workers run,
then collects immediately before character submission. All material, texture,
attachment, electric/frozen overlay, FX and GL work stays on the game thread.
Gather suppresses prop rendering and effect-history advancement. Ambient LOD
advances once in gather; submission consumes that decision and refreshes the
cache using completed vertices. World-only frames skip the overlap pipeline.
Portal views are completed before the main-view gather; later special effect
silhouettes retain their existing game-thread path.

The early visit copies only numeric skin inputs: final global poses, scales,
palette, ordered weights/indices and vertex fields. Each draw occurrence owns
separate output ranges, including multiple actors sharing a model and severed
body pieces. At submission, completed arrays copy back to the original mesh
storage, preserving actor-cache identity and ordered layer submission. Poses
and draw options are checked against the original call before using the snapshot;
a changed call or arena overflow uses the serial kernel. Neither worker receives
or dereferences a Mesh, texture, material, skeleton owner or renderer handle.
CPU types have compile-time Send checks; the frame guard remains non-Send.

Startup allocates bounded capacity for 128 draws, 2,048 surfaces, 8,192 poses,
scales/palette entries, 32,768 vertices and 131,072 weights/indices. Reset,
numeric capture, dispatch, kernel, collect and copy allocate nothing in steady
state. This does not claim allocation-free animation sampling or the existing
render traversal/layer assembly: their original allocations still exist, and the
early visitor currently repeats presentation sampling during submission. The
host allocator test measures the skin snapshot pipeline, not all character code.
Overflow stops further capture for that frame without growing buffers. No
performance win is claimed until the packaged presets are measured on hardware.

The actual persistent workers use cores 1 and 2 (masks 0x20000/0x40000), each
with a 512-KiB stack. Overlap pins the game thread to CPU0 (0x10000) before
starting workers, because J13 sampled it on CPU2. With `worker_probe=true`,
the actual threads log kernel affinity/priority and sampled CPU ids. Hardware
placement remains unverified in this local-only milestone.

A frame guard drains all pending work on normal completion or game-thread panic.
Collect waits for both workers before propagating either worker panic. Worker
shutdown also drains before any CPU storage is released. Boxed UnsafeCell storage
keeps shared immutable inputs/task descriptors stable while two disjoint output
ranges run; no mutable reference to the live Mesh crosses the boundary.

The light profiler CSV adds `skin_collect_wait_us` after `swap_us`: one integer
residual wait for each accepted measured frame, zero on serial/world-only frames.
Its fixed 60-row ring exports without a per-frame print or file write. Full
profiling also records `skin_collect_wait_us`, `skin_worker_service_us`, and
`skin_overlap_fallback_draws`; a per-frame `[lg/skin-overlap]` line reports wait,
service, fallback and overflow. Worker service is not an additional frame delay.

Verification commands (with the existing pinned environment and locked deps):

- `cargo test --locked --features vita_renderer`
- `cargo check --locked`
- `python3 tools/check_gl_confinement.py`
- `cargo build --locked --features vita_renderer`
- `looking-glass --data <local base> --no-audio --skin-workers-check`
- `looking-glass --data <local base> --no-audio --skin-overlap-render-check`

The first native check compares real Alice and NPC SKB/SKA snapshots, including
duplicate shared-model instances, palettes, scales, normals, colors and UV bits.
The second compares serial/overlap captures over six real maps, both worker
counts, Alice ghost and the actual NPC/level-art/effect graph. No player saves
are used. The pipeline tests also cover zero allocations, fixed storage,
rejected/cull draws, overflow, input mismatch, both-worker panic recovery and
frame unwind/drain/reuse.
