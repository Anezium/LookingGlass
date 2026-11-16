# Hatter2 delivered for root review

Branch vita-fight-pilot-20261116; native build head 7fd23e2a8084f4240d25e57c15aac7efc3b7cb42, base RC14 37f9d06.
Host proof PASS: 2983 frames, Normal, one retry; boss 462 cumulative damage, Alice 185;
100/100 replay checks, diff=0/event_mismatches=0 with pilot OFF. See docs/fight-pilot/REPORT.md.
Private SELF/VPK and root bundle are in /Users/anezium/vita-j0/plan-2026-11-12/release-week/fight-pilot. PACKAGE.json lists full hashes.
11 offline tests pass, including actual native pad-function stub regression.
Native device testing and p95 remain root work; other map routes are not implemented.

Root procedure: tools/fight-pilot/README.md. Default off; no shipped config changes.
No console, FTP, forbidden ports, Terminal.app or kit edit. Build/exact replay ignore
mac-timing.lock under root's updated policy. Temporary caches removed at handoff.
Resume after root review with centipede1, then skool1 and short fights. Do not edit a
shipped config or use host frame numbers as native measurement windows.
