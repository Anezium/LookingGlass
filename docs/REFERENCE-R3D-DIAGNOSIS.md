# Restore and village entry diagnostics

The r3c human session establishes a black Load Save result despite 310 submitted
frames. It does not establish the cause. The saved reference Autosave has no
active village entry/fade. Restoring paused should still draw world and HUD.
No restore semantics or renderer defaults are changed on that assumption.

The additional Chapters -> village evidence (princesslog.txt lines 5542-5994)
selects a fresh initial entry, not an opening RoQ. The front end explicitly sets
opening_requested=false for Chapters. After `Story gvillage: 8 conversations`,
the initial viewer loads Alice, aim pointer and village LevelArt. The log contains
Alice, more texture uploads, then the 720x408 scale target and opaque world depth
marker. It therefore reaches the first world rendering path, rather than stopping
inside film validation. No ten-frame window follows in the supplied interval.
The village entry is an in-engine cinematic and uses the same wall-time skip
action as RoQ, including the film-only handheld Cross alias.

The audio callback runs on CPU1 at priority 128; skin worker zero also uses CPU1
and waits are synchronous. About 69-73 over-budget callbacks per second can
starve lower-priority skin work. This is a source-supported starvation hypothesis,
not proof of where the main thread stopped. Creating music/ambient sinks paused,
gating their updates until an actual viewer yield, and caching filter coefficients
remove that known loading DSP from New Game, restore and Chapters alike.

The 115,768-byte free-arena reading is unused space inside that malloc arena,
not all remaining capacity of the reserved 192 MiB heap. Later readings grow the
arena and retain about 96-97 MiB used. It does not prove an allocation failure.
Do not treat it as an OOM diagnosis.

One-shot `[lg/load-diagnostic]` markers bracket first camera preparation, skin
gather, world submission, worker collection, actor work, upscale and HUD queuing.
They report current framebuffer binding/completeness, viewport, existing/new GL
errors, allocated scale target dimensions, camera pose, actual world draw/triangle
counts and whether HUD draw calls were queued. Queries do not flush, read pixels,
bind targets or repair state. A final `frame-presented` sample follows the actual
frame yield; neither queued draws nor swap completion prove visible pixels.
`entry_after_story` and `entered_resources` split the formerly uninstrumented
resource phase. `[lg/audio-load] scene_presented=true` follows the real frame yield.

Use `configs/PL-r3d-baseline.json` first, then separate `PL-r3d-actor-prepare-off.json`,
`PL-r3d-native-scale.json`, and `PL-r3d-preview-every-frame.json` arms if the image stays black.
Each arm changes exactly its named field. Capture the displayed output and retain
all stage markers; the last completed stage localizes any remaining first-frame
stall. The exact menu interaction, Vita GL state and priority behavior require
console observation. A host successful restore cannot certify the console image.

## Resume collapse takes precedence over film hypotheses

The threading step-0 report and rejected village log establish a visible paused
Autosave restore, then a 4361.08 ms/frame block after resume (update 1.307 ms,
draw/application coroutine 4359.479 ms, swap 0.233 ms), followed by no further
frame window for 100 seconds. Audio warning windows cluster around 70/70 late /
over-budget estimates per second. This is normal village audio/play, independent
of RoQ and the front-end path. The CPU run-clock deltas that exceed their wall
windows cannot be used as a valid CPU utilization estimate. The old pkg-2 timing
predates enabled Vita audio and is not an audio-on performance baseline.

Inspection finds short pending-source and sink-control mutex sections in rodio;
the dynamic mixer does not hold its pending-source mutex while calling source
next/DSP. The host lock probe instruments both thread roles. A decoder blocked
inside next still permits game volume/pause/position calls to finish. This refutes
a callback-wide mixer-lock explanation for that host case; it does not locate
the Vita main thread. CPU1 callback saturation at priority 128 starving skin
worker zero (CPU1, priority 159), followed by the main thread waiting in worker
collection, remains the strongest scheduling explanation. Console stage readback
is needed to distinguish it from another main-thread wait.

r3d removes both hazards from its explicit buffered candidate. Game sink controls
are atomics and bounded single-producer queues: no volume/position/append mutex,
blocking source queue, synchronous seek or sleep-until-end is exposed. Mixing,
resampling, cached ambient DSP and MP3 decoding run on one persistent producer
(priority 161, CPU2, 1 MiB stack; policy read back and logged). The high-priority
CPAL callback consumes fixed PCM blocks, does at most four stale-block drops,
and substitutes silence if no block is ready. It never calls a decoder, filter,
game control, source destructor or capture mutex. Two ready blocks plus the
current device block bound normal software prefetch to about 34.83 ms. Film time
subtracts queued prefetch; a skip invalidates prefetched film blocks atomically.

`audio_pcm_worker` remains false in implicit/canonical defaults. All packaged
r3d candidate presets explicitly enable it. `PL-r3d-audio-direct.json` is a separate
single-field control arm; it retains the lock-free controls and cached DSP but
executes the mixer directly. This separates callback isolation from the other
fixes and provides an explicit rollback without changing simulation policy.

Per-second `[cpal/vita]` p50/p95/max measure callback+format conversion.
`[lg/audio-realtime]` reports producer mix percentiles, PCM starvation, rejected
queues, intentional film flushes, live-sink peak and game API wall-time max.
`game_audio_lock_attempts=0` / `game_audio_lock_wait_max_us=0` are structural zeros
because the new control path takes no locks, not a fabricated pthread timing.
The old-host probe separately measures real lock hold/wait maxima on both sides.
Game-stage IDs are 1=audio update, 2=skin collect, 3=skin gather/dispatch,
4=audio clip preparation, 5=level audio load; age is wall time in that stage.
A callback under budget does not certify clean sound: steady PCM starvation must
also be zero. Cortex-A9 producer throughput, actual listening, visible resumed
play and the precise original Vita wait site remain console gates.
