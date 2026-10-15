# Coverage fixture status — Mac, 16 October 2026

Final entire three-repeat host rehearsal: **45 complete, 24 explicitly uncovered, zero failed**. Manifest SHA-256: `31f46b02599815fe5ea087a6ede3c5a167b86e10697704af404d5026351c17c8`. Runtime/package source: `615c1253322462284c180aa212bc0cf7b3616ae4`.

| Family | Fixture status | Complete / uncovered / failed | Scope or gap |
| --- | --- | --- | --- |
| V0 | Validated: opening and closing exact-reference controls | 6 / 0 / 0 | 20s settle + 60s live each; six complete cells. |
| C1 | Validated: acquisition, Blade and Cards | 9 / 0 / 0 | Normal one-guard fixture; every required epoch, nine complete cells. |
| C4 | Partial: acquisition and Cards | 6 / 3 / 0 | Blade excluded: no successful guard contact within its unchanged four-second epoch. |
| F4 | Partial: primary Ice, Blunderbuss, actual dismemberment | 9 / 9 / 0 | Alternate Ice wall lacks impact proof; Staff r3 lacked impact; Jackbomb r2 lacked a shot. |
| W | Uncovered | 0 / 3 / 0 | garden3 chase lost supported/alive state at 8.52s; no 60s moving-world proof. |
| E | Uncovered | 0 / 3 / 0 | wchess1 active/species 0/0; two nearby bishops do not prove a mixed encounter. |
| B | Partial: Intro, Birth and two death-FX stages | 12 / 6 / 0 | Queen2 Hatter/Scream lost live state at 16.40/3.85s; no boss-combat coverage. |
| T | Validated: real gvillage → pandemonium$player_start | 3 / 0 / 0 | Three fresh source checkpoints, normal input, 20s source and 60s destination each. |

All retained predicates, epochs, durations, normal guard health and input tapes are unchanged. Staff and Jackbomb are excluded after required-repeat failures; their successful calibration/other repeats are preserved but do not establish full coverage. Strict state/epoch/nonblank-image checks, all 45 image reviews and isolated-save/input-hash audits pass. Host timings are rejected for sampled load and never establish console acceptance.

846 feature tests pass with two existing ignores; default check and fresh 109-image native Mac no-manifest parity pass. Current A-stock-mac-v5 package passes pinned SDK/Cargo.lock/ELF/import/ZIP/hash checks; gap 1,471,544 bytes. No console or SSH job was used. `docs/RCHESS1.md` remains the uncommitted CRLF artifact.

See [final report](coverage/FINAL-REPORT.md), [console protocol](coverage/PROTOCOL.md) and [frozen manifest](coverage/coverage_survey.json). Authoritative local raw evidence and resume: `~/vita-j0/village-retained-world/jalon-coverage/`. Console reservation: 70–100 minutes, hard 2h, a planning assumption.
