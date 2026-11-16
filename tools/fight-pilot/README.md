# Fight pilot (private test candidate)

Default off (`replay_fight_pilot=false`); no release config is changed. The current
pilot supports hatter2 and centipede1. Read the external REPORT.md before using any build.
A source commit or successful compilation alone does not qualify a tape or device build.

The viewer supplies live player state and hostile combat targets before `Session::begin_step`.
The pilot substitutes ordinary SceCtrl stick bytes and the pinned backend's translated
button keys. The existing recorder writes the installed input, including its pad read.
No simulation state is changed by the pilot. On host replay, an installed pad selects
the native analog path from the tape itself, independently of the pilot config. Vita
already takes that path. The tape format is unchanged.

Hatter2 approach is the production route's blade/scene goal (5664,160), reached from
the authored chapter spawn. Targets come from NPCs, Duchess, encounter/school owners,
and registered level controllers filtered by `hostile_target`. Puzzle shot surfaces,
Alice's ice and summon are deliberately excluded from hostile planning. Hatter's action
music is used to avoid fighting the visible idle boss before the clock fight starts.
The driver applies horizontal/vertical aim through real stick curves, cycles to Cards
with the D-pad, checks visibility and weapon readiness, and strafes in combat. It
releases all synthetic inputs during a scene. It caps total time at 180 seconds,
visible-target combat at 45 seconds for hatter2 and 60 for centipede1, retries at two and a non-scene approach stall at
18 seconds. It stops via ordinary Escape/Start, so `reason=menu` is required.

Health telemetry samples the Hatter controller's existing read-only snapshot at 1 Hz;
weapon-contact output is not by itself proof of health loss. Host proof requires a
positive sampled enemy-health reduction and positive player damage, plus an exact replay.
No claimed Vita planning cost or fight performance is inferred from host timing.

## Host proof

Run from this worktree. Builds and exact replay proofs do not use `mac-timing.lock` (root release-week rule).
The host builder APFS-clones the shared cache into this worktree's `target-pilot` to avoid
Cargo contention; remove that cache at handoff. All builders refuse less than 41 GiB free.

```sh
python3 tools/fight-pilot/build-host.py
python3 tools/fight-pilot/prove-host.py --binary /absolute/path/from-host-build.json --map centipede1 --out /absolute/new/evidence-directory
python3 -m unittest discover -s tools/fight-pilot -p 'test_record.py' -v
```

The proof opens the real host viewer with local assets read-only. It records a chapter
start on Normal using the Vita layout, then replays with the pilot false. It requires
`result=PASS diff=0 event_mismatches=0`. The record process is terminated only after
the normal menu stop has flushed the tape. The replay exits normally. Private saves
and all outputs belong in the evidence directory, never the game-data tree.

## Root console procedure (requires reviewed package and console slot)

No worker executes these device commands. Root owns observation, the console slot,
AC/lock checks, owner backup, closure acknowledgement and restoration. Use the exact
immutable kit below; never edit it. See the play-record README for its gate semantics.
Each take can write normal autosaves: retain the owner backup and use `--keep-saves`.
Never run plain restore. No Terminal.app is needed.

```sh
P=/absolute/new/fight-session
KIT=/Users/anezium/vita-j0/plan-2026-11-12/rc1-session/rc1kit.py
SRC=/Users/anezium/vita-work/lg-fight-pilot
SELF=/absolute/reviewed-test-package/eboot.bin
MAP=centipede1 # or hatter2; one take at a time
python3 "$SRC/tools/fight-pilot/make-config.py" --map "$MAP" --out "$P/configs-$MAP"
# Verify kit SHA-256 = 93f434a6286b2180bd66d89ffad1394b7f27b5e385296c67a70b2271256558b3.
# Root: acquire slot, preflight, start arms receiver, observe, inspect the exact PNG,
# acknowledge closed --observed, backup true-owner SELF/config/saves once, tape ls.
# Refuse an existing tape name; use --name when generating a fresh config if needed.
python3 "$KIT" --session "$P" preflight
python3 "$KIT" --session "$P" log start arms
python3 "$KIT" --session "$P" observe
# Root inspects the printed PNG, then:
python3 "$KIT" --session "$P" closed --observed /exact/printed.png
python3 "$KIT" --session "$P" backup
python3 "$KIT" --session "$P" tape ls
python3 "$SRC/tools/fight-pilot/record.py" start --session "$P" --self "$SELF" --config "$P/configs-$MAP/record-$MAP.json"
# Wait <=360s from launch for the exact clean recorded fp-<MAP>-01.lgtape ... reason=menu line.
# This is a root timeout budget, not a measured Vita load-time claim.
# Missing line, failed record or crash: stop and use root's kit recovery procedure; do not label PASS.
python3 "$SRC/tools/fight-pilot/record.py" recover --session "$P"
# Root inspects recover's exact PNG and acknowledges closure:
python3 "$KIT" --session "$P" closed --observed /exact/recover.png
python3 "$SRC/tools/fight-pilot/record.py" get --session "$P"
python3 "$SRC/tools/fight-pilot/record.py" restore --session "$P"
# Root verifies RESTORE VERIFIED and read-back hashes, keeps restore receipt, stops
# receiver/owned helpers and returns slot. No task Terminal windows were opened.
```

Run one tape at a time. The active receipt prevents starting another before get.
Root must separately inspect the retrieved native tape (END/reason/menu/frame count),
its hash, damage/fight telemetry and source/config/SELF hashes. Never overwrite a tape.

## RC14 replay measurement

After retrieving a valid native Vita tape, root starts a fresh measurement session
with the same preflight/receiver/observe/backup gates. Retrieve the tape with the kit
into that session too, preserving the exact `tape get` provenance in session.jsonl.
Do not rename files and invent retrieval receipts. Keep measurement outputs separate
from the recording session so restoring either session cannot confuse its state.

Select reviewed fight windows of <=481 frames using fight_start/health telemetry and
replay check anchors. Write `windows.json` as
`{"windows":[{"tape":"fp-hatter2-01.lgtape","window":[FIRST,LAST]}]}`.
Spike frame 1 corresponds to tape step 0. Do not include load/scene/idle intervals.

```sh
M=/absolute/new/measurement-session
RC14=/absolute/verified/rc14/eboot.bin
python3 "$SRC/tools/fight-pilot/prepare-measure.py" --session "$M" --windows "$M/windows.json" --rc14-self "$RC14"
export FIGHT_PILOT_SESSION="$M"
python3 "$SRC/tools/fight-pilot/measure.py" --step
# Root inspects the exact returned PNG and uses kit closed --observed.
# Repeat --step for each selected window, with root observation between each.
python3 "$KIT" --session "$M" restore --keep-saves
```

The offline preparer pins the unchanged kit and RC14 SELF hashes by default, indexes
only native Vita tapes with clean END, and requires actual kit retrieval provenance.
The reused play-record measure.py gates require full-tape `PASS`, zero differences and
event mismatches, no dropped spike rows, exactly every selected frame, config PASS
and root observation transactions. RC14 configs omit the new pilot key, so this does
not require changing RC14. Whole-map p95 remains unqualified: report.json labels all
measurements SELECTED_WINDOWS_ONLY even if the selected fight window passes 50 ms.
The fake-kit test qualifies orchestration only, not console gameplay or performance.


## Handoff: adding the next map

The authorized scope of this batch ends after centipede1. Next worker owns skool1
(longer fights), then grounds1, grounds2, jlair2, potears3 and qlair.

1. Read the relevant production controller and route. Approach sources:
   `src/levels/centipede1/check.rs::drive` (Flora), `src/school_route.rs::run`
   (skool1), `src/levels/grounds2/route.rs`, `src/jabberwock/check.rs` and its
   route helpers (grounds1/jlair2 registrations delegate to Jabberwock),
   `src/duchess_check.rs` (potears3), `src/levels/qlair/check.rs::drive_inner`.
   Borrow a short waypoint approach, not their separate 120 Hz controls/timing.
2. Extend `src/replay/pilot.rs`: supported-map match, per-map waypoint list, combat
   distance/weapon choice, bounded time/retry/stall limits and completion. Commands
   must remain Frame input only. Never edit player/controller state or game data.
   Keep 0.3 s neutral input after checkpoint restoration: Input::suppress latches
   each stick until neutral. Keep menu stop/manual Start and all default flags off.
3. Targets are gathered at the top of `src/viewer.rs`, immediately before
   Session::begin_step. Compare its production combat target gather later in that file:
   NPCs, Duchess, encounter/school controllers and registered level targets.
   `src/route/fight.rs::engaged`, `src/npc/route_threats.rs` and
   LevelController::hostile_target show filtering. Puzzle shot surfaces, ice,
   allies and protected pupils are not automatic hostile aim targets. Hatter2 needs
   fight-phase gating; do not apply its action-music gate blindly to Flora/other maps.
4. Add read-only health telemetry. The viewer currently passes the registered
   controller snapshot whose id equals the map. Hatter2 uses boss.health;
   centipede1 tracks ants[].ant.id/health at 1 Hz. Legacy skool1/potears3 owners may
   require encounters/NPC/Duchess snapshots instead. Key health by stable identity,
   clear tracking after retry, ignore health increases, and document sampling limits.
   Contacts are not proof of applied damage. Do not silently count target disappearance
   as a kill or let respawn/array-index reuse masquerade as health loss.
5. Add the map to the choices in prove-host.py and make-config.py and to the accepted
   chapter starts in record.py. Extend the map-specific health analysis in prove-host.py.
   Generate separate test configs outside Git. Never change a shipped config.
6. Verify HEAD, df >=41 GiB, then build-host.py. It clones the shared cache into the
   worktree's disposable target-pilot, uses opt-level 1/no LTO for iteration, and copies
   a hash-pinned executable before returning. Builds and exactness runs take no
   mac-timing.lock. That lock is for actual quiet-machine wall-clock measurements.
7. Run prove-host.py --binary ABS --map MAP --out NEW_EXTERNAL_DIRECTORY. Inspect the
   real record/run.log: approach displacement, fight_start, actual sampled enemy health
   loss, player damage after fight_start, contacts, retries and clean reason=menu.
   The same driver then launches a new process with replay_fight_pilot=false and the
   original recorded tape. Require full-tape done PASS, all checks matched, diff=0,
   event_mismatches=0. Header preferences carry the recorded controls; the pilot must
   not be invoked during replay. Preserve negative runs; do not weaken proof assertions.
8. Revalidate hatter2/centipede1 if shared input/combat behavior changes. Once a batch
   is proven, commit clean inputs and run build-vita.py once. It makes a private test
   SELF/VPK using the pinned RC14 no-prelude archive. Keep the original pinned kit
   untouched. Verify hashes and gates, update report/status, push and remove private
   target-pilot and target-vita caches at handoff. Root alone operates the console.

Pitfalls actually encountered:

- Pinned miniquad key codes are X11 values: LeftControl=0xffe3, not GLFW 341. Read
  the tape header key table; do not import an unrelated key-code numbering scheme.
- Cached injected Vita sticks must be used only during RECORD/REPLAY. After stop,
  real physical input must resume. test_native_pad.py extracts the production native
  function and tests it with an inert local SceCtrl stub; no device is contacted.
- Centipede1 exposed the post-retry stick neutral latch; uninterrupted non-neutral
  movement can leave Alice stationary despite movement frames in the tape.
- Flora's big Centipede is not the first hostile fight target. The first fights use
  registered Ant targets. The old partial replay digest can print hostiles=0 even
  while controller enemies fight; full state hashes and owner health matter.
- Cards funding is finite. Flora falls back to the ordinary Knife through D-pad
  cycling, rather than repeatedly selecting unfunded Cards. No inventory/stat edits.
- Check-sampled path includes traversal and retry displacement; report it as sampled,
  and separate the full fight span from visible-target combat seconds. Player damage
  after fight start can include falls; do not claim every point was an enemy hit.
- A host-generated tape has platform=host. Root must record a new platform=vita tape
  with the private SELF, then derive native windows. Do not reuse host frame numbers.
- The old production native cache path was archived. build-vita.py uses the retained
  RC15 cache as a seed only, copies it privately, and pins the RC14 archive separately.
- A successful SELF build is not native play/replay qualification. No console result,
  p95, CPU planning cost or full-map release verdict is inferred from these host proofs.
