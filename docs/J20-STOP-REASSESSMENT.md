# J20 continuation stop after package 2

The next listed lever, ray-shaped light sweeps, duplicates the pinned implementation. `SweepBounds::overlaps` at `src/collision.rs:61` performs segment slabs, BVH traversal uses it at `:510`, dynamic collider roots and hulls use it at `:398` and `:404`, and static support hulls use it at `:816`. No collision or lighting code was changed and no 0.7–1.3-ms console gain is credited.

Only the outer dynamic-union/collider loop at `:751`/`:763` retains an AABB guard. A passing collider immediately executes its complete root slab test before the hull loop. Moving that predicate outward does not remove hull clipping and would duplicate positive-root work unless refactored. The union guard could avoid a small outer loop in an aggregate miss; it is not the proposed static-BVH optimization and has no measured console gain supporting the 0.3-ms selection threshold. The listed work's incremental estimate is 0 ms because it is already implemented. The brief's below-0.3-ms rule therefore stops continuation after package 2.

The existing independent AABB oracle was run against user-supplied hedge2, hedge3 and hatter1 data, for both world variants: 60,000 random queries plus hull-center cases pass exact trace comparisons. The fixture pruning/oracle test also passed in the 842-case feature suite. These verify inherited slab work; they are not a newly implemented lever.

Step 6 is not entered after this stop. Shared world VBO/IBO pages, pinned layout-cache feasibility and contact-shadow reuse are deferred, without claimed savings. No package 3 was produced. Skinning and actor-layer implementations remain untouched.

Packages 1 and 2 have verified Linux builds and captures, but their historical Mac depth-fix discrepancy remains **UNRESOLVED**. Neither has a `PACKAGE-READY` marker. The fix stays on. Console FX-upload, heap, loading and normal-play acceptance are pending/unmeasured. Complete final evidence is indexed in `/srv/fast/projects/alice/jalons/jalon-20/FINAL-REPORT.md`.
