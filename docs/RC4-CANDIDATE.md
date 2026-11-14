# Conservative RC4 integration candidate

The default `configs/PL-rc4.json` preserves the played RC3 settings. It writes
every current runtime key, leaves all later opt-ins disabled, and retains the
proven direct scaled target, frustum cuts and zero-alpha spray cut. The existing
gradient `sky=true` stays enabled; authored sky, cube and depth additions stay off.

Isolated configurations allow later qualification of fog; exact blend factors
with fog; water normals; full fill approximations; cached sky with explicit
capture completion and fog; and adjacent-world state/submission reuse. Their
names state the combination. Record/replay files use the existing `a2-#` and
`a2-04` tape names. A combination being packaged does not qualify its picture,
performance or interaction on Vita.

The sky variant completes capture before retiring its resources. The adjacent
world path retains generic actual-pipeline equality and all alpha interruption
fences. The integrated dynamic pipeline index is machine-sized rather than an
8-bit field, avoiding wrap before the finite GXM program-table fallback. Values
outside that table remain unrecorded; the table is not enlarged. Host packet
tags keep the exact old bytes for indices up to255 and distinguish wider classes.

All runtime candidates remain default off. Cached sky animation and outdoor
sampling, exact-blend/detail pictures, water/fill tradeoffs and the actual Vita
cost of adjacent submission reuse need their own explicitly coordinated console
slots. The adjacent candidate did not reduce the sampled river draw count in its
host experiment; it must not be described as a river draw-count solution.

No game data or compiled artifact is stored in Git. The exact package, hashes,
test results, conflict resolutions and open gates are recorded outside Git in
`~/vita-j0/plan-2026-11-12/rc4/REPORT.md`. This branch is for root review and a
bounded console comparison, not a production default change or release approval.
