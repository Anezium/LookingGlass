# r8 SAH amendments fixed before console data

These amendments take precedence over the unchanged inherited r6 brief. No
console data has been collected for r8.

The package is alice-sah-r8, branch vita-sah-r8-20261024. The common SELF comes
from that branch's engine revision in PACKAGE-IDENTITY.json. Package identities
are baseline, sah, baseline-census and sah-census. Every config explicitly sets
audio_static_cache=false. The only wall-arm difference is collision_sah. Both
switches and census remain default off in the engine.

Use two separately preserved/restored sessions to retain the 150-minute cap:

1. Exact fresh `session-s10-stack-load-smoke`: the two `smoke_sequence` B-only
   entries, Hedge3 then Pandemonium. Run the same complete 60+120-frame fixture,
   so each smoke proves loading and a sampled benchmark plateau. It is excluded
   from every performance comparison. Full preservation, common-SELF staging,
   integrity/audio/heap checks, clean exit and exhaustive restoration are required.
   Admission: 69 minutes including restoration, plus >30-minute expiry margin.
2. Exact fresh `session-s10-stack`: `sequence` is Hedge3 ABBA then Pandemonium ABBA;
   `optional_sequence` contains the two REQUIRED diagnostic Hedge3 A/B runs.
   Its ten-launch plan is 145.5 minutes including restoration. The inherited
   name `optional_sequence` is a helper schema field, not permission to omit them.
   After helper binding, run `admit-smokes.py --root <fresh-session> --smoke-root
   <completed-smoke-session>`. It verifies hashed passing smoke audits and
   restoration against this exact common SELF. Every non-smoke launch refuses
   to proceed without that admission. Keep the completed smoke directory intact.

Both exact session directories must be absent from the sealed source/package
and ZIP. Create them exclusively only after the mandatory offline main-path test
passes, sole ownership is granted and driver expiry/budget is freshly checked.
`bind_helpers.py` accepts exactly those two names. Re-preserve the human state
before the main session; the smoke session's restoration does not authorize
reusing an old baseline or stage ledger. The same binary bytes are staged once
per session. No scripted whole-campaign driver command is provided.

READING-RULES.md replaces the inherited +1 MiB delta gate with <=+8 MiB per
oriented benchmark pair and Hedge3 plateau <165 MiB. Whole-run >180 MiB samples
stop, including loading and shutdown. Global crash/OOM also stops in every
phase. The expected retained Hedge3 delta is 3–5 MiB before precise host/native
measurements. Load peaks are separate from benchmark sampled maxima.

Benefit requires both Hedge3 oriented median B-A deltas <-1.000 ms; both
Pandemonium deltas must be <=+1.000 ms. All integrity, identity, audio and heap
gates must pass. No trimming, pooling, observer correction or census wall gate.
Census reports per-owner static miss time, nodes, hull IDs, clips, planes, legacy
certificates and fallbacks. It keeps all exports and initial absences restored.

late_callbacks is report-only in every phase. Benchmark underrun_estimates>0
stops; every other strict-audio gate remains. Loading/shutdown counters are
reported separately. A reporter count does not establish an audible defect.

Retain human SELF SHA-256
387ccf678251d4267b8ced53e207830358826f8e0642631a0459c6b0b9939990 and save SHA-256
ea4cd221d9f950e613188ccf3ecf2c1eabb1d34106f413125864320d364c8fa9. Read back
independently, restore every save/route-save companion, config, census export,
initial absence, inventory and shared stage-cache index. Inspect HOME and return
ownership without a redundant quit. en0 preflight, fixed driver identity and
expiry, explicit image acknowledgements, serial locked FTP and no MKD remain.
The implementation worker must not contact the Vita or shot driver.
