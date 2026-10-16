# Round-two simulation/CPU candidates — console owner's protocol

Mac-built handoff only. This track has no console access or console results.
Production behavior: vita-sim-maps-2 `f9ef6ba`, based on r3c `a209719`;
follow-up source/package identity is recorded in VERIFICATION.json.

## Package and arms

Verify root SHA256SUMS and A-simmaps-2/VERIFICATION.json, then the package's
SHA256SUMS.txt before installing. The identical A-simmaps-2/looking-glass.self
(or eboot.bin from looking-glass.vpk) must remain installed in **every arm**.
Use configs/PL-simmaps-2-off.json as private/vita_renderer.json for A, and
configs/PL-simmaps-2-on.json for B; use configs/vita-sim-all.json in both.
All four first-round switches remain explicitly true. Only these three booleans
differ: sim_triangle_candidates, sim_fan_candidates, fx_merge_local_reuse.
Runtime defaults remain false for all three pending console acceptance.

The triangles/fans/merge configs isolate one candidate with the other two false.
Use them to attribute a result or diagnose an interaction; do not compare a new
SELF with an older reference as the primary performance pair. Read back and hash
SELF and effective config before each arm; require the expected retained renderer
selection and switches, no parser error and no construction fallback. Retain any
fallback as a distinct failed/legacy observation.

**Rollback SELF and renderer config together.** Reference r3c and older binaries
reject the new keys and fall back to all renderer defaults. Preserve original
SELF/config bytes and verify their paired restoration.

## Preservation and visit method

The console owner first reads ../allmaps/PROTOCOL.md and ../sim-maps/PROTOCOL.md
in full and follows their app/private path, input-driver, capture, backup and
read-back rules. Back up the complete saves tree, auto.json, configs and installed
SELF with directory inventories, byte counts and SHA-256. Preserve reference
Autosave 4e5dab48ede6b88f30ac80fce6fb117c3bf4bacf6e76aabd5aeec260bb188714.
Do not overwrite existing allmaps or sim-maps evidence. **Around every arm**, with
app closed, back up the whole save inventory (primary saves, backups, auto.json,
previews and directory/file inventory); restore that entire baseline before entry
and after closing the arm, including failed/aborted arms. Read back every file,
byte count and SHA-256 and require identical inventory, with no extra files.
The reference Autosave above must be byte-equal before AND after each arm.
Never New Game. No console MKD (directory creation), promotion or reboot.

Owner observation tonight: r3c-based Chapters -> village stalls over two minutes
on the opening film/entry path; front-end Load Save produces a black image.
Round one visited 22 maps on r2 with the same movie code without this problem.
These are retained failures, not reasons to change the other maps' working route.
Village is entered ONLY by restoring/loading the reference Autosave above; never
via Chapters or New Game. Front-end Load Save is not a certified working bypass:
if restoring that save does not produce gameplay, abort/record the village arm.
Do not replace it with a chapter start, another save or a different movie path.
Record the exact restore/resume input route and its outcome, including black images.
No driver changes, system configuration changes, reboot, promotion or new console
permission is implied by this implementation handoff.

1. Restore/read back the whole starting save inventory and configs before each
   arm. Cold-start a fresh process. Village uses only the reference Autosave
   restoration above. Every other map uses the round-one route exactly: launch
   Alice -> Chapters -> d-pad to the zero-based chooser index below -> touch
   Begin, first entry (entry=null). Do not use Cross as a substitute for touch
   Begin (it also clicks the current cursor). Take a screenshot after EVERY
   entry attempt, including a black/stalled entry, before timing. Record
   chooser index, actual map and entry, effective config, clocks, timestamps and
   alive noncinematic controllable gameplay. Keep upper bounds distinct from exact
   timings. Use the same audio-enabled configuration and output device in both
   arms; record audio startup failures and audible faults separately.
2. Use an identical camera and neutral input. Settle 10 seconds after accepted
   gameplay, then retain the same 30-second neutral window, without FTP, captures
   or other diagnostic activity inside it. Record exact population boundaries,
   raw light CSVs, N, median/p95 wall, frame CPU, render CPU children and swap.
   A window with N<120 remains a smaller measured population. Do not silently
   extend only one arm or convert host timings to expected Vita milliseconds.
3. Before/after-window captures must show identical framing and preserve alpha,
   particles, actors and mechanisms. Then walk/look outside the timed window.
   Inspect hedge3 mesh contacts/riders/side pushes, tower2 near/far fan collision,
   crushing and rollback, and skool2 actor/particle material order and animation.
   Exercise entry, pause/resume, death/retry, save/load and chapter transitions.
4. Retain every load failure, crash/core, hang, wrong entry, death and missing
   window. Abort and record the arm when no frame-stat line arrives for 60 seconds,
   including during entry; retain its screenshot and last line/timestamp. Do not
   wait over two minutes for village. At most one retry per
   failed arm. Never merge failed partial populations into successful statistics.
5. Record loading and settled memory separately throughout: newlib used/arena
   free/committed, vitaGL RAM/VRAM/PHYCONT free, with timestamped samples. Partial
   triangle and full geometry can coexist in one placement. Sampled extrema do
   not certify transient/contiguous peaks. Compare against A and report any
   regression or allocation failure; no host memory claim establishes headroom.

## Fixed camera requirement for skool2

Use the actual round-one entry: Chapters index 6, first entry (entry=null), which
selects info_player_start skool2_start1. Its authored origin is (-64,-3440,-24),
initial eye (-64,-3440,24), yaw 90 degrees, pitch 0; Player::spawn/settling can
adjust the vertical position. This entry's untouched settled pose defines the
camera, not the reference village save or the earlier mismatched-yaw captures.
Capture the entry pose and record its actual settled feet/eye, yaw and pitch when
telemetry is available; use the first accepted rehearsal's screenshot/landmarks
and settled pose as the reference for BOTH arms. Release every control before
Begin and after launch; do not move, turn, recenter or touch gameplay look areas.
Preserve captures immediately before and after the window, plus the complete
input trace. Match the same static landmarks, position, yaw and pitch in both
arms; retain numerical camera telemetry if available. The earlier campaign's
mismatched yaw cannot serve as this baseline. If framing differs or heading drifts,
reject the **whole pair**, rehearse outside timing, then restore and rerun both
arms. Do not report a skool2 performance delta from different camera headings.

## Visits

Each row is A/off then B/on with the same SELF, entry route/state and camera.
All chooser indices below are zero-based, Chapters -> d-pad -> touch Begin,
first entry (entry=null); village is the sole restored-Autosave exception.
Repeat three alternating pairs (A/B, B/A, A/B) for hedge3, tower2 and skool2,
and for any apparent regression. The other rows require at least one matched pair.

- gvillage — reference Autosave restoration ONLY, no chooser; retained control.
- potears3 — first, chooser 10; fast control.
- fortress1 — first, chooser 2; fast control.
- hedge3 — first, chooser 32; lazy triangle candidates, general FX reuse.
- tower2 — first, chooser 31; fan candidates, general FX reuse.
- skool2 — first, chooser 6; FX merge indices; fixed camera as above.
- hatter1 — first, chooser 23; FX merge, moving supports and particles.
- potears1 — first, chooser 8; FX/actor population and memory.
- garden1 — first, chooser 12; FX/pickup/particle population.
- funhouse — first, chooser 22; later NPC contacts and mechanism activity.
- centipede1 — first, chooser 16; moving actor/FX population and memory.

For funhouse also retain a matched longer untimed contact/animation rehearsal;
the host late window was frames 960–1199, not an entry-only NPC observation.
Stop acceptance for any simulation/contact/image discrepancy, memory failure or
material p95/control regression. Keep the candidates off until the owner has
reviewed measured benefits and correctness; host workload reduction alone does
not enable them in reference builds.

## Report and final restoration

Store new console evidence under console/: one visit record per arm/pair,
raw logs and exact selected CSV rows, input/camera records, images, hashes,
loading/settled memory extrema and audio outcomes. Record missing values honestly.
Report each candidate's isolated result where needed and combined A/B deltas only
for matched populations; retain raw stalls and provenance. Preserve the renderer's
known sky/rope/portal/fog limitations when describing fidelity.

After EACH arm close app and restore/read back the complete baseline save
inventory byte-equal, even after failure. At final handoff release all controls,
verify the existing keepalive/screen state, and
restore the complete original saves/config tree byte-equal with inventory checks.
Record the final installed SELF and the owner's selected matching config. Restore
SELF/config together if rolling back; leave capture/keepalive drivers untouched.
Checksum console evidence separately after the report is complete.
