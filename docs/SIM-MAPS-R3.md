# Remaining map CPU work, round three

Base: reference r3g `5fa69f2`, including the seven console-accepted round-one/two
switches. The new `sim_hatter_door_candidates` startup switch defaults to false.
It defers Clockwork door world/collider traces until the existing strict range
and facing tests pass. Search order, arithmetic, collision geometry, visible
prompts and use decisions are preserved. There is no saved-state change.

The six-map host baseline uses the round-two method: fixed raw elapsed, native
cast, neutral input, 960x544, .75 render scale, no audio, full profiling, last
240 of 300 frames (funhouse: last 240 of 1200). Accepted samples hold the shared
Mac timing lock with all sampled 1/5/15 load averages at most 3. CPU snapshots
include diagnostic serialization preparation; the residual is not pure simulation.

The largest simulation spans are late funhouse NPC collision and centipede1's
combined combat/Alice work. These timings do not prove redundant queries.
Hatter1 sampling identifies eager door traces whose results are discarded by
range/facing rejection. That is the selected exact optimization.

Render preparation includes scene/skin gather, visibility, world parameters,
FX merge, particle quads, pickups and Attached sampling. An attribution-only
probe checked whether unchanged certified source ranges explain FX copy costs.
It found zero reusable vertices/frame in hatter1, skool2, funhouse and centipede1,
0.425 in potears1 and 1.4 in garden1. No render reuse optimization is justified
by that population. Existing animated copies and all render behavior are retained.
The new attribution counter is host-only and absent from the Vita package.

Private evidence: `~/vita-j0/atoz/sim-maps-3/ANALYSIS.md`, PROFILE.json,
PROOF.json, gate results and the console owner's PROTOCOL.md. Each arm uses the
same binary; host state hashes and sampled images do not certify console timing,
audio, transient memory or complete visual fidelity. Roll back SELF and renderer
config together because older binaries reject new keys. Console validation and
switch acceptance remain pending; this implementation never contacts the console.
