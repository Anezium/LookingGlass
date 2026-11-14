# Exact boolean collision traversal

`sim_sweep_any` is an optional, default-off companion to `sim_sweep_blocked`.
It enables boolean solid sweeps and makes the static SAH answer during traversal,
without gathering, sorting or imposing the 256-candidate scratch limit. Other
geometry still uses the existing boolean sweep completion. Full traces, including
contact fractions and normals, are unchanged.

Each static hull still passes the same spatial-node, hull-bound and legacy
ancestor admission predicates and the same `clip_hull` calculation. For finite,
nonnegative inputs, a boolean sweep is the OR of admitted hulls that set
`start_solid` or reduce the fraction below one. The first true answer is independent
of hull order. Invalid inputs retain the legacy fallback. The existing bounded
certificate cache and the map-load tree are reused; no frame allocation or new
map-load storage is needed. The SAH tree build caps depth at 48.

The key affects the flying-route clearance and ground horizontal yes/no sweeps.
A* expansion order, clearance answers and its 1536-expansion cap remain unchanged.
With both boolean keys off, the original full sweep is used. With only
`sim_sweep_blocked` on, the previous ordered boolean traversal is retained for A/B.

Host checks extend the collision differential test (static, settled, moving and
weapon hulls, SAH on/off), the SAH extreme-input/admission/fallback tests, and a
300-hull dense-query test that checks short circuit and zero allocations. Host
replay of the actual Vita tape is required separately.

For host-only snapshot timing, build with `sim_maps_probe` and set
`LOOKING_GLASS_FLIGHT_BENCH=1` during the cross replay. Each actual route search
is repeated 24 times against its immutable World; every repeated path and expansion
count must equal the original. The repetitions print `[lg/flight-bench]` and must
not be used to time the containing replay frame. This code is excluded on Vita.

The existing `spike_log_ms` diagnostic remains bounded and default off. On Vita,
read frame 2030's `flight_route`, `npc=ladybug` and `sim_split=npcs`; read
`gxm_end` and `gxm_end_in=scene` separately on frames 2033–2038 and 2083–2119.
EndScene is a main-thread wait, not a GPU duration query. This change does not
remove the near-camera corpse fill or prove an improvement on the console.
