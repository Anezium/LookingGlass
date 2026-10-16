# Reference r3 validation and console handoff

Historical r3 report. Superseded by [the r3b fix round](VITA-REFERENCE-R3B.md)
and its [console protocol](REFERENCE-R3B-PROTOCOL.md); the original r3 evidence remains archived unchanged.

Runtime `7bfd6611cf2e7ad456f059d009770eb3b24047d7` on `vita-reference-r3`, from `vita-reference` `0e245cc`.
The base reference branch is untouched. Merge commits preserve, in order,
allmaps `342d8b3`, actor single preparation **v1** `64b49e3` and sim maps r2
`5beb3a4`; v2 was not selected.

Allmaps merged cleanly. Actor conflicts in `src/render.rs` and
`src/render/vita_renderer.rs` combined the map-selection/fallback constructor
with publication reset and kept both selection and strict actor validation.
Sim's `Cargo.toml` conflict kept both optional diagnostic features. The actor
review nits restore a shared Task reference and remove duplicate blank lines.

## Five r3 changes

1. `retained_maps` defaults to `["*"]`; explicit `legacy_maps` still wins.
2. `actor_single_prepare` defaults to true, retaining v1 prerequisite validation.
3. Invalid map-name entries warn and drop individually without losing other
   fields; valid names absent from the mounted game maps warn at startup.
4. Selection logs distinguish listed, unlisted and excluded maps; renderer
   override and construction fallback keep their separate reasons.
5. `deny_unknown_fields` stays enabled. `configs/PL-def-r3.json` contains exactly
   all 81 runtime defaults, verified by a complete serialization comparison.
   **SELF and config always roll back together.** Older eboots reject new keys.

These are full-quality, scale-1 code defaults; the installed r2 PL-def was low
quality/scale .75 with other overrides. The evidence retains CONFIG-DIFF-R2.json;
matched comparison presets isolate code parity, and cross-config timing is not
an isolated renderer/actor effect. The four sim-map caches remain OFF in r3.

## Validation on the Mac

Full all-feature suites pass with sim switches off and on: **874 passed, zero
failed, three ignored per arm**. The real-data tower fan test passes separately
in both arms (two other inherited asset ignores remain). Default features:
**794 passed, zero failed, three ignored**. Source-boundary audit and eight
source-package boundary tests pass; Cargo.lock and all six game archives remain
byte-identical to their pinned inputs. Own targets and four Cargo jobs were used,
checking the shared timing lock before each build/test/render/replay launch.
No timed host performance campaign or hardware acceptance is claimed.

All fifteen pkg-2 village PNGs match byte-for-byte with actor preparation OFF
and ON under the original matching installed preset (30 comparisons).
Allmaps inventory: 36 maps/39 visits, 156 full/low legacy/retained entry PNGs
and exact structural inventory (load wall times excluded) match the archived allmaps source; injected construction
failure restores the identical Fortress legacy view and portals. World-entry
images exclude gameplay cast/HUD and full cinematic fidelity.
Sim parity: ten maps, 3,900 frames per arm; off/on state hashes match every frame,
and every frame hash also matches sim r2's archived raw logs. Matching window
workload counters are unchanged. These deterministic neutral-input probes are
functional evidence, not traversal, console memory or timing acceptance.

Save tests pass **36 registered restart fixtures plus 17 native mechanism Store
fixtures per sim arm**, using separate writer/reader processes, identical saved
payloads and continued futures. The headless legacy checker also restores the 36
written cases per arm; its fifteen external historical fixtures are absent and
remain untested. The broader
save writer reproduces the inherited garden2 Cat-trigger failure after 188
successful fixtures in each arm; it remains a documented failure, not a pass.

R2 host audio/control/preview checks pass: real host device playback intervals,
bounded village audio cache, handheld input, clean preview persistence/overwrite/
backup/menu, 241 advancing frames without periodic play capture, and the
explicit every-frame witness. Audio, input, preferences, menu, chapters,
inventory and Cargo.lock source remain unchanged from 0e245cc. The only preview
source change initializes the retained view/alpha scheduler in the diagnostic
`--save-preview-check` fixture, whose former village-only assumption crashed on
retained skool1. Actual preview encode/capture/persistence behavior is unchanged.

## Native package and pending console work

Mac native `~/vitasdk`, nightly-2026-10-11, locked dependencies, `-neon`, exact
vitaGL archive/header pins, 192 MiB newlib heap and two 8 MiB stacks.
ELF margin **1,434,264 bytes >= 3,348**; imported symbols,
regenerated VELF equality, ZIP CRC and embedded SELF/SFO equality pass. Production
ELF excludes the host sim probe and optional actor diagnostics.
SELF SHA-256: `b1c13209ce767148971b9e97b612e0cadb7894f6dfb93b793e7a576e3eca5aa7`.
VPK SHA-256: `dfc409fd5042b025e9eae82f2f1d9c259c135db43f505d4f3ddf18b8d11c0d26`.

Deliverables: `~/vita-j0/atoz/reference-build-r3/`, with A-stock, configs,
PROTOCOL.md, FINAL-VERIFICATION.json, raw parity/restart evidence and SHA256SUMS.
PROTOCOL.md retains r2 listener/control/thumbnail checks, all 39 cold chapter
visits, then at least five natural exits including **utemple -> potears1** and
one save/reload in each of ten chapter groups. Sample newlib used/free and
vitaGL free throughout transitions; stop if newlib used exceeds **180 MiB**.
The next scene is built while the old one lives: sparse samples leave its peak
unresolved. The owner performs this protocol; no console/server work occurred.

Prepared sim-on commit `b2f48cbaff68f8dc61dbe05a8ffae09b9c86fdc5` is pushed on
`vita-reference-r3-sim-on-r2` and **not applied** to r3. One-command adoption after
console acceptance: `git cherry-pick b2f48cbaff68f8dc61dbe05a8ffae09b9c86fdc5`. It changes matching
runtime defaults, preset and assertions together (a coherent multi-line patch,
not a literal one-line diff); build a new package after adoption. The earlier
candidate/reference package remains archived/superseded without rewriting refs.
