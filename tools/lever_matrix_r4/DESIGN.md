# Alice lever matrix r4

Two targeted repairs on r3 881988ec: immutable static collision backing shared by World and off-main audio; guarded renderer compaction plus borrowed exact uniform comparison. See SOL-REPORT.md for file:line changes, numeric host proofs, native disassembly, allocation accounting, limitations and hashes.

The backing is moved at World construction. Mutable colliders/caches and audio_complete ordering remain on main; actor/player masks are independent. Reset clears owned handles, and completed worker packets release their temporary geometry handle. Cached endpoint results retain their validity only within the same client scene. Queue capacity, fallback timeout, producer priority and audio settings are unchanged.

Merge scans preserve all eligibility/state/adjacency/coverage fences. Only equal-index swaps are skipped. Uniform equality borrows complete padding-free words; equal parameter IDs skip work only with no inline overrides. Static packing and material scheduling are unchanged. No draw-count reduction occurred in the host Pandemonium probes.

Baseline PL is byte-identical to r3. Audio off-main, static merge and material sort stay false. The same new SELF serves baseline/audio/static-merge packages. Re-establish A1/A4 on that SELF; old results cannot serve as A baselines.

Qualification uses paired r3/r4 optimized host snapshots, deterministic map audio, randomized sweeps, full allocation bytes including first begin, original-kernel versus corrected-kernel replay, sealed source host PNG/state/light-bit equality, and the pinned r3 native build. Source archives and all Cargo outputs stay outside asset repositories; host Cargo target is the shared /Users/anezium/vita-j0/cargo-target-host. Native task target is removed at handoff after retaining sealed artifacts.

Console draft only: S4 audio Pandemonium then Hedge3 ABBA (127.5-minute independently admitted cycle), then guarded static merge Pandemonium ABBA (91.5-minute separately admitted cycle). Twelve launches remain pending. Same 60 warmup/120 measured positive wall intervals, zero GL observers, strict audio windows and >180 MiB heap stop. Both oriented wall deltas <=+1 ms are required; replicated benefit requires both <-1 ms. Whole logs must pass r3-1 integrity and config-record-length checks. R3-2 parity is descriptive/non-gating and never pooled. No console/GPU/audibility/memory-peak result is inferred from host evidence.

Read CONSOLE-BRIEF.md for current paths, sole-ownership and dynamic-driver admission, en0 route preflight, serial FTP/no-MKD/eboot-only restrictions, exact human SELF and measurement save identities, image interlocks, quiet windows and exhaustive restoration. INHERITED-R3-BRIEF.md is archival operational detail, not an executable r4 plan. No default/promotion change.
