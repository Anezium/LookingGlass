# Exact simulation and FX candidates, second round

Base: reference r3c `a209719`. The four console-accepted first-round switches
remain enabled. These independent startup switches default to **false**, including
in `configs/PL-def-r3.json`; console off/on acceptance is still pending.

- `sim_triangle_candidates`: lazy model sweeps first prepare transformed triangle
  vertices and the **same** six-point prism bounds as the existing hull builder.
  They construct bevel planes only for intersecting triangles, once per placement.
  Brush/triangle order, degeneracy handling, clipping arithmetic and full-collider
  consumers are preserved. New placements own new immutable caches. Non-finite
  bounds cannot certify a rejection. Cache size is bounded by model primitives;
  requesting a full collider after a partial query can retain both representations
  until that placement is released, so console memory measurements remain required.
- `sim_fan_candidates`: tower2 tests the existing conservative lazy placement
  bounds before constructing a fan substep collider. It preserves every angular
  sample, contact/crush decision, rollback, and final eager world publication.
- `fx_merge_local_reuse`: a change elsewhere in the sorted FX plan no longer
  discards every destination's geometry certificate. Reuse requires the exact
  recycled output version and the ordered source identity/topology/vertex versions.
  Changed sources still copy; changed topology still rebuilds indices. Output
  certificates prevent aliasing across opaque/transparent merge banks. No draw,
  material, sorting, source order or worker-lifetime policy changes.

The measured leads were hedge3's per-frame lazy triangle construction, tower2's
eager fan contact candidates, and skool2's repeated FX index work. Smaller pickup
and attached-particle preparation pools were profiled and left for later work.
These changes do not introduce threads, approximation, simulation cadence changes,
serialized state fields or asset dependencies.

Host evidence and the console owner's same-SELF protocol are in the private
`~/vita-j0/atoz/sim-maps-2/` directory. The host probe remains feature-gated and is
excluded from Vita packages. Workload counters and simulation/image equality are
host evidence; they do not establish console wall-time, memory or fidelity acceptance.
Rollback the SELF and renderer configuration together: older references reject the
new keys and otherwise fall back to all renderer defaults.
