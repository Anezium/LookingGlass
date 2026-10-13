# Same-frame skin overlap (milestone 17)

`skin_overlap` defaults to true with `skin_workers=2`. With one or two workers it enables
an early CPU snapshot of the gameplay character pass. The benchmark switch is
incompatible with overlap. The existing serial and J13 per-actor persistent
worker paths remain available with overlap disabled.

The viewer prepares the final simulation, dialogue, view, projection, lighting
and portal views first. It visits Alice, the placed NPCs and controller-owned
level art before `scene.draw` and dispatches all accepted skeletal body draws as
one batch. The game thread submits the opaque world while the workers run,
then collects immediately before character submission. All material, texture,
attachment, electric/frozen overlay, resource-owning FX and GL work stays on the game thread.
With `actor_layer_reuse` (default true), the same worker batch also prepares
two plain numeric alpha variants and exact position/normal/color revisions.
The game thread preserves and checks caller-edited UVs, publishes immutable
layer geometry, and reuses empty bounded pass containers. Untracked or
camera/normal-dependent layers retain their original preparation path.
Gather suppresses prop rendering and effect-history advancement. Ambient LOD
advances once in gather; submission consumes that decision and refreshes the
cache using completed vertices. World-only frames skip the overlap pipeline.
Portal views are completed before the main-view gather; later special effect
silhouettes retain their existing game-thread path.

Skeleton parsing creates an owning `Arc<SkinSurface>` per mesh, containing only
immutable weights, indices, skin vertices and bind-output vertices. Gather clones
these handles and copies per-frame poses, scales, palette and draw flags. It never
copies weights, indices or vertex arrays, and never clears or initializes output
vertices on the game thread: the workers initialize their disjoint output ranges
from load-time bind data. Handles keep all referenced CPU data alive even if the
skeleton owner or live meshes are dropped. Neither worker receives or dereferences
a Mesh, texture, material, skeleton owner or renderer handle. CPU types have
compile-time Send checks; the frame guard remains non-Send.

Animation interpolation also reuses exact immutable clip/time/loop keys between
gather and submission, including the Magma local-pose path. Each caller receives
an independent mutable local-pose vector; uncaptured late FX samples keep their
serial evaluation. This cache is bounded to 128 distinct keys and resets each
frame. Alice and NPC/controller skeletal draws prepare their facial/global pose and bone
scales once in gather. A bounded game-thread cache retains owning presentation
handles for each draw occurrence; submission checks skeleton/face identity, local
pose bits, mouth and bone-scale roots before reusing that result for the skin and
attachments. Palette preparation likewise happens once per occurrence and reuses
the same values for capture and submission. Duplicate instances of one model and
severed pieces occupy separate entries. Changed presentation inputs are prepared
afresh, and the skin snapshot's input checks route them through the serial kernel.
Serial presentation does not enter these caches or clone their input keys.

At submission, completed position/normal/color fields copy back to the original
mesh storage. The serial kernel's untouched live UVs remain intact, including
caller-edited UVs. Pose, flags, CPU handle identity and live topology are checked
before using the result; a changed call or arena overflow uses the serial kernel.

Startup allocates bounded capacity for 128 draws, 2,048 surface handles, 8,192
pose/scales/palette entries and 32,768 output vertices. The old 131,072-entry
weight/index arenas and 32,768-entry source-vertex arena are removed. Reset,
handle capture, dispatch, worker kernel, collect and copy allocate nothing in
steady state. Presentation caches retain at most 128 entries each. Local animation-pose
vectors still allocate, while interpolation is reused; cache input-key vectors and
handles warm up on first use. The allocator test measures the numeric skin
pipeline, not the full render traversal or layer assembly. Overflow stops further
capture without growing arenas. No performance win is claimed before console A/B.

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

The light CSV keeps its original columns first:
`frame,wall_us,frame_cpu_us,swap_us,skin_collect_wait_us`, then appends
`skin_gather_us,skin_fallbacks,skin_overflow`. Gather time runs from frame-guard
begin (including arena/cache reset) through the end of traversal, before dispatch.
Fallbacks count every submitted skin call that used the serial path while the
overlap frame was ready; overflow is 0/1 for bounded arena overflow. Metrics are
recorded after character submission so late fallbacks are included. Serial and
world-only rows keep all overlap metrics zero. The fixed 60-row ring exports
without a per-frame print or write. Full profiles additionally report gather,
wait, worker service, fallback and overflow; worker service is not another frame
delay. Native parity asserts that every gathered presentation was reused exactly
once, so a duplicated pose/palette computation cannot silently pass that check.

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
load-handle lifetime, caller-edited UVs, changed topology, pose/palette reuse,
rejected/cull draws, overflow, input mismatch, both-worker panic recovery and
frame unwind/drain/reuse.
