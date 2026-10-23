# Console handoff: candidate lever matrix

**Prepared offline on the Mac. Another agent owns the Vita for P.T. No Vita or driver contact was made for this preparation.** Do not act until that owner has exhaustively restored, inspected HOME and explicitly handed back sole ownership. Historical ownership receipts are insufficient. This brief replaces the old matched-observer S2–S4 plans. Read DESIGN.md, CONSOLE-PROTOCOL.json, PACKAGE-MATRIX.json and the per-package VERIFICATION.json files first.

OUT is `/Users/anezium/vita-j0/resume-2026-10-23/alice-lever-matrix`. Python is `/Users/anezium/vita-j0/venv/bin/python`. Use only OUT's Round 3-derived `console/*.py`, copied as code into a **fresh** direct child `session-s2-world`, `session-s3-fx` or `session-s4-audio`. Repeat directories need a new name. Never copy historical baselines, saves, inventories, ledgers, evidence, owner-returned markers or completed statuses. ROOT is the copied helper directory; PACKAGE is ROOT.parent. All protocol statuses are PENDING.

Before ownership admission, create the local session and qualify its binding:

```bash
PY=/Users/anezium/vita-j0/venv/bin/python
OUT=/Users/anezium/vita-j0/resume-2026-10-23/alice-lever-matrix
SESSION="$OUT/session-s2-world"
mkdir "$SESSION"
cp "$OUT/console/"*.py "$SESSION/"
PYTHONDONTWRITEBYTECODE=1 "$PY" "$OUT/scripts/dry-run-console.py" --root "$SESSION" --output "$SESSION/HELPER-BINDING.json"
```

Local mkdir must fail if the directory exists. A failed binding check blocks admission. The offline runner imports and exercises every helper with transport blocked/mocked; it starts no receiver and invokes no driver. Use `/opt/homebrew/bin/bash` for Mac shell commands. Do not use historical campaign/finalize scripts.

## Ownership, route and driver

Only after sole-ownership handback, before any device action, run:

```bash
/sbin/route -n get 192.168.1.23
```

Require `interface: en0`. If it is `utun*`, use the prior amendment **without sudo**, then repeat the check:

```bash
/Applications/Tailscale.app/Contents/MacOS/Tailscale set --exit-node-allow-lan-access=true
/sbin/route -n get 192.168.1.23
```

Any interface other than en0 blocks console work. These commands were not executed during this preparation.

**Driver lifetime.** The pilot never restarts the driver; an orchestrator restart between sessions is expected and is read at admission. The pilot never starts or extends it either. Between sessions only, after RESTORE-COMPLETE.json and explicit handback, the orchestrator may restart driver-v5 in place, archiving the old receipt and giving a fresh 12 h lifetime. No other controller or title is allowed. Read `/Users/anezium/vita-j0/resume-2026-10-21/timing-and-exit-followup/driver-v5/DRIVER-READY.json` at admission (or the receipt path named at handback, using `--driver-ready`). Do not use a hard-coded expiry. The budget helper checks receipt lifetime, disabled auto-restart, live local PID and `terminal-driver.py` command line without a driver call. Confirm current readiness and ownership separately.

Admit with `check-session-budget.py --session-start <ISO> --admit "$SESSION/DRIVER-ADMISSION.json"`. This exclusively records pid, started and fixed_expiry; an existing admission file is never overwritten. Planned end **including 30 min restoration** must be **more than 30 min before expiry**. The helper prints `latest_admission_for_one_session = fixed_expiry − 157.5 min`. Before every arm and at every pair boundary, rerun with `--admitted "$SESSION/DRIVER-ADMISSION.json"`, the same session start and actual remaining counts. If any driver identity field changes, stop measurement at that safe boundary and restore with the current ready driver. If none is ready, stop and report. Never wait for or ask for a restart mid-session. Sessions that do not fit remain PENDING for the next driver lifetime.

Exact call form, for the later admitted operator only:

```bash
/Users/anezium/vita-j0/venv/bin/python /Users/anezium/vita-j0/resume-2026-10-21/timing-and-exit-followup/driver-v5/driver-call.py <tool> '<json>'
```

Example single call after admission: `vita_shot '{"name":"lever-matrix-initial-home"}'`. Allowed tools are vita_shot, vita_cmd, vita_press, vita_launch and vita_quit. No direct driver RPC or input replay after a missing/ambiguous reply. Pilot retains the Round 3 exact-response recovery without replay.

Use `ui.py` and `ack.py` for image interlocks. Inspect/hash/label and acknowledge every returned screenshot before another action. Required images: initial HOME, title/menu, chapter selection or Auto/Load selection, actual entry pose, paused resume control, normal-exit HOME, final restored HOME. Never take screenshots during the armed benchmark. Root UI actions are limited to button/close/shot; experiment actions require an **exact, unique protocol label**. Unknown/modified labels fail before input; A/B substrings or suffixes never choose audio policy.

## Transport and prohibited operations

**Serial FTP only**, one locked connection at a time, including readbacks/restoration. Use `/Users/anezium/vita-j0/bin/vita-stage`, `vita-ftp` or `vita_ftp.Session`; preserve the shared lock and persistent index `/Users/anezium/vita-j0/cache/vita-stage-index.json`. No raw/parallel FTP. Invoke vita-stage/vita_ftp.py with the Python above; vita-ftp is a `/bin/sh` wrapper. Close FTP before launch, resume, measurement and exit.

**No USB, reboot (including recovery reboot), MKD, remote directory creation/deletion, VPK installation, SFO/sce_sys writes, storagemgr/config.txt edits or other-title control.** No exception for recovery. Stop, retain evidence and report if the allowed workflow cannot continue. Never delete a directory, foreign recovery file or another owner's data. All remote directories, especially `ux0:/data/alice/private/frame_profile/`, must already exist.

Within the app installation, stage **only `ux0:/app/ALICELG01/eboot.bin`**. Every `stage-<config-arm>-self.json` contains that sole app path. Never transfer ELF, VELF, VPK, assets or metadata. Data config/mode manifests are separate and limited to four private data files. Inspect manifests and absolute local paths before use. No stager setup that creates remote directories. No `/Users/anezium/vita-j0/tools/fast-verify` integration or invocation.

## Fresh preservation before any write

After route/driver/ownership/image admission, run fresh-session `verify-human-entry.py`: it independently reads fixed eboot.bin twice. Human SELF must be **37,371,674 bytes**, SHA-256 **`387ccf678251d4267b8ced53e207830358826f8e0642631a0459c6b0b9939990`**. A mismatch blocks entry; resolve with the current owner. Never replace it with a historical backup or an experimental package.

Run `preserve.py`, then `baseline-r3.py` and `save-proof.py`, with no interleaved writes. The historical inventory file read by preserve supplies **directory names only**, never preserved bytes. Record fresh inventories, independent readbacks, initial absences, configs, profile_mode, preferences/audio settings, all saves/route-saves and companions, existing reports/doors companions and SFO identity. SESSION.json records all four current package hashes. Preserve the real save even before Chapters. Reuse an existing TCP 9999 listener only if `lsof -p <PID>` shows that exact process has `/Users/anezium/vita-j0/work/vita.log` open. The UI launch helper enforces this local provenance check. A listener without that file blocks launch: report it and do not replace it or attempt to bind the occupied port. If no listener exists, start the copied `host-log-receiver.py` only after admission, and keep its readiness receipt. It appends `/Users/anezium/vita-j0/work/vita.log`; retain launch offsets.

The only admitted human autosave is **181,900 bytes**, SHA-256 **`ea4cd221d9f950e613188ccf3ecf2c1eabb1d34106f413125864320d364c8fa9`**, logical visit **`pandemonium$first`**, canonical saved map **`pandemonium`**. `save-proof.py` verifies the two independent current copies and all companions; PANDEMONIUM-ENTRY.json freezes this session's unit. Changed, absent or unloadable data blocks entry. No historical/session backup, synthetic save, rename, Chapters, Village or forced progression may substitute for the actual current save.

## Session admission and order

S2 world, S3 FX, S4 audio each run Pandemonium ABBA followed by Hedge3 ABBA, then fully restore and hand back. Each session is a fresh cycle. A is candidate baseline; B is the current session's treatment. The binaries in `packages/baseline`, `world`, `fx`, `audio` are identical; per-arm configs define the treatment. World enables only material sort/static merge/uniform persist. Both world GL interception caches remain off. FX enables only fx_cpu_inline; audio enables only audio_offmain. No per-GL observers in any arm.

Use the **exact labels in CONSOLE-PROTOCOL.json**, for example S2 Pandemonium:

1. `s2-world-pandemonium-1-A` baseline
2. `s2-world-pandemonium-2-B` world
3. `s2-world-pandemonium-3-B` world
4. `s2-world-pandemonium-4-A` baseline

Then the four `s2-world-hedge3-*` labels in ABBA order. S3 and S4 use their own protocol labels. All launches are fresh. Never share an A run or pool absolute medians across blocks/sessions/maps.

Budget **127.5 min per session**, including 15 preservation, one 4.5 SELF stage, 8×9 launches, 4×1.5 Pandemonium handling and **30 restoration**. Cap 150 minutes. Three sessions need 382.5 minutes in total. The receipt at each admission supplies expiry; each session must independently fit its current driver lifetime. Leave whole unadmitted sessions PENDING.

At admission, record ISO session start and run copied `check-session-budget.py --session-start <ISO> --admit "$SESSION/DRIVER-ADMISSION.json"` (defaults admit the entire 8-launch session). At each arm/pair boundary use `--admitted "$SESSION/DRIVER-ADMISSION.json"` with actual **remaining whole session** counts. Example after setup before the first launch: `--launches 8 --self-stages 0 --pandemonium-launches 4 --preservation-minutes 0`. Before Hedge3: launches 4, Pandemonium launches 0, both setup allowances 0. The helper enforces restoration, strict >30 min expiry margin, unchanged driver identity and the cap. Slower actual pace requires stopping/restoring; elapsed time is never approval. No partial block carryover or automatic repeat.

**Accepted amendment 7:** if Pandemonium returns INCONCLUSIVE_REPEAT_REQUIRED while every evidence, audio, identity and save gate passes, continue to Hedge3 when the budget helper admits the remaining four launches with the same driver identity. Stop before Hedge3 on FAIL_WALL_GATE, invalid evidence, a stop rule or budget refusal. The comparator exits 2 for both wall failure and inconclusive results: inspect its JSON status rather than treating exit 2 alone as a stop. The lever's overall verdict stays unresolved when either map is inconclusive.

## SELF staging and each launch

Once preservation passes, stage the common executable **once per session**, using `stage-baseline-self.json` with ROOT/**self-stage-ledger.json**. Keep all SELF ledgers at ROOT and names ending `ledger.json`; never inside a RUN. The original in this first ledger must be the freshly verified human SELF. No further SELF stage is needed because all package bytes match. Verify the local package SHA against VERIFICATION.json before transfer and the fixed path independently afterward. Every fresh launch runs `verify-arm-self.py LABEL` to produce RUN/SELF-IDENTITY.json and a fresh fixed-path readback. UI launch checks its status/hash/size against that label's package.

Use library `session.stage(LABEL,'A','full')` or role B in the fresh session. It resolves exact protocol identity to `stage-baseline-full.json`, `stage-world-full.json`, `stage-fx-full.json` or `stage-audio-full.json`, performs independent fixed-path config readbacks and records STAGE.json. For Pandemonium it first restores and twice independently verifies this session's frozen actual save/companion unit and writes SAVE-IDENTITY.json. Never edit a startup switch in a running arm. `session.py` is library-only, not a campaign CLI.

Launch only ALICELG01 with `ui.py LABEL launch`. Enter in `profile_mode=full`, profiler off. Hedge3 uses zero-based Chapters 32 and inspected Begin. Pandemonium uses the native **Load Save → Auto → Load** menu: copied `load-auto` navigation stops on Load without activating. Inspect/ack that selection, activate the actual Load with the displayed control (previous native run used Cross), and record ENTRY-ACTIVATION.json. **Never issue `ui.py LABEL begin` on Pandemonium**; the helper now rejects it before input. The loaded real save previously returned to a paused overlay displaying **Cross or Start** to resume. Inspect the actual current overlay and use its displayed control. Hedge3 prior pause menus used Circle; do not assume that for native Load. Cross in another menu can open New Game.

Capture/inspect actual map, pose and camera; record entry-to-resume timing, not an unsupported complete simulation identity. Use passive host log capture and `entry-state.py LABEL` before arming as needed. `arm-mode-only.py LABEL` first checks canonical map-load and native restored-scene receipts, plus the save receipt for Pandemonium. Only then does it stage `stage-fullbench-mode-only.json`, independently read all four configs, and save every input to RUN/inputs. It does not rewrite unchanged renderer settings while paused.

Always pass the inspected control explicitly: Pandemonium previously displayed Cross/Start, so use `ui.py LABEL resume cross` when that overlay is confirmed; Hedge3 previously displayed Circle, so use `ui.py LABEL resume circle` when confirmed. A changed overlay requires fresh inspection and the displayed supported control; never guess. The helper rejects omitted controls before input. This releases inputs and passively observes logs until fullbench done and clean exit. From arming/resume through termination: no movement/look/fire, FTP, screenshots, device polling or active diagnostics. Warmup 60 accepted frames, measured 120, then natural export/exit. Missing termination, report failure or stop rule means retain failure and restore; do not retry that arm.

## Collection and independent gates

After clean exit and inspected/acknowledged HOME, call library `session.collect(LABEL)`, `entry-state.py LABEL`, `audit-run.py LABEL --root ROOT --package OUT`, and `strict-audio.py LABEL --root ROOT`. Collection makes serial independent export readbacks, canonical filenames, 120 positive wall rows numbered 1..120 and exact doors coverage. It compares the light CSV SHA-256 against every earlier COLLECT.json in this session; an identical hash writes COLLECT-INVALID.json and rejects the run as a stale export. Stop and restore; retain both reads and the earlier receipt. Audit requires sealed SELF, exact logged/read-back configs, actual clocks, retained/buffered policy, zero-hook receipt and blank non-wall/doors fields. `profiler=light` in the historical done line does not override the required profiler-off config.

After `audit-run.py`, inspect `offmain_lines` and `offmain_audit` in AUDIT.json. Every A and every non-audio run requires `[lg/audio-offmain] requested=false active=false`; audio B requires `requested=true active=true`. Missing or mismatched engagement invalidates evidence: stop and restore. The audit enforces these values. Report raw `submitted`, `reused` and `fallback` counts for each B run, including zero values for non-audio B if emitted; audio B must have counter windows. A nonzero fallback is reported and is not itself a stop.

Audit each run before the next. `restore-arm.py LABEL` unwinds only that run's small config/mode ledgers and reads back configs; it never restores SELF. Keep raw logs, both CSV reads, STAGE/SELF/SAVE identities, inputs, entry observations, QUIET-START, images and acknowledgements. Unknown/full world-age/NPC state, continuous memory peak and listening remain unavailable unless actually established.

All candidate roles require every in-benchmark producer p95 **strictly <8 ms**, raw starvation/rejected sinks/dropped reports/music and CPAL underruns zero, and complete reporter records. Warmup and boundary-overlapping windows are included. No former 606dacf role-A exception remains. Stop/restore for any audio defect, crash/OOM, heap >180 MiB, no progress, report/export/identity/config/map/pose failure. Retain loading/shutdown counters separately; no percentile pooling or intentional-flush subtraction.

For each S2 world map block, visually compare the acknowledged entry-pose images **A1 with B2 and B3 with A4** before interpreting its gate. Record exact labels, image paths and SHA-256 hashes, image acknowledgement references, the inspected world region outside the pause overlay, findings and PASS/INVALID in `ROOT/<block>-WORLD-ENTRY-REVIEW.json`. A visible world difference outside the pause overlay invalidates the block; stop and restore, retain the images, and report a render defect. The final report must state **"render equivalence of the world arm not verified on console"**. Entry-image inspection is limited evidence; host coverage is currently a raster-model test. Prior host PNG equality at b3d3616 covered five switches, not this three-switch config.

**Requirement for later host validation, not a console step:** no world-lever promotion without a host PNG equality check on this exact sealed build (a859cee, identical to parity candidate 5e072af), comparing PL-world.json with PL-baseline.json. Do not add that host run to the admitted console workflow.

After four valid runs, use only `compare-abba.py --root ROOT --block s2-world-pandemonium` (or the exact other block). It validates audits/audio, pairwise pose equality and Pandemonium identities before deciding. Both oriented B−A wall-median deltas must be ≤+1 ms; mean >+1 fails; threshold straddle is inconclusive. Replicated benefit separately needs both deltas <−1 ms. No observer correction, phase/CPU attribution or map pooling. `make-report.py` is diagnostic only and must not decide admission/promotion. A repeat requires a separately admitted whole fresh ABBA with original results retained.

## Exhaustive restoration and handback

Reserve **at least 30 minutes**. On success, failure or planned stop: preserve all evidence, release inputs, close Alice under ownership/image interlocks, then call `session.stage('human-restore','human-restore','full')`. This dispatches `restore-ledgers.py`, `verify-self.py`, `restore-files.py` in order. It is not an experimental arm and cannot produce a launchable STAGE receipt.

Reverse every owned ledger in reverse modification order, including root SELF stage and each arm's small ledgers. Do not replay wedged/uncertain transfers. verify-self compares fixed eboot.bin to ROOT/self-stage-ledger.json's freshly preserved original, checks owned parts absent, then independently reads it **again in a separate locked session** and requires the exact human hash `387ccf678251d4267b8ced53e207830358826f8e0642631a0459c6b0b9939990` and 37,371,674 bytes. Both SELF-INDEPENDENT-VERIFY.json and HUMAN-SELF-SECOND-INDEPENDENT-VERIFY.json must pass.

restore-files preserves changed-file evidence, restores this session's original saves and **every companion**, configs/settings/reports and recorded initial absences, independently reads/hashes all original paths and verifies all initial directory inventories. It may remove only owned new files/reports whose initial absence is recorded; never a directory or foreign recovery file. An SFO change stops **before any write/delete**: do not repair it by writing SFO. Retain ledger-owned cache files according to the existing restoration policy and preserve caches present at admission. No fast verifier replaces any of these reads.

Require RESTORE-FINAL.json PASS_ORIGINAL_STATE_VERIFIED, both human SELF receipts, restored autosave/companion identities and final inventories. Release inputs, leave Alice closed, capture/visually inspect/acknowledge final HOME, record RESTORE-COMPLETE.json and write a report with every measured/unmeasured block, raw failure and restoration proof. Then explicitly hand back sole ownership. No production/default promotion follows this matrix. If restore cannot finish within allowed operations, retain precise state/ledgers and report the unresolved condition; do not use USB, reboot or driver restart.
