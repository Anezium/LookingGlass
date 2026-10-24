# Alice matrix r3: atomic logs and isolated world switches

Prepared on the Mac only while P.T. owns the Vita. No Vita, route, receiver, driver RPC or lifecycle operation was performed. All new runs are PENDING. Ownership must be explicitly returned after the current owner exhaustively restores and inspects HOME.

## Evidence and implementation

The old common SELF was 65f55814b2d4884f4ca8af308d9ec9a281d0d44cb3a272843ff79bf16ff936bb. S2 Pandemonium bundled world medians were A1 41.663, B2 44.034, B3 43.950 and A4 42.319 ms: oriented deltas +2.371 and +1.631 ms, FAIL_WALL_GATE. Hedge3 was unmeasured. Entry images matched, which does not prove complete rendering equality.

S3 Pandemonium A1/B2/B3 medians were 41.601/41.595/41.481 ms. B2−A1 was −0.006 ms. B3 is invalid because its audio reporter interleaved with completion; A4 and Hedge3 were not run. S4 was not admitted. No partial result carries forward.

New branch vita-lever-matrix-r3-20261024 starts at the exact lever-matrix build a859ceea2e35fdffec97872bc9cea46956ccbbd2. The sole engine change is common logging: format the complete record and newline before acquiring a shared sink mutex; serialize stdout and stderr together and hold both standard stream locks during write_all. This also excludes dependency diagnostics using std's stream locks. Existing descriptors and downstream debug transport/file destinations are preserved. No queue, reporter/audit policy, renderer, timer, clocks, defaults or dependencies change. The strict audio parser is byte-identical to Round 2. See proof/SOURCE-DIFF.patch and proof/LOGGING-PROOF.json.

The new SELF is required because logging changed. **Every block must establish fresh A1 and A4 on the new SELF. Historical A runs on 65f55814 are inadmissible.** An old/new-SELF comparison is a source/binary investigation, not a runtime parity result. Logging allocation/locking and code-layout changes can affect wall time, which makes the new A baseline mandatory.

## World diagnosis

proof/WORLD-DIAGNOSIS.json preserves hashes and extracted facts. Console entries retain 1,513 groups, 44,610 vertices, 37,031 layer triangles and 2,541,906 GPU geometry bytes. Static packing reduces shared geometry buffer pairs from 1,513 to 231. All four restored entry receipts show 304 world draws and 6,700 triangles. These are entry snapshots, not 120-frame draw counters. All non-wall CSV instrumentation is unavailable; blank fields are never read as zero.

Static merging packs immutable VBO/IBO data at load. Per-frame engine geometry uploads remain zero. It then scans prepared opaque draws and compares buffer identity, adjacency, state, exact uniform bits and projected coverage. Equal buffer packing does not guarantee draw coalescing: the frozen scene keeps the same draw count. Packing does not break the original batching by splitting a group; it keeps original index subranges, and only joins certified adjacent ranges. It can change GPU buffer placement/locality and add scan/projection work without fewer draws. These are plausible costs, not a measured attribution.

Material sorting rebuilds an insertion-style schedule when the view generation or visible draw-ID sequence changes. A rebuild allocates items and projects eight AABB corners per eligible group; worst-case reorder is quadratic. Even cache hits compare the input, count moved items, copy scratch and rewrite draw order every opaque pass. Front-to-back sorting is retained first; only rank-equal, disjoint static opaque groups can cross. The frozen-camera case can reuse its schedule, but dynamic visibility/order changes can invalidate it. No captured schedule-hit count proves how often that happened here.

Uniform persistence skips invalidating two private program-field caches. It can compare the first draw's 143 words where invalidation previously short-circuited, but avoids unchanged field uploads and creates no new upload path. It does not skip visibility, parameter preparation or per-pass state caching. No redundant-write defect was found.

Independent historical host Pandemonium counters (last 240 of 300 frames at a different entry/build) show 306.808 opaque-owner draw calls in baseline and each single lever. Material sorting changes texture binds 272.846→259.138 with 13.425 fewer total calls; static merging changes buffer binds 613.617→561.692 with 545.213 fewer total calls; uniform persistence saves 13 uniform calls (11 vector, 2 matrix). These corroborate mechanisms only. Source certificates/caches are unchanged from that host evidence, but its configuration, entry, observed path and platform differ; its wall elapsed times and zero-filled phase fields are not performance evidence for tonight's console block. No winner or culprit can be identified from bundled timing plus call counts. **No renderer behavior was changed without a demonstrated defect.**

## Runtime configurations and sessions

All six packages contain one byte-identical new SELF. The baseline config is byte-identical to qualified B. Each treatment differs from baseline by exactly one switch:

- fx: fx_cpu_inline=true; fx_merge_ranges=false.
- audio: audio_offmain=true; PCM worker remains enabled.
- world-material-sort: world_material_sort=true only.
- world-static-merge: world_static_merge=true only.
- world-uniform-persist: world_uniform_persist=true only.

Both GL interception caches stay false, as does light_gl_observers. The direct resolver and frame-total-only timer remain selected. Shader/texture caches and all other baseline settings remain unchanged: profiler off, capture on, render scale 0.75, two skin workers, maximum clocks, retained rendering and qualified light/NPC/audio settings. Split levers remain startup runtime config on the same SELF, never hot-edited in a running arm.

Order: S3 FX rerun (Pandemonium ABBA then Hedge3 ABBA), S4 audio (same), then three independent Pandemonium-only sessions: s5-world-material-sort, s5-world-static-merge, s5-world-uniform-persist. Each session preserves, stages the new SELF once, uses fresh launches and fully restores before handback. A1/B2/B3/A4 within each block; no shared A run, cross-session medians or old/new SELF pooling. Exact labels and PENDING states are in CONSOLE-PROTOCOL.json.

FX and audio each require 8 launches, 480 warmup and 960 measured frames. Each world session requires 4 launches, 240 warmup and 480 measured frames. Total 28 launches, 1,680 warmup and 3,360 measured frames, five preservation/restoration cycles. World Hedge3 remains outside this diagnostic scope; a Pandemonium result cannot establish overall two-map non-regression or promotion. Interactions between world sub-levers remain untested even if all three singles pass.

## Time admission

Budget for each FX/audio session: 15 min preservation + 4.5 min SELF stage + 8×9 min launches + 4×1.5 min Pandemonium handling + 30 min exhaustive restoration = 127.5 min. Each single-world session: 15 + 4.5 + 4×9 + 4×1.5 + 30 = 91.5 min. Total 529.5 min (8 h 49 min 30 s), excluding handback gaps. Each session independently remains below the 150-minute cap; this total is not admission to one driver lifetime or sole ownership.

Read current DRIVER-READY.json only at operational admission, including pid, started, fixed_expiry, 43,200-second lifetime and disabled auto-restart. The local metadata helper checks live PID and exact terminal-driver.py command, exclusively records DRIVER-ADMISSION.json, and requires the unchanged identity at every arm/pair boundary. The pilot never starts/restarts/extends the driver. Only the orchestrator may restart between fully restored and handed-back sessions. Never wait for a mid-session restart.

For FX/audio use helper defaults (8 launches, 1 SELF stage, 4 Pandemonium launches, 15 preservation minutes). For each world session pass --launches 4 --self-stages 1 --pandemonium-launches 4 --preservation-minutes 15 at admission. Completion including 30 min restoration must be strictly more than 30 min before fixed expiry. Latest starts are expiry−157.5 min for FX/audio and expiry−121.5 min for world; the helper's latest_admission_for_one_session field describes its eight-launch default only. Recheck actual entire remainder and elapsed time before every launch/pair; after setup, zero both setup allowances. A changed driver identity, insufficient time, evidence defect or stop rule requires safe-boundary restoration. Unadmitted sessions stay PENDING.

## Measurements and gates

Timer source is unchanged: one begin-to-begin wall interval includes simulation, render, submission, swap, yield and scheduling. Discard 60 accepted warmup frames, retain 120 positive intervals, close frame 120 at report-only boundary. Require gl_hooks=0, phase_timers=0, gl_observers=direct, profiler-off readback and blank non-wall/doors fields. The legacy done text profiler=light does not change the actual config requirement.

Quantiles: sorted[n//2] median and sorted[floor(.95*n)] p95. Retain all raw exports, min/max/sum, oriented B2−A1 and B3−A4 and percentages/mean. Both deltas ≤+1 ms are needed for non-regression; mean >+1 fails, one orientation above threshold with a passing mean is inconclusive. Replicated benefit needs both strictly below −1 ms. Pandemonium's 1 ms band is provisional, not a calibrated noise bound. No observer subtraction, phase attribution, trimming or automatic retries.

Strict audio is mandatory for A and B: every in-slice producer p95 <8 ms; raw starvation, rejected sinks, dropped reports, music/CPAL underruns zero; all required reporter records complete including warmup and boundary overlap. No reconstruction or exclusion of the old corrupt line. Audit offmain requested/active false/false except audio B true/true; retain submitted/reused/fallback counters, nonzero fallback alone is not failure. Reject stale light CSV hashes across the whole session. Crash/OOM, sampled heap >180 MiB, no progress, invalid identity/config/pose/export or incomplete evidence stops and restores.

For FX/audio only, an evidence-valid Pandemonium INCONCLUSIVE_REPEAT_REQUIRED may proceed to Hedge3 if unchanged-driver admission covers the remaining four launches. FAIL_WALL_GATE, invalid evidence or any stop rule cannot. Overall lever non-regression requires both maps; either unmeasured/invalid/inconclusive stays unresolved. Single-world sessions end after their Pandemonium block with restoration regardless of the comparator outcome; repeats require a separately admitted full ABBA.

Compare acknowledged A1/B2 and B3/A4 entry images for every single-world block, record hashes/ack references/findings in block-specific WORLD-ENTRY-REVIEW.json, and stop on visible world differences. Reports must state “render equivalence of the world arm not verified on console”. A later host PNG equality check of each enabled config against baseline on this exact new sealed source/build is required before any world promotion. Historical PNG equality is not that check. Full world age, NPC state, continuous memory peak and audible continuity remain unavailable unless separately established.

## Operational preservation and restoration

CONSOLE-BRIEF.md retains every Round 2 amendment: driver admission, audio engagement/counters, stale CSV rejection, listener provenance, world entry images and exact-build host PNG requirement, inspected resume control, conditional Hedge3 after inconclusive, and real-manifest/budget dry runs. The route must be en0; only serial locked FTP, only eboot.bin inside the app, no new directories, VPK/metadata writes, USB/reboot or other-title control. Use fresh helpers and current actual save bytes, never historical saves or synthetic console inputs.

Human SELF must be 37,371,674 bytes, SHA-256 387ccf678251d4267b8ced53e207830358826f8e0642631a0459c6b0b9939990. The sole autosave is 181,900 bytes, SHA-256 ea4cd221d9f950e613188ccf3ecf2c1eabb1d34106f413125864320d364c8fa9, logical visit pandemonium$first, canonical map pandemonium. Restore/read back this session's actual preserved save and every companion before every Pandemonium launch. Finish Pandemonium before Chapters.

Exhaustive restoration reverses every owned ledger including root SELF, independently reads the human SELF twice, restores every original file and initial absence, checks SFO before mutations and verifies every initial directory inventory. Preserve existing caches; handle only ledger-owned caches by the inherited policy. Require both human SELF receipts, RESTORE-FINAL PASS_ORIGINAL_STATE_VERIFIED, restored autosave/companions, final acknowledged HOME, Alice closed, released inputs and explicit sole-ownership handback. Offline fixture PASS does not admit console use or establish runtime performance.
