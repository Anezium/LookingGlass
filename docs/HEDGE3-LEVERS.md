# Exact hedge3 light and dynamic collision levers

Both startup renderer settings default to `false` and require booleans:

- `lights_static_cache`: cache up to 64 static occlusion certificates per World,
  keyed by the exact camera/light endpoint float bits. A static hit with fraction
  below 0.995 proves rejection regardless of moving, settled or weapon geometry.
  Static-clear entries still run the ordinary complete sweep every frame. Color,
  radius, model mask, flare, stable score order, 32-candidate cap and platform
  selection limit remain evaluated every frame. Moving either endpoint requires
  a new proof. Certificates cannot survive World replacement; no PVS/leaf
  approximation is used. The initial proof can add work when positions change.
- `sim_dynamic_buckets`: rebuild spatial groups of four collider AABBs whenever
  dynamic geometry is published. Actor worlds share the immutable bucket data
  when copying dynamic geometry. Queries prune only using conservative unions
  with the original SKIN-expanded AABB predicate, then sort candidate indices
  back into publication order before the unchanged narrow phase. Equal-fraction
  contacts retain their original normal; flags and fractions use unchanged math.

`dynamic_colliders_tested` counts individual candidate AABB visits, excluding
bucket bounds. `dynamic_colliders_rejected` counts rejected individual candidates.
`lights_sweeps` excludes reused negative certificates, and includes a newly built
static certificate or a normal visibility query. A static-clear first miss can
perform both a proof and a regular sweep; host `lights_certificate_builds` exposes
that extra work. No render image or simulation compromise is intended.

The host-only `sim_maps_probe` adds selected-light float bits, light-selection
elapsed time, candidate counters and optional original-loop comparison via
`LOOKING_GLASS_HEDGE3_VERIFY=1`. Production packages exclude that harness.
Unit tests run either startup arm with `LOOKING_GLASS_TEST_HEDGE3=off|on`.

Console acceptance is pending. Use one frozen SELF for both arms, restarting to
change startup configuration, and preserve the same map entry, render/audio
settings and neutral camera. The implementation task's `OUT/CONSOLE-BRIEF.md`
contains the owner handoff protocol. No device operations are part of host proof.
