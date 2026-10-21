# Passive GPU wait split

`gpu_wait_probe` defaults to false. It arms only with `profile_capture=true` and
`profiler="light"` in `ux0:data/alice/private/vita_renderer.json`. Config is read at
startup: restart between arms. No new synchronization, state change, worker,
GPU query, or readback is introduced. This ports the existing threading step-0b
`gpu_wait.rs` design from 095e4ade7a0f89b3fb0131a5c141bc5c370af0bf rather than
adding another recorder. The native-timing recorder remains unchanged: its
viewer-owned swap fields are still unavailable. Its session IDs must not be
assumed to equal light-frame IDs.

The existing light ring owns the data: a row closes at the next viewer begin,
including deferred framework draws and swap of the preceding accepted gameplay
frame. Rejected frames do not enter capture. Storage is bounded to 4,096 rows;
the new 36 u64 values add 1,179,648 bytes to an armed capture and 17,280 bytes to
the 60-row ring. No allocation, print, or file export occurs in the wrappers.
Exports occur at the inherited explicit paused/end report boundary. The ordinary
light CSV and native recorder schemas stay unchanged. A sibling
`<map>-<mode>-light-<count>-gpu.csv` contains frame, wall_us, and 36 GPU fields,
including zeros in probe-off captures. All profiles require their ordinary
capture/export controls; this switch alone does not arm profiling.

Each native wrapper checks an atomic flag first (off: no clock, syscall or probe
TLS access). When armed it checks the SceKernel game-thread ID before any Rust TLS
access, so GXM display/GC threads are ignored safely. Coarse retained submission
brackets include scale preparation, world pass state/draw submission, FX uploads
and joint-alpha draw submission. Deferred framework GL packets join this union;
nested brackets count once. Numeric visibility, sorting and actor preparation
are outside these submission brackets. The broader `gpu_pre_swap_*` bracket runs
from the existing viewer CPU start until immediately before vglSwapBuffers;
it includes simulation, render preparation, reduction and framework submission.
It excludes swap. It is an upper bound for render build/submission CPU, not a
render-only CPU number. Compare it with inherited render-phase/FX/skin counters.

Wall time uses Instant; scheduled CPU uses the existing SceKernelThreadInfo
run_clocks layout via sceKernelGetThreadInfo. Both clock reads are read-only.
`gpu_submit_nonwait_cpu_us` subtracts scheduled CPU inside the union of GXM calls
from scheduled submission CPU, never elapsed time. `gpu_pre_swap_nonwait_cpu_us`
applies the same rule across the wider pre-swap interval. Missing, changed-thread,
nonmonotonic or inconsistent CPU clocks increment gpu_clock_errors. All-zero
submission/pre-swap CPU across an active window also fails offline validation.

Observed APIs (forward original arguments and results exactly once):

- glFlush and glFinish, both direct-link and vglGetProcAddress paths. The former
  submits/resets scenes; it does not prove completion. The latter calls GXM Finish.
- vglSwapBuffers through the existing frame-phase wrapper, with inclusive wall
  and scheduled CPU. It includes queue backpressure, GC and display housekeeping.
- sceGxmFinish: wait for context rendering completion; sceGxmTransferFinish:
  transfer completion; sceGxmNotificationWait: fence/notification completion.
- sceGxmDisplayQueueAddEntry: queue submission/backpressure, including presentation
  pacing. sceGxmBeginScene and sceGxmEndScene: scene setup/submission which can
  synchronize. These scene calls include ordinary driver CPU, not just waiting.

`gpu_wait_api_*` is the non-overlapping union of the GXM calls above. Individual
API columns are inclusive; never add parent GL/swap durations to GXM children.
`gpu_wait_off_cpu_us` sums max(0, wall minus scheduled CPU) per outer GXM call.
This measures off-CPU time inside those APIs, including preemption, and is not a
hardware GPU timer. glFlush, glFinish and swap have separate parent CPU fields,
allowing the same per-row off-CPU estimate even for GC/pacing outside GXM calls.
The inherited step-0b DisplayQueueFinish fields remain reserved zeros: the pinned
archive has no reference, so this port does not introduce that import.

GPU busy time is unavailable with the pinned public API/archive. Its query API
provides occlusion samples, not elapsed-time/timestamp queries; Razor metrics
in nearby source are compile-time devkit/debug facilities, not a public busy
timer in this pinned header. Do not infer GPU busy from the wait estimates or
change the driver/build to obtain it in this experiment.

Run `python3 tools/gpu_split_report.py capture-gpu.csv --mode on` (or off).
Default population is frames 61..180, two 60-frame windows. The reducer checks
exact ordered schema, contiguous frame IDs, complete windows, per-row arming,
valid clocks, exactly one swap, and nonzero submission CPU and scene/queue hook
coverage in each window. It preserves zero-valued unused APIs and every stall.
It reports median/p95/max/mean/total, parent off-CPU estimates computed before
quantiles, and GPU busy=null. Pair with the same-frame light CSV; wall_us must
match exactly. See GPU-SPLIT-PROTOCOL.md for console-owner execution.
