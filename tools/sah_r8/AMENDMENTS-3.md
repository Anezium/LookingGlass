# Main admission offline test correction

This amendment supersedes the offline session-vacancy instruction in
CONSOLE-BRIEF.md and AMENDMENTS.md only at main admission. All experiment,
ownership, preservation, launch, measurement and restoration rules still apply.

Before main admission, keep the completed `session-s9-sah-load-smoke` intact and
run the following on the Mac, with `OUT` set to the authoritative package:

```sh
PYTHONDONTWRITEBYTECODE=1 "$PY" "$OUT/scripts/test_session_e2e.py" "$OUT" \
  --main-admission --include-optional \
  --output /tmp/alice-r8-e2e-before-main.json
```

Require exit zero, `status=PASS`, `mode=main-admission`,
`real_smoke_admission.status=PASS`, `real_package_session_path_absent=true` and
`final_status=PASS_ORIGINAL_STATE_VERIFIED`. Output and optional synthetic
evidence must be outside the package. The test validates both passing B-only
smokes, their exact package SELF readbacks, two restored human SELF readbacks,
the preserved and restored measurement save, and final restoration. It then
copies those inputs into scratch and rehearses the entire ten-launch main path
(eight wall arms and both diagnostic census arms) through final restoration.
Every transport and driver boundary in that rehearsal is fake.

The exact `session-s9-sah` path must be absent, including files and dangling
symlinks. Only after the offline test passes may the console owner create that
fresh directory and follow helper binding, smoke admission and the existing
console protocol. This test grants no console ownership and performs no real
launch or transport action. `test_protocol.py` remains a fresh-package check;
its two-path vacancy requirement is covered by the main-admission mode here
when preparing the main session after completed smokes.

For a fresh package before smoke admission, retain the existing full offline
mode: omit `--main-admission`, require both exact session paths absent, and
rehearse synthetic smoke and main sessions. Negative fixtures are available as
`scripts/test_main_admission.py OUT --output /tmp/alice-r8-admission-negative.json`.

The console ZIP excludes session evidence, `main-admission-attempt`, and
`smoke-admission-verification`. Keep those directories alongside the local
package, unchanged; the artifact manifest records their non-cache files. A ZIP
alone cannot satisfy main admission without the completed smoke evidence.
