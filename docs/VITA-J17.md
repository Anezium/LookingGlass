# Milestone 17: light selection, FX state, actor layers

The default is now `skin_overlap=true`, `skin_workers=2`. The three independent
switches `lights_early_out`, `fx_submit_state_cache` and `actor_layer_reuse`
default to true. Turning all three off retains the J15 overlap2 behavior.
No dependency, SDK, driver, Cargo lockfile or build flag changes are required.

## Exact light visibility

Selection retains validation, stable score sorting and the 32-candidate boundary.
Vita stops visibility sweeps after two successes; the host preserves its eight
light limit. Hidden candidates still receive a query until the limit is reached.
Selected values and flare flags do not change. Aggregate candidate, sweep and
selected counters use the existing profiling route.

## Owned FX submission state

The retained actor/FX renderer caches its private pipeline/depth key, all vertex,
index and three image handles, and the complete 147-float uniform payload by
exact bits. Pipeline changes reapply bindings and uniforms. Draw order, ranges,
counts and triangle output remain live. The cache invalidates for each finish,
the opaque/alpha boundary, every world callback, and pass completion, which also
covers target/view and macroquad interop changes. Actual pipeline/depth and
binding application/skip counters replace the old draw-count alias. The existing
uniform-only toggle remains available when the complete state cache is off.

## Actor publications and workers

The existing persistent skin batch prepares both numeric vertex-alpha variants
and compares exact position/normal/color bits against the previous output.
Surface identities retain an owning immutable CPU handle and the same arena
range; replacement or traversal/range changes assign a new monotonic identity.
Every completed output advances its revision even when submission falls back or
does not consume it. Panic recovery invalidates identities after both workers
drain. No worker receives a Mesh, Pass, texture, material, renderer or GL handle.

After validated skin consumption, the game thread synchronously passes numeric
output into the simple-layer cache. It compares live UV bits (including caller
edits after gather), publishes at most once per alpha variant, and trusts only
the certified immutable topology. Normal/camera-dependent stages, timed FX,
entity-opacity layers, overlays and other untracked mutations keep the existing
exact observation/preparation path. The caller's complete live meshes remain
updated for overlay/attachment consumers. Immutable queued publications retain
both arrays until their packets are released.

Empty pass vectors are recycled on the game thread, bounded to 256 containers
of at most 64 passes each. The extra worker output arena is bounded to 32,768
vertices with two 40-byte variants: 2,621,440 bytes, plus bounded revision and
surface metadata. It is not allocated when `actor_layer_reuse` is off.
Console memory headroom and performance remain unmeasured.

## Local verification and package

Run `cargo test --locked --features vita_renderer`, native
`--skin-workers-check`, `--skin-overlap-render-check` and `--vita-alpha-check`.
Unit coverage includes moving occluders, stable ties and candidate boundaries,
complete state keys and interruptions, exact producer outputs, live UV edits,
queued-packet lifetime, changed topology/identity, and unconsumed worker output.
Existing allocation, ordered fallback, unwind and worker-panic checks remain.

The handoff in `~/vita-j0/village-retained-world/jalon-17/` contains A-stock,
build/import/ELF evidence, host logs and capture hashes, and five renderer
presets: `PL-def`, `PL-def-off`, and one preset for each switch disabled.
The inherited `vita-sim-all.json` stays byte-identical. Build with
`cargo vita build vpk --release --locked --features vita_renderer`, stock
`-C target-feature=-neon` and the existing mio cfg flags. ELF RX/RW/init-array
margin must pass the existing verifier (minimum 3,348 bytes).

This milestone requires no console contact. Local captures establish fixed-input
host equivalence; they do not establish console timing or moving-play acceptance.
