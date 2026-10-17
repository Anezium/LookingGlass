# r3f console protocol (human execution; Mac package has no console qualification)

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

For inherited steps 1–7 below, use the r3f SELF and this package's matching
configs wherever the procedure says r3d (`configs/baseline.json` is the r3f
candidate; the single-field arms are derived from it); keep the original r3c
comparator.
The steps' transactions and observations remain unchanged.

0. Cold Load Save -> reference Autosave -> visible paused village -> resume for
   **105 seconds** of the same movement/look per arm, cold-restoring before each.
   All three arms use the same r3f SELF:
   - `candidate`: `configs/PL-r3f-candidate.json` (package `configs/candidate.json`).
     r3e baseline settings plus `audio_log_thread` (the producer never prints;
     an `audio-report` thread at priority 191 does) and `audio_music_thread`
     (music decodes on an `audio-music` thread at priority 170 on CPU2 into a
     0.74 s PCM ring). Four ready blocks, 46.44 ms prefetch.
   - `cushion-8`: `PL-r3f-cushion-8.json`, candidate plus eight ready blocks:
     92.88 ms prefetch, +46.44 ms over candidate, +69.66 ms over r3d.
   - `r3e-control`: `PL-r3f-r3e-control.json`, exactly the r3e baseline settings
     (producer prints directly, music decodes on the producer, four blocks).
   Optionally repeat the control with the r3e SELF
   (`9b61371542d7ed77f68e92fb20a0295c6fdb12a750e8d2f311b8dd7077a00b39`) and the
   r3e `configs/baseline.json`; the r3f JSON files are rejected by that SELF.
   Require in every arm the `[lg/audio-producer]` line with effective policy
   readback (priority 161, affinity 0x40000) and its `log_path` (`reporter` in
   candidate/cushion-8, `producer` in control); a missing producer is a failure.
   Require `[lg/audio-music]` startup readback (priority 170, affinity 0x40000)
   in candidate/cushion-8. Preserve per-second producer and stage p50/p95/max (music
   decode, effect decode, resampling, voice DSP/spatial/filter, summing,
   conversion/delivery, control), audible/virtual ambience counts, live-sink
   peak, game API max, lock counters, CPAL callback/deadline counters, PCM
   starvation/rejections, and stage/age, plus the new per-window fields: `producer_gap_max_ms` (longest interval between producer
   wakeups), `producer_sleep_overshoot_max_ms`, `producer_gaps_over_block`,
   `producer_log_ms` (time the producer spent printing; 0 on the reporter path),
   `dropped_reports`, and the `[lg/audio-music]` decode/underrun/ring lines.
   Reading the gap fields: a gap above one block with large `producer_log_ms`
   points at logging; a large sleep overshoot or a gap with no log time points at
   preemption. `underrun_frames>0` means music dropped out, not a late mix block.
   After the first five resumed seconds, gate **every window** on producer p95
   <=8 ms and starved blocks=0; engineering target <=5 ms. Require callback max
   well below 1 ms, no rejected queues or lost input, music `underrun_frames=0`,
   and frame median within 2 ms of r3d candidate 37.202 ms (35.202–39.202 ms),
   with no multi-second frames. Gate each arm separately; the control is expected
   to reproduce r3e's ~1 starved block/s. Never average away a bad window. On an
   unsafe stall abort that arm and restore. Do not raise producer urgency above
   the skin workers (159).

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

## Listening checklist (human, headphones, after step 0 passes)

Use the `candidate` arm (and `cushion-8` if it is the one that passes). Note the
time and a short description of any click, dropout or level jump.

1. **Ambience near the voice cap.** The village pose never engaged the cap (max 5
   audible ambient voices). Walk to a spot where `[lg/audio-voices]` reports
   `audible_ambient=8` with `virtual_ambient>0`, for example funhouse, hedge3 or
   potears1, which have 48, 75 and 65 emitters. Turn slowly in place and walk
   back and forth across the spot for one minute. Expected: loops fade in and out
   over about 12 ms; no clicks, no rapid on/off flutter between two sources.
2. **Music.** Ten minutes of village and one map change. Expected: no dropouts,
   stutter or pitch change; crossfades on mood change as before. Any audible gap
   should match a window with `underrun_frames>0`.
3. **Voices.** Stand near the village speakers playing `q2_idle.wav` and trigger a
   dialogue line. Expected: speech always audible at its distance, never cut while
   eight louder ambient loops play.
4. **Footsteps and player effects.** Walk, jump, land, swing the knife on stone,
   wood and water. Expected: crisp attacks, no softened onset, no clicks when
   moving fast past emitters (pan now ramps over each block).
5. **Pause/resume and film.** Pause and resume twice; skip the opening film once.
   Expected: no stale audio after resume or skip.
