# r3i console protocol (console owner only)

This build has Mac qualification only. The implementation run must never contact
192.168.1.23. Console measurements, backups and installation below are pending
work for the console owner; they are not evidence that the package was installed.
No MKD, VPK installer, promote, reboot, storagemgr, config.txt or asset transfer.
Keep the capture driver, keepalive, nosleep and power watchdog untouched.

## Identity and preservation

Verify package/SHA256SUMS.txt and VERIFICATION.json. Pair r3i SELF with
configs/r3i-reference.json (profiler off, profile_capture false) for human play.
configs/r3i-capture.json explicitly arms the light capture. r3f comparator is the
human SELF 387ccf678251d4267b8ced53e207830358826f8e0642631a0459c6b0b9939990
and its unchanged r3f-candidate.json; the r3f SELF rejects the new config keys.
Keep vita_sim.json equal to configs/vita-sim-all.json throughout.

With Alice closed, back up and hash the actual installed eboot.bin, renderer/sim
configs, preferences.json, audio-settings.txt, gpu_probe, profile_mode, route
files, frame_profile files and the entire saves/ tree (primary, backups, previews,
auto*.json), recording relative paths, byte counts and SHA-256. Current human
files win over historical reference values. Read the backup inventory back.
Do not create directories on the console. If a required directory is absent,
stop and report it. Write evidence and backup folders on the Mac only.

Install by SELF-only overwrite of ux0:/app/ALICELG01/eboot.bin and paired config
overwrite of ux0:data/alice/private/vita_renderer.json with Alice closed. Read
back and hash both and the sim config, then check the effective renderer config
line: profiler, profile_capture, attached_puff_batches, all round-two switches,
audio settings; reject parser or construction fallback. Roll back SELF/config
together. Restore the starting save inventory between arms; move generated saves
to Mac evidence and remove only those generated console files.

## Periodic hitch, 105 seconds per arm

Use the same safe standing pandemonium view as r3f, or a cold reference village
Autosave route if that view cannot be restored. Never substitute Chapters for the
village save. Run three matched r3f/r3i pairs, alternating order. Use r3i-capture
for r3i (profiler light, profile_capture true); cold launch resets the buffer.
Settle 10 s, then retain 105 s with no input, captures or FTP; press Start only
after the window to export. The game keeps the first 4096 accepted rows in
bounded RAM and writes once at the pause/end boundary. There is no periodic CSV
write or per-export notice. Reject truncated captures (4096 rows), retries or
load intervals in the measured window. CSV wall_us is begin-to-begin; use the
recorded settle/window boundaries and report N, median, p95, max and all spikes
above 100 ms. Compare the six-ten-frame-block cadence with r3f, whose live
ordinary/spike-block medians were 37.13/79.755 ms (not individual frame maxima).
Do not claim the historical inferred ~426 ms excess as a measured max.
Use existing individual r3f CSVs if available for p95/max; otherwise label r3f
summary statistics as ten-frame averages. Keep r3f synchronous exports enabled
as the historical comparator. An r3i repeat with r3i-reference must create no
new light CSVs; ordinary miniquad logging stays unchanged.

## Pickup and transition

Outside standing windows, collect a sanity shard (p_h1/p_h2 family): screenshot
before, burst, tail, expiry; record item identity and clock boundaries. Collect
first-use and repeated pickups and two overlapping pickups. Use a short bounded
r3i-capture run, pause after expiry, report individual frame p95/max and producer,
merge, upload CPU/GL/bytes while the 75 sprites are live, with comparable before
and tail windows. Require no runtime audio-memory load for any preloaded pickup
cue, no missing cue, no ordering/depth/expiry change. Compare r3f on the same save
and camera. Host CPU improvements do not establish a console speedup or reduced
overdraw; image parity is not a console screenshot comparison.

Walk the real village -> pandemonium exit once. Capture the transition indicator
while loading (the last gameplay frame must be replaced by Loading/Pandemonium),
then the first controllable destination frame. It is a static presented overlay,
not animated progress. Record wall time to first control, resources_audio_ms,
campaign_map_load, scene_build, entered_resources and level_art stage lines,
including asset_read_inclusive/material_parse_inclusive counters. Check audio
pauses during the indicator and resumes only after destination presentation.
The known r3f 19.625 s entered_resources included scene 7.448, NPCs 2.811,
steam .886 and level art 8.463 s; do not add scene time to that total again.
Material parsing is now shared per corpus; texture I/O/upload, skeletal model
loading, scene geometry and level-specific cast construction still remain.

## Camera, audio and functional checks

Follow CAMERA-STICK/PROTOCOL.md. At pad_sensitivity 1, right stick fully horizontal
for 5 s each direction: retain horizontal_window logs, full_deflection, peak_abs_x,
yaw_deg_s and seconds. Expect magnitude 210 degrees/s (±2%) away from clamps;
repeat in slower frames and at sensitivity .2/3 (60/420 degrees/s). Check fine
movement, inversion, release, diagonals and neutral gating after menus/loading.
Restore human preferences after the test unless the human chooses new values.

Every arm: device 44100 Hz, CPAL 44100/512, buffered=true, producer 161/0x40000
with reporter logging, music 170/0x40000; producer p95 <=8 ms, pcm_starved_blocks=0,
music underrun_frames=0, callback max, producer_gap_max_ms and gaps_over_block.
Record heap and vitaGL RAM/VRAM/PHYCONT through load/settle. Stop above 180 MiB
used, on any allocation failure, or after 60 s without frame stats (one retry).
Run r3g's functional coverage on hedge3/tower2/skool2/village: walk/look, contacts,
fans/actors/material order, pause/resume twice, death/retry where reachable,
new-slot save/cold load and opening film/skip. Keep the reference Autosave hash
and compare any failure with r3f and explicit round-two-off settings.

## Human hand-off

After qualification close Alice, preserve all original r3f backups on the Mac,
restore original saves/settings/profile files byte-equal, and install r3i SELF
plus r3i-reference.json for the human. Verify readback hashes and that Alice is
closed. Keep the original r3f SELF/config as a paired rollback. Release all input;
confirm No-sleep enabled and Keep-screen enabled. Report actual console results,
installation state and any unrun step; do not infer success from the Mac package.

Capture preflight: after backing up profile_mode (including its absent state), set its contents to full for the r3i-capture standing/pickup campaign. A stale bench/route profile_mode is not a standing capture. Restore the original file or original absence at hand-off. Diagnostic configs: r3i-round2-off.json disables just the three round-two switches; r3i-capture-attached-off.json is the old attached-publication path with bounded capture, for an isolated batching comparison.

## r3i follow-up and capture interpretation

r3i is a small follow-up to accepted r3h (ea2e9bd), which remains earlier in the
console queue; it does not replace r3h qualification. Pause/transition export
failures are logged and gameplay continues; buffered rows remain eligible for
retry. Check export existence before treating a capture as evidence.

Capture mode still reads profile_mode every 60 frames in begin_frame. A leftover
60-frame spike in capture arms may be this poll. No polling move was made:
relocating it to pause would change live diagnostic mode-switch behaviour.

The p_h2, p_m1 and p_m2 pickup fixture images intentionally match. Their separate
TIKI definitions contain identical originspawn burst parameters (also shared by
p_h1): 75 supra.spr particles with identical size, offset, life, acceleration and
fade parameters, at unit model scale. The fixture renders only collection FX at
one fixed origin/camera and deterministic sample age (0.15 s for these three),
not each item's mesh, grant amount or idle light. The burst is origin-based, so
different animation tags do not alter it. p_h1's image uses a later age because
its fixture sweeps time for the sustained timing case. No fixture repair needed.

## Serial FTP rule (console owner only)

One FTP connection at a time, strictly serial: never open a second FTP connection
(probe, listing, retry) while a transfer is in flight. Two earlier concurrent
transfer wedges left port 1337 accepting connections without a banner; only a
reboot recovered it. If stalled, stop and report. Use TYPE I before SIZE.
