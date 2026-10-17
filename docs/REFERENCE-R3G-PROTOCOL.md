# r3g console protocol (console owner; the Mac package has no console qualification)

r3g is r3f plus the seven sim-maps round-two commits (through `3ef8c85`), with
`sim_triangle_candidates`, `sim_fan_candidates` and `fx_merge_local_reuse`
defaulting to true. The round-two console campaign on r3f
(`sim-maps-2-r3f/console/REPORT.md`) measured hedge3 229 -> 122 ms and tower2
133 -> 62 ms median wall with equal framing, clean audio counters and no heap
growth. That campaign exercised only standing play at entry. This protocol runs
the functional checks it skipped and times r3g against r3f on the four maps.

## Package and configs

Verify the package `SHA256SUMS.txt` and `VERIFICATION.json` first. Install by
SELF-only FTP overwrite of `ux0:/app/ALICELG01/eboot.bin`, always paired with its
renderer config as `ux0:data/alice/private/vita_renderer.json`; never use the VPK
installer. `vita_sim.json` stays `configs/vita-sim/vita-sim-all.json` (fa64b0c5)
in every arm.

- `r3g`: r3g SELF + `configs/PL-r3g-reference.json` (byte-identical to the
  console-accepted `PL-simmaps-2-on.json`, d931f360): the r3f audio candidate
  plus the three round-two booleans true.
- `r3f`: the human's r3f SELF `387ccf678251d4267b8ced53e207830358826f8e0642631a0459c6b0b9939990`
  + `PL-r3f-candidate.json` `84b4f98b1d7e98a265a668593580376dc9ea73c32cd53d60f0276f198484802a`.
  The r3f SELF rejects the r3g config (unknown keys); never cross the pairs.
- `configs/PL-r3g-round2-off.json` (= `PL-simmaps-2-off.json`) is a diagnostic
  for the r3g SELF only, to attribute a failure to the three switches. The r3g
  SELF turns them on when the keys are absent, so the explicit false keys matter.

Roll back SELF and config together. Read back and hash SELF, renderer config,
sim config and the effective `[lg/vita-renderer] config` line before each arm:
require the three round-two keys true for `r3g`, absent for `r3f`, no parser
error and no construction fallback.

## Preservation

Never MKD, promote, reboot, storagemgr or config.txt. The human has been playing
r3f, so the starting state is whatever is installed now, not run 3's hand-off.
Before the first arm, with Alice closed, back up and hash the installed SELF,
`vita_renderer.json`, `vita_sim.json`, `preferences.json`, `audio-settings.txt`,
`gpu_probe`, the route files, the whole `saves/` tree (primary saves, backups,
previews, `auto*.json`) with inventory, byte counts and SHA-256, and the
overwritable `frame_profile` CSVs. Around every arm restore that save inventory
and read it back byte-equal; generated saves (manual slots written in step 3)
move into the evidence directory and are removed from the console. Keep the
capture driver, keepalive and power watchdog untouched; nosleep stays on.

## Routes

Village: cold Load Save -> reference Autosave
`4e5dab48ede6b88f30ac80fce6fb117c3bf4bacf6e76aabd5aeec260bb188714` (restore and
hash it first) -> paused capture -> Cross resume. Never Chapters or New Game for
the village timing. Other maps: Chapters -> d-pad to the zero-based chooser
index -> touch Begin, first entry (entry=null): hedge3 32, tower2 31, skool2 6.
Screenshot after every entry, including black or stalled ones, and check the
`map_load` line against the expected map. Abort an arm when no frame-stat line
arrives for 60 s; at most one retry per arm.

## 1. Timing, 105 s per map (r3g vs r3f)

For each of gvillage, hedge3, tower2 and skool2: cold launch, enter as above,
settle 10 s after gameplay starts (village: Cross resume; others: first 60-frame
light export), then 105 s with no input, capture or FTP. Order the pairs
r3f/r3g, r3g/r3f, r3f/r3g (three pairs per map, matched camera; skool2 uses the
fixed camera of `SIM-MAPS-R2-PROTOCOL.md`). Report per arm N, median/p95/max wall,
frame_cpu, render children, swap and `fx_merge_us` from the selected light-CSV
rows. Expected from the round-two campaign (30 s windows): hedge3 ~122 ms, tower2
~62 ms, skool2 ~83 ms, village ~38.5 ms on r3g; anything slower than r3f on any
map in two of three pairs is a regression to report, not to average away.

Audio counters in every timed window: device line (44100 Hz), CPAL 44100/512,
`[lg/audio-output] buffered=true`, `[lg/audio-producer]` readback (161,
0x40000, `log_path=reporter`), `[lg/audio-music]` (170, 0x40000), producer
p95 per window (gate <=8 ms), `pcm_starved_blocks` 0, music `underrun_frames` 0,
callback max, `producer_gap_max_ms`, `producer_gaps_over_block`. Memory: newlib
used, vitaGL RAM/VRAM/PHYCONT free, sampled through loading and settle. Stop
above 180 MiB used or on any allocation failure.

## 2. Functional checks on r3g (never run in round two)

On each of hedge3, tower2, skool2 and the village, outside the timed window,
with screenshots before and after each item and the full log:

1. Walk/look: one minute of walking and turning. hedge3: hedge mesh contacts,
   riders and side pushes along walls; tower2: walk under and next to the fans,
   near and far, and any crushing/rollback; skool2: actors, particles and
   material order while moving; village: the Autosave path to the first NPCs.
   Require no fall-through, no stuck contact, no visible pop or missing actor.
2. Pause/resume: Start, wait 5 s, resume, twice. No stale audio, no input loss.
3. Death/retry: die once (fall, enemy or hazard; hedge3 and tower2 have both),
   accept retry. Require the retried scene to be controllable with HUD and
   world; record the `[lg/transition-memory]` before_drop/after_drop lines.
   If a map offers no reachable death within two minutes, record it as not run.
4. Save/load: save to a new manual slot, move away, quit to menu, cold Load
   that slot; require the saved pose, HUD and control. Then move the slot into
   the evidence directory and remove it from the console. Recheck the
   reference Autosave hash afterward.
5. One chapter transition: from the village, walk the natural exit if reachable
   in the session; otherwise Chapters -> the next chapter's first map. Record the
   transition-memory lines, entry stages and the first controllable frame.

Keep r3f as comparator only where a check fails on r3g: repeat that check once
on r3f, then once on r3g with `PL-r3g-round2-off.json`, to tell a round-two
regression from an inherited one.

## Hand-off

Close Alice. Restore the starting state found at backup: r3f SELF
`387ccf67...9990` as `eboot.bin`, `vita_renderer.json` `84b4f98b...802a`,
`vita_sim.json` `fa64b0c5...334b`, `preferences.json`, `audio-settings.txt`,
the full save inventory and the overwritten `frame_profile` CSVs, all read back
byte-equal with equal inventories (run 3's values for the settings files were
`c23e7cb8` and `d4e4c359`; the human's current files win if they differ).
Release all inputs, confirm "No-sleep enabled." and "Keep-screen enabled.",
leave the driver and keepalive running.
