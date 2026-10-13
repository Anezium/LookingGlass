# J21 GPU skinning: feasibility and implementation

## Feasibility gate, recorded before engine changes

Base: `e705a9a`, branch `vita-night-j21`. No device access. The graphics source
inspected is vitaGL `dca4b9d143290d78ec043131a19be36c78cdc7b5`; miniquad remains
pinned at `6f891804784bf4a5b39313ee6499344cb0d7e8b6`.

The village skeleton census below counts all stored surfaces, including hidden
caps. Surface palettes count positive weights, not attachment tags. Per-surface
details and archive identities will accompany the local evidence.

| Model | Bones | Surfaces / vertices | Maximum surface palette | Influence histogram (influences: vertices) |
|---|---:|---:|---:|---|
| Alice | 131 | 19 / 784 | 40 | 1:643, 2:141 |
| Torch gnome 3, 4, 5, 6 (each; same SKB) | 45 | 8 / 765 | 28 | 1:490, 2:207, 3:65, 4:3 |
| Cheshire | 53 | 4 / 621 | 34 | 1:306, 2:247, 3:40, 4:18, 5:4, 6:5, 7:1 |
| Club guard | 34 | 8 / 671 | 13 | 1:413, 2:210, 3:28, 4:17, 5:3 |
| White rabbit | 53 | 1 / 826 | 47 | 1:624, 2:199, 3:3 |
| Insane child (existing native regression, outside village) | 39 | 5 / 584 | 30 | 1:458, 2:105, 3:21 |

Village knife and guard staff attachments are TAN vertex animations, not SKB
skins: no bone palette/influence count applies. They retain their current CPU
path. Skeletal tag transforms remain CPU-owned for all attachments and gameplay.

vitaGL `source/get_info.c:473` reports 2,048 vertex uniform components, but `:678`
reports only 128 vec4 vertex uniform vectors. Those are inconsistent; **128 vec4
(512 floats / 2,048 bytes)** is the conservative planning budget. Attributes and
streams are 16 each (`source/shared.h:40`, SDK `psp2/gxm.h:1285`). Varyings are
reported as eight vec4 and also 40 floats; use eight vec4. The SDK header/API docs
expose default-uniform-buffer reservation/reflection, not a universal byte limit
that resolves the contradictory GL queries. Shader compilation remains a device
gate. See [SDK limits](https://docs.vitasdk.org/group__SceGxmUser.html).

An affine palette costs three vec4 / 48 bytes per bone. Reserving eight vectors
for projection/model leaves 40 bones: Alice's largest surface is exactly at that
budget, Cheshire fits, rabbit needs splitting; uploading all Alice's 131 bones
would cost 393 vectors / 6,288 bytes and is rejected. Positions alone are feasible:
seven independent bone-local offsets plus two index and two weight vectors, UV
and normal would use 13 of 16 attributes. Four-weight truncation is unacceptable
(Cheshire has ten vertices above four influences, guard has three).

**Normals, rather than the palette, bound the initial accepted path.** The CPU
zeros normals, accumulates unnormalised cross products in triangle order, then
normalises for directional vertex colour (`character/skinning.rs:86-112`). This
is not generally the weighted rotation of a bind normal. Incident triangle counts
reach Alice 10, gnome 11, Cheshire 10, guard 8, rabbit 9. Directly supplying the
deformation of all neighbours exceeds 16 attributes; a vertex-texture adjacency
algorithm would be a separate, much more expensive design, not established here.

**Decision: implement a conservative hybrid, with exact normal semantics.** Only
whole surfaces whose every vertex has exactly one unit weight on the same bone
qualify. For positive uniform scales and unit rotations their triangle normals
transform by that bone's rotation, so static
bone-local positions/normals plus a single affine matrix suffice. All other draws
remain on the existing persistent CPU workers. This intentionally does not claim
general multi-bone GPU skinning or the brief's assumed 3–5 ms opportunity. A
single-bone draw needs three matrix vectors, one argument vector and four
projection vectors: **8 / 128 vectors**, existing **4 / 16 attributes**, existing
12 / 32 varying components. No palette split is needed for accepted draws.

Alice has 82 / 784 stored vertices on seven such surfaces (caps included),
Cheshire 30 / 621 on one, guard 70 / 671 on one; gnome and rabbit have none.
Actual visible coverage is lower when caps/surfaces are hidden. Existing native
child data also has two rigid surfaces (38 / 584 vertices). This is a bounded
first GPU package, not a route to recovering the entire actor budget.

Normal-dependent material stages (environment UV, dot alpha), expanding/frozen/
electric/power shells, untracked live UV/topology changes, and any CPU-position
consumer require the original CPU path. Ordinary alpha/ghost/dissolve can retain
the original grouping, layer order and fragment shader. Alice's contact shadow
uses feet, support sweeps and a separately generated 26-vertex mesh
(`lighting.rs:175-231`), not body vertices; it needs no GPU skin readback and is
outside this change (owned by the concurrent milestone).

Acceptance tolerance, fixed before implementing: position absolute error <=
0.001 world units; normal direction <= 0.5 degrees for nondegenerate normals;
vertex RGB <= one byte; alpha exact. Captures: unchanged silhouettes within one
pixel, <= two bytes per channel on actor interiors, with raster edges separately
reported. Zero-normal/near-degenerate cases must fall back if they fail these
criteria. Switch `gpu_skinning` defaults off. Static uploads happen once per
immutable model surface, never once per occurrence/frame; per-draw skin data is
48 matrix bytes plus 16 argument bytes. No steady-state allocation is permitted
in the new skin descriptor/cache path after warmup. Full-frame presentation
already allocates and is not claimed allocation-free.

Expected console gain is **0–0.4 ms**, assumed, for this small accepted subset;
3–5 ms is not supported for this design. The best next CPU alternative is
authoritative producer revisions on the game thread to skip unchanged layer
comparisons/copies (planning 0.3–1.5 ms, unmeasured), retaining original worker
kernels and avoiding the measured J17 +2.09 ms worker-side comparison regression.
No console gain, GPU instruction cost or shader compiler result is proven.

## Stop requested for migration

The orchestration changed while the first implementation edits were in progress.
This branch therefore commits **feasibility documentation only**. Engine sources,
Cargo.toml and Cargo.lock match the J19 base. The incomplete edits were saved as
`jalon-21/WIP-gpu-skinning.patch` in the local handoff directory and removed from
the worktree because the referenced GPU module/shader had not yet been written.
That patch does not build and must not be treated as an implementation package.

The rigid path above is a candidate design, not an accepted, tested GPU feature.
General multi-bone skinning is not proven impossible: palette splitting fits the
conservative budget, while exact normals remain an unresolved design problem.
Vertex-texture adjacency is supported in principle by the reported 16 vertex
texture units (`get_info.c:697`), but its precision, shader compiler acceptance,
workload and cost were not investigated sufficiently to accept or reject it.

No new unit/native/GPU tests, host captures, allocation measurements, A-stock
build, import check or ELF-margin check were run. No J21 VPK, package/config set,
visual parity or performance result exists. Historical J19 build/evidence is not
J21 implementation evidence. No device/server access occurred. See the local
`jalon-21/HANDOFF.md` for the draft's known defects and exact continuation order.
