# Alice lever matrix r4: console draft, NOT EXECUTED

Prepared on the Mac only. Another agent owns the console. This package makes no claim about Vita wall time, continuous peak heap, audible continuity or console rendering. Do not act until the current owner has exhaustively restored the device, inspected HOME and explicitly handed back sole ownership. Historical ownership receipts do not admit entry.

OUT is `/Users/anezium/vita-j0/resume-2026-10-24/alice-lever-matrix-r4`; Python is `/Users/anezium/vita-j0/venv/bin/python`. Read SOL-REPORT.md, PACKAGE-MATRIX.json, package VERIFICATION.json, DESIGN.md and CONSOLE-PROTOCOL.json. INHERITED-R3-BRIEF.md preserves the detailed operational history and amendments; its OUT, packages, SELF identities and FX/material-sort/uniform-persist sessions are archival. Only this brief's two sessions and exact protocol labels are admitted for later review. All 12 launches are PENDING.

## Two separately admitted sessions

1. S4 audio: Pandemonium A1/B2/B3/A4, then Hedge3 A1/B2/B3/A4, using the new r4 common SELF. A uses PL-baseline.json; B uses PL-audio.json, differing only in audio_offmain=true. The PCM producer and every other audio setting are identical. Require the existing 180 MiB sampled-newlib guard and all strict audio gates. Record loading/preparation/plateau heap receipts and submitted/reused/fallback counts. The former unsafe r3 deep-copy B is never rerun. World transitions are outside the timed window; preserve any transition evidence separately.
2. S5 guarded static merge: independent Pandemonium ABBA on the same r4 SELF, PL-baseline.json versus PL-world-static-merge.json. Only world_static_merge differs. Material-sort remains false. Preserve all rank/cull/alpha/texture/uniform/adjacency/coverage fences. No combined treatment and no pack-only diagnostic is authorized by this draft.

Use fresh A1 AND A4 on the r4 SELF in every block. Never reuse an r3 A run, share A runs between sessions or pool maps, versions or absolute medians. Both original levers and material-sort stay disabled in the baseline. No promotion follows this package.

S4 budget: 127.5 minutes (15 preservation + 4.5 SELF stage + 8x9 launches + 4x1.5 Pandemonium handling + 30 restoration). Static merge: 91.5 minutes (same setup/restoration, four launches and four Pandemonium handling allowances). Total 219 minutes across separate preservation/restoration/handback cycles; each independently below 150 minutes. Do not admit one combined driver session.

## Fresh helper binding, ownership and driver admission

Create a fresh direct child `session-s4-audio` or `session-s5-world-static-merge`. Local mkdir must fail if it already exists. Copy only OUT/console/*.py, never evidence, saves, baselines, ledgers, inventories, completed status or ownership markers. ROOT is that copied directory; PACKAGE is its parent. Run the offline binding first:

```bash
PY=/Users/anezium/vita-j0/venv/bin/python
OUT=/Users/anezium/vita-j0/resume-2026-10-24/alice-lever-matrix-r4
SESSION="$OUT/session-s4-audio"
mkdir "$SESSION"
cp "$OUT/console/"*.py "$SESSION/"
PYTHONDONTWRITEBYTECODE=1 "$PY" "$OUT/scripts/dry-run-console.py" --root "$SESSION" --output "$SESSION/HELPER-BINDING.json"
```

An offline fixture PASS does not grant ownership or qualify console performance. Use `/opt/homebrew/bin/bash` on the Mac. Do not use historical campaign/finalize helpers.

Only after explicit sole-ownership handback, before device action, run `/sbin/route -n get 192.168.1.23`; require `interface: en0`. For utun, the inherited amendment allows `/Applications/Tailscale.app/Contents/MacOS/Tailscale set --exit-node-allow-lan-access=true` without sudo, then repeat the route check. Any non-en0 route blocks entry. None of these commands was run in r4 preparation.

Read the CURRENT driver-v5 DRIVER-READY.json at admission (or the receipt named by the owner). The pilot never starts/restarts/extends a driver, never waits for a mid-session restart, and never substitutes historical PID/expiry values. check-session-budget.py verifies live local PID, exact terminal-driver.py command, 43,200-second fixed lifetime and disabled auto-restart without a driver call. Exclusively record DRIVER-ADMISSION.json with --session-start ISO --admit PATH. Require planned completion INCLUDING 30 minutes restoration strictly more than 30 minutes before fixed expiry.

For S4 use eight-launch defaults; for static merge pass `--launches 4 --self-stages 1 --pandemonium-launches 4 --preservation-minutes 15`. Latest starts are expiry minus 157.5 and 121.5 minutes respectively. At every arm/pair boundary rerun with --admitted, same session start and actual ENTIRE remainder. After setup use self-stages=0, preservation-minutes=0. Decrement launches/Pandemonium launches only after completion. Before Hedge3 use launches=4 and pandemonium-launches=0. Changed driver identity, insufficient remainder or failed evidence stops measurements at a safe boundary and requires restoration. Only the orchestrator may restart between fully restored, handed-back sessions.

Use ui.py and ack.py for screenshot interlocks. Inspect/hash/label and acknowledge each image before another action. Required: initial HOME, title/menu, actual save or chapter selection, actual entry pose, paused resume control, exit HOME and restored final HOME. Never screenshot during armed timing. Only ALICELG01 and allowed vita_shot/vita_cmd/vita_press/vita_launch/vita_quit calls; no other controller/title, direct RPC or input replay after ambiguous response. Inherited exact-response recovery remains in pilot.py.

## Serial transport and preservation

Only serial locked FTP via `/Users/anezium/vita-j0/bin/vita-stage`, vita-ftp or vita_ftp.Session. Preserve the shared lock and `/Users/anezium/vita-j0/cache/vita-stage-index.json`; one connection at a time, independent readbacks/restoration included. Close FTP before launch/resume/timing/exit. No raw or parallel FTP, MKD, remote directory creation/deletion, USB, reboot/recovery reboot, VPK installation, SFO/sce_sys writes, storagemgr/config.txt changes or fast-verify. No recovery exception. Stop and report if allowed operations cannot proceed.

All remote directories, especially frame_profile, must already exist. Inside the app, stage ONLY `ux0:/app/ALICELG01/eboot.bin`. ELF/VELF/VPK are Mac artifacts and must not be transferred. Inspect absolute local manifests before use. Full manifests contain only four private data files; mode-only arms only profile_mode.

Before ANY write, verify-human-entry.py must independently read human SELF twice: 37,371,674 bytes, SHA-256 `387ccf678251d4267b8ced53e207830358826f8e0642631a0459c6b0b9939990`. Mismatch blocks entry. Then run preserve.py, baseline-r3.py and save-proof.py without interleaved writes. Preserve fresh original bytes, inventories, independent readbacks, initial absences, preferences/audio/configs/modes, all saves/route saves and companions, reports/doors companions, caches and SFO identity. Historical inventories provide directory names only. SESSION.json records current r4 package identities.

The sole measurement autosave is 181,900 bytes, SHA-256 `ea4cd221d9f950e613188ccf3ecf2c1eabb1d34106f413125864320d364c8fa9`, logical visit pandemonium$first, canonical map pandemonium. PANDEMONIUM-ENTRY.json freezes this session's actual verified save and every companion. Restore/twice independently verify that unit before EVERY Pandemonium launch. Missing, changed or unloadable saves block entry. No historical backup, synthetic input/save, rename, Village, Chapters or progression substitute. Finish Pandemonium before Chapters.

A reused TCP 9999 listener must have exact process provenance: lsof -p PID shows `/Users/anezium/vita-j0/work/vita.log` open. A listener without that file blocks launch. Do not replace it or bind an occupied port. Start the copied host-log-receiver.py only after admission if no listener exists; retain readiness and launch offsets.

## Staging, launch and quiet window

After preservation, verify package hash and stage common SELF ONCE with stage-baseline-self.json, ROOT/self-stage-ledger.json. SELF ledgers remain at ROOT and end ledger.json. Its original must be freshly verified human SELF. Read back the fixed path independently. Every launch requires verify-arm-self.py LABEL and RUN/SELF-IDENTITY.json matching the exact package.

Use library session.stage(LABEL,'A' or 'B','full'), which resolves exact protocol labels, restores/verifies Pandemonium save units and independently reads configs. Never edit startup switches while running. Launch through ui.py LABEL launch with profile_mode=full and profiler off. Pandemonium: native Load Save -> Auto -> Load; inspect/ack selected Load before activating its actual displayed control, record ENTRY-ACTIVATION.json. `ui.py LABEL begin` is prohibited for Pandemonium. Hedge3: zero-based Chapters 32, inspected Begin. Inspect actual map/pose/camera and paused overlay. Record entry timing and state honestly; complete world/NPC age is unknown.

arm-mode-only.py requires canonical map-load/restored-scene receipts and Pandemonium save identity, then stages mode only and retains all four independently read inputs. Resume with an EXPLICIT inspected control: prior native Pandemonium Cross or Start, prior Hedge3 Circle. Never guess from another menu. From arming/resume through termination: release inputs; no movement/look/fire, FTP, screenshots, polling or active diagnostics. Passive host logs only. Keep 60 accepted warmup frames and 120 positive wall intervals, closing frame 120 at the report-only boundary. Natural export/exit only. Missing completion is a stop, never an automatic retry.

## Evidence, logging and gates

After clean exit and acknowledged HOME: session.collect(LABEL); `python OUT/scripts/log-integrity.py LABEL --root ROOT`; entry-state.py; audit-run.py --root ROOT --package OUT; strict-audio.py --root ROOT. Then restore-arm.py before the next arm. Collection requires two independent serial export reads, exact 120 rows numbered 1..120, positive wall and complete doors coverage. Reject any light CSV hash repeated anywhere in the session; retain COLLECT-INVALID and both reads.

Keep r3-1 logging gate: scan the WHOLE runtime-log.txt, loading through shutdown, for `.+\[(lg|cpal|miniquad)/` (tag after column zero) and `^ [a-z_]+=` (orphan fragment). Record counts and EVERY matching line/line number in LOG-INTEGRITY.json and inspect every match. A split/nested record invalidates the run, stops ALL remaining r4 sessions and requires restoration. Require the longest renderer-config record to have the same byte length in every run of the session, preserving r3-1 verbatim. A truncated record is a logging failure. The existing atomic logger is unchanged; no line reconstruction/exclusion is allowed. Retain raw byte lengths explicitly.

Require gl_hooks=0, phase_timers=0, direct GL resolver, profiler-off readback, retained/buffered policies, actual clocks, sealed SELF/configs, and blank non-wall/doors fields. Legacy done text profiler=light does not override profiler off. Missing fields are unavailable, never zero. Every A/non-audio run requires offmain requested=false active=false; audio B true/true. Keep submitted/reused/fallback/raw window counts; nonzero fallback alone is reported, not automatically failure.

Every producer p95 strictly <8 ms, raw starvation/rejected sinks/dropped reports/music and CPAL underruns zero, complete reporter windows INCLUDING warmup/boundary overlaps. No percentile pooling or intentional-flush subtraction. Crash/OOM, sampled heap >180 MiB, no progress or any identity/config/save/pose/export/audio defect stops and restores. Keep loading/shutdown counters separate. Retained heap plateau is sampled, not a continuous peak.

After four valid runs use compare-abba.py --root ROOT --block EXACT. Median sorted[n//2], p95 sorted[floor(.95*n)]. Report raw exports, min/max/sum, B2-A1 and B3-A4 ms/%/mean, and A1-A4 drift. Both oriented deltas <=+1 ms required for non-regression; mean >+1 fails; threshold straddle is inconclusive. Benefit requires both strictly <-1 ms. No observer subtraction, phase attribution, trimming, cross-session pooling or automatic repeat. Pandemonium band is provisional. S4 evidence-valid INCONCLUSIVE_REPEAT_REQUIRED may continue to Hedge3 only if unchanged-driver remainder fits; FAIL_WALL_GATE/invalid/stop rule cannot. Overall audio two-map status remains unresolved if either map is invalid/unmeasured/inconclusive. Static merge ends after Pandemonium with restoration; no two-map claim.

For static merge compare acknowledged A1/B2 and B3/A4 entry images outside overlays, retain hashes/acks/findings in block WORLD-ENTRY-REVIEW.json; visible differences invalidate and stop. State “render equivalence of the world arm not verified on console”. Exact sealed host PNG/state equality is in the r4 report; it does not establish hardware render equivalence. Unknown GPU phases and audible continuity remain unknown.

Retain r3-2 descriptive parity readout in EACH session report: Pandemonium A1/A4 medians, A1-A4 and their mean; old A medians 41.663/42.319 ms (S2) and 41.601 ms (S3), reference mean 41.861 ms. If abs(new A mean - 41.861)>1.0 ms write NEW_SELF_PARITY_FLAG. This is cross-session/cross-SELF, descriptive and NON-GATING, never pooled into a verdict. A separately prepared old-SELF versus r4-SELF ABBA remains required before promotion relying on observer-free qualification, with or without a flag. No old SELF is staged in this draft.

## Exhaustive restoration and handback

Reserve at least 30 minutes. On success, failure or stop retain evidence, release inputs, close Alice under ownership/image interlocks and call session.stage('human-restore','human-restore','full'), dispatching restore-ledgers.py -> verify-self.py -> restore-files.py. Reverse EVERY owned ledger in reverse modification order, including root SELF. Never replay uncertain transfers.

Require independent human SELF verification twice, in separate locked sessions, exact human hash/size above; SELF-INDEPENDENT-VERIFY.json and HUMAN-SELF-SECOND-INDEPENDENT-VERIFY.json both PASS. Restore every original file/save/companion, settings/reports/configs and recorded initial absence. Independently hash/read originals and verify EVERY initial directory inventory. SFO mismatch stops BEFORE write/delete; never repair SFO. Preserve pre-existing caches; handle only ledger-owned new caches per inherited policy. Delete only owned new files whose initial absence is recorded, never a directory or foreign recovery file.

Require RESTORE-FINAL.json PASS_ORIGINAL_STATE_VERIFIED, both human SELF receipts, exact restored save/companions and final inventories. Leave Alice closed, release inputs, inspect/hash/ack final HOME, record RESTORE-COMPLETE.json and explicitly hand back sole ownership. If restoration cannot complete, report exact unresolved state/ledgers; no reboot/USB/driver restart recovery.
