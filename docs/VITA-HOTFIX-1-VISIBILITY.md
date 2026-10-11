# Village hotfix 1: visibility (iteration 7, step 2)

Base: `6f036e19175ca83d9380e77bfe49e675993445a6`, branch `vita-hotfix-1`.
Implementation and validation use the host only. Console measurement belongs to
the orchestrator; this change makes no performance claim before that measurement.

## Selection and ownership

`world_visibility_bounds` covers global bounds/frustum, candidate part
bounds/frustum and the LOD decision. `world_visibility_pvs` covers membership and
its cache. `world_visibility_indices` covers materialisation or the constant-time
Full exchange. Global face-mask construction remains outside the parent span.
There are no per-face/per-leaf spans or new counters.

Each batch retains one PVS entry keyed on the **current DrawContext cluster**.
The uninitialised entry is distinct from the valid fail-open `None` key. Part
membership is `All`, `Empty` or ordered `SomeParts`; fixed, unanimated props without
inline models cache the bounds boolean. A bounds snapshot protects against a
changed AABB. Transformed/offset fixed props compute bounds PVS directly without
overwriting their fixed-pose entry. Disabled PVS paths do not overwrite an entry.
All global/part frustums and LOD decisions still run on each invocation.

`Scene` holds one recent face mask, shared as `Rc<[bool]>` by main, portal,
mirror, sky and aperture contexts. A -> B -> A replaces the recent entry each
time; it does not reuse B for A. New Scenes start with empty masks and batches.
Loading/source reconstruction calls `Batch::prepare_visibility`, which
invalidates membership, reserves scratch capacities and verifies contiguous
source ranges. Rope bounds updates explicitly invalidate membership as well.
No existing code replaces `map.visibility` within a live Scene; any future such
replacement must invalidate both the Scene mask and all batch entries.

Full is allowed only for all retained parts with contiguous, ordered ranges, or
the unchanged source topology without parts. A Drop guard exchanges source and
mesh Vecs and restores them after all submissions, including returns/unwinding.
The original source length is retained for compaction comparisons. Face-side
filtering and legacy transparency sorting use mutable scratch instead. Filtered
and Detail selections copy into reserved mesh scratch; Detail keeps its original
triangle-saving statistics. The legacy transparency path retains the scratch's
capacity instead of replacing its Vec. No transparent order is cached.

The hot batch path introduces no heap allocation: partial-PVS storage is moved
back into scratch on replacement, not reallocated, and both part scratch lists
are reserved at loading. Cold/changed Scene masks still allocate once per mask
replacement. Existing downstream transparent packets/compaction and profiling
storage have their existing allocation behaviour.

## Host verification and remaining rendering checks

The synthetic visibility fixture is shared with the existing two-room PVS tests.
New tests compare **exact index sequences** and batch/culled/triangle/detail
statistics against the original selector, cold and warm: all/empty/some parts,
same-cluster rotation, cluster switches including A -> B -> A, `None`, required
PVS with optimize=false, fixed/transformed/changed bounds, sky views, near/far
LOD, motion/deforms/animation/inline guards and normal-only water deformation.
Additional tests check Vec restoration on unwind, mutation isolation, scratch
pointer stability, mask sharing and reconstruction invalidation.

Host results: `cargo check --locked` passes; `cargo test --locked` reports
689 passed, 0 failed, 1 pre-existing ignored (676 baseline + 13 new tests).

The no-PK3 unit checks for axial billboards, material face-side/sky winding,
water normal waves, oblique portal clipping and render_fx triangle clipping run
as part of `cargo test --locked`. The full GPU checks in `billboard_check`,
`sky_face_check`, `water_check`, `camera_portal::check` and `render_fx::check` all
load original map/material data and **require game PK3s**; they cannot run with
synthetic fixtures alone and were not run here.

For aligned console frames require identical `batches_drawn`, `batches_culled`,
`world_counts.triangles`, DrawStats triangles and `detail_saved`. The supplied
iteration-6 reference has 642 visits, 164 drawn, 478 culled and 166.3 ms in
world_visibility; no reference triangle count was supplied. Confirm counts and
images at the same camera/time/pose/options, including sky, portals/mirrors,
water and transparency, then measure the three sub-spans on the bench and in
nopreview. No console result is inferred from host tests.

The design follows Astra section 3. The pure projected-error helper and injected
test callbacks permit window-free LOD tests; production uses the same projection
and screen height as before. Bounds snapshots and a contiguous-range check make
the proposed invalidation and Full assumptions explicit.
