# J23 phase 2 review fixes and revised Mac package

Runtime source `6cfc5f8801e2300a1e576a22900064c07718dc6d`, branch `vita-night-j23`. All implementation checkpoints are pushed. The explicit 2026-10-15 night decision authorizes this revised console A/B package irrespective of the short host rerun. Actor preparation and puff fusion remain default off. No console or remote host was accessed; console acceptance, moving parity, transition captures and soak remain pending.

## Review fixes

- `778ff4d`: publication banks use nonserialized runtime actor identity plus immutable CPU surface identity. NPC ids survive vector moves and visibility/LOD skips; model and Alice scopes cover the remaining skin producers. Previous numeric inputs are found by actor identity rather than draw position. Idle ids expire after 120 unused preparation frames; capacity pressure evicts older unused ids without evicting current/previous-frame actors. Tables remain bounded to 2048 ids, 65536 bank vertices and 262144 bank indices. Repeated same-actor draws reserve disjoint destinations; held immutable views remain valid.
- `12ce606`: Scene::from_parts resets banks and previous numeric snapshots for map transitions and same-map save restoration, only after the overlap frame has drained. Tests cycle twelve replacements/reloads with 20480 bank vertices per warm scene, assert every reservation succeeds and the warm baseline repeats, and prove old SkinSurface weak references expire while held geometry stays readable.
- `cf8ecac`: validate() rejects missing prerequisites (including retained renderer selection); startup prints actor_single_prepare active=true/false. Invalid configuration follows the existing loud fallback-to-defaults policy, rather than silently accepting an ineffective switch.
- `671d5a4`: the six new freeze/electric mutable-consumer guards are conditional on active single preparation. Inherited preservation guards stay active as before. Disabled-arm policy is covered by a targeted test and the base comparison below.
- `6cfc5f8`: light CSV appends actor_direct_surfaces, actor_mutable_consumer_surfaces, actor_publication_banks and actor_publication_capacity_bytes (128 total columns). Surface counters accumulate across views; bank/capacity values are gauges. Existing CSV tests verify sums, last-value gauges, chronology and frame resets. No exclusive probe was added to delivery configs.

## Tests and parity

Feature suite: test result: ok. 862 passed; 0 failed; 2 ignored; 0 measured; 0 filtered out; finished in 1.00s
Default suite: test result: ok. 782 passed; 0 failed; 2 ignored; 0 measured; 0 filtered out; finished in 0.83s
Default check, native host build and both GameThread negative Send/Sync checks pass. Cargo.lock remains d18dd40246bf2eaca095b2ff364a2eca20d7c95fabd0c76231ff1072205775ee; the ownership marker and inherited source-allowlist exceptions are unchanged. The changed-source audit passes. Initial compile-fixture failure is preserved separately and is not a passing run.

Live off/on and private Slot 4 reload off/on each match all 180 simulation and final submission digests. Both live frame-90 write/read witnesses pass. All 120 matrix PNGs and 15 renderer PNGs match across arms; actual-data one/two-worker checks pass. Cases cover six map/resource replacements, changing poses, duplicates, blink/live UV, shells, hidden/reappearing surfaces, attachments, fullbright, first-person and second views. All 135 disabled controls also match the prior functional6 source controls. Exact delivery configs receive an additional 15-image check in each arm against J20 final3/J23 functional6.

The disabled arm matches base 86967ea for 180/180 simulation digests, 180/180 submission digests and 15/15 renderer images. Base source is an isolated git archive, never a checkout mutation of another clone. BASELINE-SOURCE.json pins the archive; BASELINE-OBSERVER.patch adds only host submission observation and the same fixed 1200/680 replay aspect as the candidate harness. The observer adds no actor optimization or preservation change. Both binaries and all raw samples/captures are preserved.

All 120 retained live frames match draw/uniform/upload bytes, stream capacity and buffer growth. On-arm direct activation is nonzero in every retained live frame. Frozen baseline uploads zero bytes for all 120 frames in both arms. TRAFFIC.json and FROZEN-TRAFFIC.json record distributions, bank/capacity gauges and consumer counts. Array-capacity accounting excludes allocator/shared-asset metadata, GPU ring memory and external held views; no whole-game zero-allocation claim is made.

## Short Mac timing rerun

One pinned binary; three alternating AB/BA/AB pairs; 180 frames per arm, discard exactly 60, retain every remaining 120 including stalls. The pipeline owns mac-timing.lock across workloads. Each accepted pair has every observed 1/5/15-minute load <=3, with raw uptime and one-second monitoring preserved. All three pairs qualified on their first attempt; maximum observed 1/5/15-minute loads, including one-second monitors, were 2.72900390625/2.51025390625/2.9990234375. No attempt was rejected, repeated or trimmed. Preparation uses the same exclusive component sum as the prior actor gate, including collection and shadow/atmosphere; positive finish/upload deltas are charged. Quantiles are sorted[floor(q*n)], with p95 and p99 retained in timed/PHASE2-SUMMARY.json. These are host observations, not Vita predictions.

- Pair 1: prep median/p95 0.2731245/0.3950010 -> 0.2848120/0.5956250 ms; body median/p95 2.5093960/3.0556660 -> 2.2679795/3.0959580 ms. Median reduction -4.279%, charged reduction -4.279%; FAIL.
- Pair 2: prep median/p95 0.2663550/0.3988320 -> 0.3093545/0.6038340 ms; body median/p95 2.4012500/3.1585420 -> 2.3845415/2.9693340 ms. Median reduction -16.144%, charged reduction -16.144%; FAIL.
- Pair 3: prep median/p95 0.2693545/0.3927510 -> 0.2343125/0.5573760 ms; body median/p95 2.1966250/3.0574170 -> 2.0819795/2.9074580 ms. Median reduction 13.010%, charged reduction 12.665%; FAIL.

Full rerun gate: **FAIL**. Preparation p95 worsens in every pair, so the tail is not gone. Exclusive gather and simple-publication p95 are higher in all three pairs; collection-wait p95 is lower in all three. These component observations do not establish a cause. The earlier campaign remains unchanged; this result does not establish a console improvement. Complete per-component p95/p99, body distributions, counts, accepted/rejected attempts and load evidence are under timed/.

## Native revised package

One A-stock native Mac build: ~/vitasdk, nightly-2026-10-11, CARGO_BUILD_JOBS=4, --locked, vita_renderer and the same pkg-2 pipeline. Isolated targets and archived source are entirely within this task's clone/evidence paths. No other clone or target was modified.

VPK SHA-256: `bd1dbee3d252c7b14baa93172790a5f8c55b65e50696566690a4a7e4c5f9eb22`.
SELF SHA-256: `f6d43d80b81cb850568e60ca44e779fe223422d0a79a27da71f3e430b3fedde4`.
ELF RX-to-RW/.init_array margin: **1569944 bytes**, minimum 3348.
Pinned vitaGL library `6842d410b2571fe4a31d4fe933dd76c5679330a28e80a5c27b1539a534d91b3b`, header `5da33f7b11eadce8e8d4174ff63d94ec6e3460e03c193b4c7b722a5afb8b14d7`. Native import resolution, regenerated VELF equality, ZIP CRC, two-entry VPK and embedded SELF/SFO identity pass. No game data is packaged or committed.

PL-def, PL-def-actor and vita-sim-all are byte-identical to the original phase-2 package. The sole arm difference remains actor_single_prepare=true; profiler=light, actor_prep_profile=false, world_puff_fusion=false. Both arms use the same SELF.

PROTOCOL.md retains the three-pair console median/p95 gate, historical 36.63 ms comparison, separate moving check and >=300-second on-arm soak. It now requires activation/consumer counters, bank/capacity traces, skin gather/collect p95 and p99, and captured on-arm freeze/electric, real map change and same-map save reload witnesses outside timing. Console execution and acceptance remain pending; no 30-fps or PACKAGE-READY claim.

SHA256SUMS covers deliverable evidence, raw cells, scripts, configs and binaries, excluding build targets, archived baseline source (separately pinned), Python caches and itself. No cargo update, force push, prohibited attribution, game/save payload in Git or process-pattern kill occurred.
