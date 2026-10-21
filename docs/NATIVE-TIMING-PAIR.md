# Frozen native timing pair assessment (host only)

`tools/native_timing_pair_check.py` assesses one standing-pose pair of original
exports. It performs no network, device, capture, native build, or deployment work.
The primary experiment uses identical SELF bytes, A with
`sim_hatter_door_candidates=false`, B with `true`. No r3j/r3k fixes are included.
Gate 0 implements the host checker. Gate 1 is limited to two A/A feasibility runs
and four Pandemonium ABBA runs, D=150 seconds. Hedge3 and the 13-map campaign need
a separate authorization after Gate 1 variance is reviewed.

## Invocation and frozen inputs

Copy `NATIVE-TIMING-PAIR-SPEC.example.json`, fill every expected identity, choose
the pair ID/order and preserve its exact bytes and SHA-256 **before either run**.
The example is a template, not a completed preregistration: null expected hashes
remain UNKNOWN. Do not amend a spec or select alternate windows after a result.
Paths in the spec and receipts must be absolute. The CLI hashes original spec
bytes; the Python API defaults to canonical sorted compact JSON unless its caller
supplies `spec_sha256` for the original bytes.

```sh
python3 tools/native_timing_pair_check.py \
  --spec /absolute/frozen-pair.json --evidence /absolute/pair-evidence.json \
  > /absolute/pair-result.json
```

Each arm's `directory` can name the original native export or a preregistered
parent directory that will contain **exactly one** original export. The latter
allows freezing paths before native random run IDs exist. There is no recursive
search, concatenation or favorable choice among children. Place receipts outside
those arm parents. The original capture directory name/manifest identity checks
still apply. Distinct run IDs are mandatory, even for A/A. Generation/operation
IDs are validated inside each process and need not match numerically across runs.

`experiment_kind=AA` requires identical configuration bytes and parsed values,
including the declared switch value. `AB` requires A=false/B=true and exactly
one parsed top-level key difference, the declared switch. Missing keys, boolean
versus integer differences, extra configuration changes or mismatched source,
build/SELF or recorder-config identities cannot pass. Configuration receipts
must cover the complete effective configuration; a convenient subset is not
independent proof of the renderer/audio/clock policies.

The spec fixes route/map, actual native entry (null differs from string), install
reason, input-save hash, route/intro/skip/input policies, arm generations, D and
30,000,000 us duration. Selected save bytes are independently uploaded/read back;
a null native save hash supplies no proof. A Chapters route may explicitly have
no input save, with null expected/save receipt fields; it still needs the actual
entry baseline. All other routes require a known immutable input-save hash.

Duplicate/unknown/missing JSON fields, wrong types, booleans used as integers,
nonfinite numbers and nesting deeper than 64 levels reject. Spec/JSON receipts
are capped at 256 KiB, review requests at 1 KiB, images at 32 MiB and original
artifact files/readbacks at 256 MiB each. Artifact hashes are streamed. Capture
limits and structural validation remain unchanged. `--allow-test-clock` is an
explicit host-fixture opt-in, never native evidence.

The CLI always emits JSON for input/assessment failure and returns 2; it returns
0 only when pair equivalence, per-pair performance, audio and external evidence
gates all pass. Argument-parser usage errors also return 2 (argparse stderr).
Each check includes a stable code, field, value, limit and PASS/FAIL/UNKNOWN.
Structural failures use `CAPTURE_INVALID` with the original checker error text.
Other principal codes are `PAIR_MAP_ENTRY_MISMATCH`,
`PAIR_SAVE_ASSOCIATION_UNKNOWN`, `PAIR_TREATMENT_MISMATCH`,
`PAIR_INSTRUMENT_MISMATCH`, `PAIR_OFFSET_MISMATCH`, `PAIR_POSE_BITS_MISMATCH`,
`PAIR_SIM_AGE_TOLERANCE`, `PAIR_PHYSICS_TICK_TOLERANCE`,
`PAIR_STATE_BRACKET_UNKNOWN`, `PAIR_EXTERNAL_EVIDENCE_UNKNOWN`,
`AUDIO_PRODUCER_P95_GT_8MS`, `RAW_COUNTER_NONZERO_OR_UNKNOWN` and
`PAIR_CAPTURE_REUSED`. Measured failure dominates missing-evidence UNKNOWN.

## Observed state and populations

`validate_capture` retains the strict loader, identities, sequence, loss, guards,
operations and scheduler checks in a `ValidatedCapture`. `check_capture` remains
the legacy single-run wrapper. Audio `validate_structure` retains all journal,
fence, membership, lifecycle, loss and coverage checks; `assess_producer_gate`
reports p95 failure separately. Audio `validate` and legacy `check_capture` still
raise the original `producer intersecting window p95 exceeds 8ms` exception.
Thus a valid reference over 8 ms retains its metrics in a pair result and reports
FAIL; it cannot excuse a candidate failure.

Ready baseline state is bit-exact across arms (all five pose bits, environment
and pickup clock bits, physics ticks). The standing pose must be bit-exact across
arms, throughout settle/window and at both observed boundaries. Ready-to-settled
landing before the settle period is allowed; the settled pose need not equal the
ready pose. Signed zero is distinct; no epsilon, mask or retiming is applied.
Clock/tick observations must be nonfinite-free and monotonic from ready.

For each S/T, an exact frame begin uses that `before` observation. A straddling
frame uses its before and the next frame's before, requiring the current `after`
to equal the next before. `after` is sampled before swap; `end_us` belongs to the
next begin, so continuity must be checked explicitly. No sound endpoint means
UNKNOWN. The scheduler's delayed start/end states are not substituted for S/T.
Output retains absolute decoded clocks, original bits, sample native and relative
ages, sequences, bracket widths and baseline-relative/S-to-T ranges. No exact
physical observation instant or sub-observer-cost timing is asserted.

For environment and pickup separately, the maximum absolute difference between
any cross-arm bracket endpoints must be <=0.050 seconds at S and T, relative to
the equal ready baseline, and for the S-to-T advance. Physics uses the same
worst-case rule with <=6 ticks. Interval subtraction retains both endpoint
uncertainties; identical wide brackets can fail. Limits are checked against the
declared real-valued 0.050 without rounding the limit to f32 or adding epsilon.
The tests cover the predicate at exactly 0.050 and its next double, plus the
adjacent f32 observations either side of the limit (0.050 is not itself exactly
representable as f32).

Frame metrics use **all whole viewer intervals intersecting** `[S,T)`, including
boundary overhangs, without clipping/filtering or extending a short population.
Median is the middle value or mean of the middle two; p95 is sorted
`ceil(.95*N)-1`; max is the largest whole duration. N, N<120, total duration and
count/rate strictly above 100,000 us are retained. A frame exactly 100 ms is not
slow. Producer p95 keeps its existing `sorted[(n-1)*95//100]` rule on each whole
fence, including boundary fences. Fence p95s are never pooled.

`performance_gate` is a **single-pair screening gate**: median B/A <=1.010,
p95 B/A <=1.020, max B <= max A+5,000 us, and both >100 ms count and rate B<=A.
It is not a confidence bound. Repeated-map geometric means <=1.000 and one-sided
95% Student-t upper bounds <=1.010/1.020 require the predeclared collection of
independent pairs and are explicitly unresolved here. A/A establishes no candidate
benefit. No benefit or campaign flag is inferred from a favorable pair.

Both arms report every intersecting fence's p95 against the absolute 8.000 ms
limit. Both raw fields must prove delta=0 on their prescribed containing superset.
Positive surrounding deltas fail; absent routes/brackets remain UNKNOWN. Busy,
wrap/reset, loss, empty fences and attempted flush subtraction preserve their
existing conservative structural outcomes. Reference audio failure remains FAIL.

## Independent external receipts

Hashes and native receipts are **UNATTESTED**. Even complete, self-consistent
external logs cannot prove their author's honesty, actual image inspection,
physical exposure time, a silent unlogged request or audible route activity.
`external_evidence_gate=PASS` means the complete independently retained evidence
joins consistently, not cryptographic/device attestation. Missing evidence is
UNKNOWN; present contradictions fail. Do not invent timestamps or fill missing
logs with assertions. A boolean `approved` is never sufficient.

All receipt references have exactly `{"path":"/absolute/file","sha256":"64 lowercase hex"}`.
Retain originals and hash full files, including full uploads and independent
readbacks. The evidence root has exactly:

```json
{
  "schema": "alice-native-pair-evidence-v1",
  "spec_sha256": "SHA256_OF_EXACT_FROZEN_SPEC_BYTES",
  "freeze": {"path": "/absolute/freeze.json", "sha256": "HASH"},
  "arms": {
    "A": {
      "deployment": {"path": "/absolute/A-deployment.json", "sha256": "HASH"},
      "review": {"path": "/absolute/A-review.json", "sha256": "HASH"},
      "quiet": {"path": "/absolute/A-quiet.json", "sha256": "HASH"}
    },
    "B": {
      "deployment": {"path": "/absolute/B-deployment.json", "sha256": "HASH"},
      "review": {"path": "/absolute/B-review.json", "sha256": "HASH"},
      "quiet": {"path": "/absolute/B-quiet.json", "sha256": "HASH"}
    }
  }
}
```

The following are the exact nested receipt fields (no extras):

- Freeze: `schema=alice-pair-freeze-v1`, `pair_id`, `spec_sha256`,
  `frozen_host_us`, `host_clock_id`. Preserve before either arm starts.
- Deployment: `schema=alice-pair-deployment-v1`, `pair_id`, `run_id` (32 hex),
  `manifest_sha256`, `records_sha256`, `host_start_us`, `host_clock_id`,
  `policy` (exactly `route`, `intro`, `skip`, `input` from the spec), and `files`.
  `files` has exactly `source`, `build`, `game_config`, `recorder_config`, `save`;
  each contains `upload` and `readback` file references. Save alone may be null
  for a Chapters route. Hashes must match expected identities, nonnull native
  metadata and the original export. Source is the preserved source artifact
  whose SHA was supplied to native metadata; build is the actual SELF bytes,
  not a Git revision. The config readback is parsed to prove the exact treatment.
- Review: `schema=alice-pair-review-v1`, `pair_id`, `run_id`, `host_clock_id`,
  `image` file reference, `request` with `upload`/`readback` file references,
  `token`, `request_id`, `image_request_id`, `inspector`, and integer
  `image_return_host_us`, `inspection_host_us`, `action_host_us`,
  `upload_host_us`, `readback_host_us`, `rename_host_us`, `ftp_closed_host_us`.
  These times strictly increase after deployment start. Request IDs differ.
  Token has exactly `ordinal`, `viewer_seq`, `submitted_us`, `generation`,
  `operation`, `auth` (16 octets); it must equal the native reviewed token.
  The request file/readback is the unchanged v2 `{"run_id":[16 octets],
  "token":{...including auth...}}`. Native ACK deadline checks remain mandatory.
- Quiet: `schema=alice-pair-quiet-v1`, `run_id`, `host_clock_id`, `before`,
  `after`, `log_begin_host_us`, `log_end_host_us`, `ports=[21,5000,5001]`,
  `requests`. Each request has exactly `port`, `begin_host_us`, `end_host_us`.
  Each anchor has `host_pre_us`, `host_post_us`, `native_us`, `observation` file
  reference. That independent observation JSON has exactly
  `schema=alice-pair-clock-observation-v1`, `run_id`, `host_clock_id`,
  `native_us`, `host_pre_us`, `host_post_us`, matching its anchor. A correlation
  bracket must actually bound observation of that process's native clock; a
  camera return time alone is insufficient. The before native observation is
  at/after ACK and <=S-10 s; the after observation is >=T. The full outer host
  interval `[before.host_pre_us,after.host_post_us]` must be covered by the quiet
  log and contain no root-port request, including endpoints. FTP closes before
  that superset. Do not acquire new observations during settle/window.

Freeze, deployment, review and quiet use the same independently recorded host
monotonic session ID. Planned AB/BA order is checked against deployment starts.
Host microseconds are **never equated** with native microseconds, UTC or another
launch; only explicitly joined correlation brackets can bound external quiet.
These receipts are an evidence contract, not a new device collection tool.
If existing console logs cannot supply a sound correlation bracket, report
UNKNOWN and resolve the logging method before claiming a qualifying comparison.

## Limits of this gate

`method_qualified`, `campaign_qualified` and `candidate_benefit_established` stay
false. Observed clock/pose equivalence leaves enemies, RNG, contacts, pickups,
cinematics and other unobserved dynamic state UNKNOWN. Listening, active-route
coverage, native RAM/stack, sampled heap <180 MiB/no allocation failure, visuals,
contacts/FX/material ordering, pickup/natural-exit and pause/retry/save-load/
transition requirements and the 13-map campaign remain separate contracts.
No SELF rebuild is needed for these Python tools/documentation changes.

## Host verification

Generate fresh actual writer fixtures with the existing workflow:

```sh
ALICE_TIMING_TEST_OUTPUT=/absolute/new/fixtures cargo test --locked --offline --features native_timing
ALICE_TIMING_TEST_OUTPUT=/absolute/new/fixtures python3 -B -m unittest discover -s tools -p 'test*.py' -v
ALICE_TIMING_TEST_OUTPUT=/absolute/new/fixtures python3 -B tools/test_native_timing_pair_mutants.py --output /absolute/new/mutants
```

Use a new fixture root for each Python interpreter/full rerun, because adversarial
tests preserve exclusive output paths. Pair mutants use copied Python modules
and meaningful test assertions, never edit production sources or native code,
and distinguish syntax/import failures from killed mutants. The existing
compiled hook/session/memory probes remain separate host checks.
