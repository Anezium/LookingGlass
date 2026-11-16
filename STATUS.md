# Fight pilot WIP — blocked before compilation

Task: docs/briefs/RELEASE-FIGHT-PILOT-2026-11-16.md in the meta repository.
Branch: vita-fight-pilot-20261116, base RC14 37f9d06.
Worktree: /Users/anezium/vita-work/lg-fight-pilot.
Runtime implementation commit: 55ca6245d6363f5408ec1138bcafead070ef8d42.

**Hatter2 is NOT delivered. No Rust build, host combat proof, native tape or test SELF
has been produced. Do not install or treat this WIP as qualified.**

The shared mac-timing.lock remained occupied throughout the attempted admission:
RC15 host oracle pid 92451, then actor census build pid 6847, then snapshot stage9
pid 90649. The worker waited in 60-second intervals while implementing the offline
package, then stopped only its own waiting build driver pid 9267. It did not remove
or change another worker's lock or stop another worker. No build was started and no
shared target artifact was replaced. Acquire a genuine host slot before resuming.

Implemented but uncompiled: default-off replay_fight_pilot; viewer input override;
native sticks recorded and selected on host by tape input, independently of pilot;
hatter2 approach and bounded fight driver; complete hostile owner categories excluding
puzzle/ally surfaces; scene/phase gating; readiness/LOS; sampled boss health and player
damage telemetry; ordinary Start/menu stop. Other map routes are not implemented.

Measured: 10 offline fake-kit CLI tests pass, including recording lifecycle and native
tape indexing -> RC14 measurement -> p95 output, plus fail-closed negative controls.
Python syntax, pilot Rust formatting/parser and git diff whitespace checks pass.
No Rust type checking or gameplay result is claimed. See tools/fight-pilot/README.md
and docs/fight-pilot/REPORT.md for exact limitations and root procedure.

Resume in this worktree:

1. Read this file, the report and task brief. Check free disk >=41 GiB and verify HEAD.
2. Run `python3 tools/fight-pilot/build-host.py` once the shared slot is available.
   It uses the shared cache and copies the binary with a hash receipt. Correct compile
   errors, commit and rebuild before testing. No dependencies or release configs change.
3. Run `python3 tools/fight-pilot/prove-host.py --binary ABSOLUTE_BINARY_FROM_RECEIPT
   --out NEW_ABSOLUTE_EVIDENCE_DIRECTORY`. This opens the real viewer. Inspect negative
   results and fix the real input/approach/fight behavior; do not bypass assertions.
4. Require movement, attacks, positive sampled boss health loss, player damage, clean
   menu-stop tape and pilot-OFF replay PASS/diff=0/event_mismatches=0. Record fight windows.
5. Run appropriate Rust tests under the shared lock, including replay input/tape checks
   and default-off config checks. Add focused regression coverage for any actual defect.
6. Commit all inputs, verify HEAD and clean tree, then `python3 tools/fight-pilot/build-vita.py`
   for a separate private-test SELF/VPK. Hashes are currently NONE. Check native gates.
7. Update external REPORT.md and its Git mirror, push, deliver hatter2 to root immediately.
   Root alone reviews and operates the console with the supplied offline-tested procedure.
8. Only then implement centipede1, skool1 and short-fight routes.

Evidence root: /Users/anezium/vita-j0/plan-2026-11-12/release-week/fight-pilot/.
No console contact, FTP, ports 1337/1338, Terminal.app, kit edit, shipped config edit,
game-data edit or Git assets. No Terminal windows were created. Foreign jobs preserved;
no claim about the count of pre-existing visible Terminal windows. Main meta DECISIONS.md
was not edited because this task is restricted to the dedicated Looking Glass worktree.
