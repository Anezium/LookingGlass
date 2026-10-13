# J13: light gameplay profiler and persistent skin worker diagnosis

Normal Vita play (`profile_mode` absent or `full`) resolves an omitted/null
`profiler` key to `light`. Diagnostic modes, including `nopreview`, `renderbench`
and routes, resolve it to `full`. Explicit `"profiler":"light"` or `"full"`
overrides that selection. Host cumulative profiling retains its existing
`LOOKING_GLASS_FRAME_PROFILE` opt-in. The Vita full profiler no longer requires
pre-creating its output directory. The packaged presets specify their level.

Light recording skips span timestamps/stack records, hot-counter reduction,
BTreeMap aggregation, heap walks, ten-frame JSON logging and periodic JSON reports.
The existing rendering and route counters that support route behavior remain.
There are three light timing regions: begin-to-next-begin wall, viewer CPU, and
`vglSwapBuffers`. The wall sample belongs to the previous accepted gameplay frame;
menus/rejected gameplay frames produce no row. `renderbench`/`fullbench` still
warm up for 60 accepted frames, freeze simulation as before, measure 120 frames,
and exit after completing the last wall interval, even with explicit light mode.
Mode polling remains once per 60 viewer frames and clock-setting mode still works.

## Light output contract

The ring contains exactly 60 fixed rows, with no growth, sorting, quantiles or JSON
construction. At the next begin after each completed 60 measured frames, it exports
one CSV under `ux0:data/alice/private/frame_profile/`:

```
<map>-<mode>-light-<frames_end>.csv
frame,wall_us,frame_cpu_us,swap_us
1,60807,51231,370
...
60,59955,50650,287
```

The example values are illustrative. All durations are integer microseconds from
`Instant`, not GPU time. `frame` counts accepted measured gameplay frames since
map/mode entry. Each file has a header plus 60 chronological rows; the first file
contains 1–60, the next 61–120. Full files remain the previous JSON format.
Incomplete final windows are not exported. Calling report again cannot duplicate
an already exported completed window. A CSV export still has a small synchronous
I/O cost, included in the following wall interval; there is no heavy reduction.
The one-line export notification is:

```
[lg/light] window mode=full frames_end=60 rows=60 file=gvillage-full-light-60.csv
```

A console parser can read the CSV with a standard CSV reader, divide microseconds
by 1000 for milliseconds, select frames 61–120 for the second gameplay window, and
compute medians/p95/untrimmed means from those raw rows. Keep export I/O visible in
wall samples. The final light bench notification ends with `profiler=light`;
full profiling retains the previous final JSON-path notification.

## Persistent play switch and ownership

`skin_workers` now defaults to 0. Setting it to 1 creates one worker at application
startup, with a 512-KiB stack and application affinity mask 0x20000. The existing
2 switch creates a second persistent worker on 0x40000. There are no actor-scoped
thread spawns, thread joins, handle vectors or heap job vectors during play.
A pthread-backed condition variable wakes each sleeping worker and signals
completion; the pinned Vita pthread OS abstraction backs blocking with kernel
semaphores. A ready acknowledgement completes pinning before dispatch. With
`worker_probe=true`, the actual persistent threads report kernel affinity,
priority and sampled CPU ids; there are no substitute startup probe threads.

The game thread builds up to 128 CPU-only surface descriptors on its stack and
lends existing mutable vertex slices until all workers complete. More than 128
surfaces falls back to the serial path. Output storage therefore remains the
original mesh arrays, with their cache/publication identity unchanged. Palette
scratch, when enabled, is filled on the game thread before dispatch and borrowed
read-only by workers. Kernel math uses only immutable inputs and disjoint output
arrays: positions, ordered triangle normals, then colors. Workers do not receive
Mesh, Texture, material, renderer or GL state. Ordered layer submission stays on
the game thread. Closures are Send; a panic drains every borrower before being
propagated, and shutdown drains pending work before joining persistent threads.

`skin_main_service_us`, `skin_worker_service_us` and `skin_join_wait_us` remain
available in full profiles. The persistent path sends all surfaces to the worker,
so main service is zero; join wait is dispatch-to-completion elapsed time, including
worker service, rather than an additional quantity to sum with service.

## Fixed-pose worker benchmark

`skin_worker_bench` defaults to false and requires `skin_workers=0`. Its separate
persistent worker is also created at startup and pinned to 0x20000. At the first
visible skinned actor, the game thread captures an owned fixed pose, weights,
indices, palette, options and preallocated vertex output once. Snapshot setup
allocations happen before measurement. All three arms repeatedly use this same
snapshot and the exact same allocation-free kernel, never live gameplay arrays.

On each subsequent unpaused frame: (a) compute on the game thread, (b) dispatch on
the persistent worker and wait, (c) dispatch on that worker and run the normal
viewer frame before joining. The normal game continues to use serial skinning.
There is no log, Box construction, vector growth, clone or scratch allocation
inside the measured kernel. The immutable snapshot stays fixed across animation
and map changes; its counts are logged when captured. An UnsafeCell owns stable
boxed storage; the game thread never inspects worker output before completion,
and benchmark shutdown waits before releasing the snapshot.

The post-join log is a space-separated key/value record:

```
[lg/skin-worker-bench] frame=1 vertices=784 fixed_pose=1 main_ns_per_vertex=... idle_ns_per_vertex=... overlap_ns_per_vertex=... main_malloc_calls=0 idle_malloc_calls=0 overlap_malloc_calls=0 idle_dispatch_wait_ns=... overlap_dispatch_to_join_ns=... overlap_join_ns=...
```

Service timings exclude dispatch/wait/logging. Additional fields measure idle
round-trip, overlap dispatch-to-join and residual join. Divide kernel nanoseconds
by the logged vertex count; Vita's underlying timer resolution still limits
precision. The allocation counter wraps Vita malloc/calloc/realloc entry calls
only while that specific kernel thread runs; allocations by the game thread
while arm (c) runs are excluded. It is inactive for normal persistent play.
Host tests instrument Rust's System allocator instead. Internal newlib allocation
entry paths bypassing these three symbols are outside this counter's coverage.
Benchmark frames include extra diagnostic work and logs and are not performance
baselines for the play A/B.

J12's math kernel contained no heap allocation. Its actor path allocated a Job
vector, a handle vector, formatted thread names and Rust/pthread scoped-thread
startup/teardown resources for six actor batches per frame. The global newlib
allocator lock and lifecycle/scheduling are plausible contributors to the measured
21.4-ms join wait. The 28.9-ms actor batch is not a direct worker arithmetic timer:
J12 measured about 4.8 ms summed worker service and 1.9 ms main service. The precise
hardware cause of the residual slowdown remains unproven until these three arms
run on Vita. No performance gain is claimed from host timings.

## Validation and package

The J13 package in `~/vita-j0/village-retained-world/jalon-13/` contains the A-stock
VPK/ELF/SELF/VELF/SFO, build/import/margin logs, locked dependency identities,
host evidence and four presets: `PL-def` (light, skin0), `PL-def-full-profiler`,
`PL-def-skin1-persistent`, and `PL-def-skin-bench`, plus `vita-sim-all`.
Host coverage includes light policy/CSV chronological bounded storage, persistent
start/reuse/stop/panic recovery, both-worker panic draining, three-arm allocation
counts and exact vertex-field parity, fixed snapshot/drain lifecycle, existing
synthetic skin comparisons and real Alice SKB/SKA parity. GPU confinement and
native FX/depth/HUD/preview regressions are checked independently. Console
placement, allocation-counter coverage, performance, I/O cost and visuals remain
unverified. This milestone involves no console or remote-server access.
