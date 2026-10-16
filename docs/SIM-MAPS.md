# Exact per-map simulation caches

The four startup booleans in `private/vita_renderer.json` independently select:

| Switch | Work retained between requests |
| --- | --- |
| `sim_school_mesh` | School-two indexed prop-mesh topology and identical lazy placements |
| `sim_tower_templates` | Tower-two brush geometry/orientation, unchanged water, per-object rebuilds |
| `sim_npc_queries` | Identical static/settled queries within one immutable NPC update; settled bounds rejection and unchanged mover placements |
| `sim_mechanism_state` | Exact per-object mechanism dependencies and prebound entity indices in Funhouse/Clockwork |

These switches do not alter time steps, actor activation, contact samples, or save formats. They are separate from the visual compromises in `vita_sim.json`. Builds without `vita_renderer` keep the original paths. The off configuration in the package uses the same executable and disables all four switches.

Mesh topology deduplicates only identical local vertex bits. Queried triangles still compute world-space edges, degeneracy, normals, plane order and support distances with the original arithmetic. Conservative placement bounds defer hull construction until a query reaches the mesh. Cached placements and hulls are immutable after publication.

Tower fans retain every original two-degree angular sample and the final contact/rollback checks. Brush templates exclude patches, matching `Collider::model`. Water reuses the existing volume only while its pose is unchanged. Each accepted or rolled-back fan updates its own collider; the initial global rebuild still advances water/stage state.

NPC query reuse covers static and settled collision only. Dynamic contacts are combined on every request. The cache is scoped to an immutable cast update, is bounded, and is discarded before subsequent mover publications. Query keys include all start/end/half-extent float bits. Bounds rejection includes collision skin. No previous-frame trace is reused.

Mechanism keys contain exact dependency bits, not approximate transforms. Unchanged objects skip pose/flag evaluation and unchanged publications retain their collider. Required intermediate publications for contacts, scene transitions and rollbacks remain ordered as before. In ordinary play, the second global mechanism pass has no changed objects. Keys and bindings are transient and reconstructed on load; saved layouts remain unchanged.

`sim_maps_probe` is a host-only diagnostic feature. With `LOOKING_GLASS_SIM_MAPS_PROBE=1`, the live viewer uses a fixed `LOOKING_GLASS_SIM_MAPS_DT` and reports executed work, elapsed CPU and SHA-256 of simulation snapshots plus published transforms each frame. The probe is excluded from Vita packages. Timing results rank host workloads only and do not establish console performance or fidelity.

Private implementation, parity, route/save, timing and package evidence is in `~/vita-j0/atoz/sim-maps/`. Its `RESUME.md` identifies the source checkpoints, known inherited check failures and exact handoff state. `PROTOCOL.md` specifies same-SELF off/on chapter visits for the console owner; no console access belongs to this implementation task.

For a reproducible private host comparison, build with `cargo build --locked --features vita_renderer,sim_maps_probe` and use `tools/sim-maps/check-live.py`. It requires explicit `--tree`, `--evidence`, `--binary`, `--data`, `--config` (allmaps renderer settings) and `--sim` paths. Select `--fix sim_school_mesh` (or another switch, or `all`), `--label <unique-run>` and `--maps <names...>`. Each arm gets its own working directory and save folder; the data archives are read only. The driver checks the shared Mac timing lock before each replay, archives source/binary identities, compares every frame hash and reports the last 240 frames' counters. It does not report those untimed runs as timing evidence.
