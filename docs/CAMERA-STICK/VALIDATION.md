# Camera stick validation and package

Runtime source: `8a480e3c235c92b967f443f16bfd2c5f174fe0d8`; base r3f `0bd6241`.

- Old full-axis yaw/pitch at 30/40 FPS: default 34.377/45.837 degrees/s;
  maximum 103.132/137.510 degrees/s (source-derived, before pitch clamp).
- New full-axis default yaw/pitch: 210/135 degrees/s at both frame rates;
  sensitivity .2/1/3 maps to 60/210/420 yaw and 40/135/270 pitch degrees/s.
- Radial rescaled dead zone .24 (existing adjustable .1–.4), magnitude exponent
  1.7, immediate response/release; no temporal ramp. Render elapsed scales
  angular velocity, with a .1 s cap for long stalls. Inversion remains adjustable.
- Host tests cover rate/dt independence at 20/30/40/60/120 FPS and alternating
  25/38 ms, sensitivity range/continuity, dead zone, response curve, inversion,
  wrap/clamp, neutral bit preservation, measured yaw and modal neutral gating.
- Direct host measurements of the production turn/logger code at 30/40 FPS:
  209.9998/210.0003 degrees/s default yaw and 420.0000/420.0001 maximum.
  `RATE-MEASUREMENT.log` records these separately from pending console checks.
- Existing six test-suite arms (default/all features, sim off/on, real-data fan)
  passed; real PK3 and collision trace ignored suites passed. Host renderer
  image parity, controls, save previews and config checks passed.
- Host tests use the required shared `CARGO_TARGET_DIR`, native
  `aarch64-apple-darwin`, and branch-specific intermediate `CARGO_BUILD_BUILD_DIR`
  under that shared directory, with four Cargo jobs. Compiled test executables
  are reused for switch arms and data fixtures.
- Ten-map frame gate: all 7,800 neutral-input per-frame state hashes equal r3f,
  in both simulation switch arms (3,900 frames each). Full logs remain in the
  evidence directory; no performance claim is made.
- Native Mac Vita build uses nightly-2026-10-11, locked dependencies,
  vita_renderer only and r3f's -neon flags. SFO, process parameters, SDK/vitaGL
  identity and packaged renderer/simulation configs match r3f. VPK CRC,
  SELF/SFO contents, imports, ELF metadata margin and absence of host probes passed.
- SELF SHA-256: `0a40bc0ff5b4eb684e99b9902f6c4828035632524bb1aaa1b8d18d8d9bef33ab`
- VPK SHA-256: `a4f9f1850353fb5c74389c499999450369ecb09c000343c0dbf067b6b1ec7c47`

Artifacts and reproducible drivers: `/Users/anezium/vita-j0/atoz/camera-stick/`.
Package: `package/`; detailed results: `FINAL-VERIFICATION.json`, `SUITES.json`,
`GATE-camera.json`, `HOST-VERIFICATION.json` and `package/VERIFICATION.json`.
No assets are committed. No console was contacted. Human feel and console-rate
validation remain pending the companion `PROTOCOL.md`.
