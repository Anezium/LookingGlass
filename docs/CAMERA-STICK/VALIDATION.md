# Camera stick validation and package

Runtime source: `528823daeaa8643674760e01f68dd94cd541dd65`; base r3f `0bd6241`.

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
- Direct host measurements of production turn/logger code at 30/40 FPS:
  about 210 degrees/s at rescaled |x|=1 and 195.921 degrees/s at |x|=.96,
  with peak_abs_x and full_deflection classifications reported correctly.
  `RATE-MEASUREMENT.log` records these separately from pending console checks.
- Existing six test-suite arms passed: 908 all-feature and 825 default tests
  in each simulation-switch arm, plus real-data fan fixtures in both arms.
  Real PK3 and collision trace ignored suites passed. Focused layout tests
  verify row labels and spacing; pure logger tests verify peak windows,
  including worn-stick values below .99. Host renderer
  image parity, controls, save previews and config checks passed.
- Host tests use the required shared `CARGO_TARGET_DIR`, native
  `aarch64-apple-darwin`, and branch-specific intermediate `CARGO_BUILD_BUILD_DIR`
  under that shared directory, with four Cargo jobs. Compiled test executables
  are reused for switch arms and data fixtures.
- Host-only ten-map frame gate: all 7,800 neutral-input per-frame state hashes
  equal r3f in both simulation switch arms (3,900 frames each). This ran a host
  build where `Input::vita_look` and its caller are cfg-gated out. It proves
  the non-Vita path is unchanged; it does not test or prove Vita camera behavior.
  Host unit tests separately exercise the pure stick math and logger. Vita SDK
  input, menu wiring and actual console behavior still need the human protocol;
  the Vita package build checks compilation, not playability or target replay
  equivalence. Full logs remain in the evidence directory; no performance claim
  is made.
- Native Mac Vita build uses nightly-2026-10-11, locked dependencies,
  vita_renderer only and r3f's -neon flags. SFO, process parameters, SDK/vitaGL
  identity and packaged renderer/simulation configs match r3f. VPK CRC,
  SELF/SFO contents, imports, ELF metadata margin and absence of host probes passed.
- SELF SHA-256: `9cffa0b720ce67031a7e17fa529bdf1f8eb6fb08d50104438347dcf172cbed51`
- VPK SHA-256: `eb6eb0fe4d702aa6763a924a89fb68444c3922357a82e1b4347d78f22e5016de`

Artifacts and reproducible drivers: `/Users/anezium/vita-j0/atoz/camera-stick/review-fixes/`.
Package: `package/`; detailed results: `FINAL-VERIFICATION.json`, `SUITES.json`,
`GATE-review.json`, `HOST-VERIFICATION.json` and `package/VERIFICATION.json`.
No assets are committed. No console was contacted. Human feel and console-rate
validation remain pending the companion `PROTOCOL.md`.
