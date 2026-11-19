# Default-off overlap kill probe

Base: RC14, 37f9d06. This branch does not move simulation or renderer ownership.
`diag_overlap=false`, `diag_overlap_burn_ms=0` preserve the release configuration.
The shipping JSON is unchanged. All graphics calls remain on their owner thread.

## Boundary

The existing render interval ends at `spike_log::hud_begin`. It is not followed
by a world-free frame suffix: HUD/menu drawing, native timing, replay/probe
snapshots, input suppression, audio presentation and the optional survey still
read simulation-owned state. `world_last_read` retains the conservative boundary
at CPU-end and advances it through the remaining tail. The split is clamped to
render, so `render_world + render_free = render`, with `render_free=0` on this
unextracted implementation. This is an intentionally negative measurement, not an
assumption that actor gather finishes world extraction. A future snapshot design
must extract HUD/tail inputs too; this probe does not measure its possible benefit.
The current HUD's reads alone suffice to prove that zero render time follows the
last shared-state read. Presentation may still contain GPU/vblank work, which is
not included in `render_free` and is not claimed as safe concurrency here.

## Core reservation

There is no free application core in RC14: game CPU0, skin workers CPU1/CPU2,
CPAL callback CPU1, producer/music CPU2. Under the explicit diagnostic key only:

- exactly one skin worker is required and pinned to CPU2;
- buffered audio is required; producer/music/reporter stay on CPU2;
- CPAL first validates its original startup policy, then its first buffered-source
  callback moves itself to CPU2 and records kernel readback;
- the game remains CPU0; burn is CPU1, with kernel readback.

All three diagnostic arms use this same reservation. Therefore the control is a
split-timer/reserved-core control, not an unmodified RC14 scheduling baseline.
The host oracle additionally compares keys-off output against RC14. No performance
comparison to production scheduling or isolated memory-bus attribution is claimed.
Audio and worker crowding on CPU2 can affect the measured frame budget.

The burn owns a 2 MiB volatile memory buffer, writes one word per 64-byte line,
and checks elapsed time between full sweeps. It acknowledges start before render
continues. Rows report elapsed burn milliseconds, logical read/write bytes (not
measured bus traffic) and actual burn service overlapping the render interval.
Completion is joined after `hud_begin`, so an overlong burn waits in HUD rather
than being hidden inside render inflation. The thread is joined on viewer exit.
Values are bounded to 0..20 ms. Zero allocates no burn buffer or thread.

## Root package

The host-only handoff is under `~/vita-j0/plan-2026-11-12/a5-overlap/`.
Read its REPORT.md, README.md and READY.json. The real rc1kit is immutable.
One step per invocation, no marks after observation, mutation guard immediately
before launch, self-exiting replay must report not-running, root observes between
steps and restores once. Replay config records whose recorded value is absent are
informational. Missing timers, core readbacks, burn work or rows are failures.
The analysis drops the current overlap if control median world-free render is
below 6 ms OR the 12 ms arm inflates median render by more than 15 percent.
Only root can run the console. A build is not device evidence.
