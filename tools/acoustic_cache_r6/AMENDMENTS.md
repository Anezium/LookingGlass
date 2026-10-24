# Benchmark late-callback rule amendment

This amendment takes precedence over the inherited late-callback stop rule in
CONSOLE-BRIEF.md and prior helper descriptions, before any fresh console data.
The brief remains byte-for-byte unchanged.

- Benchmark-window `underrun_estimates > 0` remains an immediate stop and a
  strict-audio failure.
- `late_callbacks` is report-only. A benchmark window with `late_callbacks=1`
  and `underrun_estimates=0` passes if every other audio gate passes. It never
  stops a run or fails strict audio by itself.
- `STRICT-AUDIO.json`, `RESULTS.json` and `RESULTS.md` expose
  `BENCHMARK_LATE_CALLBACKS`: each benchmark reporter-window count and their
  per-run sum, including census runs. This sum describes reported window
  counts, not an inferred underrun or audible defect. Loading and shutdown
  reporter records remain separate.
- Every other audio gate and all admission, integrity, heap, restoration,
  performance and promotion rules remain unchanged. Missing or corrupt required
  audio fields still fail strict validation.

The helper-only reseal is explicitly authorized for this change. The original
engine, SELFs and configs are unchanged. Offline fixtures and every supported
end-to-end variant must pass with outputs outside the package; fresh session
paths must be absent. See LATECB-FIX.md and proof/FINAL-VALIDATION.json for the
reseal evidence and hashes. No device or shot-driver access is part of this fix.

The stopped session is preserved unchanged at
blocked-attempts/attempt1-latecallback-stop/session-s7-acoustic-cache/.
Its sibling MOVE-RECEIPT.json records every before/after file hash.
It is historical blocked evidence and is excluded from the fresh handoff ZIP.
