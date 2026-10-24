Adopted by the orchestrator: M1–M5 and R1–R10, verbatim below, before console data; R1 uses the rule change, with no rebuild.

### Mandatory before admission

**M1. Native reload operation name.** In `console/audit.py` (and `tools/cache_qual_r8/console/audit.py`), the
same-map reload check accepts the engine's actual kind: `e['operation']=='save-reload'`
(`viewer.rs:803`). The cross-map Auto load is also `save-reload`; the chapter change is
`chapter-or-console` (`viewer.rs:2149`). Require one `save-reload` with `from!=to`, one `save-reload` with
`from==to=='pandemonium'`, and one `chapter-or-console` with `to=='hedge3'`, in that order. Change the
rehearsal fixture to emit these exact engine strings, and add a negative fixture in which the reload is missing.

**M2. Pose matching only for reproducible entries.** In `report()`, apply
`dist(first_eye A, first_eye B) <= 1` only to `hedge3-move`, `pandemonium-move`, `reload-post` and
`transition-post`. Report `hedge3-check` and `pandemonium-check` first and last eyes, and their A/B distance,
as descriptive. Make the fixture's B movement windows end at a different eye than A (for example 25 units
apart) and add a negative fixture where an entry window differs by more than one unit.

**M3. Every window starts from the inspected modal Start menu.** This supersedes the "paused overlay" wording
in brief steps 1, 3, 4 and 5. After chapter entry (the game runs unpaused, `viewer.rs:2193`) and after a native
Load (non-modal paused overlay, where circle does nothing, `viewer.rs:1784-1795`), run
`operate.py LABEL button start`, then inspect and acknowledge the Main pause menu. Only then run `move`, `check`
or `post` with `circle`, which is Back → Resume in the modal menu. Do not use cross to resume: it is Enter plus
a left click, and in gameplay it can also act on the first resumed frame. If the inspected Main menu does not
show Back/Resume on circle, stop and restore. Make the fixture model both states: circle in the non-modal
overlay resumes nothing and yields an empty window, which must fail.

**M4. Restoration only with the title closed.** Before `session.py restore`, and before any save restoration
after a stop or abort, record `pilot.cmd('appstate ALICELG01')` showing the title is not running, plus an
inspected and acknowledged HOME image. If the driver is lost or the state cannot be proven, do not write saves
or the SELF. Preserve the evidence and ask the human to close ALICELG01 from LiveArea (this kills it without the
in-game autosave). Never use in-game Quit after restoration has started. Then run `session.py restore`. Reason:
in-game Quit writes `saves/auto.json` and `auto.previous.json` (`viewer.rs:4386-4392`, `save.rs:304-311`) over
the restored human Auto.

**M5. Reseal.** Re-hash the changed helpers, rebuild `CONSOLE-PACKAGE.zip`, rerun the loose and the ZIP main-path
rehearsals to `PASS_ORIGINAL_STATE_VERIFIED` with the session path absent, and record the new hashes in
PACKAGE-VERIFICATION and FINAL-VALIDATION. Keep `CONSOLE-BRIEF.md` byte-identical (`eac84e7c…`). Engine and SELF
are unchanged.

### Recommended (decide before data; each changes a reading rule or the protocol, so seal it if adopted)

**R1. Transition absolute heap.** Make the absolute 180 MiB limit descriptive for `transition` and `load`
events, and keep the paired B-A ≤ +1 MiB rule as their gate, as r6 amendment 4 specifies. Keep 180 MiB for
gameplay rows. A crash or OOM still fails the run. Alternative without a rule change: size check and post
windows at 1,024 rows (engine rebuild, about 4.4 MiB less).

**R2. Motion feasibility before the slot.** Inspect the retained r6 entry images for the Hedge3 chapter-32
spawn and the `pandemonium$first` pose for lateral clearance of about 2 s of walking each way. If either is
doubtful, choose the axis now (for example y=64/192 at x=128) by amendment. Do not adapt after results.

**R3. Movement margin.** Add descriptive per-pulse moving time to AUDIT.json so a near-miss under 60 s can be
diagnosed (RPC gap versus wall).

**R4. Post-load self-check.** Set `check_stride=64` in `reload-post` and `transition-post` for both arms, and
allow `checks>0` there in the audit. Those windows are descriptive for timing, and they are where a surviving
stale entry would show.

**R5. Performance reading for default-on.** Report B-A `prepare_acoustics` mean and p95 for each movement
window. State now that a default-on recommendation also requires each movement-window B-A mean ≤ +0.25 ms
(observer overhead is equal in both arms). Otherwise default-on needs written human acceptance of the moving
cost against the r6 idle gain.

**R6. Paired heap noise in movement windows.** If a movement-window B-A maximum exceeds 1 MiB and the arms'
`[lg/audio-memory] event=runtime:` lines differ, classify the result as UNKNOWN (a new matched pair under a new
amendment), not as a cache heap regression. Transition and load pairs stay strict.

**R7. Settings drift per arm.** Make `session.py saves`/`stage` also hash-verify `preferences.json` and
`audio-settings.txt` against the baseline, and stop on a difference.

**R8. Chapter navigation wording.** Replace the touch reference: Select → Chapters, D-pad via
`operate.py LABEL button down N` (N ≤ 8, image after each), Cross to Begin (`menu/chapters.rs:42`).

**R9. Load-interval audio.** Before admission, confirm from r5/r6 logs that native Auto loads produced no
`pcm_starved_blocks` and no producer p95 ≥ 8 ms. If they did, decide now whether load reporter windows are
gated.

**R10. Literal final check.** Have RESTORE-FINAL assert the restored `saves/auto.json` SHA-256
`ea4cd221d9f950e613188ccf3ecf2c1eabb1d34106f413125864320d364c8fa9` and 181,900 bytes, alongside the SELF literal.

## R2 decision and retained image evidence (fixed before data)

Keep the lateral axis for BOTH hedge3 and pandemonium: x=64/192 at y=128,
two seconds per pulse, 16 left/right pairs. Do not adapt after results.
The Hedge3 chapter-32 begin image shows a broad circular platform with rails
ahead and on its sides. The Pandemonium first-visit image shows a narrow floor
between close walls and pillars, with an opening ahead. Neither single view
establishes the world distance covered by two seconds of walking, rear clearance,
or superior clearance in both directions on another axis. I cannot judge two-second
clearance from these images, so the required fallback is to retain the lateral axis
on each map. This is a visual choice, not measured clearance or console qualification.
The retained Hedge3 menu image also visibly says Start/Circle: Back to Game.

- `/Users/anezium/vita-j0/resume-2026-10-24/alice-acoustic-cache-r6/session-s7-acoustic-cache/s7-acoustic-cache-hedge3-1-A/captures/begin.png` — SHA-256 `4a7ed1fde03d27f975c10201b481e6cf20a42084bea72fe9843a4bdcbfba9d25`.
- `/Users/anezium/vita-j0/resume-2026-10-24/alice-acoustic-cache-r6/session-s7-acoustic-cache/s7-acoustic-cache-hedge3-1-A/captures/paused-overlay.png` — SHA-256 `7efa782b49f3e5adb0b587d18ea099c317496a8901eaef5c9e18ef2439fb2c86`.
- `/Users/anezium/vita-j0/resume-2026-10-24/alice-acoustic-cache-r6/session-s7-acoustic-cache/s7-acoustic-cache-pandemonium-1-A/captures/entry-pose.png` — SHA-256 `89064ff07d64a532530ba6cb20d6c528e3776cf51d32e3eedb9425e010805a0b`.

## R9 load-interval finding and gating decision (fixed before data)

Inspected every retained native Auto save_restore interval in the r5 and r6
per-run runtime logs: save_restore begin through restore_resources end, including
one audio reporter immediately before and after to cover boundary-straddling seconds.
The accompanying proof/REVIEW-EVIDENCE.json records file hashes, line ranges, and
every selected reporter. There are four r5 and five r6 native Pandemonium Auto loads.
All nine loads have zero pcm_starved_blocks and zero producer p95 >= 8 ms reporters.
The largest producer p95 is 0.401 ms in r5 and 0.620 ms in r6.
Decision: KEEP load reporter windows gated. All audio gates remain strict from the
first measured start through the final measured stop, including intervening native
Auto loads, the chapter change, and menus. Initial loading before the first start
and shutdown after the last stop remain separately descriptive. These historical
logs establish pre-admission feasibility only; they are not r8 qualification data.
