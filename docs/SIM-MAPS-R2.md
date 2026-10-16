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

## Mac validation, 16 October 2026

Runtime checkpoint `f9ef6ba` passes four independent/combined 13-map off/on
campaigns: 4,800 frame hashes per arm per campaign, 19,200 exact comparisons,
and 52 identical live PNG pairs. The original eight-map r3c baseline hashes
also match with the new switches off. The saved-scene renderer fixture adds
15 identical image pairs, including camera, material-clock and fullbright variants.
Both full unit suites pass 881 tests with zero failures and three explicit ignores;
the real-data fan fixture passes separately in both arms. Native tower2/hedge3
checks and routes plus funhouse checks pass in both arms. The final guard changes
only FX certification; the native route checkpoint has identical simulation code.

Mean workload counts in the final 240-frame windows:

- hedge3 triangle hulls constructed: 1,536 -> 24 per frame.
- tower2 template brush hulls placed: 270 -> 42 per frame.
- skool2 merged indices rebuilt: 26,206.35 -> 0 per frame. The 8,252 animated
  vertex copies remain, preserving the current geometry.

The native Mac pkg-2/r3c pipeline passes imports, regenerated VELF equality,
VPK integrity and embedded SELF/SFO equality. ELF metadata margin: 1,464,860
bytes, above the 3,348-byte requirement; heap/stack remain 192/8 MiB. Cargo.lock,
the pinned vitaGL archive/header and game archives are unchanged. The production
ELF excludes the host probe. No console timing, memory or fidelity acceptance is
claimed; all three candidates remain default off. Private ANALYSIS.md,
FINAL-VERIFICATION.json, checksums and PROTOCOL.md provide the complete handoff.
