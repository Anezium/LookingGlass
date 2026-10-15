# J23 track B final report — 15 October 2026

> Historical campaign report. The 2026-10-15 night decision superseded its package-eligibility restriction for a default-off console A/B. The four review fixes, fresh checks, failed short timing rerun and verified revised package are documented in [J23-PHASE2-REVIEW-FIXES.md](J23-PHASE2-REVIEW-FIXES.md).

**Result: both complete host gates fail. Both switches remain default off. Phase 4 is ineligible; no package or new console A/B is authorized by these results.**

All work ran on the Mac in `/Users/anezium/vita-work/lookingglass-j23`, branch `vita-night-j23`. The Mac-only amendment at planning commit 0d1f1f0 supersedes the earlier remote packaging instructions. No remote build host or console was accessed. The coverage clone/target, pilot, console ports and keepalive were untouched.

## Source and reproducibility

- Starting checkpoint: 7e5e1cff9d2537e9ded6e6058f6640f4ed7b1afa. Its recorded target was absent, so the clean source was rebuilt with CARGO_BUILD_JOBS=4, --locked, feature vita_renderer and the isolated target-j23-host directory.
- Stable pushed commits: dbded28 (checkpoint functional evidence), b4bdf81 (exact per-surface dependency revisions fixing excess uploads), 40daefc (actor gate report), 1a988c0 (puff fusion and functional evidence). Final documentation adds no runtime change.
- Actor runtime source: b4bdf81e2943c1368bcf6466acf865bb059ec49e; binary SHA-256 32f3bc3a22842b07c500ca744b1704b2834540b8dc22ccb73474806b4a679b60.
- Puff runtime source: 1a988c09055d3759c2b9c7f1597f7872ab582c91; binary SHA-256 4b4c788464b2144a9de696bdc3c2de92abb4f464376c5c6eb89fb5a933fc76a2.
- Evidence root: `~/vita-j0/village-retained-world/jalon-23-phase2/`. Functional5/6, both timing manifests, all raw selected/full samples, uptime monitors, rejected attempts, binary pins, source patches and driver/analysis scripts are preserved there. Earlier failed/superseded attempts remain excluded and preserved.

## Protocol and qualification

Each campaign uses one binary, prescribed alternating 1/30 and 0.0366 second dt, neutral input and the real normal viewer update from the pinned reference Autosave. Three pairs alternate AB/BA/AB. Every arm runs 180 frames, discards exactly 60 warmup frames and retains all 120 remaining frames including stalls. There are 720 accepted retained frames per campaign. Save witnesses, parity digests and readbacks are absent from timing; snapshots/hashing/report construction remain outside the body timing.

Each campaign atomically acquired the shared Mac timing lock, waited for every observed 1/5/15-minute load average to be <=3, logged raw uptime before/after arms/pairs and monitored once per second. A violation rejects the whole pair. Actor accepted attempts are 1/1/2 (pair 3 attempt 1 rejected); puff attempts are 1/2/1 (pair 2 attempt 1 rejected). Both locks are released, no process remains in flight, and no performance-based rerun or trimming occurs. Both campaigns completed within their 3300-second bounds.

- timed5: maximum accepted observed 1/5/15-minute loads [2.3134765625, 2.1357421875, 2.99609375].
- timed6-puffs: maximum accepted observed 1/5/15-minute loads [2.10791015625, 2.55908203125, 2.98974609375].

Quantiles are calculated after forming each per-frame sum; p95 is sorted[floor(.95*n)]. Body excludes the deferred framework/GPU tail. These are Mac host observations, not Vita wall timings or a host-to-console scaling model.

## Phase 2: actor redesign — FAIL

Single actor traversal, exact existing numeric skin jobs and owned immutable publications remain default off. Bounded two-bank surface storage, held-view generations, mutation/consumer fallbacks and the five-slot GPU ring are preserved. The resumed follow-up replaces whole-skeleton dirtying with exact used-bone dependencies on the game thread, including signed zero and zero-weight entries; it adds no worker output comparison loop.

Preparation sums exclusive gather, discovery, consume, simple publication, modifiers, packet publication, shadow/atmosphere and collection. Collection/shifted work is charged; worker service is not added. Median/p95 below are milliseconds.

- Pair 1: prep 0.2212505/0.372709 -> 0.1067285/0.339458; median reduction 51.761%; body 1.4437500/1.924667 -> 1.3961875/1.888292. PASS.
- Pair 2: prep 0.2170210/0.377584 -> 0.1130000/0.405792; median reduction 47.931%; body 1.4096250/1.790250 -> 1.3935420/2.144667. FAIL.
- Pair 3: prep 0.2252295/0.393750 -> 0.1013540/0.237625; median reduction 55.000%; body 1.4358125/1.766291 -> 1.3463545/1.750416. PASS.

All medians exceed the 30% reduction threshold, including conservative charges for positive paired finish/upload deltas (0/0/0.007912 ms). Pair 2 nevertheless worsens both prep p95 and body p95. The full gate therefore fails. Accepted pair 2 is retained, not replaced. This actor/gather-tail failure is unrelated to puffs, permitting the independent next phase.

Functional5 at its pinned source: live and private Slot 4 reload pairs each match all 180 simulation and final submission digests; both frame-90 write/read witnesses pass. All 120 cross-arm matrix PNGs and 15 renderer PNGs match, each arm passes 60 serial/overlap comparisons and actual-data workers pass. Cases cover poses, duplicate gnomes, blink/live UV, shells, hidden/reappearing surfaces, attachments, fullbright, first-person/second view and map/resource replacement.

All 120 retained functional frames match draws, uniform/upload bytes, ring capacity and buffer growth. The original extra 1280 actor bytes in 85 frames are resolved by b4bdf81. Frozen baseline uploads zero bytes in both arms. Logical consume replay draws drop 6 -> 0, with six gathers retained. Publication-bank arrays remain 168686 bytes / 51 slots; allocator/shared asset metadata and peak heap are excluded. Targeted lifetime/bounds/zero-warm-reserve tests pass. Source suite at that point: 852 passed, 0 failed, 2 inherited ignored.

## Phase 3: puff fusion — FAIL

world_puff_fusion defaults off. Numeric workers produce owned corners/colour; registration preserves plain numeric values and final batch packing constructs Vertex records. Exact legacy arithmetic, joint ties/order, material first-fit policy, world/actor/EQUAL barriers, 60000/180000 bounds, old-publication lifetimes and existing GPU phases/ring are preserved. Numeric recipes and descriptors still exist; no complete elimination of arithmetic/copies is claimed.

Actor preparation is off in both arms. The measured per-frame sum is puff_producer_ns + puff_joint_sort_ns + puff_prepare_merge_ns: world-puff production plus complete joint ordering and puff-batch planning/packing/publication, conservatively charging shared ordering/barrier work in full in both arms. It excludes later generic upload/submission, which are reported with total body. All moved Vertex construction stays inside this sum. Median/p95 in milliseconds:

- Pair 1: puff pipeline 0.0533335/0.080833 -> 0.0537295/0.081375; median reduction -0.742%; body 1.3736870/2.141916 -> 1.4237710/2.205583. FAIL.
- Pair 2: puff pipeline 0.0569990/0.085875 -> 0.0345835/0.047625; median reduction 39.326%; body 1.3604165/1.785625 -> 1.1263960/1.626250. PASS.
- Pair 3: puff pipeline 0.0554795/0.091084 -> 0.0546465/0.079376; median reduction 1.501%; body 1.3968335/1.776416 -> 1.3839375/1.773750. FAIL.

Only pair 2 reaches 20%; the consistent-three-pair gate fails. No accepted pair is rerun or trimmed to change that verdict. All three pairs match 120/120 simulation hashes and every checked draw/uniform/upload/capacity/growth/final-write counter. Candidate intermediate puff Vertex records remain absent.

Functional6: live and private Slot 4 reload each match 180/180 simulation/submission digests; both frame-90 write/read witnesses pass and source Autosave is unchanged. All 120 matrix and 15 renderer PNGs match across arms; actual-data workers pass. All 135 new default-off controls match the prior source controls. Frozen baseline: all 120 frames upload zero bytes in both arms.

All retained live frames match final traffic. Intermediate puff Vertex payload writes 40960 -> 0 bytes/frame; numeric output and descriptor payloads each 6656 bytes/frame; final Vertex writes 20480 bytes/frame in both arms. Warm producer output capacity 39680 -> 12896 bytes. Final batch storage matches frame by frame. These are logical payload/capacity counters, excluding enum/vector/allocator metadata, recipes and full/peak heap; they do not establish a bandwidth-derived CPU gain.

Final feature suite: 855 passed, 0 failed, 2 inherited asset-dependent ignored. Independent real-puff arithmetic/centers, membership/material changes, ties/barriers, vertex/index boundaries, held views, unchanged revisions and zero warmed modified-kernel allocation tests pass. The allocation claim is targeted kernel tests, not zero allocations throughout live gameplay.

## Audits, console prerequisite and package

Final default-feature check and changed-source audit pass; Cargo.lock stays d18dd40246bf2eaca095b2ff364a2eca20d7c95fabd0c76231ff1072205775ee and the 12 inherited allowlist exceptions stay unchanged. Both negative GameThread Send/Sync checks pass at unchanged marker source; worker closures remain plain numeric and all feature builds retain the CPU ownership bounds. No dependency update, force push, game/save payload in Git, prohibited attribution trailer or process-pattern kill occurred.

The separate console-owner probe is published locally: conservative replay/validation/publication median/p95 6.599/6.895 ms, above 1.5 ms in all pairs. It passes the prioritization threshold, not an optimization gate. This track has not accessed the console. A future accepted candidate would still need equally instrumented console normal-play A/B, moving parity and the coverage runner combat/boss scenes once accepted. No 33.3 ms or 30-fps result is established.

**Phase 4: not entered.** Neither full host optimization gate passed, so no A-stock VPK, SELF, ELF margin or package SHA-256 exists. The Mac-only pkg-2/native-SDK/nightly/pinned-vitaGL/margin/SHA256SUMS requirements would apply only to an eligible package; no remote package pipeline was run. Both new switches remain default off. There is no PACKAGE-READY marker.

Final detailed results are in PHASE2-REPORT.md, timed5/PHASE2-SUMMARY.json, timed6-puffs/PUFF-SUMMARY.json, FINAL-CAMPAIGN-AUDIT.json and the preserved raw cells. RESUME.md records completion and no running work.
