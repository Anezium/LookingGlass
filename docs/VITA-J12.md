# Milestone 12: scoped skin math, versioned FX preparation and frame attribution

Base: `vita-night-j11` at `6e7a0e2`. No device or remote-server access is required.
All settings remain in the strict renderer JSON and require a restart.

| Setting | Default | Purpose |
| --- | --- | --- |
| `worker_probe` | false | Startup kernel inventory and 8-ms CPU samples of entry/game and the scoped worker policy |
| `skin_workers` | 1 | 0 serial, 1 worker plus game-thread math, 2 experimental workers plus game-thread math |
| `fx_prepare_cache` | true | Immutable geometry generations, validation/merge/upload reuse and common uniform preparation |
| `frame_phase_probe` | false | Fixed-capacity timestamps in profiled viewer runs |

## Skin scope and renderer ownership

Following the review, this first implementation uses `std::thread::scope`, not a persistent pool.
Each actor supplies surface batches weighted by vertex count. Each surface's triangles remain
in their original reduction order. A one-surface actor runs on one worker; tiny/empty batches
never start an extra worker. The scope joins before every layer/GL consumer, in the same frame.
Worker closures receive only immutable weighted vertices/pose/scales/palette and disjoint mutable
vertex arrays plus immutable indices. Existing mesh objects/vectors retain their addresses.
Animation, culling/preservation decisions, palette setup, textures and GL calls remain on the game
thread. The serial switch uses the same extracted arithmetic with its original span labels.

Workers use 512-KiB stacks and explicit kernel masks `0x20000` and `0x40000`; failed pinning aborts
the batch rather than silently accepting an inherited mask. No system-core mask is used. Startup
probes instantiate the same scoped stack/affinity policy; their thread IDs are distinct from the
later per-actor threads. `sceKernelGetThreadInfo` supplies effective mask/priority and CPU sampling
supplies residency, rather than pthread's ambiguous affinity readback. The game/entry affinity
is inventoried without changing helper-thread inheritance. Hardware placement and contention
remain unverified; the second worker is experimental. Startup probe stacks are released before
backend initialization, so scoped stack allocation under live rendering still needs device QA.

`Pass`, retained `Gpu`, and world `Renderer` own a `GameThread` marker containing `PhantomData<Rc<()>>`.
They cannot implement `Send` or `Sync`; immutable geometry packets also use `Rc`.
A renderer owner binds the thread identity; debug assertions guard skin/submission/retained-upload
entry points. The pure job types are `Send`; they have no mesh/texture/material/backend handles.
Compile-fail checks and compile-time trait ambiguity checks protect the ownership boundary.

Worker service and join wait are recorded separately in microsecond counters. Summed worker
service is not a nested wall span and must not be added to game-thread CPU time. The enclosing
`skin_actor_batch` includes spawn/pin/join overhead. There is no cross-frame queue or pipeline.

## FX generations and storage

A geometry publication has a monotonic identity plus separate vertex and topology generations.
Indices are validated when topology or vertex count changes. Immutable `Rc` publications survive
frozen frames; queued old publications remain immutable when a producer changes. Skin layer
alpha variants retain independent geometry histories. Mutable legacy producers are compared
once at their publication boundary, including signed zero and NaN bits. This conservative scan
is still required because existing public mesh arrays have no mutation notification API.
Unchanged generations skip downstream merge and upload scans; this is not a claim that all
producer comparison work disappears.

Merge caches preserve material/actor/world/alpha barriers and output order. They retain reusable
signatures and destination/source lists. Merged geometry carries separate topology/vertex history.
Each physical ring page compares the full publication identity and generations; growth replaces
history. Changed vertices can upload without redundant unchanged indices. Texture/binding and
material/appearance/camera state remain live inputs. Common uniform fields are expanded once
per model/lit pair per finish, then per-pass fields are patched bit-exactly. Packet/range/shared
upload scratch storage and geometry vector capacities are reused. Turning the switch off keeps
the byte-comparison/upload-policy fallback.

## Phase attribution and protocol

The phase probe preallocates capacity for 10,000 rows before entering the viewer. Each row has
a viewer sequence, an optional measured-frame index and monotonic microsecond timestamps for
frame start, CPU start/end, profiler reduction, periodic LG log work, report work, yield,
swap start/end, input polling, and the next viewer start. Zero marks an absent phase. Overflow
is counted; storage never grows. Swap/input hooks wrap linked SDK/driver entry points without
changing dependency pins. Only completed rows are exported as `frame_phases` in the ordinary
post-window report; the final measured swap belongs to that final measured frame.

The residual from yield to swap includes the Macroquad tail. Swap-end to next viewer-start
includes backend logging, input/update and scheduling; input polling has its own timestamps.
Backend tail flush and logger internals are not separately instrumented. Early rejected viewer
paths keep timestamps and a null measured-frame index. Preserve original wall populations and
flag report/capture frames. Before a future hardware A/B, the pilot screenshot must be moved
outside the selected gameplay window and median/trimmed/untrimmed results published together.
No pilot/device protocol was executed or modified in this milestone.

## Validation and packaging

`cargo test --locked --features vita_renderer` exercises fixed-pose serial/scoped byte parity,
strict settings, publication lifetime/topology/bit equality, alpha variants, merge output/order,
complete uniforms and fixed trace capacity. `tools/check_gl_confinement.py` compiles negative
Send/Sync cases. `--skin-workers-check` checks actual user-supplied Alice data without graphics.
The native `--vita-alpha-check` includes 18 additional ring/mutation captures; compare every PNG
with preparation on/off. Native world/scale/save diagnostics remain applicable.

Use the J11 A-stock build (`-C target-feature=-neon`) with locked dependencies. Package VPK,
ELF/SELF/VELF/SFO, imports verification, ELF margin, revision/checksum manifests, presets and
host evidence under `~/vita-j0/village-retained-world/jalon-12/`. Presets preserve J11 PL-def
and vary each new switch independently, plus combined-off and skin2. Device visual parity,
actual worker placement/priority/stack usage, performance and live phase exports are unverified.
