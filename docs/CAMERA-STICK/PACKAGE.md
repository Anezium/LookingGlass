# Vita camera package

Built on Mac from runtime commit `8a480e3c235c92b967f443f16bfd2c5f174fe0d8`, based on r3f `0bd6241`.
Artifacts: `/Users/anezium/vita-j0/atoz/camera-stick/package/`.

- SELF SHA-256: `0a40bc0ff5b4eb684e99b9902f6c4828035632524bb1aaa1b8d18d8d9bef33ab`
- VPK SHA-256: `a4f9f1850353fb5c74389c499999450369ecb09c000343c0dbf067b6b1ec7c47`
- nightly-2026-10-11, cargo-vita 0.2.2, `--release --locked --features vita_renderer`.
- Effective flags: `-C target-feature=-neon --cfg mio_unsupported_force_poll_poll --cfg mio_unsupported_force_waker_pipe`.
- Lockfile, VitaGL archive/header, package SFO and renderer/simulation configs
  match r3f. No game assets are included in the VPK or committed.
- ELF metadata slack: 1,489,116 bytes (required >=3,348).
  Imports resolve; regenerated VELF matches; ZIP CRC and packaged SELF/SFO pass.
- Process parameters match r3f: 192 MiB newlib heap and 8 MiB game-thread stack.
  Host simulation probes and optional actor diagnostics are absent.

The isolated Vita target was built independently while other functional-only
host work held the shared host lock. No timing/performance claim is made.
Host regression verification is reported separately in VALIDATION.md when complete.
No console was contacted; follow PROTOCOL.md for the pending human feel/rate test.
