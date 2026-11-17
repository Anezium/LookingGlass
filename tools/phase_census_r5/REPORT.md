Implemented the r5 phase census in the isolated clone `/Users/anezium/vita-work/lg-phase-census-r5`, branch `vita-phase-census-r5-20261024`, from exact r4 `81f3d1eb1fe4303ff955afee491ef10a7ceecb2a` (r4 engine `cd33c35`). Engine/build revision: `62704971576f7e3d30499962388b69bf2ed692d7`. No other clone was modified. No Vita, 192.168.1.23, route query, shot driver, receiver, console helper action or driver admission was contacted/executed. All eight console launches are PENDING.

The common-SELF package and console draft are complete in this directory. `CONSOLE-PACKAGE.zip` contains only measurement SELF files, configs, manifests, helpers and instructions. Native ELF/VELF/VPK remain separate Mac artifacts. Source/reproducibility tools live under `tools/phase_census_r5/` on the branch. No game assets were added to a repository, no dependency/build manifest changed, and no cargo update was run.

**Changes, with source locations.**

- `src/phase_census.rs:6` fixes capacity at 180 accepted frames, table capacity 4096 and 16 probes. Rows/storage are at lines 26/41. Initialization at line 97 occurs before gameplay for B; reset/begin/accept at lines 99/107/114 align with the production benchmark. There are no growing containers, allocation, logs, file operations, locks, GL queries or GL dispatch hooks in the census hot path.
- `src/phase_census.rs:120` and line 130 implement two-read elapsed brackets, a bounded nesting stack, inclusive/exclusive sums and observed parent masks. Root bracket union defines the explicit per-frame residual. Line 149 deduplicates delegated sweep APIs to one outer elapsed/count sample; line 157 accounts acoustic-kernel sweeps as a subset. Line 62 implements exact scene/endpoint history with separate prior-frame timestamps, bounded probing and visible live-eviction counters. Line 167 validates alignment/closure and writes only after the window; tests begin at line 202.
- `src/render/vita_renderer.rs:109` adds the sole config boolean; line 269 defaults it FALSE. `configs/PL-baseline.json` is byte-identical to r4. B's only effective config difference is phase_census=true; the omitted baseline key defaults false. Both runtime config records serialize it, preserving A-minus-B length=1.
- `src/frame_total_profile.rs:149`, line 157 and line 177 bind reset, next-begin frame closure and post-window export to the unchanged wall-sample method. `src/frame_profile.rs:1136` passes the same accepted/measured status; line 60 rejects transition frames. `src/main.rs:220` initializes B storage before gameplay. `src/frame_phases.rs:68` times swap inside the EXISTING swap wrapper. `build.rs`, atomic `src/logging.rs`, direct resolver and dependency pins are unchanged. No new link/GL hook was introduced.
- `src/viewer.rs:2316`, line 2347 and line 2362 bracket objects/interactions, movers and player movement. NPC/controller update scopes are `src/npc.rs:2616`, `src/encounters.rs:354`, `src/interaction/hooks.rs:418`, `src/duchess.rs:675`, `src/school2.rs:648`; fixed physics is `src/movement.rs:775` and line 807. These wrap existing operations without changing their inputs, ordering or results. NPC totals include animation/combat/setup, not an isolated AI-think kernel.
- `src/audio.rs:1259` brackets the outer prepare; line 1329 brackets prepare_acoustics; lines 1334/1337/1355 tag room/ambient/effects queries without changing traces. `src/collision.rs:647` exposes immutable scene identity; lines 834/844/866/898 bracket outer sweep/camera/geometry APIs. Audio/player/NPC/camera/other are per-frame owner views, with delegated work counted once. `src/lighting.rs:22` brackets selection.
- `src/render/vita_renderer.rs:1355`, line 1356, line 1419, line 1553 and line 1711 cover opaque, visibility, parameters, submission, alpha preparation and deferred alpha submission. `src/viewer.rs:3610`, line 3658, line 3662, line 3733 and line 3872 cover actor gather, collect, publication, FX gather and HUD. `src/render_fx/retained.rs:1044` retains shared joint-finish work as a containing region; line 1184 times actual actor versus effect-only submission groups. `src/render/vita_renderer/scale.rs:171` covers present. Shared joint preparation/upload/sort remains explicitly in its parent's exclusive time; it is not invented as one child's cost.
- `src/audio/offmain_tests.rs:70` enables census only for the second arm of sample comparisons; the added main-versus-main map fixture proves pure census on/off equality. `tools/phase_census_r5/census_report.py` checks integer closure and emits the fixed table format. prepare.py adapts the inherited serial transport, preservation, save, health and restoration contracts; test_tools.py blocks transport while exercising new collection/report/observer rules.

**Host proofs.**

The final committed-engine host binary has SHA-256 `0d2ccbb5fd3dbd5410c7ae725cac0e1ac3b394cd84098a739982f7270f8bb90d`. The release/offline/locked build uses stable Rust, `--cfg observer_free_B`, features native_timing,frame_total_only,sim_maps_probe and the required shared target. Two maps each ran A/B/B/A for 180 deterministic neutral-input frames. Every simulation state hash, selected-light bit, world/joint-alpha draw order and final PNG byte matched in all four arms of EACH map. Both B exports also passed exact 60+120 alignment and per-frame exclusive-plus-residual closure. Raw logs, per-frame hashes and CPU samples are in `host/` and `proof/HOST-RENDER.json`.

- Pandemonium PNG: `d087e3c2a5223e7a4f87ba19e39c76fbb949f294c4914db068b382be0d66b691`.
- Hedge3 PNG: `f1d5adde197b4429fb31ff111e5ecb6a2e0fd362313023dc0712d9d4354ac9cf`.

Production audio replay with deterministic decoding/main acoustics is sample-bit-exact with census off/on on BOTH maps: 120 frames and 368,640 scalar samples per map, including moving listener/live mover, pause, mute, water and positional effect updates. Hashes are Hedge3 `8f39280937a29ea7bf4e142f77080636e3d59f993899fbe313b30b064036239f`, Pandemonium `9f2f034666543add85e9b0dd0e9eb64049467dcd83f6ea5e6b71889aaf2746ca`. See `logs/test-audio-census-maps.log`. No sample cache or result reuse was added. The warmed allocation fixture covers actual region/sweep/query bookkeeping and reports zero allocation calls. The older asset-backed audio fixture's printed hot-allocation field does not measure all game preparation allocations; it is not used to claim that the game allocates nothing.

Final host test selections PASS: audio 74 (9 ignored), collision 27 (2 ignored), census 4, frame-total/resolver 3 (including all 29 GL names), atomic logging 2. The ignored two-map census audio test was explicitly run and passed. Selections overlap; do not sum them as distinct coverage. An additional existing-test substring selection passed 14 tests but is not used as resolver evidence. `proof/HOST-TESTS.json` and logs retain commands/results. Initial qualification exposed a zero-duration synthetic-clock fixture and a reused exclusive audio-export directory; the timing fixture now supplies a positive interval and the runner gives each invocation a fresh fixture directory. No production behavior was changed to satisfy those fixtures.

**Host observer cost.**

The final source-sealed host render ABBA uses fixed dt 0.04184 on Pandemonium and 0.060 on Hedge3, no audio, normal host entry (not the console save). CPU brackets include state-snapshot construction and exclude encoding/log I/O. They exclude framework swap and the small begin/accept bookkeeping; the separate microbenchmark includes bookkeeping. These are Mac measurements, never Vita forecasts.

- Pandemonium A1/B2/B3/A4 CPU medians: [1.565042, 1.505709, 1.513167, 1.458625] ms. Oriented B-A: [-0.05933300000000008, 0.05454199999999987] ms; mean -0.002396 ms.
- Hedge3 CPU medians: [1.923125, 1.950125, 1.949625, 1.9606670000000002] ms. Oriented B-A: [0.027000000000000135, -0.011042000000000218] ms; mean 0.007979 ms.

Signed whole-frame differences are small/noisy, not evidence of a speedup. The bounded microbenchmark alternates ABBA twice, 1,000 iterations/arm: per frame, 28 brackets, 100 outer sweeps each with a delegated call, 24 exact-key lookups and frame bookkeeping. Recorded off-arm times are 356.625/314.542/306.334/314.333 ns; on-arm 6134.500/6014.208/6008.458/5996.333 ns. Oriented incremental costs are 5777.875/5699.666/5702.124/5682.000 ns, mean **5.715416 microseconds/frame** on this Mac. This is a synthetic observer workload, not real acoustic or Vita timing. Earlier exploratory host runs also passed equality but had noisier deltas; only the final sealed run above supplies the reported end-to-end host numbers.

Host State storage is 330,416 bytes. The sealed ARM disassembly proves a B-only TLS allocation request of **330,280 bytes** (`movw r0,#2600; movt r0,#5; __wrap_malloc`), initialized at startup before gameplay. This is requested allocation size, not live/rounded newlib usage. The template/code is common to both arms. Default-off branches and the small inactive TLS gate still exist on the new common SELF, so no zero-cost or old/new SELF parity claim is made.

**Vita recipe and identity.**

Exact r4 command: `cargo vita build vpk --release --offline --locked --features native_timing,vita_renderer,frame_total_only`, nightly-2026-10-11, VITASDK=/Users/anezium/vitasdk, RUSTFLAGS=`-C target-feature=-neon --cfg observer_free_B`, CARGO_INCREMENTAL=0, four jobs. Cargo.toml, Cargo.lock, build.rs and .cargo have no diff from the requested r4 base. `proof/BUILD-COMMAND.json` and TOOLCHAIN.json retain provenance. Native build took 117.64 seconds. ELF metadata margin **1,004,108 bytes** PASS; actual process parameters retain 192 MiB newlib and 8 MiB main stack. Symbol scan finds no render observer hooks. Regenerated VELF/SELF exactly match the originals; an independent clean relink was not repeated.

- ELF SHA-256: `02f8ee5c0d3009a638a3929c29eddcc6e2bdbb2c56d546e7e5e52b2a9f35be22`.
- VELF SHA-256: `42ad488fd595aa97b94ddd105b4aaa038f4b1e290828ba58d9ba9c943845c3bf`.
- SELF / eboot.bin: **38,098,654 bytes**, SHA-256 `50ac74a2ced3e4caf4ad239828df2dcbe1007f51be6b465471f00f5d3b918f69`.
- VPK SHA-256: `c89d6abeeee6370ae85b9e9b8814abb7cb7a417c6c04122913877114c4c30022`.

Both arm SELF files have identical bytes. The VPK contains only eboot.bin and sce_sys/param.sfo, no game assets. VPK zip timestamps are not a deterministic-repack claim. PACKAGE-MATRIX.json and each arm's VERIFICATION.json seal all identities/config differences. Native storage/ELF/process/conversion evidence is under proof/ and logs/.

**Console handoff and fixed reading rules.**

CONSOLE-BRIEF.md and CONSOLE-PROTOCOL.json prepare ONE 127.5-minute, eight-launch session: Pandemonium ABBA then Hedge3 ABBA. No session was executed or admitted. Preserve the human SELF `387ccf678251d4267b8ced53e207830358826f8e0642631a0459c6b0b9939990` and 181,900-byte measurement autosave `ea4cd221d9f950e613188ccf3ecf2c1eabb1d34106f413125864320d364c8fa9`, all companions and original inventories. Entry/restoration hash checks are carried over, never presumed from old receipts.

The brief carries atomic logging and r4-1's whole-log split/orphan scan, equal within-arm config-record lengths and A-minus-B exactly one byte. It retains en0 preflight, sole ownership, unchanged-driver budget, serial locked FTP, no MKD, eboot-only staging, strict audio, 180 MiB heap stop, loading/shutdown counter readouts, sampled B-A heap readouts and exhaustive independent restoration. Off-main/static-merge stay disabled; this does not retest r4's optimization amendments. r3-2 old/new parity remains descriptive/non-gating.

A/B are wall-only off/on measurements on the SAME SELF. If either oriented median B-A OR their mean exceeds 1 ms, BOTH B tables for that map are OBSERVER_PERTURBED. No observer subtraction, trimming or pooling. Each B table separately reports observed parents, inclusive median/p95/mean, exclusive mean, calls, frame total and residual. Only per-frame exclusive sums plus residual establish closure. Collision/kernel sweep views remain nested and are never added. Acoustic room/ambient/effects report total and exact repeats for K=1/4/16; same-frame-only duplicates do not hit. Any live eviction anywhere in warmup/measurement labels opportunity a lower bound. No cached trace is ever returned.

Offline helper checks: **90 PASS**, including transport-blocked imports, exact manifest arms, paired B export reads, stale census rejection, save/heap/audio identity gates, preservation/restoration byte contracts, fresh-helper binding, observer threshold boundaries and full eight-run report assembly. All **12 r4-1 log-integrity fixtures PASS**, including original r3 A/B logs and negative/drift cases. These prove helper behavior, not actual console restoration, timing, audio or ownership.

**Open risks and scope limits.**

Hardware observer cost and the actual 60/42 ms partition remain unmeasured until the console owner runs this draft. Elapsed brackets include scheduling/API waits and do not measure scheduled CPU, worker service or GPU busy time. Host entry/dt/platform differ from the measurement save and real console dt; finite state/sample/image equality is not exhaustive gameplay equivalence or audible/realtime continuity. Shared joint-finish work stays explicitly shared; mixed actor/effect submission groups are classified with actors unless effect-only. Unbracketed work is visible as residual rather than silently assigned to a subsystem.

Bounded-table eviction loses history conservatively; exact endpoint reuse does not itself establish saved milliseconds. Scene identity relies on live immutable geometry and reset at benchmark/map changes; no stale trace reuse exists. TLS lookup/clock/cache costs on ARM are unknown, and the newly compiled default-off path still needs old/new SELF qualification before an observer-free promotion claim. Census export allocates only after measurement and may raise shutdown heap; sampled 180 MiB receipts are not continuous peaks. Off-main map-transition/final-drop behavior remains unchanged and disabled in both arms. No hardware memory, continuous peak, listening or console render-equivalence claim is made.

**Cleanup and disk.**

Removed **445,492,286 logical bytes**: task-specific native target/tmp, duplicate conversion outputs and copied host executable. No per-task target or Python cache remains; the shared host target is retained. Raw outputs, logs, proof hashes and sealed native originals remain. Other clones and targets were untouched. Initial df was 460 GiB size / 388 GiB used / 49 GiB available (89%).

Before cleanup:
```
Filesystem      Size    Used   Avail Capacity iused ifree %iused  Mounted on
/dev/disk3s5   460Gi   389Gi    48Gi    90%    3.7M  502M    1%   /System/Volumes/Data
```
After cleanup:
```
Filesystem      Size    Used   Avail Capacity iused ifree %iused  Mounted on
/dev/disk3s5   460Gi   388Gi    48Gi    89%    3.7M  506M    1%   /System/Volumes/Data
```

Plain-pushed branch `vita-phase-census-r5-20261024` to origin; no force push, co-author trailer or model identifiers were introduced. Final remote identity is recorded in proof/REMOTE.json after push. ARTIFACT-SHA256.json covers retained artifacts except this report and itself, avoiding a circular dependency.
