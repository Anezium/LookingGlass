# Alice phase census r5

Engine base: `81f3d1eb1fe4303ff955afee491ef10a7ceecb2a` on the r4 branch; r4 engine is `cd33c35`. Branch: `vita-phase-census-r5-20261024`. Default-off opt-in: `phase_census`. No game assets belong in this repository.

All tools here prepare or validate local artifacts. Console helpers materialized by prepare.py are for a separately admitted console owner. Read CONSOLE-BRIEF.md; never run a campaign during implementation.

Use `/opt/homebrew/bin/bash`, Python `/Users/anezium/vita-j0/venv/bin/python`, and the shared host target `/Users/anezium/vita-j0/cargo-target-host`. Do not run cargo update. From the r5 clone, with OUT set to `/Users/anezium/vita-j0/resume-2026-10-24/alice-phase-census-r5`:

1. `PYTHONDONTWRITEBYTECODE=1 "$PY" tools/phase_census_r5/prepare.py "$OUT"` materializes configs, manifests, the protocol, inherited operating contracts and r5 helpers.
2. Build the host with `PATH=/Users/anezium/.cargo/bin:/opt/homebrew/bin:$PATH RUSTUP_TOOLCHAIN=stable CARGO_TARGET_DIR=/Users/anezium/vita-j0/cargo-target-host RUSTFLAGS='--cfg observer_free_B' cargo build --release --offline --locked --features native_timing,frame_total_only,sim_maps_probe`.
3. `host.py OUT` runs a deterministic render ABBA on each map and records all-frame state/light/draw-order and PNG equality plus host CPU observer deltas. It uses the existing private local game-data corpus and saves only outside the repository. `run_tests.py OUT` runs the explicit map sample-clock proof and existing bounded-census/collision/audio/timer/logger/direct-resolver checks. Audio fixture directories are fresh per invocation.
4. Commit engine changes before native build. `build_native.py OUT` uses the exact r4 native recipe, four jobs, pinned nightly, offline/locked, no incremental compilation, and a task-local native target. `seal.py OUT` validates ELF margins/process parameters, observer symbols, conversion equality and asset-free VPK members, then seals the common-SELF two-arm matrix.
5. `test_tools.py OUT` blocks network/process transport while testing console helper imports, manifests, ownership-preservation/restoration contracts, collection and diagnostic reading. `OUT/scripts/test-log-integrity.py` retains all r4-1 fixtures, including the original real r3 log pair. The r5 observer threshold also has exact-boundary and mixed-sign fixtures.
6. Console-only later: a fresh `session-s6-phase-census` copies helper source files; `OUT/scripts/bind_helpers.py --root ROOT` verifies the fresh set and hashes without transport. After fully admitted measurements, compare each block and run make-report.py as described in the brief. Raw census JSON remains the authority.

The build target, temporary conversion duplicates and copied host binary are removed after sealing; logs, hashes and proof data remain. The shared host target is deliberately retained. See SOL-REPORT.md for exact hashes, evidence scope and open risks.
