# Round-3 review amendments D1-D5 (adopted by the orchestrator before any data)

Verbatim from REVIEW-R3.md. Highest precedence for the r11c console run. No hashed file changes; no reseal.


Adopted before any console data. These change no pass/fail rule and no hashed file. AMENDMENT-R11.md, AMENDMENT-R11B.md and the r11c AMENDMENT-R11C.md stay in force.

1. **D1. Which amendment file applies.** Run only from `alice-transition-mem-r11c/` and its sealed ZIP a02b19fa…. `alice-transition-mem-r11/AMENDMENT-R11C.md` has the same name but belongs to the superseded r11b package. Do not use it.
2. **D2. Scope of C2.** Round 3 found that the camera-portal double delete was not reachable from LG's render targets (all use `sample_count: 1`). The C2 restriction to Hedge3 → Pandemonium → Hedge3 remains as experiment scope. `key not found` or `panicked` anywhere is still FAIL: stop and do not retry.
3. **D3. Baseline line, informational.** In the result, for each arm, report the first in-game Hedge3 sample (uordblks, arena, fordblks) before the first transition. Next to it, report the matching value from the most recent earlier console run of the same lever arm, and name its file. Host r11c sits 783,234 bytes above r11b before any transition. The cause is not yet attributed, and this line is the first hardware reading of it. It changes no label.
4. **D4. Frame time, informational.** If the log carries `native_timing` frame totals on Hedge3, report the median and p95 per arm. Do not add samples, launches or console time to get them.
5. **D5. What a PASS supports.** A PASS without `PASS_LOW_ARENA_HEADROOM` on SELF 78e81848… is console evidence for merging r11c, for chapter transitions between Hedge3 and Pandemonium only. It does not qualify Retry memory, portal maps on hardware, other maps, or frame time. `PASS_LOW_ARENA_HEADROOM` remains non-evidence, as in C1.

## Verified vs assumed

**Verified, by reading code, recomputing hashes or diffing:**

- **Forks:**
  - fork parents (upstream tag v0.4.14, `vita-frame-profile` tip);
  - pinned revs equal the pushed branch heads;
  - crate `src/` equals the git tree at 2c60f75;
  - reviewed patches carried byte for byte;
  - fork diffs limited to the stated changes;
  - lock changes limited to two `source` lines;
  - licences kept;
  - textual merge with vao-r7 is clean.
- **R2-M1 correction:** LG never uses `sample_count == 0`, so the double delete was unreachable.
- **Code semantics:**
  - guard and ownership logic;
  - weak-candidate leak and fix;
  - table step attribution (from the recorded stacks);
  - console bound;
  - E3 control flow, and that `expect` is safe.
- **Proof records:** the E2 trend numbers and gate; the +783,234 B baseline shift and its bisection between the two diagnostics.
- **Package:** all hashes, seal, ZIP members, SELF/VPK identity, helper diffs, restoration literals, grammar source hash.
- **Repo rules.**

**Recorded by r11c and not re-executed by me:** every host proof run, CLEAN-HOST-BUILD, the dependency unit tests, the e2e runs, and the native build and process-parameter checks.

**Assumed, not verified:**

- The cause of the baseline shift (the future-layout hypothesis).
- That no LG code drops a Material inside an open raw pass (carried from R2).
- That no per-frame resource churn triggers the table shrink every frame.
- The persistent-failure screen's behaviour.
- The source of the 6,400 B and +800 B steps.
- Everything on hardware: arena effect of the table regrowth, the native baseline, vitaGL pools, portal maps under vitaGL, Retry memory, frame time, audio continuity.
