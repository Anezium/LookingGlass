# Alice r10 stack, console handoff, not executed

Mac-only preparation. Another agent owns the console. This package does not admit ownership and does not contact the Vita or the shot driver. Read SOL-REPORT.md, READING-RULES.md, AMENDMENTS-R10.md and CONSOLE-PROTOCOL.json before admission. AMENDMENTS.md and AMENDMENTS-3.md are the inherited r8 procedure. Where they name s9, a single SAH switch, or a 1-byte config delta, AMENDMENTS-R10.md replaces them.

OUT is /Users/anezium/vita-j0/resume-2026-10-24/alice-stack-r10. PY is /Users/anezium/vita-j0/venv/bin/python. Use that interpreter with PYTHONDONTWRITEBYTECODE=1. Do not pass python -I.

One common SELF for every arm. A is PL-baseline.json, all three switches false. B is PL-stack.json, audio_static_cache, collision_sah and skin_actual_consumers all true. Census configs add phase_census=true and do not otherwise differ from their wall arm. phase_census is false on all eight wall arms.

## Order

1. B-only smoke, Hedge3 then Pandemonium, in exact fresh session-s10-stack-load-smoke. Full 60 warmup and 120 measured frames. Not a wall arm. Proves load, one COLLISION_SAH_LOAD, and heap.
2. Restore, then exact fresh session-s10-stack. Hedge3 ABBA, Pandemonium ABBA, then Hedge3 census A and census B.
3. Pandemonium uses the measurement autosave. Hedge3 uses Chapters 32. Do not reuse an older session directory.

Smoke budget: --launches 2 --pandemonium-launches 1, 69 minutes. Main budget: --launches 10 --pandemonium-launches 4, 145.5 minutes, inside a 150 minute cap with more than 30 minutes before driver expiry. Do not invent a nine-launch plan.

## Admission

Both session paths must be absent for the full offline test. After the smoke exists, main admission uses --main-admission, which accepts that smoke and still requires session-s10-stack to be absent. Output of the offline test stays outside OUT.

```sh
PYTHONDONTWRITEBYTECODE=1 "$PY" "$OUT/scripts/test_session_e2e.py" "$OUT" \
  --output /tmp/alice-r10-e2e.json
```

Require PASS_ORIGINAL_STATE_VERIFIED and real_package_session_path_absent=true. Then create exactly one session directory with mkdir and copy only OUT/console/*.py. bind_helpers.py accepts only session-s10-stack and session-s10-stack-load-smoke.

Before any device action, `/sbin/route -n get 192.168.1.23` must show interface en0. Any other interface blocks entry. FTP is serial, one connection, through vita-stage or vita_ftp.Session. No MKD, no remote directory creation, no reboot, no appstate, no storagemgr, no USB, no VPK install. vitacompanion commands are only those in cmd_definitions.c. This run uses launch, quit, press, wait, release, version, help, screen and nosleep. It does not reboot.

## Identity and restoration

Before any write, verify the human SELF twice: 37371674 bytes, SHA-256 387ccf678251d4267b8ced53e207830358826f8e0642631a0459c6b0b9939990. The measurement autosave is 181900 bytes, SHA-256 ea4cd221d9f950e613188ccf3ecf2c1eabb1d34106f413125864320d364c8fa9, visit pandemonium$first. Restore and verify that save before every Pandemonium launch. After the last launch, session.stage('human-restore','human-restore') must end at PASS_ORIGINAL_STATE_VERIFIED. Do not quit a title that has already closed.

Stage only ux0:/app/ALICELG01/eboot.bin inside the application. The VPK contains eboot.bin and sce_sys/param.sfo and no game data. Frame profile directories must already exist.

## Gates while collecting

Whole-log scan, arm-aware config length, census export restoration, benchmark-window heap, and report-only late_callbacks are unchanged from r8 except the length delta. Inside one session, longest(A) minus longest(B) is 3 bytes. A mismatch stops the remaining launches. Census closure: when audio_static_cache is true, hit+miss equals queries. When it is false, those three counters stay zero.

B logs contain exactly one COLLISION_SAH_LOAD. Hedge3 also contains one shared footing line. A second SAH line, a legacy footing line on Hedge3 B, a whole-run sample above 180 MiB, a Hedge3 benchmark plateau at or above 165 MiB, or an oriented benchmark heap delta above +8 MiB stops the run.

Benchmark underrun_estimates, starvation, rejected sinks, dropped reports and music underrun_frames stop. late_callbacks is reported and does not stop. Producer p95 must be strictly under 8 ms.

## Reading

Apply READING-RULES.md before looking at the medians. Both Hedge3 deltas below -1 ms, both Pandemonium deltas at or under +1 ms. Put the wall deltas next to the separate idle gains and label that comparison descriptive. Do not add -4.7, -4.3 and -4.0 and call the sum the result.
