# Coverage survey console protocol

This is an opt-in named-scene survey derived from installed J20 pkg-2 (`86792fe186736125219de139ee1ddec44dc356fb`). Runtime/package source is `615c1253322462284c180aa212bc0cf7b3616ae4`. The A-stock package was built on the Mac with pinned native Vita SDK, nightly-2026-10-11 and four build jobs. No console was accessed during this work. The console owner performs installation and the run in a separate slot.

The frozen manifest SHA-256 is `94794b6aa6904f5d6184e9e95a5f0f0bf612d78d52ce7c977fa53b2d236c580d`. It contains 23 ordered segments, each repeated three times: 16 active, seven explicitly uncovered. Expected authoritative results: 48 complete, 21 uncovered, zero failed. Read FINAL-REPORT.md for the completed host rehearsal disposition before use. This survey does not establish campaign completion or a 30-fps floor.

## Files and preservation

1. Close Alice. Preserve the installed eboot, the full private renderer/simulation configuration, any profile_mode file, any preexisting coverage_survey.json, and all player saves/previews. Record paths, sizes and SHA-256s. Do not overwrite another experiment's files or run directories.
2. Verify the supplied console-bundle/SHA256SUMS. Use the established package/eboot installation procedure. The VPK contains only `eboot.bin` and `sce_sys/param.sfo`; it supplies no game data. Use the current A-stock-mac-v5 package, not an earlier similarly named artifact.
3. Copy bundle `private/coverage_survey.json` to `ux0:data/alice/private/coverage_survey.json`. Copy `private/fixtures/` beneath that same private directory, preserving relative names. Check the staged hashes. The fixture payloads remain local and are never committed to Git.
4. Stage the byte-preserved `vita_renderer.json` and `vita_sim.json` from the bundle. Remove profile_mode for the run. Required stock settings: effective clocks [444,222,222,166], render scale .75, two skin workers, worker/frame/per-draw probes off, 128 world puffs and puff collisions `none`. Do not tune settings or quality between cells. Confirm the startup effective settings and clocks in retrieved logs.
5. Ensure sufficient free storage for unique CSVs/states/milestones, keep the display/no-sleep policy appropriate for the slot and release physical controls. Start once through the ordinary bubble; Vita discovers the manifest without argv.
6. Every active cell uses its own Store under `private/coverage-results/<unique-run>/<scene>-r<repeat>-saves/`. Checkpoints are hash-verified and copied to Slot 1. Quick/auto/quit/finale writes are disabled for all survey cells. The player's normal save directory is never the survey Store. After closure verify the original player-save inventory and hashes.

## Order and windows

The loop is scene-outer, repeats-inner: V0 r1/r2/r3, C1-acquire r1/r2/r3, and so on. Exclusions produce reasoned result entries without loading. Each active repeat starts from a fresh checkpoint or authored fixture. T includes a real destination replacement from each fresh source; final V0 follows all T repeats.

| Segment | Initial map | Per-repeat windows | Disposition |
| --- | --- | --- | --- |
| V0 | gvillage | 20s settle + 60s | active |
| C1-acquire | skool1 | 12s / 2 epoch(s) | active |
| C1-blade | skool1 | 24s / 4 epoch(s) | active |
| C1-cards | skool1 | 24s / 4 epoch(s) | active |
| C4-acquire | skool1 | 12s / 3 epoch(s) | active |
| C4-blade | skool1 | 24s / 6 epoch(s) | uncovered; skip |
| C4-cards | skool1 | 24s / 6 epoch(s) | active |
| F4-ice-primary | skool1 | 12s / 3 epoch(s) | active |
| F4-ice-alternate | skool1 | 12s / 3 epoch(s) | uncovered; skip |
| F4-jackbomb | skool1 | 12s / 3 epoch(s) | active |
| F4-staff-release | skool1 | 12s / 3 epoch(s) | uncovered; skip |
| F4-blunderbuss | skool1 | 12s / 3 epoch(s) | active |
| F4-dismember | skool1 | 12s / 3 epoch(s) | active |
| W | garden3 | 20s settle + 60s | uncovered; skip |
| E | wchess1 | 20s settle + 60s | uncovered; skip |
| B-queen2-hatter | qlair | 20s / 1 epoch(s) | uncovered; skip |
| B-queen2-scream | qlair | 10s / 1 epoch(s) | uncovered; skip |
| B-intro | qlair | 10s / 1 epoch(s) | active |
| B-birth | qlair | 10s / 1 epoch(s) | active |
| B-death-explosions | qlair | 5s / 1 epoch(s) | active |
| B-death-burst | qlair | 5s / 1 epoch(s) | active |
| T | gvillage | 20s source + ordinary traversal/load + 60s destination | active |
| V0-final | gvillage | 20s settle + 60s | active |

School C1/C4/F4 uses one/four distinct normal-health guards sharing the supported rig, normal AI and funded weapons. Coordinates/view are frozen in the manifest. These are synthetic diagnostic fixtures, not natural campaign checkpoints. Each school epoch restores the complete initial gameplay snapshot, particles/audio/clocks outside timing; there is no resurrection or immunity inside a timed epoch. Alice is not god/notarget and Watch is not stopping the world. Guard support, initial active/in-view counts and actual contacts/shots/impacts/deaths are checked. Attempted swings or requested fire alone are insufficient. C4 Blade stays uncovered because the unchanged four-second predicate observed no successful club contact.

F4 uses normal Ice primary, Jackbomb with normal retreat and Blunderbuss. Staff release is excluded because repeat 3 failed its final unchanged impact predicate despite a normal shot. The dismemberment diagnostic uses normal Cards damage then one normal Knife throw, with ordinary funding and cooldowns. Alternate Ice creates a wall but lacks required impact/activity proof, so it is excluded. No predicates, epoch counts or durations were lowered to promote a fixture.

Queen Intro/Birth and two death-FX stages are module-local authored setups whose state advances normally. They are not Queen combat coverage. Hatter/Scream are excluded for losing live supported/alive state; W/E are excluded for missing reproducible 60-second moving-world/mixed-enemy proof. Other maps, water, mirror, other bosses and films are not covered.

The first cold presented scene frame is retained with timed=0; lazy scene/pipeline construction is startup. Settle/encounter clocks begin after that presentation. School first-use setup is a separately labelled segment followed by an outside-window reset. Weapon first-use stalls inside epochs remain in primary distributions. V0 also retains the historical frame 61..180 comparison; those are raw warmup frames, not a replacement for its 60-second live segment.

T retains source and destination in separately named CSV segments. It waits 20 seconds at source, then follows normal run/turn/walk input around the curved approach into `pandemonium$player_start`. No warp or headless shortcut is used in the live run. Exit-request and destination-first-presented milestones bracket replacement. They do not provide a separate first-controllable-frame timestamp.

The 60-frame synchronous CSV export cadence stays unchanged, including labelled export stalls and partial tails. No capture, FTP, installation or external control traffic during retained windows. Retrieve only after app closure. Host verification PNGs are read after the final completed row/flush, outside all retained windows; they show the retained world/actors/FX target without HUD. Console visual/audio verification remains the console owner's work.

## Watchdogs and failure handling

Each active cell/load has a 300-second maximum, including synchronous loads. A separate watchdog thread writes watchdog-failure.json and exits on a blocked load/deadline. Overall campaign limit is 7,200 seconds. There are no unbounded retries. Logs contain scene, frame, age, live map and load/memory milestones.

Wrong map/entry, failed checkpoint, menu/pause/flight, death or unsupported state, missing live simulation or required activity fails a cell. An uncovered cell is skipped with its full reason and never counts as measured gameplay. Inspect results.json and every per-cell result: application exit zero alone does not establish success. Preserve interrupted/failed directories; recovery or relaunch is a separate run. Never splice an interrupted window into a completed one.

## Retrieval and analysis

Preserve identity.json, effective-manifest.json, results.json, per-cell result/initial/final state, validation/epoch/milestone JSONL and every raw CSV. Keep startup effective settings/clocks, package/fixture/tape hashes and renderer/audio/fallback status. Retain load/first-use observations rather than warming them away. Unsupported counters remain explicit/null.

Run `audit-fixtures.py <output>` and `summarize.py <output>` from the coverage checkout. The strict host image audit applies to host runs with verification images; Vita does not emit those host-only PNGs. The underlying states, per-epoch/activity predicates, map/entry and raw rows remain required on Vita. Do not claim a Vita run passed that host-only image check.

Report each repeat and each school epoch first. Separate T source and destination distributions, transition milestone deltas, and first-use/warm epochs. Summary output retains segment statistics and raw CSV hashes. For each window report n, median, p95 = sorted[floor(.95*n)], mean, max, fractions above 33.3/50ms and longest consecutive run above each threshold. Keep every stall/export frame. Pool only after showing individual repeats; distinguish cold/warm caches. Three load observations yield a maximum at this p95 index, not a reliable population percentile.

Compute phase sums and render/unscoped/outer residuals per raw row. Do not add independent phase medians or describe unscoped time as exclusive simulation. FX retains actor submission semantics. GL-call elapsed is CPU driver time, not GPU execution. Host present/HUD attribution and disabled audio do not validate Vita playback or driver behavior. V0 comparison to the old report must disclose the opt-in tail-accounting correction.

Memory is sampled at load milestones and about once per second. Report newlib used/committed/free-arena separately from minimum free vitaGL RAM/VRAM and stream capacity/growth. Sample extrema do not prove a transient peak fits. Allocation-failure counts and largest contiguous block are unavailable/null rather than invented. Host memory/output is not Vita pool evidence.

## Slot duration and restoration

Planning estimate, not a measured console prediction: 16 active segments × 3 repeats = 48 fresh scene loads, plus three T destination loads = 51. Applying the historical village restore median 49.672s to every load is an explicit assumption: 42.22 minutes. Warmup plus retained windows add 414s per pass × 3 = 20.7 minutes; T approach traversal, epoch resets, startup and other-map differences add time. Reserve **75–105 minutes unattended console occupancy**; the hard two-hour watchdog is unchanged. If an individual map exceeds five minutes it fails rather than consuming an endless slot.

After app closure retrieve evidence. Remove the staged startup manifest, restore any previous manifest, eboot, renderer/simulation settings and profile_mode, and compare all preserved hashes/inventories including player saves. Keep new run directories as evidence. Record restoration and failures in the console owner's report.

## Host proof boundary

Mac runs use optimized native OpenGL, audio disabled, separate targets and at most four build jobs. The shared Mac timing lock prevents new peer build/test/render/replay work while owned; launch waits for all 1/5/15-minute load averages ≤3. Any subsequent violation rejects host timing, while raw functional evidence stays preserved. No conversion from Mac timings to console acceptance is permitted.

Manifest-enabled Mac camera projection is fixed at 960/544, independent of Cocoa's recorded physical-window resizing; captured target dimensions can differ from Vita. Normal absent-manifest behavior matches the separately rebuilt Mac pkg-2 baseline in all 109 frozen captures. This parity is native Mac proof, not cross-platform pixel equality. Current package, manifest, host executable and runtime source hashes are recorded in FINAL-REPORT.md and local verification JSON.
