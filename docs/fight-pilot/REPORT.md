# Hatter2 fight pilot — host proof passed; private native test delivered

Branch: vita-fight-pilot-20261116, base RC14 37f9d06. Native build/source head:
`7fd23e2a8084f4240d25e57c15aac7efc3b7cb42`. Host proof runtime head: 6ad8d5a6d8c9117a3f086b3dc32c4a1d6d138fc8.
The final Git head may add this report/status only. No merge or production install.

## Measured host proof

| Map | Start/difficulty | Movement | Attacks/contacts | Actual health loss | Fight | Pilot-OFF replay |
| --- | --- | --- | --- | --- | --- | --- |
| hatter2 | Authored chapter, Normal | max displacement 1269.281 units; 1956 movement-input frames; check-sampled path 3590.350 units | 106 attack-held frames, 66 weapon contacts | boss 462 cumulative across one retry; Alice 185 after fight start | first visible target 23.817 s; span 47.503 s, visible-target time 45.009 s | 2983/2983 steps, 100/100 checks, diff=0, event_mismatches=0, PASS |
| centipede1 | Not implemented | NOT RUN | NOT RUN | NOT RUN | NOT RUN | NOT RUN |
| skool1 | Not implemented | NOT RUN | NOT RUN | NOT RUN | NOT RUN | NOT RUN |
| grounds1/2, jlair2, potears3, qlair | Not implemented | NOT RUN | NOT RUN | NOT RUN | NOT RUN | NOT RUN |

Tape duration 71.314639 s; clean ordinary menu stop (`reason=menu`). One death/retry is
included. This is a first-fight sample, not a boss victory or whole-map qualification.
Health increases on retry are excluded from cumulative damage. Cards funding runs out
in each attempt; later frames still contain boss attacks and player strafing/retry.
The path length is sampled at replay checks, not an exact per-tick path integral.

Recording and replay use the real host viewer, ordinary Vita-layout input, no audio,
no route simulation, no synthetic-host driver, no timestep/focus probe override.
Full simulation state hashes are included every 300 steps; ordinary checks every 30.
The existing partial target digest counts NPC targets only (hostiles=0 here), while
registered Hatter state is covered by full hashes. No pixel/audio equality claim.

Proof: `/Users/anezium/vita-j0/plan-2026-11-12/release-week/fight-pilot/host-hatter2-01/proof.json`; record/play run.log below that directory.
Evidence hashes: `/Users/anezium/vita-j0/plan-2026-11-12/release-week/fight-pilot/HOST-EVIDENCE-SHA256.json`.
Host binary SHA-256: `2325e023d1ba394541efd25bd7ed9cd8bcc489141c24b4e4b9d902c0d86e19b8`.
Tape SHA-256: `32876bb3597eabe75286188a1ec32cad7b8b70415240f49e7cb4563345d43405`.
The first metrics pass used GLFW code 341; pinned miniquad uses X11 0xffe3. The
analyzer was corrected and existing clean tape/logs reanalyzed; gameplay was unchanged.

## Design and validation

`replay_fight_pilot` defaults false and is absent from every shipped config. The viewer
supplies live player/controller state and complete hostile-owner categories before the
ordinary recorder installs a frame. The pilot writes only input: native analog bytes
and the pinned backend's translated button keys. Tape format is unchanged. Host replay
selects the native analog path from tape bytes independently of the pilot flag. Vita
already takes that path. No player/NPC/controller/RNG state or game data is changed.

Hatter2 follows the route's approach goal (5664,160), waits through the Cat scene,
filters the idle boss until the fight phase, aims horizontally/vertically, checks LOS
and weapon readiness, cycles normally to Cards, attacks and strafes. Targets come from
NPCs, Duchess, encounter/school and registered level owners (hostile_target filtered);
puzzle shot surfaces, ice and allies are excluded. Bounds: 180 s overall, 45 s visible
target combat, two retries, 18 s non-scene approach stall. Stop is normal Start/menu;
manual Start is honored. Default Vita bindings are the intended test setup; custom
bindings and other map routes are not qualified. Planning has no large state search;
its on-device CPU/memory cost is unmeasured.

11/11 offline tests pass (`test-final-offline.log`): ten fake-kit CLI recording and
RC14 measurement tests plus a compiled Rust test of the actual native pad function with
a host SceCtrl stub. That regression failed before, passed after, the fix that releases
cached native input after recording ends. Covered modes: injected record, replay with
no pad, real-pad record capture, OFF after cached input. Native build was restarted
before delivery; owned shutdown receipt is native-rebuild-shutdown.json.
The only runtime source difference from host-proven head is that one Vita-only guard;
host-visible source is identical (SOURCE-PARITY.json). Rust type checking/builds passed
on host and Vita. No full Cargo unit suite or on-console test is claimed.

## Private test artifacts

Native directory: `/Users/anezium/vita-j0/plan-2026-11-12/release-week/fight-pilot/builds/7fd23e2`.
SELF: `f62491b1a08c8b0f496f90b7930644150e37f5ad0539a9c10776429d937c35ec`.
VPK: `8841bf68779c646816a1b940137a36288f7d9fb9f364c7b0a77d1e53a1688a9d`.
ELF: `fc8393a31ad0c9a7a0dae40f6b5d9f52ed384580435b3016cee13924b8b7bc33`.
Root bundle: `/Users/anezium/vita-j0/plan-2026-11-12/release-week/fight-pilot/fight-pilot-hatter2-test-7fd23e2.tar.gz`.
Bundle SHA-256: `208d553dd896fe2b16038236447b8f2e07b6ed742030f7e74699b49d8a5a7330`.
The bundle includes SELF/VPK/ELF, test config, build/check receipts, host metrics,
source parity and the root procedure/scripts, with SHA256SUMS. It contains no game data.
Native build uses the pinned nightly-2026-10-11, release features
native_timing,vita_renderer,frame_total_only, 192 MiB heap, and RC14 no-prelude archive
0e340441f6909c1169b6f33bcd6e0223ffa91c9d7ce5155f3b27eb1bdd0b38a5.
Private-test build deliberately does not qualify release notices/LGPL distribution.
ELF nm output has eight weak undefined symbols and zero strong undefined symbols
(undefined-symbols.txt); no strict import-table audit is claimed.
Native heap/stack/path/SFO/VPK and LiveArea checks pass in check.txt/livearea.txt;
ARM ELF RX-to-RW/init-array margin is 304908 bytes (minimum 3348). No fresh allocator
disassembly or strict import-table audit is claimed.

## Root procedure and limits

Read `tools/fight-pilot/README.md` (also in the root bundle). Root reviews the branch,
gets the console slot, checks AC/locks, starts receiver, observes/acknowledges closed,
backs up true-owner SELF/config/saves once, checks unique tape name, installs this SELF
plus record-hatter2.json, launches, verifies config, and waits for the clean recorded
line. Then recover, inspect its exact PNG, closed --observed, tape get, verify/hash tape,
restore --keep-saves and read back. One tape at a time; never plain restore. Recording
may write normal autosaves; the immutable owner backup remains necessary.

Kit remains unedited, SHA-256
93f434a6286b2180bd66d89ffad1394b7f27b5e385296c67a70b2271256558b3.
For later measurement, retrieve the native tape into a fresh session through the kit
and select actual native fight windows (do not reuse host frame numbers). The supplied
prepare-measure.py and reused play-record measure.py enforce tape-get provenance,
pinned kit/RC14 SELF, exact full-tape replay and complete, undropped <=481-frame spike
windows with root observations. RC14 SELF is pinned to
966eac903585a56df7bbe5a015a1f9b620d0bd4923ea5be9d1965742b919bca0.
Reports label SELECTED_WINDOWS_ONLY; no whole-map p95 verdict or console performance
is inferred from host proof. Offline tests cover this orchestration using an inert kit.

Not done: centipede1/skool1/short maps; native recording/replay and RC14 fight p95;
on-device cost/feel; whole-map release qualification; independent review/merge.
No console contact, FTP, ports 1337/1338, Terminal.app, kit/shipped-config/game-data edit,
or Git binaries/assets. Builds/exact proof do not use mac-timing.lock under root's
updated release-week rule; it is reserved for wall-clock measurements. Private caches
are removed at handoff (CLOSURE.json records both paths and actual free bytes). No task Terminal windows were opened;
no claim about pre-existing visible windows. Main meta DECISIONS.md was not changed
because work is restricted to this Looking Glass worktree.
