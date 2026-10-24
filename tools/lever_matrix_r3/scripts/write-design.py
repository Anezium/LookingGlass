"""Write the r3 design and carry forward every operational amendment."""
from pathlib import Path
O=Path(__file__).resolve().parents[1]
P=Path('/Users/anezium/vita-j0/resume-2026-10-23/alice-lever-matrix')
design='''# Alice matrix r3: atomic logs and isolated world switches

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
'''
(O/'DESIGN.md').write_text(design)
brief=(P/'CONSOLE-BRIEF.md').read_text()
brief=brief.replace('# Console handoff: candidate lever matrix','# Console handoff: Alice lever matrix r3')
brief=brief.replace(str(P),str(O))
brief=brief.replace('`session-s2-world`, `session-s3-fx` or `session-s4-audio`','`session-s3-fx`, `session-s4-audio`, `session-s5-world-material-sort`, `session-s5-world-static-merge` or `session-s5-world-uniform-persist`')
brief=brief.replace('SESSION="$OUT/session-s2-world"','SESSION="$OUT/session-s3-fx"')
brief=brief.replace('all four current package hashes','all six current package hashes')
start=brief.index('S2 world, S3 FX, S4 audio each run')
end=brief.index('**Accepted amendment 7:**',start)
brief=brief[:start]+'''**The SELF changed for the common logging fix. Re-establish both A baselines on the new SELF in every block. Never reuse S2/S3 results or any A launch from 65f55814.** All new entries remain PENDING; no performance or audibility result is claimed.

Run S3 FX anew, then S4 audio: Pandemonium ABBA first, Hedge3 ABBA second. Then run three separate Pandemonium-only cycles, s5-world-material-sort, s5-world-static-merge, s5-world-uniform-persist. Restore and explicitly hand back after each cycle. The six packages baseline/fx/audio/world-material-sort/world-static-merge/world-uniform-persist share one new SELF; exactly one runtime switch differs in each B. Both GL interception caches remain off. No combined world treatment is admitted here.

Use exact CONSOLE-PROTOCOL.json labels. S3 starts s3-fx-pandemonium-1-A, s3-fx-pandemonium-2-B, s3-fx-pandemonium-3-B, s3-fx-pandemonium-4-A; then its four Hedge3 labels. S4 uses its own labels. Each world block has s5-world-<switch>-pandemonium-1-A through -4-A in ABBA order. Never share A or pool absolute medians across blocks, sessions, maps or SELF versions.

FX/audio budget: 127.5 min each, with 15 preservation + one 4.5 SELF stage + 8×9 launches + 4×1.5 Pandemonium handling + 30 restoration. Each single-world budget: 91.5 min with 4 launches and the same other allowances. Five cycles total 529.5 min (8 h 49 min 30 s); independently admit each against the current driver lifetime and 150-minute cap. No automatic repeats. Leave whole unadmitted cycles PENDING.

FX/audio admission uses check-session-budget.py defaults with --session-start <ISO> --admit "$SESSION/DRIVER-ADMISSION.json". **World admission must also pass --launches 4 --self-stages 1 --pandemonium-launches 4 --preservation-minutes 15.** Latest starts are expiry−157.5 min and expiry−121.5 min respectively. The helper's latest_admission_for_one_session output is its eight-launch default, not the four-launch world deadline.

Before every arm and pair boundary use --admitted with the same start and actual entire remaining counts. After setup, FX/audio starts with --launches 8 --self-stages 0 --pandemonium-launches 4 --preservation-minutes 0; single-world starts with --launches 4 --self-stages 0 --pandemonium-launches 4 --preservation-minutes 0. Decrement launches and Pandemonium launches after each completed arm. Before Hedge3, launches=4, Pandemonium=0, both setup allowances=0. The helper includes restoration, elapsed time and the strict >30 min expiry margin. Stop and restore when the remainder no longer fits or driver identity changes. Elapsed time never authorizes a restart.

'''+brief[end:]
brief=brief.replace('S2 Pandemonium','the current Pandemonium block')
brief=brief.replace('`stage-world-full.json`','`stage-world-material-sort-full.json`, `stage-world-static-merge-full.json`, `stage-world-uniform-persist-full.json`')
brief=brief.replace('For each S2 world map block','For each single-world Pandemonium block')
brief=brief.replace('a859cee, identical to parity candidate 5e072af','the new revision in packages/baseline/VERIFICATION.json, including the common logging fix')
brief=brief.replace('comparing PL-world.json with PL-baseline.json','comparing each enabled PL-world-<switch>.json with PL-baseline.json')
brief=brief.replace('--block s2-world-pandemonium','--block s3-fx-pandemonium')
brief=brief.replace('SESSION.json records all four','SESSION.json records all six')
brief=brief.replace('The helper prints `latest_admission_for_one_session = fixed_expiry − 157.5 min`.','The helper prints the eight-launch default `latest_admission_for_one_session = fixed_expiry − 157.5 min`; see the four-launch world admission below.')
brief=brief.replace('**Accepted amendment 7:** if Pandemonium','**Accepted amendment 7 (FX/audio two-map sessions only):** if Pandemonium')
brief=brief.replace('## SELF staging and each launch','Single-world sessions have no Hedge3 continuation: restore after their four Pandemonium launches. The comparator next_action now reflects this scope. No single-map result promotes a lever or resolves the world bundle interaction.\n\n## SELF staging and each launch')
brief=brief.replace('Entry-image inspection is limited evidence; host coverage is currently a raster-model test. Prior host PNG equality at b3d3616 covered five switches, not this three-switch config.', 'Entry-image inspection is limited evidence; current exact-build host coverage is a raster-model test. Historical PNG equality does not qualify this new sealed build and its single-switch configs.')
(O/'CONSOLE-BRIEF.md').write_text(brief)
print('Wrote design and comprehensive amended console brief.')
