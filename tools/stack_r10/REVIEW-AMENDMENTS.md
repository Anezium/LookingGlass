# Review amendments, adopted

Adopted for the r10c re-seal. The text below is the r10 review's amendment list, copied verbatim. `AMENDMENTS-R10C.md` records what this seal changed in code. `CONSOLE-BRIEF.md` and the earlier amendment files are unchanged.

## Amendments (for a separate file, after B1 and B2 are fixed)

1. **Gate the skin fallback before the smoke.** Every run must have zero `[lg/skin] serial fallback` and zero `afterimage consumer must be declared` lines. Add both patterns as a stop in `run_health.health_text` (whole log, every arm) and as an assert in `audit-run.py`. Add the same assert to `tools/skin_exact_r9/host_exact.py` and `tools/stack_r10/host_stack_oracle.py`, and record `serial_fallbacks=0` per run in the reports. A run with a fallback line is invalid for timing and visuals.
2. **Before `skin_actual_consumers` may become default-on, remove the UB at `npc.rs:1117-1118`.** Either make `require_live_skin` and the consumers that call it take `&mut self` (the ambient and resident call sites already hold `&mut Model` after `draw_pose`), or adopt the other r9 option: log and skip the overlay in release. Export the fallback counter outside `cfg(test)`, in the frame report or the census. Add a test that compiles and executes the release branch.
3. **Wording of 8(b).** In SOL-REPORT and DECISIONS, call the fallback "serial replay of this frame's gathered draw". It is not exact when no draw for the model was gathered this frame (`overlap.rs:63,69`), for example an afterimage drawn without a body draw. The release branch has never executed in any test or oracle run.
4. **Host test evidence before the smoke** (host-only, no rebuild). On the `d503c23` tree, run and archive `proof/HOST-TESTS.json` with logs, as r8 and r9 did, for:
   - the r6 audio tests on four maps (with the four SHAs and the non-ignored SHA);
   - the SAH synthetic suite, the shared-footing test and the real-map corpus (mismatch totals and the 5881888 B peak);
   - the actor-publication overflow test, the serial-fallback test and the freshness should-panic tests;
   - the census tests.

   Re-run the all-on versus all-off simulation equality with r8's silent mixer (`LOOKING_GLASS_SAH_SILENT_AUDIO=1`, as in `run_host.py`), so the audio cache does work. Require nonzero `static_cache_hit_miss_eviction` in the all-on census arm.
5. **Proof wording.** State that `STACK-ORACLE.json` and `STACK-SIMULATION-EQUALITY.json` ran with `--no-audio` and prove SAH plus skin only. The map oracle compared per-frame packets and the final PNG. No intermediate images were compared on maps.
6. **Heap gate wording.** In the main session, the +8 MiB oriented delta and the Hedge3 165 MiB plateau are report gates (`HEAP_NOT_CONFIRMED`), not collection stops. The smoke enforces the 165 MiB plateau on Hedge3 only. Whole-run > 180 MiB and crash/OOM stop everywhere. Mark `MEASUREMENT-LIMITS.md`, `CONTRACT.md` and `AMENDMENTS.md` as r8 text superseded where they disagree with `AMENDMENTS-R10.md`.
7. **Pandemonium reading, fixed now before data.** The promotion rule stays at ≤ +1 ms. Also report a descriptive label stating whether both Pandemonium deltas are below −1 ms. A non-regression pass does not show that the r7 skin gain (−4.0 ms) or the r8 SAH gain (−1.6 ms) survived the stack.
8. **Before `collision_sah` may become default-on,** pass the loaded world's static geometry from `campaign::arrive` (`campaign.rs:331`) and any other caller that holds a world. Host-check one normal exit into Hedge3 for a single SAH line and no throwaway hull build.
9. **Census closure.** "Cache on" closure has not been observed with SAH and skin also on, on host or console. If census B fails `census_report.validate`, it is the last launch. Treat it as a diagnostic failure, not a wall invalidation, as with r8's census A.
10. **Provenance.**
    - Future packages must record the host binary's source tree and keep the receipt for any claimed second native build.
    - In DECISIONS, map the package's `84b93e1` to `d503c23`, with the six tree-equal commit pairs above.
    - The smoke must report the single-build Hedge3 load peak next to r8's two-build whole-run 164.3 to 170.5 MiB.

**Required for B1 in the re-seal.**

- The synthetic e2e writes a `SYNTHETIC-OFFLINE-ONLY` marker into every session root it creates, and stamps `synthetic_offline_only: true` into its `SESSION.json`, `RESTORE-FINAL.json` and `AUDIT.json`.
- `smoke_admission.inspect` rejects any such marker.
- Remove `--retain-smoke`, or confine it to a directory name that `bind_helpers.py` refuses.
- Rehearse main admission only on a temporary copy of the package, never inside OUT.
- Add a tenth negative fixture: a synthetic smoke at the exact smoke path is rejected.
- Relabel or annotate `proof/SESSION-E2E-MAIN-ADMISSION.json`, whose `real_smoke_admission` came from synthetic data.

**Required for B2 in the re-seal.**

- Smoke admission requires a non-None Hedge3 benchmark sample under 165 MiB, with an explicit message.
- It records Pandemonium's benchmark plateau as `unavailable` when absent and does not apply 165 MiB to Pandemonium.
- `make-report.py` treats a missing wall sample as `HEAP_NOT_CONFIRMED`, not an assertion.
- Add a synthetic smoke fixture whose Pandemonium benchmark window has no heap sample.
- Re-run the full e2e, the main-admission e2e (on a copy) and the negative fixtures, then re-seal.
