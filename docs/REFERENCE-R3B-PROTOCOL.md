# Reference r3b console handoff — pending owner execution

Mac implementation only: no console or remote-server operation was performed.
This file is the authoritative r3b protocol. Every observation below is pending.
Use A-stock/looking-glass.self (or its verified VPK) with configs/PL-def-r3.json
as ux0:data/alice/private/vita_renderer.json and configs/vita-sim-all.json as
ux0:data/alice/private/vita_sim.json. The renderer preset is exactly the full
81-field runtime default: every map retained, legacy exceptions take precedence,
actor single preparation ON, all four sim-map switches OFF. No renderer JSON is
required for these defaults; install the supplied copy for reproducible evidence.
It now uses the console-proven r2 PL-def tuning: low quality, render_scale .75,
gpu222, material FX batches, ring streaming, upload caching, one FX layer,
actor frustum culling and explicit light profiling. Every explicit r2 PL-def
field matches; CONFIG-DIFF-R2.json also checks all inherited r2 defaults.
Intentional additions: retained_maps ["*"], legacy_maps [], actor_single_prepare
true; four new simulation-cache switches stay false pending console acceptance.
Actor v1's -1.23 ms measurement used this r2 tuning; host tests are not a new
performance measurement. Diagnostic profiles must explicitly select profiler
full if they need the full profiler (the shipped preset explicitly selects light).

Verify SHA256SUMS and A-stock/SHA256SUMS.txt before deployment, then read back
SELF, renderer config, sim config and preferences. Back up the installed SELF
and BOTH configs, settings, preferences and entire saves tree with hashes first.
**SELF and config always roll back together.** Keep the exact backed-up pair:
older eboots reject newer keys because deny_unknown_fields remains enabled.
Never copy an r3 renderer JSON onto an r2/pkg-2 SELF. Preserve the approved driver,
keepalive and storage configuration; no reboot, promote, new installer or console
automation is introduced here. Use only the owner's established deployment,
readback, sampling and capture workflow in their allocated slot.

Run the complete r2 listener/control/thumbnail checks reproduced below first.
If silent, stop acceptance and preserve timestamped logs and the human report.
Then complete the cold chapter visits and natural-transition sweep below.
The sim-on candidate is deliberately not the shipped default. Only after a
separate console off/on decision may its prepared commit/config be adopted.
For that separately authorized comparison, configs/PL-def-r3-sim-on-diagnostic.json
changes ONLY the four sim-map booleans to true, using the same r3 SELF, clocks,
actor mode and renderer quality. It is a diagnostic arm, not the shipped default.
Follow ../sim-maps/PROTOCOL.md for mechanisms/traversal coverage, retaining the r3
listener and memory stop gates. Restore/read back PL-def-r3.json before the cold
chapter visits and natural-transition sweep below. A sim-on default commit still
needs a freshly built, verified SELF/config pair after acceptance.

## Cold chapter visits on r3 defaults

One cold process for EACH of the 39 chapter visits (36 maps and three returns),
in the order below from ../allmaps/PROTOCOL.md. Use r3 SELF/defaults throughout,
including the three return entries. Cross now confirms the highlighted chooser
row; verify loaded map AND entry, never assume the cursor chose correctly.
Record app launch, load request, first presentation, first alive/grounded/
noncinematic controllable gameplay, and precise/bounded timestamp status.
Expected selection is renderer=vita reason=listed; retain all earlier lines,
construction errors/counts and final construction-fallback lines. Explicit
legacy exceptions log reason=excluded; a nonlisted map logs reason=unlisted.
Fallback is a distinct outcome, never retained acceptance.
Settle 10 seconds then retain 30 seconds of neutral advancing play with all
light frame CSV blocks. No FTP/capture overlap. Report median/p95/N, only actual
instrumented fields, and missingness. Capture and walk/look after the interval;
inspect alpha, water, ropes, frozen scenery, sky, mirrors/portals and fog.
Retain timestamped newlib used/free-arena/arena and vitaGL RAM/VRAM/PHYCONT free
from launch through load and settled play, including extrema and load spikes.
Compare only matching map+entry and matching configs; retain absent baseline
measurements. At most one retry; no frame progress for 60 seconds is a hang.
Retain every failure/core, including death after the measured interval.

## Natural-transition memory sweep — mandatory separate gate

Cold visits do not test scene overlap: the destination scene/cast is constructed
while the outgoing scene is alive. Host inventories are partial buffer counts
and cannot establish a console transition high-water bound.

1. Stay in one normally advancing gameplay process for at least FIVE real level
   exits. One MUST be utemple -> potears1; the owner may start from a backed-up
   legitimate near-exit save, then play the exit normally. Choose at least four
   further reachable real exits and record actual source/destination map+entry.
   Chapter chooser loads, console map commands, replay/fixed dt and cold launches
   do not count as natural exits. Never force progression or weaken gameplay.
2. Preserve exactly one `[lg/transition-memory]` JSON line for EVERY completed
   natural exit (`kind=natural-exit`), chapter/console replacement and save reload
   (`kind=save-reload`, including retry). Each line has Unix `time_ms`, `from`,
   `to`, and **both** `before_drop` and `after_drop` samples. Each sample reports
   byte counts: mallinfo `heap_used`/`heap_free`/`heap_arena`, vitaGL
   `vgl_ram_free`/`vgl_vram_free`/`vgl_phycont_free`. The **before_drop sample is
   the built-before-drop overlap peak**: the destination is fully built and the
   outgoing scene is still alive. `after_drop` is taken immediately after the
   outgoing scene destructor; other outgoing resources may still be alive.
   Sampling and the single log emission exclude that whole transition frame
   from gameplay CPU/wall profiling. Do not include these frames in timings.
   Read both samples into TRANSITIONS.json; stop on either heap_used >180 MiB.
   A missing line/field leaves the memory gate pending. Host nulls are expected
   and cannot replace console readings. Keep the PrincessLog MQ memory logger
   running through exit activation, construction, presentation and 30 seconds
   settled afterwards, preserving audio-memory/startup/selection lines too.
   The before-drop sample resolves the reviewed scene-overlap gap; intermediate
   allocator construction spikes still require the established external samples.
3. STOP the sweep immediately if ANY newlib used sample exceeds 180 MiB
   (188743680 bytes), or on allocation failure, crash, hang, persistent memory
   growth or loss of control. Preserve logs/core and the exact last save; do not
   attempt the next transition. Missing used/free or vitaGL readings leave the
   memory gate pending; no hardware fit claim follows from host validation.
4. Also perform one save/reload per chapter GROUP, using a backed-up test slot.
   Cover: village/pandemonium/fortress (indices 0-4), school (5-7), pool/utemple
   (8-11), garden (12-15), centipede (16-17), forest/chess (18-21), funhouse/hatter
   (22-24), Jabberwock/forest return (25-27), hedge/tower (28-33), queen grounds/
   facade/keep/lair (34-38). Ten groups, minimum ten save/reloads. Record map,
   entry, slot, writer/readback identity, pose/toy, preview and post-reload
   progression/mechanism state. Sample the same memory fields before, through
   reload construction and 30 seconds after. These reloads are additional to
   the five natural exits, and do not count towards that exit minimum.
5. For each transition/reload retain a row in TRANSITIONS.json with run ID,
   source/destination map+entry, natural-exit or save-reload, chapter group, slot
   when applicable, wall/app/frame timestamps, sampling cadence, all raw sample
   paths, newlib peak used/min free and vitaGL minima with timestamps, observed
   overlap flag, control/renderer/fallback/audio outcome and stop reason. Retain
   every raw row; report postsettle growth separately from construction peak.
   Acceptance requires >=5 actual exits including utemple -> potears1, all ten
   group reloads, observed transition samples, no >180 MiB used sample and no
   crash/OOM/hang. Missing before-drop samples leave the overlap gate pending; external sparse
   samples can still miss intermediate construction spikes.

## Cold visit order (chooser index zero based)
 (chooser index is zero-based story order)

| Order | Map | Entry | Chooser index | Legacy-pass record |
|---:|---|---|---:|---|
| 1 | centipede1 | first | 16 | 29-centipede1 |
| 2 | potears3 | first | 10 | 13-potears3 |
| 3 | tower2 | first | 31 | 31-tower2 |
| 4 | garden3 | first | 14 | 34-garden3 |
| 5 | garden4 | first | 15 | 28-garden4 |
| 6 | utemple | first | 11 | 02-utemple |
| 7 | potears1 | first | 8 | 19-potears1 |
| 8 | hedge3 | first | 32 | 03-hedge3 |
| 9 | funhouse | first | 22 | 06-funhouse |
| 10 | hatter1 | first | 23 | 04-hatter1 |
| 11 | gvillage | first | 0 | 01-gvillage |
| 12 | wchess1 | first | 19 | 05-wchess1 |
| 13 | jlair1 | first | 25 | 07-jlair1 |
| 14 | wchess2 | first | 20 | 08-wchess2 |
| 15 | qlair | first | 38 | 09-qlair |
| 16 | jlair2 | first | 26 | 10-jlair2 |
| 17 | hatter2 | first | 24 | 11-hatter2 |
| 18 | centipede2 | first | 17 | 12-centipede2 |
| 19 | fortress2 | first | 3 | 14-fortress2 |
| 20 | skool1 | first | 5 | 15-skool1 |
| 21 | skool2 | first | 6 | 16-skool2 |
| 22 | wforest | first | 18 | 17-wforest |
| 23 | hedge2 | first | 30 | 18-hedge2 |
| 24 | rchess1 | first | 21 | 20-rchess1 |
| 25 | pandemonium | first | 1 | 21-pandemonium |
| 26 | keep | first | 37 | 22-keep |
| 27 | facade | first | 36 | 23-facade |
| 28 | potears2 | first | 9 | 24-potears2 |
| 29 | garden2 | first | 13 | 25-garden2 |
| 30 | fortress1 | first | 2 | 26-fortress1 |
| 31 | garden1 | first | 12 | 27-garden1 |
| 32 | tower3 | first | 33 | 30-tower3 |
| 33 | hedge1 | first | 28 | 32-hedge1 |
| 34 | grounds2 | first | 35 | 33-grounds2 |
| 35 | tower1 | first | 29 | 35-tower1 |
| 36 | grounds1 | first | 34 | 36-grounds1 |
| 37 | fortress1 | fortress1_start2 | 4 | 37-fortress1-return |
| 38 | skool1 | skool1_start2 | 7 | 38-skool1-return |
| 39 | wforest | wforest_start2 | 27 | 39-wforest-return |


## Reference r2 checks carried forward verbatim

# Playable reference console protocol — owner only

No console operation has been performed by the implementation track. Run only in
the orchestrator's console slot. All results below are pending human execution.

1. Verify `A-stock/SHA256SUMS.txt`, `A-stock/VERIFICATION.json` and source revision.
   Back up installed SELF, PL-def, vita-sim-all, preferences, audio settings and
   the complete save inventory, with hashes and byte copies. Keep game archives,
   SFO, installer, keepalive and capture driver untouched. Use the established
   overwrite/readback deployment; no promote, reboot or storage configuration.
2. Install/read back the supplied SELF. Install `configs/PL-def-r3.json` and
   `configs/vita-sim-all.json` at their existing runtime destinations. Preserve
   absence of profile_mode. Keep max clocks and all baseline worker/renderer
   options. Back up/remove untouched or custom preferences only if necessary to
   activate the documented handheld preset; alternatively copy
   `configs/preferences-vita.json` as runtime `preferences.json` after backup.
   Never lose the user's custom file.
   Audio must be enabled/unmuted, music/effects nonzero. Record startup audio
   device/rate/channels and port/error lines. Human listens to menu music, Alice
   and NPC voices, positional effects (left/right, near/far), attacks and ambience.
   Exercise pause/resume and repeated scene/menu swaps; log pops, dropouts,
   latency, clipping or missing cues. An output device line alone is not audible proof.
3. In the village exercise both sticks, R primary, L alternate (including held
   attacks), Cross jump/climb/swim up, Square use, Circle descend and menu
   back, Triangle retry/skip, Cross confirm, D-pad left/right every available toy (all ten
   in a suitable stocked save), Up hint, Down inventory, Select chapters, Start
   pause/menu/resume. Verify menu-held attacks cannot leak into resumed play.
   Swim/climb, all toys and cinematic skip need explicit observations; record
   unavailable actions as uncovered, not passed.
4. Let an autosave occur, manually save to a backed-up slot, overwrite it, then
   return to menu and Continue. Also load the manual slot. Confirm thumbnails
   are upright, current, match their corresponding position/toy and contain no
   HUD/menu; check backups if a write/load error occurs. Change position between
   saves. Confirm save preview capture is absent during ordinary play across
   frames 120 and 240 and remains present at save boundaries. Retain all stalls;
   never infer zero captures from missing counters (this build has no J24 probe).
   Source/unit evidence establishes removal of periodic requests; use continuous frame traces. If span counts are needed, run a separate
   diagnostic with `profiler: full` and otherwise identical config, then restore
   PL-def; never use that diagnostic for the light timing comparison.
5. Performance: same village Autosave input SHA-256
   `4e5dab48ede6b88f30ac80fce6fb117c3bf4bacf6e76aabd5aeec260bb188714`,
   PL-def and vita-sim-all, normal advancing play with neutral input. Three clean
   launches, retain every frame 61–180 from each (120 contiguous unique frames,
   360 pooled). No bench/route/replay/fixed dt, FTP, capture or keepalive overlap
   in retained windows; reject and repeat the entire launch on interference.
   Record per-launch and pooled wall median/p95/mean, CPU, swap and memory
   (p95 = sorted[floor(0.95*n)], as in J24),
   all raw rows and load/clock/power conditions. Compare against archived pkg-2
   36.63 ms median / 41.77 ms p95 with its session limitations stated. If the owner's launcher supports `--no-audio`, isolate audio cost with three
   additional alternating audio-on/no-audio pairs on this same reference binary,
   same save/config and preview policy; retain all rows. Otherwise report direct
   isolation unavailable. Muting still runs the mixer and is not a no-audio control.
   Cross-build delta alone includes the new preview policy and is not pure audio cost.
6. Five-minute normal advancing-play soak with movement, combat, music, voices,
   menu/pause, save/load and scene swaps. Record crash/audio/heap growth and actual
   duration. Sample newlib and vitaGL memory outside timing windows.
7. Movie regression gate before claiming whole-game playability: full opening
   and ending with human-audible sound, skip and return to menu/game, memory and
   A/V synchronization. Non-village progression/fit/performance remain unproved.
8. Close the app, release controls, restore the orchestrator's agreed installed
   reference and exact config/save/settings backups. Read back and hash restored
   bytes; leave console/keepalive state according to the established handoff.

Acceptance requires human audio and every binding, correct save/Continue previews,
three clean timing launches, no periodic-play capture and a five-minute soak.
No console acceptance or performance improvement is claimed by the host package.


## R2 review-fix gates (mandatory additions)

If the console is silent, record and stop. Preserve startup/error/frame logs and
record the human's silence observation with a timestamp. Do not continue the
performance or soak acceptance, and do not infer audibility from device success.

### Audio worker diagnostic, outside every timing window

Back up the exact active PL-def file; make a diagnostic copy with
`worker_probe: true` (keep all other fields identical, especially worker count,
overlap, affinity and max clocks). Launch once, unmuted. Retain the complete
`[cpal/vita]` startup line: BGM, 44100 Hz, 512 frames (11.610 ms), priority 128,
affinity 0x20000 (CPU1), 1048576 requested stack bytes, 32768 stereo volume,
thread ID, info_rc, effective priority/affinity/stack size. It is a kernel
sceKernelGetThreadInfo readback, not merely requested policy. Confirm skin workers
remain priority 159 / CPU1+CPU2 and the overlapping game thread remains on CPU0.
Audio must have higher priority (smaller number) than workers and exclude CPU0.
Restore and read back the original PL-def before measured launches. Never time
worker_probe: the worker inventory deliberately samples/burns 8 ms.

`late_callbacks` counts callback-start intervals beyond one block + 1 ms;
`underrun_estimates` counts callback/conversion work exceeding one block. These
are software deadline estimates, not hardware queue/underrun readbacks. Nonzero
windows log once per second with time_ms (Unix wall clock) and elapsed_ms/window_ms. Retain every line and
correlate it with human observations; a zero/missing counter is not audible proof.

### Input regressions, explicit observations

1. In normal play note the equipped toy, press Down once: inventory opens and
   the toy stays unchanged. Release Down; navigate toys with Left/Right or Up. Down toggles the inventory
   closed; Circle also returns. Reopen and repeat. Place the pointer over a different toy; Cross must keep the D-pad-selected
   toy and close inventory without jumping or attacking through the overlay.
2. Open Select chapter chooser. Press/release Down repeatedly: the highlighted
   chapter moves, inventory never opens. Put the virtual cursor over a different
   chapter using the established shot-driver pointer positioning (or touch a row,
   then move the highlight using D-pad). Cross must load the highlighted chapter,
   never the stale cursor row, including repeated/released Cross edges. Exercise
   the main-menu Chapters entry too. Touch must still choose its own row.
3. Cross jumps/climbs/swims up, Square uses, Triangle retries/skips, Circle
   descends/backs out. Check every documented action prompt shows Vita names.
   In a safe village passage and then on a narrow ledge, use partial stick
   deflection for walking and full deflection for running. Release/recenter
   after each menu/load: held stick must not move Alice before recentering.

### Thumbnail checks with the existing shot driver

Use the orchestrator's established shot-driver commands; preserve driver and
keepalive identities and serialize captures with other console traffic. No new
console tool or installer is part of this package.

1. Load the backed-up village Autosave. Record Alice's position, facing, toy,
   save slot and current log/frame timestamp; take an ordinary gameplay shot.
2. Move to a visibly distinct safe pose A, release controls, save to a backed-up
   manual slot through Start. Record the save-boundary frame/log timestamp.
3. Open Load/Save, highlight that slot and take a shot-driver screenshot after
   the thumbnail settles. Archive the full screenshot and the thumbnail crop.
   Check upright orientation, correct pose A/toy, no HUD/menu baked into preview.
4. Move to distinct pose B and overwrite the same slot; repeat screenshots and
   compare visibly with A. Reopen menu, Continue, then load the manual slot;
   take gameplay screenshots and verify pose B and the corresponding thumbnail.
5. Trigger an autosave at distinct pose C and repeat the Autosave highlight /
   screenshot / Continue sequence. Keep auto.previous.json and manual backups
   intact. A diagnostic full-profiler launch can correlate captures to saves;
   restore light PL-def before performance measurements.

### Heap/cache and load latency versus pkg-2

For each clean audio-enabled launch retain timestamped app launch, first usable
menu, village load request and first controllable village frame. Report menu
load and village load intervals separately, with exact start/end definitions.
Compare village with pkg-2's archived 49.7 s, including session/environment
limitations; collect matching pkg-2 menu timing in the owner's comparison slot
if the archive has no menu baseline, otherwise mark it unavailable.

Retain every `[lg/audio-memory]` preload/runtime growth line and the final
`event=level:gvillage` line after the village loads, including cache_samples,
cache_bytes, cap_bytes=25165824, newlib heap_used, heap_free_arena and heap_arena.
Record menu heap and village post-load heap with timestamps; compare with pkg-2
under identical save/config conditions, outside timing windows. Music/voices
stay compressed and decode incrementally; the 24 MiB cap covers retained mono
PCM effects only, not total audio/decoder/transient/active-loop/retiring-sink allocations.
Level emitters and core player cues preload; unrelated global effects load on
actual requests through the same bounded cache, with transient playback at the cap.
Record cache-limit failures, missing cues and peak transition memory explicitly.
The host cannot provide Vita mallinfo or validate console load latency.

### Timestamped human listener sheet

Use one sheet per launch. Synchronize the listener's wall clock with capture/log
receive timestamps before starting; record that offset and estimated uncertainty.
Retain raw frame logs continuously. Record elapsed app time and the nearest
frame/log timestamp for every cue, pop, dropout and menu/scene transition. Do not
invent subsecond precision if the listener cannot distinguish it.

| UTC timestamp | App elapsed / frame | Scene / action / expected cue | Heard (yes/no) | Pop/dropout/latency and duration | Nearby CPAL deadline line |
|---|---|---|---|---|---|
| pending | pending | Menu music | pending | pending | pending |
| pending | pending | Village music / voice / positional effect | pending | pending | pending |
| pending | pending | Pause/resume / save / chapter transition | pending | pending | pending |

All r1 audible, save/Continue, three-launch frames 61–180, five-minute soak and
opening/ending gates still apply. Unknown/unavailable results remain pending.
