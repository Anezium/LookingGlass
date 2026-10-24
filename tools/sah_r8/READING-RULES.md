# Rules fixed before r8 console data

One common SELF, A collision_sah=false, B collision_sah=true. Acoustic cache and
census off in both wall arms. B-only load smoke on each map must pass first.
Then Hedge3 and Pandemonium wall ABBA, plus one separate census run per arm on
Hedge3. Both Hedge3 oriented B2-A1 and B3-A4 median deltas must be strictly below
-1.000 ms. Both Pandemonium deltas must be <=+1.000 ms. No pooled arms, trimming,
observer correction or census timing in the wall gate.

Expected retained B-A heap is approximately 3–5 MiB for Hedge3, from compact
hierarchy/certificate/index storage; the exact model will be retained before
console execution. Replace the inherited +1 MiB rule: both oriented sampled
benchmark heap deltas must be <=+8 MiB, every Hedge3 benchmark plateau must be
<165 MiB, and any whole-run sample >180 MiB stops immediately. A loading peak
and a benchmark plateau are reported separately. The process heap remains
192 MiB. A smoke cannot prove a continuous peak; sampled evidence is labelled.

late_callbacks is report-only; benchmark underrun_estimates, starvation, dropped
reports and rejected sinks stop. Producer p95 must be strictly <8 ms. Global
crash/OOM stops throughout loading, benchmark and shutdown.

This package must not be executed by the implementation worker. Preserve r5/r6
arm-aware log integrity, complete census restoration, fresh session vacancy,
mandatory offline main-path test, en0 preflight, serial locked FTP, no MKD, fixed
sole driver ownership/budget and exhaustive human SELF/save restoration.
