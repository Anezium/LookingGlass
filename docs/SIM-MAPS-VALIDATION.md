Simulation caches: runtime 302d071ecf9a7c40bc5bdc2379914d47ccd82a68, vita-sim-maps; Mac-only delivery, no console access.
275495f skool2: triangle hull construction 4476 -> 0/frame; indexed immutable mesh topology, exact queried world hulls.
cac72f9 tower2: Collider::model 45 -> 0 and brush topology rebuilds 321 -> 0/frame; 270 exact template placements remain, fan samples unchanged.
77f7d43 hedge3/funhouse: settled tests 19107/45644.725 -> 2/3; executed static sweeps 579/466.7625 -> 229/216.7625 per frame.
c58b1f2 funhouse/hatter1: mechanism objects 282/254 -> 17/21 and rebuild passes 2 -> 1/frame; required scene/rollback publications remain exact.
Per-fix gates pass; combined 10-map gate compares 3900 frame hashes per arm, all identical. Full suite:846 pass,0 fail,2 inherited ignores; default check passes.
36 registered save fixtures pass separate-process write/restart and continued-state checks; relevant functional, rider and state/restore checks pass.
School-two, tower2 watched/skip, hedge3 and funhouse full routes pass; Hatter traversal passes. Inherited school-return/Hatter native-route failures reproduce exactly on archived allmaps.
Broad saves pass188 fixtures then fail garden2-cat-talk; archived allmaps reproduces the exact Cat-trigger failure. These three inherited failures are not passes (HOST-VALIDATION.json).
Host ranking only, workload counting disabled, three alternating accepted pairs/map, all sampled1/5/15 loads<=3: skool2 4.790->2.519 ms (-47.4%); tower2 3.289->2.282 ms (-30.6%); funhouse 2.924->2.114 ms (-27.7%); hedge3 4.566->3.974 ms (-13.0%); hatter1 2.823->2.511 ms (-11.1%).
Native pkg-2 pipeline:nightly-2026-10-11,~/vitasdk,pinned vitaGL6842d410…/5da33f7b…,-neon,4 cargo jobs,192MiB heap,two8MiB stacks; ELF margin 1424380 bytes >=3348.
ELF imports/regenerated VELF/ZIP CRC/SELF-SFO equality and probe exclusion pass; game archive and Cargo.lock hashes unchanged.
VPK SHA-256: 09165d697045e43e0087af1d9717b57a9f3681423663d85f19f4c71b537a2a8d
SELF SHA-256: f74343b00c372c458bb93e5ce5c0dad19da77faa4075a97ce6c460106db8c00c
Package:A-simmaps/looking-glass.vpk; configs:PL-def-allmaps(all on),PL-def-allmaps-simoff(all off),vita-sim-all; same SELF in both arms.
Console owner protocol:PROTOCOL.md (22 off/on visit pairs,including Garden3 movement); evidence integrity:SHA256SUMS. Console performance/memory/fidelity acceptance remains pending.

Private evidence: `/Users/anezium/vita-j0/atoz/sim-maps/`; read `RESUME.md`, `HOST-VALIDATION.json`, `HOST-RANKING.json`, package `VERIFICATION.json` and `PROTOCOL.md`.
