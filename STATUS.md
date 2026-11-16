# wchess1 render: first package ready for root measurement

Package: /Users/anezium/vita-j0/plan-2026-11-12/wchess1-render.
Read REPORT.md and README.md there; READY.json is authoritative.

Native head: 9da106ad0d14c2f441650edd5533b0be5049f36d.
Host oracle head: a6c6c12bafa4b9aecb69ca349c62c632ac2f0f09.
Unit head: 5c4fca705d77c5f30d74d2760d69780534f20c48.
runtime-equivalence.json verifies exact known cfg(test)-only fixture/schema edits.
Later source commits contain package tooling/status only.

- READY SHA256 cd596771888f7882ae636556433abd4a76d941e71f722e42e1d92178c5904030
- SELF SHA256 48848ca9f1e606bcf631b6cc435fe31c6bab796fee811bc871ba54d9719302f2
- Driver SHA256 1a18fe7b6e36fb581cea1dfdb2ae6c44019b08168511469d25089eb59187bc8d
- Kit unchanged 93f434a6286b2180bd66d89ffad1394b7f27b5e385296c67a70b2271256558b3

20/20 full-tape comparisons PASS on a2-04, wchess1 A4, owner wchess1 play,
skool1 and potears1: 51716 compared frames, no state/event/packet/pixel tolerance.
1416 Rust tests PASS, 16 ignored (opt3, test-only LTO disabled); feature-off PASS.
Production oracle and SELF retained production LTO. 18 Python tests PASS:
12 real-kit fake-transport step tests, 3 comparator tests, 3 receipt tests.
Native static/library/notices/LiveArea gates PASS.

Default-off: world_alpha_parameter_once, actor_skin_output_reuse, combined.
Order: A4 700..1180, play 1486..1966, play 796..1276; within each window,
control / alpha-once / skin-reuse / combined (12 arms). Full tape per arm.
Root owns console authorization, image acknowledgement and restoration.

The >=8 ms Vita gain and <=50 ms playing p95 bar are NOT established.
Host samples: 307 opaque, 116 alpha, 28 other actor, 19 chess, 8 FX, 8 HUD and
one upscale draw. Environment-mapped chess bodies use deferred/raw publications,
excluded here. Wchess1 skin coverage is only ~18 vertices/frame A4 and ~22 play;
no large saving is justified. Exact raw-output/layer reuse is a follow-up.

Timing lock only around individual timing runs; no build/equality run holds it.
No private host target was needed. No console/FTP/1337/1338 contact, kit edits,
default flips, game assets in Git or Terminal.app windows. Packages/proofs remain
active for root review. No visible-window-count or console restoration claim.
