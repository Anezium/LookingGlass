# Fight pilot (private test candidate)

Default off (`replay_fight_pilot=false`); no release config is changed. The current
prototype supports hatter2 only. Read the external REPORT.md before using any build.
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
visible-target combat at 45 seconds, retries at two and a non-scene approach stall at
18 seconds. It stops via ordinary Escape/Start, so `reason=menu` is required.

Health telemetry samples the Hatter controller's existing read-only snapshot at 1 Hz;
weapon-contact output is not by itself proof of health loss. Host proof requires a
positive sampled boss-health reduction and positive player damage, plus an exact replay.
No claimed Vita planning cost or fight performance is inferred from host timing.

## Host proof

Run from this worktree. Builds and exact replay proofs do not use `mac-timing.lock` (root release-week rule).
The host builder APFS-clones the shared cache into this worktree's `target-pilot` to avoid
Cargo contention; remove that cache at handoff. All builders refuse less than 41 GiB free.

```sh
python3 tools/fight-pilot/build-host.py
python3 tools/fight-pilot/prove-host.py --binary /absolute/path/from-host-build.json --out /absolute/new/evidence-directory
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
python3 "$SRC/tools/fight-pilot/make-config.py" --out "$P/configs"
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
python3 "$SRC/tools/fight-pilot/record.py" start --session "$P" --self "$SELF" --config "$P/configs/record-hatter2.json"
# Wait <=240s for the exact clean [lg/replay] recorded fp-hatter2-01.lgtape ... reason=menu line.
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
