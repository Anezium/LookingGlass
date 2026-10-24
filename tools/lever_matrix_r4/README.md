# Lever matrix r4 reproducibility

Build revision cd33c35f7ab093b053a634c0b89d4dcb5600e071 contains the engine fixes. Subsequent source-only tooling commits do not change engine bytes. See the external SOL-REPORT.md for measured results, hashes and limitations.

All commands run on the Mac. Never contact the Vita or driver during implementation/qualification. Use /opt/homebrew/bin/bash. No assets belong in this repository; probes read the external user-supplied PK3 directory. Host output always uses /Users/anezium/vita-j0/cargo-target-host. No cargo update.

The external output is /Users/anezium/vita-j0/resume-2026-10-24/alice-lever-matrix-r4. Python is /Users/anezium/vita-j0/venv/bin/python. The source scripts accept that output directory as the first positional argument except log-integrity.py (label and --root) and the unchanged console/offline fixture scripts. Read scripts before reuse; the task's clone and SDK paths are explicit.

- verify-host.py OUTPUT/host-proof: git-archive r3 and current HEAD; instrument allocator only in scratch snapshots; run exact audio, randomized sweeps and construction/begin census. OUTPUT/configs/deterministic-audio-config.json must exist.
- world-probe.py OUTPUT: reproduce the existing r3 instrument_world.py probe from its external analysis directory, compare original and corrected kernels, then build sealed host source and compare rendering. It reads the prior analysis but does not mutate it.
- scripts/build-native.py OUTPUT: committed source only, pinned nightly/offline/locked r3 native recipe. Keep packages outside the repository.
- scripts/qualify-host.py OUTPUT: existing test selections plus world-probe.py. ALICE_TIMING_TEST_OUTPUT is set locally for the real producer fixtures. Source main.rs mtime is refreshed when switching source roots because a shared Cargo target can otherwise reuse the previous root's executable; bytes stay unchanged.
- scripts/seal-package.py OUTPUT: verify unchanged pins/defaults, deterministic VELF/SELF conversion receipts, native checks and VPK contents, then write the three package manifests. Run ELF/process checks and native disassembly before sealing, as recorded in external proof/.
- scripts/native-proof.py OUTPUT: assert the retained disassembly's concrete guard, borrowed comparison and target fixed allocation requests for this build.

The reviewed console draft, configs and 12 pending protocol labels are preserved here. Copy them and source helpers into OUTPUT before offline dry-run. dry-run-console.py belongs in OUTPUT/scripts and binds only a direct child helper/session directory. Its initial fixture run writes ROUND3-CONSOLE-TOOLS-TESTS.json; inspect it and seal it as CONSOLE-TOOLS-TESTS.json before the binding rerun. No fixture starts a receiver or contacts a driver/device. scripts/prepare-console.py OUTPUT materializes the reviewed brief, design, configs, helpers and manifests outside the source repository.

No runtime promotion, console qualification or Vita wall gain follows from host equality. Preserve every gate and restoration identity in CONSOLE-BRIEF.md.
