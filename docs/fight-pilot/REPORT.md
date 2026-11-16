# Viewer fight pilot — WIP / not qualified

2026-11-16. Branch vita-fight-pilot-20261116 from RC14 37f9d06. Runtime implementation
55ca6245d6363f5408ec1138bcafead070ef8d42. Read STATUS.md before resuming.
External report: /Users/anezium/vita-j0/plan-2026-11-12/release-week/fight-pilot/REPORT.md.

## Result and blocker

Hatter2 is not delivered. No Rust compilation, host gameplay run, recorded native tape,
exact replay, test Vita SELF or package hash is available. Values below are UNKNOWN,
not zero and not a pass. Ten offline orchestration tests pass; they do not qualify the
uncompiled pilot. The brief and CLAUDE.md were read in the requested order. The cited
AUTOPLAY-FEASIBILITY.md was absent, so the cited production files were read instead.

Build admission used exclusive-create mac-timing.lock with bounded 60-second waits.
Observed holders were RC15 host oracle (92451), actor-census cargo build (6847), then
snapshot stage9 potears2-final (90649). The build never acquired the lock. Our own
waiting driver (9267) was terminated before handback after checking it owned no lock.
No other process/lock was stopped, edited or removed. Shared contention is the blocker;
the worker did not bypass it by compiling in another target directory.

## Design (implemented, uncompiled)

The replay session accepts a viewer-generated Frame immediately before installing the
recording input. The pilot never writes player, NPC, controller, RNG or physics state.
It emits analog stick bytes and the same translated button keys as the pinned Vita
backend. Existing tape recording reads and saves the installed pad. Host playback
chooses the native analog input path from the installed tape's pad, without consulting
the pilot flag. Vita already consumes that input path. No tape-format change or route
input conversion is used. Ordinary unpiloted recordings keep live input.

Config `replay_fight_pilot` defaults false and is not added to any shipped config. It
runs only when a recording is active, never on a replay. Hatter2 uses the production
approach goal (5664,160) from chapter start, waits through scene state, and engages only
once the Hatter controller reports action music. Targets include NPCs, Duchess,
encounter/school controllers and registered level owners with hostile_target filtering.
Puzzle shot surfaces, ice and summons are not hostile aim targets.

The driver turns through real native stick deadzone/curve/sensitivity, aims vertically,
cycles D-pad weapons to Cards, requires LOS and readiness before firing, strafes and
tries jumps when stationary. Bounds: 180 seconds total, 45 visible-target combat
seconds, two retries and 18 seconds of non-scene approach stall. Stop uses ordinary
Escape/Start and requires the normal `recorded ... reason=menu` flush. It samples the
Hatter controller snapshot at 1 Hz and reports player-health decreases and forwarded
weapon contacts. Contacts alone are not counted as actual damage dealt.

Unmeasured/assumed: Rust type correctness; host/window focus; natural approach and scene
completion; native controller/custom binding compatibility (default Vita layout is the
intended test setup); fight survival, ammo sufficiency, hit rate; on-device CPU/memory
cost; native replay fidelity. The prototype must be debugged on the host before root
uses any test SELF. Other maps deliberately stop rather than running an untested route.

## Per-map host proof

| Map | Implementation | Movement / attacks | Damage dealt / taken | Fight duration | Difficulty | Exact pilot-off replay |
| --- | --- | --- | --- | --- | --- | --- |
| hatter2 | Uncompiled prototype | UNKNOWN | UNKNOWN | UNKNOWN | Planned Normal | NOT RUN |
| centipede1 | Not implemented | NOT RUN | NOT RUN | NOT RUN | Not tested | NOT RUN |
| skool1 | Not implemented | NOT RUN | NOT RUN | NOT RUN | Not tested | NOT RUN |
| grounds1/2, jlair2, potears3, qlair | Not implemented | NOT RUN | NOT RUN | NOT RUN | Not tested | NOT RUN |

The host driver requires a visible fight, movement and attack input, positive sampled
boss health loss, player damage, clean menu-stop tape and a complete replay with
result=PASS, diff=0 and event_mismatches=0 while the pilot is false. It writes tape,
config/binary hashes, metrics and replay result to a new external evidence directory.
It has not been run. No fabricated proof.json is supplied.

## Artifacts and hashes

Test SELF: NONE. Test VPK/package: NONE. Host binary: NONE. Tape: NONE.
Generated separate hatter2 record config SHA-256:
7d7ab57214825c1919e1c3dc9dc9870d0419d4c379121870c4acef531cb070e0.
The private-test native builder is supplied but has not run; successful compilation
would still not be console qualification. No asset or binary is tracked in Git.

## Offline validation and root procedure

`python3 -m unittest discover -s tools/fight-pilot -p 'test_*.py' -v`: 10/10 PASS.
Evidence: external test-offline.log. These use inert fake kit subprocesses, never import
or execute the real kit, and do not open a transport. Coverage includes recording
start/recover/get/restore --keep-saves, rejecting missing clean end, missing closure,
second active take and changed kit; and native tape index -> pinned manifest/config
-> reused play-record measure gates -> selected-window p95, rejecting changed tapes,
changed kit, replay mismatch and dropped spike lines. Python compilation, pilot
rustfmt parser/format and git diff --check also pass. No Rust tests ran.

The concrete root procedure is tools/fight-pilot/README.md. `record.py` pins kit
93f434a6286b2180bd66d89ffad1394b7f27b5e385296c67a70b2271256558b3 by default, latches
one active tape, checks the post-launch clean menu stop before recover, requires closed
state before tape get, and uses only restore --keep-saves. Root supplies actual PNG
observations and owns slot/preflight/AC/backup/receiver/read-back/return gates.

The RC14 measurement uses the existing play-record measure.py gate implementation,
adapted only to select an explicit session root. prepare-measure.py accepts fp-* native
tapes, verifies exact tape-get provenance and nonoverlapping <=481-frame windows, and
pins the same kit plus RC14 SELF
966eac903585a56df7bbe5a015a1f9b620d0bd4923ea5be9d1965742b919bca0 by default. Replay
configs omit the new pilot key, use the unchanged RC14 base and per-frame spike timers.
The final report is explicitly SELECTED_WINDOWS_ONLY, never a whole-map p95 verdict.

Not done: all gameplay/build/native gates, first hatter2 deliverable, remaining maps,
Vita recording and RC14 fight-window measurement. No console contact, FTP, forbidden
ports or Terminal.app; kit and shipped configs untouched. No Terminal windows created;
foreign jobs retained and no global visible-window count claim.
