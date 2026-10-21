# Console-owner protocol (not executed by the implementer)

Use only the artifact whose full SHA-256 matches OUT/NATIVE-PACKAGE-SHA256.json.
This is an attribution candidate, not a promoted reference. Back up/read back
and hash the actual session-start SELF/SFO, renderer/simulation/audio/preferences
configs, both autosaves, save inventory and any profiler files before staging.
Preserve the human's current state even if different from historical saves.
Restore every overwritten input/output and verify byte equality at handoff,
including failure paths. Do not push, promote, change storage/kernel settings,
or create console directories. Existing input/log/capture services stay fixed.

Use the same SELF and baseline renderer/sim/audio settings in both arms. Set
profiler="light", profile_capture=true, worker_probe=false,
render_draw_timing=false, frame_phase_probe=false, depth_sentinel=false. Only
`gpu_wait_probe` false/true differs; switch via JSON and restart Alice. Keep the
same clocks (record actual ARM/bus/GPU/xbar), resolution, render scale, MSAA,
quality, simulation switches, skin workers, audio device, producer/music placement
and log routing. Disable native_timing session capture in both arms to avoid a
second observer. Do not add a flush, finish, fence, query or screenshot to the
selected windows. Off/on complete configs can be prepared by merging the single
boolean into the owner's actual baseline; do not replace unrelated settings
with runtime defaults.

Rehearse each map outside timing, capture its untouched standing pose/landmarks,
and record feet/eye/yaw/pitch when available. Release all controls before entry;
no movement, recentering, shooting, looking or menus inside a selected window.
Use matched first/third person state. For chapter entries, fresh launch ->
Chapters (Select), d-pad to zero-based index, screenshot checked chapter name,
touch Begin, first entry (entry=null). For save entries use Load Save -> touch
Autosave, verify paused loaded map, then Cross to resume. Never New Game.

Maps:

- hedge3: Chapters 32, first entry. Prior median ~122 ms (context, not a new
  measurement). GPU-side hypothesis: finish/notification/queue or parent flush
  off-CPU explains much of its excess above 33.3 ms while nonwait CPU is small.
  CPU hypothesis: scheduled submission or pre-swap nonwait CPU dominates with
  little measured GXM/parent off-CPU. Attribute CPU further with world/FX phases.
- skool2: Chapters 6, skool2_start1. Authored feet (-64,-3440,-24), initial eye
  (-64,-3440,24), yaw 90, pitch 0; use actual settled pose and matched landmarks.
  Prior ~84 ms. Apply the same wait-vs-CPU test, paying attention to actor/FX
  upload and draw counts. Reject a pair with mismatched headings.
- hatter1: Chapters 23, first entry. Prior ~69-82 ms. Test whether the historical
  ~20-28 ms unexplained part appears in completion/scene/flush off-CPU; do not
  subtract that old estimate from current measurements. High scheduled submit
  CPU with low wait instead points to CPU draw/material work.
- village (gvillage): load the agreed reference Autosave only, never Chapters.
  Record its full input hash and preserve its exact bytes through the campaign;
  use an external backup for restoration if the normal game writes on exit.
  Prior ~38 ms. This control distinguishes probe/presentation overhead from a
  slow-map effect: dominant queue/swap off-CPU can be pacing; dominant nonwait
  CPU with little wait points toward CPU work near the 33.3-ms target.
- Pandemonium (pandemonium): load its agreed Autosave and verify the map; do not
  overwrite the village input with an unrecorded save. Back up/restore each slot
  byte-equal. Prior ~47 ms. Apply the same split; scene/FX off-CPU with small
  submission CPU supports GPU-side limitation, high submission/preparation CPU
  with little wait supports CPU limitation. Queue-only waiting needs pacing
  diagnosis before calling the GPU fill-bound.

Capture from launch without per-frame writes. Remain neutral through at least
frame 240 (capture capacity 4096). Frames 1..60 are warmup; PRIMARY frames 61..180
are two exact windows, 61..120 and 121..180. Retain frames 181..240 as a separate
stability window, never silently substitute them. Pause after frame 240 to
trigger the inherited explicit report, then close Alice to collect/export if
needed. No capture, transfers, memory sampling or input during 61..240. Record
images and sampled memory/audio state outside these windows. A switch-on capture
needs nonzero submission CPU, BeginScene/EndScene and queue counts, exactly one
swap per row, probe_frames=1 and clock_errors=0. Off fields must all be zero.
Reject missing/partial/mismatched populations; retain failures and their logs.
If the export boundary fails, retain logs rather than inventing a CSV.

First qualify instrumentation with three alternating village pairs ON/OFF,
OFF/ON, ON/OFF. Compare wall/frame CPU median/p95/max and identical framing/draw
counts. If any pair's on-minus-off median exceeds 1 ms on wall or frame CPU,
stop attribution and reduce probe cost first. Then run one OFF/ON pair on each
other map; repeat order-reversed if a result is marginal, mixed or inconsistent.
Keep every outlier. No frame-stat progress for 60 s, crash, OOM, newlib >180 MiB,
new growth, broken pose or retained-window audio starvation rejects the arm;
record last timestamp/log/screenshot, allow at most one retry of entry.

Archive each raw *-light-N.csv, *-light-N-gpu.csv, logs, config/SELF/save hashes,
initial/final images and selected IDs per launch. GPU and light wall_us must
match for every joined frame. Run the reducer with --first 61 --frames 120,
and separately --first 181 --frames 60, --mode on/off. Report per-map paired
wall/frame CPU changes plus submission wall/CPU/nonwait CPU, pre-swap wall/CPU/
nonwait CPU, GXM API union wall/CPU/off-CPU, each API elapsed/count, parent GL/
swap CPU and off-CPU estimates, clock reads/errors, scope counts and hook coverage.
Never add overlapping columns or subtract independently computed medians/p95s.

Interpretation is a hypothesis gate, not a hardware-busy percentage: large
completion/notification waits with small scheduled submission CPU support GPU
backpressure; large nonwait submission CPU supports CPU submission limitation;
large pre-swap CPU but modest submission CPU points to preparation/sim/other
CPU; both large means mixed. Queue/swap-only off-CPU can be pacing or GC. Little
wait does not rule out asynchronous GPU saturation; large off-CPU can include
scheduler preemption. Busy time remains unavailable. An independent scaling
experiment may be needed later; no render setting changes belong in these pairs.

Estimated time (assumed): 3 village pairs plus 4 map pairs = 14 launches,
roughly 25 minutes with 30-100 s loads, 10-30 s neutral captures and normal
entry/export handling, plus 10-20 minutes for backup/restore/readback = 35-45
minutes. Budget 45-60 minutes with rehearsal or slow loads; order-reversed extra
pairs add roughly 3-5 minutes per map. No device time has been spent here.
