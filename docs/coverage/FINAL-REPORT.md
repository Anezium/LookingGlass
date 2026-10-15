# Coverage runner final report — Mac, 15–16 October 2026

Completed: the frozen final manifest passed its entire three-repeat host rehearsal. **45 complete, 24 explicitly uncovered, zero failed cells.** No console was accessed; host timings are rejected for sampled load and are not console acceptance.

## Implementation and source

Runtime/package source is `615c1253322462284c180aa212bc0cf7b3616ae4` on `vita-coverage`; the frozen manifest checkpoint is `3f1ad3a`. The runner is opt-in through `ux0:data/alice/private/coverage_survey.json` on Vita or an explicit host flag. It drives normal gameplay/tick inputs, isolated checkpoint Stores, authored school/Queen setups, segmented light CSVs, activity/load/memory evidence and bounded watchdogs. Absent-manifest behavior is covered by fresh native Mac parity.

The resumed Mac work added lock/load coordination and fixture/parity/package auditing tools under `tools/coverage/`. Native checkpoints `9be3e67` and `615c125` corrected actual club-contact accounting, post-window host readback and manifest-only host camera projection. Main files: `src/coverage.rs`, `src/combat.rs`, `src/viewer.rs`, `src/main.rs`, `src/render/vita_renderer.rs` and `src/render/vita_renderer/scale.rs`. Existing opt-in light-profile tail accounting remains enabled. Contacts use ephemeral feedback without changing the saved ranged-shot/death-FX seed; captures read the retained target after final row flush without advancing simulation; camera aspect stays 960/544 independently of recorded Cocoa resizing.

No predicate, epoch count or duration was lowered. Subsequent commits changed only tools/docs/manifests. All builds, tests, captures and replays ran on the Mac, with four build jobs, separate targets and the shared timing lock. No SSH job, console operation, J23 clone/target modification, dependency update or game-data write was performed. The clone's `docs/RCHESS1.md` CRLF artifact remains uncommitted. Game assets and checkpoint/save payloads remain outside Git.

## Fixture disposition

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

School and Queen are diagnostic authored setups, not natural campaign checkpoints. School guards retain normal health, support, AI and distinct IDs with a shared model. Normal funded weapons, cooldowns, contacts, deaths and resets outside timed epochs are retained. Dismemberment uses normal Cards damage then one normal Knife throw. No god/notarget/Watch freeze, damage override or warp is used to satisfy a predicate. W/E have no 60-second proof and remain skipped.

Two earlier entire rehearsals are preserved and rejected as final proof. The first had 50 complete/18 uncovered/one failed Staff cell: repeat 3's final 4-second epoch fired once but had no impact. Staff was excluded, then the whole revised manifest reran. The second had 47 complete/21 uncovered/one failed Jackbomb cell: repeat 2's final 4-second epoch had no shot/impact. Jackbomb was excluded, then the whole manifest reran again. There was no further tape correction or predicate change. Calibration-only success was not promoted over a failed required repeat. Older blank-image and varying-aspect attempts also remain preserved with rejection reasons.

## Final full rehearsal

Manifest SHA-256: `31f46b02599815fe5ea087a6ede3c5a167b86e10697704af404d5026351c17c8`. There are 23 ordered segments × 3 repeats, 15 active/eight excluded. Output: `coverage-results/coverage-baseline-1792103095751774000/`; controller: `mac-runs/rehearsal-final-v3/campaign.json`, runner/wrapper exit 0. Campaign elapsed 1294.369s, **85,758 raw frames / 68,965 timed frames**, 1,486 CSV files and 1,354 labelled export frames. No stalls or export rows were trimmed. There are zero negative outer-residual rows.

Strict raw auditing passed all 108 timed segments, 45 initial/final state pairs and 45 nonblank post-window PNGs. Every required epoch activity maximum meets its original minima. All 45 images were visually reviewed in three contact sheets. The nine checkpoint Stores contain only byte-identical Slot 1; the remaining 36 Stores are empty; excluded cells create no Store. Config/fixture hashes and the optimized host binary are unchanged. Authored manifest fields match the recorded effective representation, including explicit serialization defaults and exact f32-bit normalization; input SHA remains authoritative.

All three T cells ended at saved visit-key `pandemonium$first`, with live map/entry traces `pandemonium`/`player_start`. Source and destination CSVs are separated. Exit-request to first-presented milestone deltas were 670/658/667ms on this Mac; no separate first-controllable timestamp or console speed is inferred.

`summary.json` retains raw CSV hashes, per-repeat and per-epoch/segment n/median/p95/mean/max, fractions above 33.3/50ms, longest over-budget runs, phase/residual statistics and sampled memory extrema. p95 uses sorted[floor(.95*n)]. The historical V0 frames 61..180 slice is retained separately. Individual repeats/epochs and T source/destination must precede pooling; independent phase medians are never added. Full verification is `MANIFEST-VERIFICATION.json`, with save, epoch, visual and input audits alongside it.

## Native proof and A-stock package

846 feature tests passed, zero failed, two existing ignores; default-feature check passed. Fresh native Mac absent-manifest parity against separately exported/rebuilt J20 pkg-2 source 86792fe is **109/109 PNG-byte and decoded-RGBA exact**: 15 reference, 22 alpha, 72 actor scenes. World/actor images also passed nonuniform checks and existing render assertions. Same stock configs/reference checkpoint, native OpenGL, frozen clock step 0 and no audio/manifest. This is native Mac proof, not cross-platform equality.

Optimized host executable SHA-256: `b3650e26e566933007ea567d33270cb34dfea4beb2653bdff0100cd87f000aa3`. Standard-release A-stock-mac-v5 was built on the Mac with pinned Vita SDK archive/header, nightly-2026-10-11, ThinLTO and four jobs. Cargo.lock/SDK hashes match before/after; Mio/NEON flags are recorded. ELF margin 1,471,544 bytes exceeds 3,348. Imports, regenerated VELF equality, ZIP CRC/exact entries and packaged SELF/SFO checks all pass.

| Artifact | SHA-256 |
| --- | --- |
| looking-glass.vpk | `90caa146dc562998a80a31aa85e889dd0bdf997639eb4ac8eca009698ea45b14` |
| looking-glass.self | `d4e51efae2e317ee225689e7c5322bd4f8140b9a701b601ceeccfbe92024df49` |
| looking-glass.elf | `2f893ba9c4836695dad9d552f19d992cbf6634427f88d2cdef49050ada90d92d` |
| looking-glass.velf | `6b2b49386aac0fc1201d331d8717a041edde96bd263458fb87e518a6a2313f4d` |
| looking-glass.sfo | `8b536cc3da61685f961fadee5d9da7ac266a17f28e9543249557667b038b8452` |

The VPK contains only eboot.bin and sce_sys/param.sfo. Current package proof is `A-stock-mac-v5/VERIFICATION.json`; fresh parity is `mac-parity/native-v5/RESULT.json`. Earlier similarly named packages are historical.

## Console handoff and limits

`console-bundle/` contains the current package and regenerated proof, byte-preserved stock configs, relative fixture payloads, frozen manifest, final verification, this report, PROTOCOL.md and SHA256SUMS. The protocol specifies scene-outer/repeat-inner order, isolated saves, unchanged settings/clocks, no capture/FTP during retained windows, retrieval only after closure, 300s cell/load watchdog and 7,200s overall watchdog, followed by original eboot/settings/profile/manifest/save restoration. No console installation or run has been performed here.

Reserve **70–100 minutes unattended console occupancy**, hard two-hour campaign limit. This is a planning assumption: 45 initial loads + 3 destination loads = 48; applying the historical village 49.672s restore median to every load gives 39.74 minutes. Settle/timed windows add 402s per pass × 3 = 20.1 minutes, plus traversal, resets, startup and other-map allowance. It is not a measured console forecast.

The no-audio Mac run proves mechanics and named fixture predicates, not Vita timing, playback, visual acceptance or pool fit. Logical projection is 960/544 while recorded physical Cocoa/retained-target dimensions can differ; diagnostic images exclude HUD. GL-call time is CPU driver time, not GPU execution. Memory sample extrema do not prove a continuous transient peak; unavailable allocation-failure/largest-contiguous-block counters remain null. W/E, excluded FX/crowd/boss segments, other maps/bosses, water/mirror, movies, campaign completion and 30fps acceptance remain unproved. CodexBar was restored and its running process verified after the last host run.
