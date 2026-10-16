# Single actor preparation on the pkg-2 lineage

`actor_single_prepare` defaults to false. Enabling it prepares actor packets once and publishes bounded owned numeric worker output. It requires the retained Vita renderer, skin overlap, actor fast path, actor layer cache and preparation cache, with actor layer reuse disabled. Startup logs the effective activation state.

This branch starts at `86792fe`. It carries the actor implementation and correctness fixes through `6cfc5f8`: stable actor/surface identity, exact used-bone revisions, held-view protection, mutable-consumer fallbacks, bounded banks with idle expiry, and reset on map construction and same-map restoration. Disabled extra consumer guards do not write TLS state.

The exclusive preparation profiler, puff fusion, additional light-CSV fields, all-counter light dispatch and host submission observers are omitted. The inherited light profiler, deferred tail, puff implementation and Cargo.lock are unchanged. Optional actor counters require the `actor_diagnostics` Cargo feature and full profiling or host metrics; ordinary builds compile out their call sites and arguments. Functional switch checks remain, so zero total disabled-path overhead is not claimed.

Mac validation on 2026-10-16 used an isolated target under `/tmp` with four Cargo build jobs and locked dependencies:

- 856 tests passed; zero failures; two inherited ignores.
- Default-feature and actor-diagnostics checks passed; GameThread Send/Sync compile-fail checks passed.
- All 15 native renderer PNGs matched pkg-2 and both switch arms, including HUD and visible steam.
- All 120 actor matrix PNGs matched between switch arms. Actual-data workers passed in both arms, including the direct-publication numeric checks.
- Three-way application onto `vita-reference` at `0e245cc` was conflict-free. `vita-allmaps` at `342d8b3` conflicted in two renderer files; the reference lineage is the preferred integration target.

These are host functional checks. No console execution, package, full-game acceptance or console performance gain is established by this branch. Timing and the source audit are recorded separately in `~/vita-j0/village-retained-world/jalon-23-port/ANALYSIS.md`.

## Disabled worker and identity paths

The disabled worker callback is the pkg-2 `Task::run`, with its original `Task`,
`Inputs`, `Draw` and `Surface` declarations and unconditional bind initialization.
`Task::run_single` is a separate callback. Its destinations and actor IDs live in
parallel tables allocated and populated only for single preparation; no output
selector or normalization test is present in the disabled vertex/surface loops.

Frame startup reads the switch once and carries its mode in the existing overlap
phase dispatch. `gather_only` is one phase equality, with no publication TLS read.
Normal play selects an ambient traversal specialization once at gather; its
legacy specialization has no identity scope or refresh-consumer test. Character,
model and magma identities are lazy arguments evaluated only by direct capture.
The inherited phase checks and consumer preservation policy still apply.

Mac builds, ARM disassembly comparison and image evidence belong to
`~/vita-j0/village-retained-world/jalon-23-port/package-r2/`. Matching worker code
and host parity do not establish equal whole-frame wall time on the console;
that remains the five-arm protocol's separate acceptance gate.
