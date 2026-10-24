# Alice r9 exact skin consumers

Mac-only implementation and offline console handoff. No console ownership is implied. Read SOL-REPORT.md in the external output, CONSOLE-BRIEF.md and every AMENDMENTS file before admission. Default remains false pending review and fresh console qualification.

Output: /Users/anezium/vita-j0/resume-2026-10-24/alice-skin-exact-r9. Host target: /Users/anezium/vita-j0/cargo-target-host. Assets remain in their external private directory. Never cargo update.

- prepare.py OUTPUT materializes configs, helpers, protocol and separate amendments without a transport import.
- Build the host with stable Rust, --release --offline --locked --features native_timing,frame_total_only,sim_maps_probe and RUSTFLAGS='--cfg observer_free_B'; copy its executable to OUTPUT/proof/host-bin.
- host_exact.py OUTPUT runs fourteen 180-frame map pairs with actual GL buffer readback. Add --fixtures-only for five 180-frame PNG/GPU pairs; --binary selects an explicit executable. The PNG build differs only in persisting every intermediate fixture image and is excluded from Vita.
- host_measure.py, host_consumers.py, build_r7_oracle.py, r7_regressions.py, r7_counts.py and verify_host.py retain allocation, consumer, negative-regression and matched-workload evidence.
- build_native.py OUTPUT uses r7's exact locked/offline Vita recipe; seal.py OUTPUT verifies the common SELF, conversion, ELF margin, process parameters and VPK contents.
- test_session_e2e.py OUTPUT --map MAP --include-optional --output /tmp/RESULT.json is the mandatory transport-blocked normal Python main path. Do not python -I. Both maps and both four/six-launch variants are supported; fresh console sessions require six launches. --sealed-helpers ZIP replays archived helper bytes.
- finish.py OUTPUT --cleanup seals the package and removes only task caches, retaining the shared host target. After offline ZIP replay and final receipts, --manifest-only verifies the unchanged ZIP and refreshes the evidence manifest. write_report.py OUTPUT produces SOL-REPORT.md.

CONSOLE-PROTOCOL.json visual_sequence provides Grounds2/Tower1 labels, with ordinary full mode and separate restoration. VISUAL-QUALIFICATION.md states what is chapter-reachable in source and what the pilot must verify. These launches never enter the wall gate or census. No package is run by preparation scripts.
