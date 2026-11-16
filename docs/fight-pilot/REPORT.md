# Hatter2 + centipede1 private fight pilot — batch delivered for root review

Final runtime/build source: `f318c39b0f9ea3b088f055fbe4fb5209bd01f35a`; branch vita-fight-pilot-20261116,
base RC14 37f9d06. Host binary head 4f513929653aa8eafd3dbdf73d596ac560016bfc;
its runtime input manifest is identical to native build head (SOURCE-PARITY.json).
Final Git head may add reports/status only. Root requested STOP after this batch;
no skool1 or short-fight map implementation was added.

## Measured host proof per map

| Map | Difficulty | Max displacement, units | Movement frames / attack-held frames / contacts | Sampled enemy health lost / player damage after fight start | Fight span / visible-target seconds | Deaths | Tape frames | Pilot OFF replay |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| centipede1 | Normal | 3844.57 | 6946 / 102 / 63 | 500.00 / 140.26 | 80.453 / 51.988 | 0 | 9334 | 312 matched; diff=0 events=0 PASS |
| hatter2 | Normal | 1290.61 | 2750 / 108 / 66 | 462.00 / 175.00 | 47.198 / 45.030 | 1 | 4064 | 136 matched; diff=0 events=0 PASS |

Tapes both end with ordinary reason=menu; full tape replay exits normally. Checks
are every 30 steps, full simulation state hashes every 300. No route input export,
flight, stat/inventory edits, probe timestep/focus override or host synthetic driver.
The real viewer uses ordinary native stick/button input. No pixel/audio equality claim.

Centi tape: `58c9a4e10f2012c1e9eb6cef734768929d99839aa1a137dcc65d6e2c033d9fe7`.
Hatter tape: `0747affeae348d209b5c204a8aa236495bcc2ecb9311e89d9a873ba82a646a4a`.
Host binary: `83c7074e52086d225c604cc6260dc3e136af4b98a0c2dea1e3d67684395ee81d`.
Evidence: `/Users/anezium/vita-j0/plan-2026-11-12/release-week/fight-pilot/batch-centipede1/HOST-PROOFS.json`, HOST-EVIDENCE-SHA256.json and host-build.json.
Raw runs: `/Users/anezium/vita-j0/plan-2026-11-12/release-week/fight-pilot/host-centipede1-02` and host-hatter2-centi-regression.
The exploratory host-centipede1-01 also passed exact replay, but revealed post-retry
stick latching; it is retained and superseded by the final run, not hidden as a pass
for the final source. Hatter2 final regression includes one death/retry.

Damage is read-only sampled health loss, not contact damage summed speculatively.
For centipede1 it covers registered ants keyed by stable Ant id at 1 Hz; other NPC
contacts are not silently counted as applied damage. Hatter2 tracks boss health.
Sampling can undercount contacts/birth/death between samples; resets/heals are not
damage. Player damage is after fight_start and can include falls as well as enemy
hits. Full fight span can include traversal, visibility gaps and retry; visible-target
seconds are reported separately. Check-sampled path is not an exact physics integral.
Neither tape is a full-map/boss-victory or release p95 verdict.

## Implementation and validation

Default-off replay_fight_pilot is unchanged; no shipped config is modified. The pilot
only synthesizes Frame native stick bytes and pinned-backend button keys before the
normal recorder reads them. Replaying never invokes the pilot; tape format unchanged.
Hatter2 fight/scene gating and ordinary Start/menu stop remain. Centipede1 follows the
production Flora approach waypoints, aims at live visible hostile Ant/NPC/controller
targets, checks readiness, fires Cards, and falls back to ordinary Knife selection and
short-range movement when will is low. No inventory edits. Bounds: 180 s overall,
45/60 visible-target combat seconds (hatter2/centipede1), two retries, 18 s approach
stall. Flora can also stop after a cleared observed fight with damage dealt and taken.
A 0.3 s neutral frame interval releases both input stick latches after retry.

12/12 offline tests pass (`/Users/anezium/vita-j0/plan-2026-11-12/release-week/fight-pilot/test-centipede-offline.log`), including a new
centipede1 fake-kit recording lifecycle, existing measurement failure gates and actual
native pad-function Rust stub regression. Host exactness builds use opt-level 1,
LTO=false, codegen-units=64 for iteration; native SELF uses the normal release profile.
Those host runs are not wall-clock timing measurements and do not take mac-timing.lock.
No full Cargo unit suite or console run is claimed. Rust builds passed on both targets.

## Combined private SELF, VPK and root bundle

Native artifact directory: `/Users/anezium/vita-j0/plan-2026-11-12/release-week/fight-pilot/builds/f318c39`.
SELF SHA-256: `566d8d506981e425459e2b069c0882cb9be9004338fceac15f8c27647114f15e`.
VPK SHA-256: `9af5346ecfe74a8549400d6c70a5f00705fc9620767395b984c143be4b688ced`.
ELF SHA-256: `58ceed04d18f354c8c02febd3b1e579f4728241db6f3b6e81b413dc49abe77a0`.
Bundle: `/Users/anezium/vita-j0/plan-2026-11-12/release-week/fight-pilot/batch-centipede1/fight-pilot-hatter2-centipede1-f318c39.tar.gz`.
Bundle SHA-256: `9cd49b9408695e774adba8c9e36d30842dfcbf9371436f9683410ef0d0a03c0a`.
The bundle contains both separate record configs, root procedure/scripts with handoff,
SELF/VPK/ELF, build/check receipts, source parity and both proof summaries. SHA256SUMS
covers members. No game data. Native build used pinned nightly-2026-10-11,
native_timing,vita_renderer,frame_total_only, 192 MiB heap and the pinned RC14
no-prelude archive 0e340441f6909c1169b6f33bcd6e0223ffa91c9d7ce5155f3b27eb1bdd0b38a5.
Private test only: release distribution/notices/LGPL qualification is not claimed.
Native heap/stack/path/SFO/VPK/LiveArea checks pass (check.txt/livearea.txt);
ARM ELF RX-to-RW/init-array margin is 264328 bytes, above the 3348-byte minimum.

## Root procedure and next-worker handoff

`tools/fight-pilot/README.md` now documents both maps and contains **Handoff: adding the
next map**: supported-map switch, waypoint sources, complete hostile target sources,
read-only health telemetry, builder/config/proof commands, pilot-OFF replay requirements,
retry neutral latches, X11 key codes, native pad cache lifetime, finite Cards funding,
controller-vs-NPC digest limitations, host-vs-native frame windows and cache cleanup.

Root alone reviews and operates the console. Use the unchanged pinned kit
93f434a6286b2180bd66d89ffad1394b7f27b5e385296c67a70b2271256558b3; acquire slot,
preflight/AC/receiver, observe/acknowledge closed, back up true-owner SELF/config/saves
once, tape ls for unique names, install this SELF with one record config, launch,
check-config, wait for clean recorded ... reason=menu, recover, inspect exact PNG,
closed --observed, tape get/hash/index, then next map or restore --keep-saves/readback.
Never plain restore; recording can write normal autosaves. Preserve owner backup.

The included prepare-measure.py/reused measure.py require real native tape-get
provenance and reviewed <=481-frame windows, then replay the whole native tape with
pilot off on pinned RC14 SELF
966eac903585a56df7bbe5a015a1f9b620d0bd4923ea5be9d1965742b919bca0. Require all checks,
zero mismatches, complete undropped per-frame spike rows and root observations. Native
frame windows must come from the native tape, not these host frame numbers. Reports
remain SELECTED_WINDOWS_ONLY, not a whole-map p95 verdict.

Not done: native record/replay/p95 and planning cost; review/merge; skool1, grounds1,
grounds2, jlair2, potears3, qlair (another worker per root quota instruction). No console
contact, FTP, ports 1337/1338, Terminal.app, kit edit, shipped config edit or game-data
edit. Private caches are removed at handoff with CLOSURE.json. No task Terminal windows
were created; no global count of foreign visible windows is claimed.
