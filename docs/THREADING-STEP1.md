# Same-frame world preparation experiment

`world_prepare_worker` is strict, boolean and **false by default**. The enabled
path snapshots projection, camera/depth direction, PVS list, time, transforms,
atmosphere and numeric config on the game thread after scene preparation. It
lends skin worker 0 (CPU1, mask 0x20000, inherited priority 159) the exclusively
owned numeric scratch/recipes while the game thread gathers actor poses. The
world job joins before skin dispatch, and its results are consumed by the opaque
draw in that same frame. Alpha preparation remains inline. Secondary views with
no gather window join immediately. No extra thread, frame of latency, simulation
step or unchecked Send/Sync implementation is introduced.

The step-0 r3f placement evidence puts producer 161 and music 170 on CPU2 alongside
skin worker 1 at 159. Producer use reached 498 ms/s on potears1. CPU1 instead has
the buffered PCM callback at 128, which preempts the borrowed worker. This avoids
adding numeric work to CPU2's measured mix/decode load. The existing worker keeps
its 512 KiB stack, affinity and priority; no priority changes are needed. Console
placement and audio/p95 acceptance remain experimental and must be measured.

The job contains only Send numeric data: immutable recipes without textures,
owned transforms/atmosphere, Arc PVS lists, visibility/model/parameter caches and
numeric outputs. Resource-bearing recipes, textures, bindings and pipelines stay
in Renderer. `snapshot`/`publish` run on the game thread; only `State::run` and its
pure arithmetic callees execute on the worker. No renderer TLS, collision world,
Texture2D, material handle, GL call or profiling TLS is used by that kernel.
The pool releases its RefCell borrow before gathering. A non-Send owned job guard
drains before freeing its boxed storage, including worker and gather panics;
pool/scene replacement and skin dispatch reject an outstanding loan. Unavailable
pools fall back inline and count the fallback. Normal pool teardown drains.

`LOOKING_GLASS_WORLD_HASHES=1` on host emits both the sorted visibility-group-set
SHA-256 and an ordered hash including depth bits, selected texture frame and every
uniform bit for each opaque/alpha preparation. Hash logging is outside timing.
Same-binary off/on image/hash comparisons must pass on all 36 maps/39 entries;
advancing material clocks, camera changes, moving models and both skin worker
counts have additional host fixtures. Frozen entry images do not prove an entire
campaign playthrough. The console protocol and raw host evidence live under
`~/vita-j0/atoz/threading/step1/`.

The step-0b light schema and exact metric axes are in [COLUMNS.md](THREADING-COLUMNS.md).
The report tool rejects incomplete/duplicate schemas, partial windows and false
probe-on results. Console probe overhead target: at most 1 ms median per pair.
Host timing ranks the change; it cannot establish Vita performance or audio safety.

Numeric prepared output adds one fixed-capacity staging array and compact group
metadata, reported as `world_prepare_extra_fixed_payload_bytes` at load. Recipes
share Arc numeric payloads instead of duplicating texture-bearing parameter
recipes. Transform/atmosphere vectors retain capacity. Each asynchronous hand-off
currently allocates the owned task and closure boxes; the console memory/overhead
gates must include these. No claim of allocation-free dispatch is made.

The step-0b amendment observes existing VitaGL/GXM waits and measures scheduled
submission CPU using the kernel thread clock. It adds no synchronization points.
Read THREADING-COLUMNS.md for nested API/parent accounting and the distinction
between API elapsed, scheduled CPU and off-CPU time (which includes preemption).
These are Vita-only observations; their validity and <=1 ms probe-overhead gate
remain console acceptance requirements. The worker performance switch remains
independent of the attribution switch and stays default off.
