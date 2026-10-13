# J21 handoff — 2026-10-14

Stopped at the user's migration request. Work continues on BLS12 in a later
session; no server connection or transfer was performed here. No Vita access.

## Preserved state

- Branch: `vita-night-j21`, base `e705a9a935864bea8459d5a3856405f1eabe8066`
  (`origin/vita-night-j19`). Repository remote: `https://github.com/Anezium/LookingGlass.git`.
- Worktree: `/Users/anezium/vita-work/lookingglass-j21`.
- Committed changes: this handoff and `docs/J21-IMPLEMENTATION.md` only.
  Engine sources/dependencies are restored to the base; the branch has no GPU
  implementation and needs no WIP code to remain on the known J19 source.
- Local handoff: `/Users/anezium/vita-j0/village-retained-world/jalon-21/`.
  Read `IMPLEMENTATION.md`, `HANDOFF.md`, `WIP-STATE.json` and
  `WIP-gpu-skinning.patch`. The local patch is not part of the pushed source
  branch: copy this directory separately when preparing the server session.
- Patch: 13,450 bytes; SHA-256
  `7c1d3993eaced3bb76c41a17fe06ea7ba0c522de73a1aedf15e926e313b47f0e`.
  It is incomplete and does not build; no shader/module was created.
- The unrelated `docs/RCHESS1.md` checkout difference was preserved, excluded
  from the patch and commit. Its before/after SHA-256 is in `WIP-STATE.json`.
- No J21 VPK, package, configs, build/import/margin result, native captures or
  timing/allocation result exists. Do not install a J19 VPK as a J21 result.

## Feasibility numbers obtained

Counts come from read-only decoding of user-supplied PK3/SKB data. Surface counts
include hidden caps. Max palette counts positive-weight bones in one surface,
not all skeleton/tag bones; the detailed design is in `IMPLEMENTATION.md`.

| Model | Skeleton bones | Surfaces | Vertices | Max surface bones | Max influences | Max incident triangles |
|---|---:|---:|---:|---:|---:|---:|
| Alice | 131 | 19 | 784 | 40 | 2 | 10 |
| Torch gnome 3/4/5/6 (each; shared SKB) | 45 | 8 | 765 | 28 | 4 | 11 |
| Cheshire | 53 | 4 | 621 | 34 | 7 | 10 |
| Club guard | 34 | 8 | 671 | 13 | 5 | 8 |
| White rabbit | 53 | 1 | 826 | 47 | 3 | 9 |
| Insane child (native suite, outside village) | 39 | 5 | 584 | 30 | 3 | 10 |

Influence histograms (`influences:vertices`):

- Alice: `1:643, 2:141`.
- Each torch gnome: `1:490, 2:207, 3:65, 4:3`.
- Cheshire: `1:306, 2:247, 3:40, 4:18, 5:4, 6:5, 7:1`.
- Club guard: `1:413, 2:210, 3:28, 4:17, 5:3`.
- Rabbit: `1:624, 2:199, 3:3`.
- Native child: `1:458, 2:105, 3:21`.

Per-surface positive-weight palette sizes, in the parsed draw order:

- Alice: `material3=11, material13=2, material12=2, material1=4,
  material11=40, material14=3, material4=1, material6=9, material5=1,
  material7=1, material8=2, material19=8, material2=31, material18=3,
  material10=9, material9=1, material16=1, material15=1, material17=1`.
- Each torch gnome: `material1=9, material2=28, material3=10,
  material4=2, material9=10, material10=3, material5=2, material6=3`.
- Cheshire: `body=34, head_eyes=13, head_ring=1, head_face=16`.
- Club: `top_half=13, bottom_half=8, bottom_leg_right=4,
  bottom_leg_left=4, top_hand_right=3, top_hand_left=9, top_head=1,
  cap_body=5`.
- Rabbit: `material1=47`.
- Native child: `material3=30, material1=7, material2=5,
  material4=1, material5=1`.

Village held knife (`models/w_knife.tik`) and guard staff
(`models/w_clubstaff.tik`) use TAN vertex animation: no skeletal palette or
influence count; preserve the existing prop path. Other village world art/FX
was not included in the actor scope. Attachment tags/gameplay still need CPU
poses even if vertex deformation moves to GPU.

Pinned vitaGL: `dca4b9d143290d78ec043131a19be36c78cdc7b5` at
`/Users/anezium/vita-work/vitaGL`. `source/get_info.c:473` reports **2,048
vertex uniform components**, but `:678` reports **128 vec4 vectors**. These are
inconsistent: plan conservatively with **512 floats / 2,048 bytes**. GXM SDK
`psp2/gxm.h:1285` and vitaGL `shared.h:40` allow **16 attributes / 16 streams**.
Use eight vec4 varyings (the driver also inconsistently reports 40 floats).
The SDK API docs confirm the attribute/stream limits and default-buffer
reservation/reflection; they did not resolve a universal uniform byte ceiling.
Reported vertex texture units: **16** (`get_info.c:697`).

Affine matrices cost **3 vec4 / 48 bytes per bone**. Reserving eight vec4 for
model/projection gives a **40-bone** palette. Alice's whole skeleton would use
393 vectors / 6,288 bytes and does not fit that budget; per-surface palettes do
fit except rabbit, which can be split. Therefore **bone counts alone are not a
GPU no-go**. Never reject all GPU skinning merely because Alice has 131 bones.
Seven independent local offsets plus two index, two weight, UV and normal
attributes use **13 / 16** attributes for positions. Four-weight truncation loses
ten Cheshire vertices and three guard vertices and is not acceptable.

Current normals (`src/character/skinning.rs:86-112`) are sums of area-weighted
animated triangle cross products, then normalised for quantised directional
vertex colour. A weighted rotation of bind normals is not equivalent. Direct
per-vertex neighbour deformation payloads exceed 16 attributes on the general
meshes. Static vertex-texture adjacency could be another route; compiler
acceptance, precision, cost and feasibility of that route remain unresolved.

The initial candidate is a conservative rigid-surface hybrid: accept exactly one
unit weight on one common bone for the whole surface; fallback for all other
surfaces and every CPU-dependent effect. Candidate stored coverage: Alice
**82/784 vertices on 7 surfaces**, Cheshire **30/621 on 1**, club **70/671 on 1**,
gnome/rabbit **zero**; native child **38/584 on 2**. Caps are included, so visible
coverage is lower. Candidate shader budget: **8 / 128 vectors, 4 / 16 attributes,
12 / 32 varying components**, **48 matrix + 16 argument bytes per draw**.
No palette split needed for that subset. Estimated gain **0–0.4 ms**, assumed;
it does not substantiate the brief's assumed general-path **3–5 ms**.

Proposed tolerance (not measured): <=0.001 world-unit position error, <=0.5-degree
normal-direction error away from degeneracy, <=1 RGB byte, exact alpha. Captures:
<=2 bytes per channel on actor interiors; edge displacement <=1 pixel, report
edges separately. No host GPU capture has yet tested this tolerance.

CPU consumers to preserve: normal-driven environment/dot-alpha layers, frozen/
electric/expanding/power shells, live UV/topology edits and transparent sorting
centres. `CullGuard`/material preservation must be resolved before skipping any
worker output. Alice's contact shadow uses feet and support sweeps to produce
26 separate vertices; it does not read body vertices (`lighting.rs:175-231`).
Its optimisation belongs to J20 and was not touched.

## What is half-done in the saved patch

- `Config.gpu_skinning`, default off; a planned `gpu_skin` packet tag.
- A planned CPU-only `Rigid` metadata field on immutable `SkinSurface`.
- Eligibility hooks and per-surface worker/output skipping in `overlap.rs`.
- Submission hook preserving the serial kernel if a skipped surface cannot enqueue.
- Small pass-constructor and reusable pass-container visibility changes.

Missing: the entire `render_fx/gpu_skinning.rs` module, eligibility implementation,
metadata creation, shader, static buffers, descriptor pool, resource lifetime,
ordered renderer integration, counters, tests, captures, configs and package.
The patch references that missing module and cannot build. It is a sketch only.

Known draft issues: the no-overlap branch bypasses persistent workers for CPU
fallbacks; restore those workers. The disabled route needs its old cheap path
instead of extra per-surface Arc clones/eligibility checks. Update all callers
and synthetic struct fixtures after signature/field changes. Revisit overflow
accounting and slot/revision handling for zero-length GPU output ranges. Normal
transforms need the cofactor of the actual affine linear transform, or a strict
positive-uniform-scale/unit-rotation restriction: mirrors, non-unit attachment
rotations and degenerate normals cannot silently use a plain rotation formula.
Transparent sort centres cannot read stale CPU vertices.

## Handoff checks

`git apply --check` accepts the saved patch against the restored engine source.
Changed documentation passes the source-path/content audit and whitespace check.
The whole-worktree source audit fails on pre-existing `.cargo/config.toml` and
`configs/**` paths absent from that audit's allowlist; no audit tool or existing
config was changed to suppress this baseline failure. No compilation was run:
the commit contains only documentation and retains the known J19 engine source.

## Exact next steps, in order

1. On the server, create a dedicated checkout of `vita-night-j21`; verify its
   source base and pinned dependencies. Transfer the entire local J21 handoff
   directory separately, verify the patch SHA-256 above, and read this document
   plus the original J21 brief and prior milestone context. Preserve other
   worktrees and J20's actor-independent work. Do not run `cargo update`.
2. Resolve the scope/normal gate before applying the sketch: either accept the
   small rigid subset explicitly as the first package, or investigate a genuine
   multi-bone normal reconstruction method within actual graphics limits. Do not
   present the current subset as completed general GPU skinning. If no acceptable
   design fits, stop with the measured numbers and CPU alternative below.
3. If proceeding, apply/rework the patch only in that dedicated checkout.
   Implement immutable load-time metadata and a bounded reusable per-frame
   descriptor pool; upload each immutable model's buffers once. Keep GL/resources
   on the game thread, numeric data only on workers; keep persistent CPU fallback.
   Put any Vita-specific code under `cfg(target_os = "vita")`; portable LG code
   must remain switch-controlled. Audit all CPU-position/normal consumers first.
4. Implement the actor shader and retained draw integration. GPU packets must be
   merge barriers, preserve authored layers and joint-alpha order, and invalidate
   the old program's submission/uniform caches at each GPU-program transition.
   Validate eligible draw budgets/compilation and fall back safely on rejection.
5. Add meaningful numerical/ownership/allocation coverage, then run the existing
   native near/far actor sets, alpha/ghost/dissolve and Alice shadow captures
   against J19 at the stated tolerance. Record positive GPU eligibility/draw
   counts, CPU/fallback phase times and J19 actor upload bytes; prove descriptor/
   static-cache steady-state allocation behaviour. Host timings are not Vita ms.
6. Only after those gates pass, create `jalon-21/pkg-1` with NOTES, `PL-def`
   (GPU on), `PL-def-j21-off` (=J19 PL-def), `PL-def-off`, unchanged
   `vita-sim-all.json`, A-stock VPK/ELF/SELF/VELF/SFO, ELF-margin/import checks and
   SHA-256 manifest. Commit/push `vita-night-j21` at every package, never force.
   The console-owning agent performs hardware validation; this implementation
   stream has no console authority.

Best CPU alternative if the GPU gate fails: producer revisions on the **game
thread**, covering pose/scale/visibility/UV/topology/appearance and generation,
to skip unchanged actor-layer comparisons/copies. Retain the persistent skin
workers. Planning gain **0.3–1.5 ms**, unmeasured; J17's worker-side comparison
lost **2.09 ms** through collect wait and must not be repeated. No CPU alternative
was implemented in this session.
