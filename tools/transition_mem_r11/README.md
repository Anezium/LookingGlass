# r11 transition memory work

r11d resolves the round-3 frame-one baseline and persistent-recovery blockers.
The current branch is `vita-transition-mem-r11d-20261025`; output is the separate
`alice-transition-mem-r11d` directory. R11c is read-only and its parked-console
run stays unchanged. Read SOL-REPORT-R11D.md and AMENDMENT-R11D.md in the new output.
The recovery loop is a boxed async helper allocated only on failure. Matched
frame-one heap histograms on both maps/arms equal r11b exactly. The fork fixes
remain: defensive deletion of possibly recycled GL names, and correct but
currently unused zero-sample attachment ownership. LG uses sample_count=1.

`setup_r11d.py OUTPUT` fingerprints r11c, copies historical documents with a
visible correction, and fixes E2 at 30 cycles per operation/arm before execution.
`baseline_proof.py OUTPUT --ref REV --label LABEL` uses an isolated source archive
and an owned diagnostic target. Run for labels r11b/r11c/r11d, then run
`path_length_control.py` and `compare_baselines.py`. `validate_r11b.py OUTPUT`
now requires an uninstrumented final executable, checks the final baseline,
tests persistent screen/Enter/Escape/save integrity and reruns all earlier suites.
Never extend E2 after failure. `--resume` is valid only for the identical binary.
`future_layout.py OUTPUT` additionally emits compiler type layouts from isolated
archives into the owned diagnostic target; `--parse-only` summarizes saved logs.

Finalize with `finalize_r11c.py`, `clean_host_build.py`, `seal.py`, the loose/ZIP
offline rehearsals without `python -I`, `package_lessons_r11d.py`,
`cleanup_r11d.py`, `verify.py`, `write_report_r11d.py` and `verify_seal.py`.
Session directories must remain absent. No console or shot-driver access is
authorized during preparation. Older instructions below are historical.

r11c supersedes r11b. The output is `alice-transition-mem-r11c` beside the
original package. Read SOL-REPORT-R11C.md and AMENDMENT-R11C.md there. Source
changes to Macroquad/Miniquad now live in pinned Anezium forks; vendor/ is gone.
`dependency_proof.py` verifies the published crate and every consumed fork source
file. `texture_proof.py` requires r11b failures and isolates each layer's fix.
`validate_r11b.py OUTPUT` now also draws portals before real viewer transitions,
forces destination + outgoing-rebuild failures, and gates 30 chapter round trips
and 20 Retry/Load cycles per lever arm on a fixed full post-warm-up trend.
`--resume` retains completed checks against the identical immutable executable.
`finalize_r11c.py` replaces the older vendor-specific receipt writer. Seal and
offline rehearsals include C1 headroom labels and the unchanged sealed-map rule.
Console access remains forbidden during preparation.

r11b supersedes the original preparation below. Its engine source is a937982,
on the same branch. Read SOL-REPORT-R11B.md and AMENDMENT-R11B.md in the output
directory for the final results and fixed console policy. The console remains
unrun. CONSOLE-BRIEF.md is preserved byte-for-byte.

`validate_r11b.py OUTPUT` builds one immutable host executable, checks New Game
input precedence first, runs the original suites and expanded recovery matrix,
then the full opening films. It measures Retry ×5, native Load ×5 and ten chapter
round-trips. Allocation census mode disables retained host timing histories;
per-frame counters and ordinary lever parity histories remain active.
The local same-version Macroquad and Miniquad patches reclaim backend material
resources and texture CPU metadata. Their origins and exact changes are recorded
in vendor/*/PATCH.md and proof/DEPENDENCY-PATCHES.json.

`finalize_receipts.py OUTPUT`, `seal.py OUTPUT`, the loose/ZIP rehearsals,
`cleanup.py OUTPUT`, `verify.py OUTPUT` and `write_report_r11b.py OUTPUT` bind
the final source and packages. SEAL-R11B.json covers the final report, inventory
and console ZIP without a hash cycle. Session directories stay absent.

Base: `vita-stack-r10b-20261025`, e19f73c (engine d882a69). Implementation: c011075; final engine with failed-destination GPU recovery: ef8608c (complete New Game bracket: 6faa690). Branch: `vita-transition-mem-r11-20261025`. No assets belong in this repository.

The output lives at `/Users/anezium/vita-j0/resume-2026-10-24/alice-transition-mem-r11/`. Read SOL-REPORT.md there for the ownership trace, allocation bounds, validation and limitations. CONSOLE-BRIEF.md is for a future sole owner; no console or shot driver was used during implementation.

`prepare.py OUTPUT` assembles local helpers, configs and manifests. `build_native.py OUTPUT` uses the locked/offline r10c recipe and verifies ELF/process parameters, regenerated VELF/SELF equality and minimal VPK contents. `seal.py OUTPUT` checks the local command grammar and emits the console ZIP. It never imports transport code. The output's real session directory must stay absent during preparation.

Host target: `/Users/anezium/vita-j0/cargo-target-host`. Build with `cargo build --release --offline --locked --features native_timing,frame_total_only,sim_maps_probe`, stable, `RUSTFLAGS='--cfg observer_free_B'`. Atomically copy the executable to a unique owned path before running it on macOS; never overwrite an executable in use.

`host_proof.py OUTPUT --binary BINARY` executes the actual viewer paths, comparing overlap-reference and release-first operations with all three levers off/on. The host-only reference flag is absent from console builds. It measures all Rust allocation requests, conservative realloc overlap and 32-bit newlib chunk upper bounds. It also checks native New Game preservation and clean Quit Auto payloads. `film_proof.py OUTPUT/film --binary BINARY` runs all 3,731 opening frames in four muted Mac callback processes. `failure_proof.py OUTPUT BINARY` injects a partial destination failure and checks exact recovery. `host_visual.py OUTPUT/visual --binary BINARY` repeats the scenarios with GPU readbacks, separately from the allocation census. `visual_audit.py OUTPUT` compares packets, state, uploaded geometry and reports exact raster differences.

`run_stack_host.py OUTPUT` runs the existing 180-frame simulation suites. `tools/stack_r10/host_stack_oracle.py OUTPUT --binary BINARY` runs the existing packet/pixel oracle. `run_tests.py OUTPUT` requires a copied release test binary at OUTPUT/proof/test-bin and supplies the test suites' required fresh output and corpus environment. It refuses zero-test filters.

Run `scripts/test_session_e2e.py PACKAGE --output /tmp/r11-loose.json`, then `--zip --output /tmp/r11-zip.json`, with the local lab venv and PYTHONDONTWRITEBYTECODE=1, without `-I`. The test blocks real sockets and process launches before helper imports and uses only retained local human bytes. It runs the main preservation/staging/navigation acknowledgement/audit/restoration helpers, M3 and M4B negative fixtures, serial fake FTP, exact original SELF/Auto/settings/companion restoration, and initial absences. Every synthetic tree carries SYNTHETIC-OFFLINE-ONLY. A synthetic PASS is not console evidence.

AMENDMENT-R11.md records the absent r8 recorder in this integration branch and the fixed <170 MiB native sampled transition budget. Do not silently add its unsupported config key or claim its workload was tested. Any later protocol change must be a separate amendment file.
