# Cache qualification r8 preparation

The engine default remains cache-off. This directory contains the reproducible
Mac preparation tools and source console helpers; it never runs a console session.

Use the new clone `/Users/anezium/vita-work/lg-cache-qual-r8`, branch
`vita-cache-qual-r8-20261024`, and output directory
`/Users/anezium/vita-j0/resume-2026-10-24/alice-cache-qual-r8`.

Run `prepare.py OUT`, `run_checks.py OUT`, and `build_native.py OUT` with Python.
Commit engine changes before native building. `finalize.py OUT` seals the brief,
identities and handoff ZIP. The build recipe is exactly r6's; dependencies,
lockfile, process parameters and renderer/logging implementation are unchanged.

The required offline rehearsal is `test_session_e2e.py OUT --output /tmp/r8.json`;
repeat with `--zip --output /tmp/r8-zip.json` after sealing. It blocks real
networking and subprocesses before importing helpers, substitutes synthetic
transport/driver responses, and executes the preservation, input/recording,
collection, audit/report and complete restoration mains. The retained real human
SELF/save bytes are read from local historical evidence and never shipped.

After initial rehearsals pass, run `finalize.py OUT --clean` to delete per-task
native and temporary conversion caches and record df, then replay both variants
on those final bytes. `write_report.py` refreshes the artifact manifest without
rebuilding the tested ZIP. Retain native originals, proofs, logs and the shared host target. The source package and ZIP must contain
no `session-s8-cache-qual` directory, even as a symlink.

The console owner follows the sealed brief after an explicit ownership handoff.
Changes after sealing require separate amendment files and both offline replays.

For the adopted review fix, read AMENDMENTS-REVIEW.md and
AMENDMENTS-M4B.md with CONSOLE-REVIEW-PROCEDURE.md. Run host checks, proofs, and both main-path
rehearsals; the rule-change path does not rebuild the engine or SELF. The fixture
models native save-reload strings, modal versus overlay resume, divergent movement
endpoints, and closed-title restoration. Negative receipts name every rejection.
After committing and plain-pushing tools, seal with finalize, replay both paths,
run finish_validation, and refresh only ARTIFACT-SHA256 with
`finalize.py OUT --manifest-only`. AMENDMENTS-FIX.md is an external receipt of
those final hashes and is excluded from ZIP/manifest inputs to avoid a hash cycle.
PROVE-CLOSED-FIX.md is likewise an external receipt, excluded from the manifest.
M4B replaces only M4's closure mechanism with two process-kill replies and an
inspected HOME/LiveArea image. Prior blocked evidence stays byte-identical under
blocked-attempts; never reuse its helpers or modify it during preparation/cleanup.
For a helper-only repair, copy changed helpers/scripts/documents directly from
this directory; do not regenerate configs or rebuild the engine/SELF.
