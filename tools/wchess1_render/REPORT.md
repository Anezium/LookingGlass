# wchess1 render: first console package

READY FOR ROOT MEASUREMENT ONLY. The >=8 ms Vita gain and the playing p95 <=50 ms bar are NOT established. No default is promoted. This first package has exact host candidates, with limited wchess1 skin-reuse coverage.

Source branch: vita-wchess1-render-20261116; report-generation branch head `5c4fca705d77c5f30d74d2760d69780534f20c48`. Native build head `9da106ad0d14c2f441650edd5533b0be5049f36d`; host oracle head `a6c6c12bafa4b9aecb69ca349c62c632ac2f0f09`; unit head `5c4fca705d77c5f30d74d2760d69780534f20c48`. `runtime-equivalence.json` checks that revision differences are exactly the known cfg(test)-only fixture/schema edits; production code is unchanged. Base RC14 is 37f9d0694820a7b56cfeb30c89f978dcc6c61593.

## Attribution (measured host data, not Vita predictions)

Three draw-query frames per tape: A4 probe 900..902 and owner-play probe 1700..1702. Counts below were the same on both tapes; dynamic FX vertices differ slightly. The full recorded play tape has 6375 steps. Index vertices mean submitted triangle indices, not unique VBO vertices. Chess classification uses texture labels.

| Family | Draws/frame | Index vertices/frame | Packet vertices/frame | Observed state transitions/frame |
|---|---:|---:|---:|---:|
| world_opaque | 307.0 | 13314.0 | unavailable | 284.0 |
| world_alpha | 116.0 | 2016.0 | unavailable | 114.0 |
| actors | 28.0 | 6813.0 | 1877.0 | 28.0 |
| chess_pieces | 19.0 | 26664.0 | 7269.0 | 19.0 |
| fx | 8.0 | 980.0 | 631.3 | 7.0 |
| hud | 8.0 | 588.0 | unavailable | 8.0 |
| upscale | 1.0 | 6.0 | unavailable | 1.0 |

The dominant alpha materials are textures/testliq/bw1.tga and textures/effects/chessw.tga. State transitions compare program/texture/blend/depth tuples in global draw order and attribute each transition to its current family; the first draw counts as a state entry. Uniform/cull/sampler changes and individual GL calls are not covered by that tuple. Per-phase API application counters are retained in cpu-family.json; disabled GL-observer counters are unavailable, not measured zero.

| Host window | Opaque CPU ms | Alpha CPU ms | Actor publication/submission ms | FX ms | HUD ms | Actor gather ms | Actor collect ms |
|---|---:|---:|---:|---:|---:|---:|---:|
| A4 [700, 1180] | 0.980 | 0.556 | 0.330 | 0.728 | 0.079 | 0.216 | 0.415 |
| play [796, 1276] | 1.489 | 0.854 | 0.426 | 1.255 | 0.122 | 0.285 | 0.508 |
| play [1486, 1966] | 1.263 | 0.725 | 0.361 | 0.965 | 0.101 | 0.245 | 0.387 |

These are 481-frame instrumented arithmetic means. Full profiler scopes nest; actor gather/collect are separately timed regions and must not be added to a containing scope. The instrumented binary omits frame_total_only, while proof/SELF recipes retain it. All timing runs used mac-timing.lock individually; root later limited that lock to timing only. Builds and exact replays released it. Loader, framework, profiler and OS scheduling overhead remain. Host proportions do not predict Cortex-A9 milliseconds. The family runs stop after their requested windows and are not credited as complete oracle replays.

Known chess actor inventory at host frames 300..360 measured inclusive body invocation ~0.082 ms A4 / 0.078 ms play. Full-tape known-actor worker service averaged ~0.138 / 0.137 ms. Those are different populations and worker service is not main-thread wait. Raw logs, exact populations and IDs are in attribution.json.

Native prior evidence, not this task: actor_collect 5.41 ms, world_alpha 5.05, world_other 4.84, actor_gather 3.92 on A4. actor_collect drains skin lanes and completes publication (including deferred layer construction); it is not pure waiting. world_alpha submits retained-world callbacks interleaved with actors/FX. world_other includes opaque submission and alpha-preparation residual; it is not an exclusive draw-call clock. The live backend update/draw split is distinct from spike sim/render.

## Candidates and strict oracle

- world_alpha_parameter_once (off): immutable parameter signatures include model identity and isolate moving batches. Check generation/model/time once per alpha preparation pass; retain exact uniform words, textures, ordering and draw calls.
- actor_skin_output_reuse (off): skip only a completed, exclusively owned slot from the immediately previous epoch, with identical instance, CPU surface, numeric inputs, UVs and publication mode. Reservation alone is insufficient. Raw outputs are conservatively excluded.
- combined enables both; arm-off compares the candidate binary with both off to the clean RC14 reference.

| Tape | Frames/pair | Off | Alpha | Skin | Combined |
|---|---:|---|---|---|---|
| a2-04.lgtape | 2954 | PASS | PASS | PASS | PASS |
| a4-wchess1-01.lgtape | 1200 | PASS | PASS | PASS | PASS |
| rw-20261116-wchess1-01.lgtape | 6375 | PASS | PASS | PASS | PASS |
| a4-skool1-01.lgtape | 1200 | PASS | PASS | PASS | PASS |
| a4-potears1-01.lgtape | 1200 | PASS | PASS | PASS | PASS |

20/20 full-tape comparisons PASS, 51716 compared frames. Every state, ordered event, skin payload, submitted packet, geometry/uniform payload, world/joint order, image hash and presentation field matches without tolerance. Unknown payload fields and non-observer counters remain compared. Only clocks, documented memory observers, volatile GPU slot identity, the inherited two dynamic-collider observer counters and the two named coverage counters are normalized. Host divergence from the native recording is compared exactly to the host base, not mistaken for candidate divergence.

Coverage on wchess1: A4 alpha avoids 113357 repeated checks / 1200 frames; owner play 677432 / 6375. Skin reuses only 21233 vertices / 1200 and 137496 / 6375 (~18 and ~22 vertices/frame). This is sparse relative to the chess geometry. Neither an 8 ms saving nor a meaningful wchess1 skin gain is justified by that coverage. Environment-mapped chess bodies use the deferred/raw family, which this conservative cache excludes. Extending exact raw-output/layer reuse is a follow-up, not a claimed result.

## Package and root order

READY SHA-256 `cd596771888f7882ae636556433abd4a76d941e71f722e42e1d92178c5904030`.
SELF SHA-256 `48848ca9f1e606bcf631b6cc435fe31c6bab796fee811bc871ba54d9719302f2`.
Driver SHA-256 `1a18fe7b6e36fb581cea1dfdb2ae6c44019b08168511469d25089eb59187bc8d`.
Kit unchanged `93f434a6286b2180bd66d89ffad1394b7f27b5e385296c67a70b2271256558b3`.

Window order: A4 700..1180; owner play 1486..1966; owner play 796..1276. In each window: control, alpha-once, skin-reuse, combined (12 arms). The official live.py selection and its possible unobserved-overlay caveat are pinned. Every console arm replays its whole tape and logs exactly its inclusive 481-frame window. render_scale stays 0.75. README supplies manual root setup, recovery, separate one-tape-at-a-time put/observe/ack/get and restore/read-back/shutdown steps. No console slot is granted by this package.

Each credit requires final drained replay_gates, the final validated done line, complete steps/checks/window, diff/events=0, dropped=0, finite metrics, config PASS, not-running exit and assert_transaction on the captured observation immediately before credit. The driver never acknowledges, retries or restores.

## Validation and retained failures

Rust: 1416 passed, 0 failed, 16 ignored, opt3 release tests with LTO disabled and 256 codegen units. Production host oracle and native SELF retained their production LTO recipes. Earlier full-LTO run passed 1403 and failed 13 frozen-schema assertions; those test-only omissions were fixed. An intermediate rerun reused capture fixtures and was rejected; final fixtures use a fresh per-run directory.
Python: 12 real-kit fake-transport step tests, 3 comparator tests (including ordered-event/payload/pixel mutants), 3 receipt tests PASS. Feature-off cargo check and native release static/library/notices/LiveArea gates PASS. Offline tests do not contact the device.
Rejected timing/proof attempts remain: initial frame-zero render:null parser failure (regression fixed); host 180-frame phase-census export failed hierarchy closure and was not used. The successful family timing uses the existing full render profiler instead. No rejected run is credited.
No console/FTP/1337/1338 contact, kit edits, default flips, Terminal.app windows or game assets in Git. No console restoration or visible-window-count claim. No private host target was needed; the shared target did not block this task. Current packages/proofs remain active inputs for root review.
