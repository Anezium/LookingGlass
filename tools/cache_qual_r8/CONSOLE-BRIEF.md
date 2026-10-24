# Alice cache qualification r8: sealed console handoff, NOT EXECUTED

Another agent owns the Vita. Preparation does not confer console ownership.
OUT=/Users/anezium/vita-j0/resume-2026-10-24/alice-cache-qual-r8
PY=/Users/anezium/vita-j0/venv/bin/python
Shell=/opt/homebrew/bin/bash
Read DESIGN.md, AMENDMENTS.md, SOL-REPORT.md, PACKAGE-MATRIX.json and CONSOLE-PROTOCOL.json.

## Admission and offline rehearsal

Before any route lookup, Vita port or shot-driver action, obtain explicit sole ownership from the current console owner. Never steal an active session. Use the unchanged 12-hour driver-v5 with auto_restart=false; do not restart, replace or extend it. Reserve 120 minutes including 30 for restoration, with more than 30 minutes to fixed expiry after planned completion. If fewer than 120 minutes remain under the 150-minute session cap, do not admit. At every arm/menu boundary recheck the admitted fixed identity and remaining restoration margin.

Run the mandatory offline main-path test first, outputs OUTSIDE OUT:

    PYTHONDONTWRITEBYTECODE=1 "$PY" "$OUT/scripts/test_session_e2e.py" "$OUT" --output /tmp/alice-r8-owner-e2e.json

Require PASS_ORIGINAL_STATE_VERIFIED and real_package_session_path_absent=true. Test the loose package and --zip replay as retained in proof/. It uses synthetic transport and driver only. The real path OUT/session-s8-cache-qual must be absent, including symlinks. Never ship or reuse a session directory.

Exclusively create exactly OUT/session-s8-cache-qual locally, copy only OUT/console/*.py, then run scripts/bind_helpers.py --root ROOT --package OUT. Fresh helpers must byte-match. Never copy old inventories, original saves, ledgers or ownership markers.

Before any device write, use check-session-budget.py with --admit ROOT/DRIVER-ADMISSION.json --launches 2 --pandemonium-launches 2 --preservation-minutes 64.5 and the current session-start timestamp. This reserves 120 minutes: 64.5 preparation/navigation, 18 launch allowance, 4.5 SELF stage, 3 Pandemonium allowance, 30 restoration. At later boundaries use --admitted, actual remaining launches, --self-stages 0, --preservation-minutes equal to remaining navigation allowance; never spend restoration time on extra trials. No untested extra launch variant.

After sole ownership is explicitly handed over, run ROOT/admit.py --sole-console-owner. It checks route-to-Vita uses en0 and records ownership. Preparation must never execute this helper. Inspect fresh HOME, take/acknowledge its image, then verify-human-entry.py, preserve.py, baseline-r3.py and save-proof.py. Require the current 37,371,674-byte human SELF SHA-256 387ccf678251d4267b8ced53e207830358826f8e0642631a0459c6b0b9939990 and 181,900-byte native Pandemonium Auto SHA-256 ea4cd221d9f950e613188ccf3ecf2c1eabb1d34106f413125864320d364c8fa9 from TWO fresh independent reads. Logical visit pandemonium$first differs from runtime map pandemonium. Preserve all configs, private metadata, saves, route-saves, thumbnails, census/qualification exports, their original absences, SFO, directories and the shared vita-stage index.

FTP is serial via the existing locked Session/stager; close it before every driver action. No MKD, parallel FTP, arbitrary directory creation, base transfers or game assets. Stage only the common SELF once at ROOT/self-stage-ledger.json using session.stage_manifest(stage-baseline-self.json,...). Per launch use session.py stage LABEL, verify-arm-self.py LABEL, then operate.py LABEL launch. Validate or start the copied passive host receiver only after admission; never replace an unrelated listener. Never execute a console helper during Mac preparation.

## Two-arm protocol

Run exactly s8-cache-qual-1-A (baseline, cache off) then s8-cache-qual-2-B (cache on). Same common SELF, diagnostic recorder on, phase_census=false, off-main=false, profile_mode=full. A and B configs differ only audio_static_cache=false/true. Restore all saves from the fresh baseline before each arm and before each Auto activation. `session.py saves TAG` retains every changed/new save twice before restoring bytes and deleting only newly created files. No hand-edited/synthetic save, positional teleport or cheat. No engine script-input override.

Normal native UI navigation is operated and inspected by the console owner. `operate.py LABEL button KEY [COUNT]` accepts up to eight identical menu steps and returns an image; `ack.py 'inspection'` is mandatory before the next action. Touch/chapter selection can use the existing pilot library after inspecting the current chooser; use Chapters 32 for fresh Hedge3. Inspect the map/paused entry before measurement. The exact UI path is not assumed after a save carousel selection; Auto may already be selected on the second visit. The native Main Load/Save entry is index 1; Auto is slot index 5 and Load is index 6. On a live session Load requires its normal confirmation. Inspect and acknowledge that confirmation before activation.

For each arm, perform these windows IN ORDER; no screenshots/FTP during a running movement window:

1. Enter fresh Hedge3 (Chapters 32). At its paused overlay use `operate.py LABEL move hedge3-move circle` (use cross only if inspection shows that resume action). It starts the recorder in the existing pause/modal state, waits for native acknowledgement, resumes, performs exactly 16 (64,128,2s)/(192,128,2s) left-stick pairs, opens the normal Start menu and stops/exports. These are unsigned byte axes centered on 128. Inspect/acknowledge the final image. Native rows must prove >=60 seconds of actual listener movement; 64 seconds of pad holds alone is insufficient.
2. From the inspected Main pause menu use `operate.py LABEL check hedge3-check circle`. Five seconds stationary, stride 64. Inspect/acknowledge. The sampled check is mandatory for B; A records its uncached control. It never enters the movement timing population.
3. With warm Hedge3 cache, restore baseline saves using `session.py saves TAG`, then use native Load/Save to activate the preserved Auto into Pandemonium. Inspect/acknowledge. This is the first warm map change and a native save load. Resume from the actual paused overlay with `operate.py LABEL move pandemonium-move circle`, then `check pandemonium-check circle`, acknowledging both end images. Do not skip movement requirements if the restored pose is blocked or cinematic: fail the trial and restore.
4. With warm Pandemonium cache, preserve/restore saves again and activate the SAME Auto through the native menu. This is the required SAME-MAP reload. Inspect its paused restored state, then `operate.py LABEL post reload-post circle` for five seconds. Inspect/acknowledge.
5. With cache warmed again, use the existing chapter chooser to enter fresh Hedge3. Inspect its paused entry, then `operate.py LABEL post transition-post circle` for five seconds. Inspect/acknowledge. This independently exercises chapter map replacement. Natural traversal exits use the same Audio::load path but are not claimed exercised.
6. Quit using the native Quit/Yes UI, retaining its normal autosave behavior. `operate.py LABEL wait-exit` waits for Viewer closed cleanly AND the final post-finish qualification export. Inspect/acknowledge HOME. Do not issue redundant vita_quit to an already closed title.

If circle is not Resume in an inspected native menu, choose the existing Back to Game action, or pass cross only when it selects Resume. Never guess an uninspected confirmation. The pilot movement command itself is sealed and identical in both arms; do not adapt it after results. If entry is unsafe, blocked or insufficiently moving, qualification is not complete; preserve all evidence and restore. Death/respawn is excluded, because no safe repeatable route has been established; native reload satisfies the mandatory alternative.

## Collection and fixed reading rules

After clean exit run session.py collect LABEL, scripts/log-integrity.py LABEL --root ROOT, and audit.py LABEL. The whole raw log must have complete atomic records; scan every byte for orphan/interleaved records. Equal maximum renderer config-record length within each arm/session; A-minus-B must equal exactly one byte. Never reconstruct, trim or exempt a corrupted record. This is the r5-r6 arm-aware rule. The new recorder flag is identical in both arms and absent by default.

During motion, helpers check the passive host log after each two-second pulse. Stop further measurement on underrun_estimates>0, music underrun_frames>0, pcm_starved_blocks/rejected_sinks/dropped_reports>0, producer p95 >=8 ms, heap >180 MiB, crash/OOM, recorder error or lost acknowledgement. Release inputs, preserve evidence, abort only if still running, then restore. No automatic replay after a driver timeout. Late callbacks are REPORT-ONLY, including nonzero counts: retain raw reporter windows and their per-run sum as BENCHMARK_LATE_CALLBACKS. Do not convert late callbacks into underruns or gate them.

The audit requires all three audio reporter groups and every numeric health field in every window. Missing/corrupt counters are UNKNOWN/fail. Gate the entire interval from first start through last stop too, including intervening load/reload, menus and exports. Loading before first start and shutdown after last stop are reported separately. Intentional silence is never subtracted. Record actual audible observations separately; automated counters do not establish listening quality.

Heap is sampled from newlib inside EACH recorded active frame and at before-drop, after-drop, cache reset and post-audio-load events. This is the relevant window heap, never the whole-run maximum substituted for it. Every moving/post-load window must be <=180 MiB. Match each A/B transition/load phase in order, including same-map reload, and require every B-A sampled maximum <=+1 MiB. The old geometry must have zero entries and no retained pin after every reset; the next scene must differ and each warm load must release its last old geometry reference. Any missing warm/reset/scene/heap proof fails. Sampled maxima do not establish continuous peaks.

All native self-check mismatch counts must be ZERO. Both B check windows must check at least one actual hit; zero checked hits is UNKNOWN. Movement windows must have stride zero. Report per-class hits/misses/evictions/hit rate and prepare_acoustics mean/median/p95/max descriptively, along with measured listener duration/path and command receipts. Do not gate hit rate or turn host times into a Vita forecast. Run audit.py report after both arms; it never switches the runtime default. Default-on still needs human review of the combined r6 idle evidence and this new hardware qualification.

After A, restore-arm.py LABEL restores only its small ledgers. Preserve created saves before the next arm. No extra console run is authorized by this package. Every collected export is read twice; stale prior/other-arm bytes reject the arm. Pre-existing census files and their initial absences are preserved/restored even though census is off in r8.

## Exhaustive restoration and handback

After final HOME inspection call session.py restore. It reverses all ledgers, verifies the human SELF twice, preserves changed originals and ALL newly created saves/exports with two independent reads, restores every original byte/absence, removes only inventoried new allowed files, and verifies all original inventories and the shared stage index. SFO must remain unchanged; do not write it if it differs. Stop on unexpected directories or unknown new files.

Require RESTORE-FINAL.json status PASS_ORIGINAL_STATE_VERIFIED, restored human SELF SHA-256 387ccf678251d4267b8ced53e207830358826f8e0642631a0459c6b0b9939990 and human Auto SHA-256 ea4cd221d9f950e613188ccf3ecf2c1eabb1d34106f413125864320d364c8fa9, plus byte identity of every original companion/manual/route save and removal of all newly created saves only after retained readback. Record final independently verified inventories and cache index. Inspect HOME, release inputs, write owner-returned.json and explicitly hand ownership back. Do not mark this gameplay qualification passed if restoration is incomplete.

This brief is sealed before device data. Corrections go in separate amendment files; never edit this sealed brief.
