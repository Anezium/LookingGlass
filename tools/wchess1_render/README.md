# wchess1 render package sources

These source snapshots belong to the active external package at
/Users/anezium/vita-j0/plan-2026-11-12/wchess1-render. Copy them there before use;
sibling paths refer to the package. Do not prepare artifacts in this source tree.
No tapes, game data, binaries, console state or kit copies belong here.

run_host.py locks each timing attribution run separately. Builds and exact
oracles never hold mac-timing.lock (root's release-week rule). cpu_family.py locks
each instrumented timing run separately. run_checks.py uses fresh capture fixtures
and records the optimized test-only LTO override; production recipes retain LTO.
If shared Cargo blocks, use WCHESS_HOST_TARGET_DIR under the worktree, keep 41 GiB
free and remove that private cache at hand-off after its processes stop.

prepare.py writes explicit configs and an offline manifest. build_vita.py uses
the RC14 recipe and a package-local writer lock, not the timing lock.
runtime_equivalence.py validates only exact known cfg(test)-only revision edits.
seal.py refuses absent proof/build/test evidence. See STATUS.md and the external
REPORT.md/README.md for hashes, limitations and root's manual console procedure.
No script grants console ownership. No defaults are promoted.
