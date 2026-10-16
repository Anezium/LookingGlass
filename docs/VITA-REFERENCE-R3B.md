# Reference r3b fix round and console handoff

Runtime `27c4d40c978f015b74ee5f45ee446d0baab767b0` on `vita-reference-r3`, following r3 `4f76504`.
Fix commits: `4276025` restores r2 tuning and preserves kill switches;
`27c4d40` samples built-before-drop scene memory and excludes transition frames
from gameplay profiling. Existing source merges are unchanged; Cargo.lock is
byte-identical. No history rewrite or force push occurred.

## Defaults and complete r2 comparison

The code and `configs/PL-def-r3.json` now ship the same 81 values (the exact
serialization equality test passes): quality low, render_scale 0.75,
gpu_experiment gpu222, fx_batching material, fx_streaming ring,
fx_upload_cache true, fx_max_layers 1, actor_frustum_cull true and profiler light.
Every one of r2 PL-def's **59 explicit fields and 15 inherited code defaults**
is identical. CONFIG-DIFF-R2.json lists every field and its provenance.
Only seven new fields have no r2 counterpart: retained_maps ["*"], legacy_maps [],
actor_single_prepare true, and the four simulation cache switches false.
The first three implement reviewed allmaps and actor-v1 selection; simulation
caches remain off until console off/on acceptance. Actor v1's -1.23 ms was
measured with this restored r2 tuning. No new performance measurement is claimed.

Implicit actor preparation now disables itself with one warning if a
prerequisite is off. Legacy renderer, skin_overlap false, skin_workers zero,
actor fast-path/layer-cache off, actor_layer_reuse true and fx_prepare_cache false
preserve the requested configuration. Zero workers disables omitted overlap;
explicit overlap=true still requires workers. Only explicit actor_single_prepare
true with a missing prerequisite is rejected. Both older tests run without
actor_single_prepare false workarounds. Unknown fields remain rejected.

## Transition memory

Chapter/console replacement, natural exits and live save/retry reload emit one
`[lg/transition-memory]` JSON line each, containing Unix time/from/to/kind and
both before_drop and after_drop snapshots. Each snapshot includes mallinfo
used/free/arena and vitaGL RAM/VRAM/PHYCONT free. Before_drop has both complete
scenes alive; after_drop follows the old scene destructor and can still include
other old resources. The full transition frame is excluded from CPU/wall timing.
Startup save loads have no outgoing scene to drop. Host values are null; the
actual Vita readings remain a console gate. The native pinned header ABI and
linked symbols were checked. PROTOCOL.md requires these overlap samples,
external construction sampling, >=5 real exits including utemple -> potears1,
ten chapter-group save/reloads and immediate stop above 180 MiB used.

## Validation on the Mac

Features on: **877 passed, zero failed, three ignored in each sim arm**.
Default features: **795 passed, zero failed, three ignored in each sim arm**.
Real-data tower fan test passes separately in both arms; two other asset ignores
remain. Eight source-package boundary tests and the source index audit pass.
All 15 actor village PNGs per arm match pkg-2 byte-for-byte (30 comparisons).
Allmaps: 36 maps, 39 visits, 156 PNGs and exact structural inventory match the
source evidence; observed load milliseconds are excluded, and injected fallback
matches the legacy image/portals. Ten-map sim parity matches all **3,900 frame
hashes per arm** off/on and against source r2, with equal workload counters.
Separate-process save restarts pass **36 registered +17 native Store fixtures
per sim arm**, with equal saved payloads and continued futures. The legacy
checker restores the 36 written cases per arm; fifteen absent external fixtures
remain uncovered. The broad writer reproduces the inherited garden2 Cat trigger
failure after 188 successful fixtures per arm; this is not reported as a pass.
R2 audio/control/preview checks pass: actual host playback intervals, bounded
village audio cache, handheld controls, preview persistence/overwrite/backup/menu,
241 advancing frames without periodic play capture and the every-frame witness.
Protected r2 production audio/input/menu/preferences/chapter/inventory and
preview behavior remain unchanged. All six game archives and Cargo.lock match
the pinned inputs. Shared Mac lock checks, own targets and four Cargo jobs apply.

## Package and remaining console gate

Native Mac ~/vitasdk, nightly-2026-10-11, pinned vitaGL archive/header, --locked,
-neon, 192 MiB newlib and two 8 MiB stacks. Imports, regenerated VELF equality,
ZIP CRC, embedded SELF/SFO equality and absence of host probe/actor diagnostics
pass. ELF margin **1,452,572 bytes >=3,348**.
SELF SHA-256: `ec7ef3ae34ba514ed7d3ffff23a2beeb01bfb3b3e1a27046ded0af898eb8472c`.
VPK SHA-256: `2666984ba01aab0cfed599702afe55cda77d77738bedb1b0e324fb9df3705104`.

Deliverables: `~/vita-j0/atoz/reference-build-r3b/`; original reference-build-r3
is untouched. SHA256SUMS covers evidence and package; A-stock/SHA256SUMS.txt
covers native artifacts. SELF and configs always roll back together.
The unapplied simulation-on candidate is published on `vita-reference-r3b-sim-on`
and exported as PREPARED-SIM-ON.patch with commit/parent/adoption metadata in
PREPARED-SIM-ON.json. It flips only the four defaults, matching preset and test;
a new verified SELF/config pair is needed after console acceptance.
No console, bls12, timed host campaign or hardware performance/fit/progression
acceptance occurred. Human audibility, console memory and natural exits remain
pending the owner's protocol.
