# Amendment r4-1 helper fix

## Diff summary

- scripts/log-integrity.py compares the longest renderer config record with prior
  protocol runs from the same session and arm, using the label suffix -A or -B.
  Once both arms are present, longest(A) minus longest(B) must equal 1 byte.
  Same-arm mismatches and every other cross-arm difference return
  INVALID_LOG_INTEGRITY_STOP_ALL_REMAINING_R4 with exit code 2.
- The whole-log split and orphan patterns, scan, and stop verdict are unchanged.
  console/audit-run.py is unchanged and still checks parsed config values.
- scripts/test-log-integrity.py retains the four original cases: complete,
  nested tag, orphan field and different config length. It also checks exact
  copies of r3 S4 Pandemonium A1/B2 runtime logs in session order: both PASS,
  with longest lengths 2783/2782 bytes. The negative fixture has valid JSON
  records of 42/40 bytes: A passes, then B stops on the 2-byte difference.
  Additional checks cover equal and reversed cross-arm lengths, ABBA repetition,
  same-arm B drift, session isolation and prior protocol ordering.
- The copied logs and source hashes are under proof/log-integrity-fixtures/.
  R3 evidence was only read and copied. Historical logs remain fixtures, not r4
  console evidence. test-console-tools.py invokes the new regression suite.
- The same helpers and fixtures are synced to tools/lever_matrix_r4/ in the
  clone. Its source-only prepare-console.py now copies the fixtures when
  materializing a package. The clone's console brief was not edited.

## Verification output

Commands ran on the Mac using /Users/anezium/vita-j0/venv/bin/python.
Both commands exited 0:

```text
$ python scripts/test-console-tools.py
62 pure-host matrix tool checks passed.
12 offline log-integrity fixtures passed.
$ python scripts/dry-run-console.py --output proof/CONSOLE-DRY-RUN.json
208 offline checks passed; proof/CONSOLE-DRY-RUN.json
```

Full output: logs/R4-1-CONSOLE-TOOLS.txt and logs/R4-1-DRY-RUN.txt.
Structured receipts: proof/LOG-INTEGRITY-FIXTURES.json,
proof/CONSOLE-DRY-RUN.json and proof/MATRIX-CONTRACT-TESTS.json.
The dry-run's existing transport mocks/guards remained active. No Vita,
shot-driver or receiver contact occurred. No engine, config, protocol or SELF
bytes were changed. Build revision remains cd33c35; all three packaged SELF
files retain SHA-256 3d2b824f31234bb1b831ace90bbae190e937ef892addd21394102a9c769b0ba3.

## Artifact reseal

ARTIFACT-SHA256.json SHA-256:
0788e669272e8bd88a54bbf451811d79904d06341759cea635f4e345f9e10509

All 87 entries were checked against disk after resealing. The original
manifest entries were retained; current helper scripts, fixture files and test
output were added. This report and the manifest itself are excluded to avoid
a circular hash dependency.

CONSOLE-BRIEF.md was not edited. Its sealed SHA-256 is:
7736bbc0de1aabeade26d33791dba0a78e6ae96f73eccf2b63098fefcc0a972e
