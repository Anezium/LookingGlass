# Step 0 Mac validation — 2026-10-16

Runtime a0d6e7616e44a61ea859733a40a54bf57957bcff, branch vita-threading-step0,
from vita-reference-r3 acfd417. The reference ref remains unchanged. Cargo.lock,
CPAL/miniquad pins, native VitaSDK archive/header and existing r3b settings match.
Only sim_span_probe is added to the config, default false. See [STEP0.md](STEP0.md)
for all 43 appended CSV columns, exact boundaries and audio log semantics.

Mac checks passed: 878 all-feature tests, 796 default-feature tests (three ignored
in each suite); the real-data tower2 fan test passed explicitly. Source worktree
and index audits and two GL ownership compile-fail checks passed. Same host binary,
probe off/on: all 3900 frame state hashes and workload counters per arm match over
ten maps, including 1200 Funhouse frames; 15 renderer images and 120 native early
skin/consumer images per arm match byte-for-byte. The latter also validates 60
serial/overlap frame comparisons across six maps and consumer witnesses in each
probe arm. Actual Mac output opened; 14 measured audio playback intervals passed.
No host performance claim; no console or remote-server contact.

Pinned native Mac r3b pipeline: nightly-2026-10-11, -neon, four build jobs, own
targets. ELF margin 1447316 bytes (required >=3348), imports pass, regenerated
VELF equal, VPK CRC/content pass. Heap remains 192 MiB; entry/game stacks 8 MiB.
SELF SHA-256: 0a5b49ae79c62ed3421570ddfb8429895a9e0467ca6fa966346b8b8af753a95b
VPK SHA-256: 358743fc8e93bbe61b551f02d005dec21bfc4d15fa9e8f2e308f8ee83432a862

Evidence/package: /Users/anezium/vita-j0/atoz/threading/step0/ . It contains
A-stock/VERIFICATION.json, FINAL-VERIFICATION.json, SUITES.json, PARITY.json,
COLUMNS.md, runtime-source.tar.gz, SHA256SUMS and PROTOCOL.md. The protocol uses
one SELF, real active audio, explicit sim switches, village three alternating
pairs and one pair on potears3/hatter1/potears1/tower2, frames 61–180 with stalls
retained. Console run-clock policy/units, audio CPU, memory and instrumentation
overhead remain pending the owner's console campaign. This step authorizes no
threading optimization. The inherited docs/RCHESS1.md CRLF artifact was not staged.
