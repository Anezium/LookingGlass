# Coverage runner absent-manifest parity
On the build host, the existing J20 pkg-2 parity runner was reused with the exact PL-def renderer settings, simulation settings, reference checkpoint, frozen clock and actor fixtures. No coverage manifest was passed.

All 109 captures matched the pkg-2 baseline byte for byte: reference 15, alpha 22, actor scenes 72. The decoded RGBA pixel comparison also reported zero differences for every image. These checks cover the existing normal renderer fixtures, including original fallback paths; they are not live console performance measurements.

Evidence is in `/srv/fast/projects/alice/jalons/jalon-coverage/runs/final-no-manifest-{reference,alpha,scenes}/RESULT.json`, the associated raw logs and images, and `logs/parity-final.log`. The tested runtime revision is 829bf370b624be1e3a6f6afb9557205a790f4948; subsequent documentation/tool changes do not change runtime source. Host compilation disabled LTO and optimization for this parity binary. The optimized host rehearsal binary and the standard Vita release package are separate artifacts.

Pinned input SHA-256s:

- Cargo.lock: d18dd40246bf2eaca095b2ff364a2eca20d7c95fabd0c76231ff1072205775ee
- Renderer: abdbc1c8b1ca3cd0aa877e2f0cb47fa3ce5f92daa6e48254de95c1291d8460ab
- Simulation: fa64b0c5b79c52d8e028f7fc9d0d3dd2eec57b7641dfc8f76b34212bdcb3334b
- Reference checkpoint: 4e5dab48ede6b88f30ac80fce6fb117c3bf4bacf6e76aabd5aeec260bb188714
