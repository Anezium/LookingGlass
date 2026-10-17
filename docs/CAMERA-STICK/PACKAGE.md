# Vita camera package

Built on Mac from runtime commit `528823daeaa8643674760e01f68dd94cd541dd65`, based on r3f `0bd6241`.
Artifacts: `/Users/anezium/vita-j0/atoz/camera-stick/review-fixes/package/`.

- SELF SHA-256: `9cffa0b720ce67031a7e17fa529bdf1f8eb6fb08d50104438347dcf172cbed51`
- VPK SHA-256: `eb6eb0fe4d702aa6763a924a89fb68444c3922357a82e1b4347d78f22e5016de`
- nightly-2026-10-11, cargo-vita 0.2.2, `--release --locked --features vita_renderer`.
- Effective flags: `-C target-feature=-neon --cfg mio_unsupported_force_poll_poll --cfg mio_unsupported_force_waker_pipe`.
- Lockfile, VitaGL archive/header, package SFO and renderer/simulation configs
  match r3f. No game assets are included in the VPK or committed.
- ELF metadata slack: 1,491,168 bytes (required >=3,348).
  Imports resolve; regenerated VELF matches; ZIP CRC and packaged SELF/SFO pass.
- Process parameters match r3f: 192 MiB newlib heap and 8 MiB game-thread stack.
  Host simulation probes and optional actor diagnostics are absent.

The isolated Vita target was built independently of the host test target.
No timing/performance claim is made. Host regression verification and its
host-only scope are reported separately in VALIDATION.md.
No console was contacted; follow PROTOCOL.md for the pending human feel/rate test.
