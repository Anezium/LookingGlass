# r3e console protocol (human execution; Mac package has no console qualification)

Install by SELF-only FTP overwrite of `ux0:/app/ALICELG01/eboot.bin`, always
paired with its renderer config; never use the VPK installer. Back up the full
installed state first. Never MKD, promote, reboot, storagemgr or config.txt.
Preserve save inventories and settings. Before every cold Load Save restore and
verify reference Autosave SHA-256
`4e5dab48ede6b88f30ac80fce6fb117c3bf4bacf6e76aabd5aeec260bb188714`.
All four simulation switches are true. Timing uses resumed Autosave only;
New Game and Chapters are functional checks. Keep the existing capture and
keepalive processes untouched. Console counters do not establish audible quality;
listening is a separate human check. On failure restore the exact starting SELF,
renderer/simulation configs, settings and saves, and close Alice.

0. Cold Load Save -> reference Autosave -> visible paused village -> resume for
   **105 seconds** of the same movement/look per arm. Start with
   `configs/PL-r3e-baseline.json` (package `configs/baseline.json`), then
   `worker-probe`, `no-cap`, `parameters-sample`, `no-preresample`, `cushion-2`
   and `r3d-control` using the corresponding r3e JSON files, cold-restoring before
   each. These are paired with the same r3e SELF. Each ablation changes one named
   optimization relative to baseline, except the complete r3d software control.
   Require the producer startup line and effective policy readback in every arm;
   a missing producer is a failure. Baseline: priority 161, CPU2, four ready blocks
   (46.44 ms prefetch, +23.22 ms versus r3d's two-block cushion).
   Preserve per-second producer and stage p50/p95/max (music decode, effect decode,
   resampling, voice DSP/spatial/filter, summing, conversion/delivery, control),
   audible/virtual ambience counts, live-sink peak, game API max, lock counters,
   CPAL callback/deadline counters, PCM starvation/rejections, and stage/age.
   After the first five resumed seconds, gate **every window** on producer p95
   <=8 ms and starved blocks=0; engineering target <=5 ms. Require callback max
   well below 1 ms, no rejected queues or lost input, and frame median within
   2 ms of r3d candidate 37.202 ms (35.202–39.202 ms), with no multi-second frames.
   Gate each arm separately; an intentionally disabled optimization can fail.
   Never average away a bad window. On an unsafe stall abort that arm and restore.
   The optional r3d audio-direct control previously stalled to 1,356 ms/frame:
   it is diagnostic only and must be aborted immediately if this recurs.
   Do not raise producer urgency above the skin workers (159).

1. Cold launch r3c with its original config/preferences and reference Autosave.
   Main menu -> Load Save -> select Autosave. Capture visible output before and
   after resume, with timestamps and the entire log. Repeat cold with r3d and
   `configs/baseline.json`, identical save and handheld preferences. A 30 fps log
   is insufficient: require visible world, Alice, HUD and working movement.
2. If still black, cold-repeat r3d separately with `actor-prepare-off.json`,
   `native-scale.json`, then `preview-every-frame.json`. Each differs from baseline
   by one field. Reset baseline between arms, preserve all load-diagnostic stages,
   current framebuffer/viewport/target size, GL errors, camera and world/HUD counts.
   The diagnostic does not read pixels or force a flush. Capture displayed pixels
   externally; note if an arm alone restores them. Do not infer a cause from fps.
3. Cold launch -> Select -> Chapters -> village/first. This starts the in-engine
   village entry, not an opening RoQ. Record `entry_after_story` resource markers,
   `load-diagnostic` skin-gather/world/skin-collect/actor/present stages and the first
   `scene_presented=true`/ten-frame window. Capture visible output throughout.
   Release controls, then hold Cross for 1-2 wall seconds; repeat from a cold entry
   using Triangle. Require one accepted skip, engine exit/handoff and terminal end.
   If stuck before presentation, the last completed stage matters; the skip loop
   cannot handle input until resource/render work yields. Record whether pausing
   level sinks removes the ~69 callback-overrun/s phase and first-frame stall.
4. New Game -> Easy. Measure load wall time and first film image. Release controls
   to arm, hold Cross, and repeat on another cold New Game with Triangle. Record
   actual presented fps, wall hold duration and transaction sequence: film/owner,
   binding, down/armed/held, accepted, decoder exit, soundtrack stop, handoff,
   terminal end. A 0.65 s hold is accepted at the first input poll after threshold
   (approximately 1.0/0.8/0.7 s from first down at 1/5/10 fps). No decode/audio may
   continue after acceptance. Separately skip the following in-engine village
   entry; its owner/transaction must differ from the opening RoQ.
5. Play the opening through its end with sound and without a skip. Preserve the
   movie timing summary: decode+read, codebook conversion and upload p50/p95/max,
   dropped presentations/backpressure, wall vs 124.3667 s nominal. Verify audible
   and visible synchronization, no accelerating catch-up collapse, and exactly
   one handoff into the in-engine entry. Backpressure may pause sound when decode
   cannot keep up; record stalls and total wall duration rather than hiding them.
6. In both New Game and cold Load Save, preserve every one-second `[cpal/vita]`
   callback p50/p95/max and deadline count through loading and initial play.
   Target no callback >11.610 ms; percentile resolution is 0.100 ms, max/deadline
   count is the decisive check. These include mixing+format conversion, exclude
   blocking device output, and are software estimates rather than hardware
   underrun counters. Record audible crackle independently, including low-counter
   phases. Repeat for Chapters -> village. Confirm pause/resume and level music
   start only after the scene presents, with unchanged content.
7. From visible controllable village, save to a separate manual slot, move/alter
   inventory, quit, cold Load Save that slot, resume and verify pose/inventory/HUD
   and a visible screenshot. Recheck the reference Autosave hash afterward.
8. Only if normal movie playback/skip remains unusable, try `film-bypass.json` as
   the explicit human fallback. Require a logged pre-validation bypass terminal
   marker and no RoQ setup. It does not bypass the village engine entry or repair
   restore rendering. Restore baseline afterward. Required-film route replay
   rejects this switch rather than claiming a watched film.

Stop if memory used exceeds 180 MiB, an allocation fails, or a scene loses input.
Free arena is not remaining reserved-heap capacity. Retain the full logs/visible
capture, mark each unobserved console gate pending, and restore baseline config
and the preserved reference save when finished. No host timing certifies Vita
callback cost, GL output, hardware audio behavior or the CPU1 starvation hypothesis.
