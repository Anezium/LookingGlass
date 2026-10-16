Simulation maps review fixes: runtime/package source 099005681e899252c6fb7ff003ed3c32b2715bd1; branch vita-sim-maps, Mac only.
7f41d37: all four sim_* renderer defaults false; explicit switches retained; console acceptance remains pending.
784cbf7: world-order/outward-rounded conservative lazy bounds and 200,000 randomized placement regression; synthetic and actual fan bit-equality tests (49 poses/model).
b2839b3: NPC query scratch clears entries and retains capacity; scope, nesting, invalidation and 4096-entry cap preserved; unwind regression included.
763c51e + 50aa2c2: cwd-independent unit config, explicit off/on suite arms, nested override regression and corrected test fixture literal; 0990056 documents fixes/rollback.
Full suite, off AND on from opposite-config working directories: 851 passed, 0 failed, 3 ignored per arm; two ignores inherited, real-archive fan test executed separately and passed both arms.
Actual Tower2 fans: 245 geometry-bit comparisons per arm against Collider::model, including bounds and ordered planes; synthetic fixture also exact. Default cargo check passes.
Ten-map replay: 3900 frame hashes per arm all identical; every map/arm workload counter and window hash is unchanged from final-neutral r1.
36 registered and 17 native mechanism Store fixtures per arm pass separate writer/restart checks; off/on save payloads and future bytes match. Legacy saves pass both arms.
Broad save writers each pass 188 fixtures then hit the exact inherited Garden2 Cat-trigger failure. School-return and Hatter native-route failures remain inherited; no claim they pass.
Optional host timing not rerun (recent five-minute load >3); r1 rankings remain historical, not fresh r2 timing or console acceptance.
Native pkg-2 pins, nightly-2026-10-11, -neon, 192MiB heap, two 8MiB stacks, imports/VELF/ZIP/SELF-SFO/probe checks pass; ELF margin 1431284 bytes >=3348.
SELF SHA-256: 7a422b687d10ded4b867714a4e887830c9b60f9c5ce1dd8bd9b86289c0a537a3
VPK SHA-256: 684416ba90f6b3585a3a525e2cf1040f6cd899b569781beb30092d1bc11b6564
A-simmaps-r2/ contains the new package and explicit off/on configs; A-simmaps/ is file-for-file unchanged. Archives and Cargo.lock unchanged.
PROTOCOL.md selects r2 and requires SELF+matching renderer JSON rollback together: older pkg-2/reference r2 deny_unknown_fields rejects sim_* keys and falls back to all defaults.
Evidence: R2-VALIDATION.json, R2-CHECKS.json, GATE-r2.json, R2-COUNTER-COMPARISON.json and SHA256SUMS. Console acceptance pending; no console/bls12 access.

Private evidence: `/Users/anezium/vita-j0/atoz/sim-maps/`. Original validation/ranking remains in `HOST-VALIDATION.json`, `HOST-RANKING.json` and `FINAL-REPORT-r1.md`; current review-fix evidence is `R2-VALIDATION.json`.
