# Coverage runner absent-manifest parity

Current runtime:615c1253322462284c180aa212bc0cf7b3616ae4. A fresh native Mac build of installed J20 pkg-2 source86792fe186736125219de139ee1ddec44dc356fb was compared against the coverage executable without a manifest.

All109 existing captures match byte for byte and decoded RGBA:15 reference,22 alpha,72 actor scenes. The reference and actor-scene images also passed nonuniform-image checks, preventing blank frames from serving as parity proof. Existing native render assertions passed. No cross-platform pixel equality or console performance is claimed.

Evidence: `~/vita-j0/village-retained-world/jalon-coverage/mac-parity/native-v5/RESULT.json`, associated `*-pkg2`/`*-coverage` directories and `logs/mac-parity-v5-controller.log` (exit0). Both binaries used the same byte-preserved stock renderer/simulation configurations and reference checkpoint, native Mac OpenGL, frozen clock step0, audio disabled and no coverage manifest. Host opt-level1/LTO-off builds and the standard Vita release package are separate artifacts. No other track's clone or target was used.

- Coverage executable SHA-256:b3650e26e566933007ea567d33270cb34dfea4beb2653bdff0100cd87f000aa3.
- Mac pkg-2 executable SHA-256:7974e67be9af202e456096c7efac3d7e8c1b502d6f9a1355cb6e5d0b9ff17174.
- Cargo.lock:d18dd40246bf2eaca095b2ff364a2eca20d7c95fabd0c76231ff1072205775ee.
- Renderer:abdbc1c8b1ca3cd0aa877e2f0cb47fa3ce5f92daa6e48254de95c1291d8460ab.
- Simulation:fa64b0c5b79c52d8e028f7fc9d0d3dd2eec57b7641dfc8f76b34212bdcb3334b.
- Reference checkpoint:4e5dab48ede6b88f30ac80fce6fb117c3bf4bacf6e76aabd5aeec260bb188714.

Historical checks remain preserved:109-image prior-host proofs at829bf37 and67af7c7, plus native Mac proof at9be3e67. The copied67af7c7 image/package hashes were reverified in mac-copy-verification.json. Those older results do not replace the current615c125 proof.
