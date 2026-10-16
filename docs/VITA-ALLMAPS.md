# Retained renderer map eligibility

The `vita_renderer` feature reads `private/vita_renderer.json` on host and
`ux0:data/alice/private/vita_renderer.json` on Vita. Without new fields, pkg-2
selection remains village-only. `renderer: "legacy"` always selects legacy
(`reason=configuration`). Otherwise `legacy_maps` takes precedence
(`reason=legacy_maps`), and `retained_maps` decides eligibility
(`reason=retained_maps`). Lists contain exact lowercase map names or `"*"`.

```json
{"renderer":"vita","retained_maps":["*"],"legacy_maps":["hatter1"]}
```

If retained construction returns an error, partially created direct GPU resources
are released, the cumulative `construction_failures` count increments, and the
scene is rebuilt with legacy portal loading enabled. The error and final
`renderer=legacy reason=construction-fallback` selection are logged. Allocator
abort/panic and GPU driver failures that do not return errors are outside this
fallback. Eligibility does not certify visual correctness or console memory fit.

## Host evidence

`--allmaps-entry-check --map NAME [--entry RETURN] --frames 3 --capture PATH`
is a host-only, save-free world comparison. It uses the campaign's fresh
controllers, spawn selection, player placement and default follow camera, at
960×544, fixed seed and material time 1.0. It does not replay opening cinematics
or draw the gameplay cast/HUD. This is an entry world fixture, not a gameplay
or complete fidelity test. It writes `private/allmaps-entry.json` with camera,
transforms, selected renderer, inventory and source feature counts.

`tools/allmaps/run-host.py` pairs legacy/retained for all 39 visits with full
quality at scale 1, then installed low quality at scale 0.75. It asserts identical
camera, material clock, seed and transforms, writes PNG differences and pixel
statistics, and writes a 36-map inventory/table. It checks the Mac timing lock
before every renderer process. The final injected `after-upload` failure checks
legacy fallback and restored Fortress portals. The injection is host-only through
`LOOKING_GLASS_RETAINED_FAIL=after-upload`.

Geometry bytes use the real 52-byte vertex and 2-byte index upload lengths.
Construction buffer peaks track old CPU geometry array capacities coexisting
with pending CPU geometry and uploaded retained payloads, then compare the final
PVS/staging phase. The inventory includes deduplicated PVS lists, including the
unknown-cluster list, and separately projects their indices to Vita's 32-bit
width. Struct staging sizes are native host ABI measurements. These buffer
figures exclude allocator metadata, remap trees, recipes, textures, BSP/collision,
and driver copies. They are not total RAM peaks or console OOM bounds.

`tools/allmaps/check-parity.py` compares all fifteen village reference images to
an archived native pkg-2 binary with unchanged installed PL-def configuration,
frozen environment clock and a copied checkpoint. It verifies PNG bytes and
RGBA pixels. All game data, saves, captures and package evidence stay outside Git.

## Mac handoff, 2026-10-16

Runtime/package source is `0dde4146fcca4279e5a5f81f54b7795233d4721b` on
`vita-allmaps`, based directly on pkg-2 `86792fe`. The feature suite passed
843 tests (zero failures, two inherited ignores); default compilation passed.
All 36 maps constructed retained worlds for the 39 chapter visits. The host
campaign produced 156 paired entry PNGs and per-visit pixel statistics. The
separate village regression matches native pkg-2 for all fifteen PNGs and RGBA
pixels with the installed PL-def config unchanged.

Evidence is in `~/vita-j0/atoz/allmaps/`: `INVENTORY.json`, `INVENTORY.md`,
`IMAGE-DIFFERENCES.json`, `IMAGE-REVIEW.md`, `FALLBACK.json`, `parity/PARITY.json`,
`A-allmaps/VERIFICATION.json`, `PROTOCOL.md`, `CONSOLE-VISITS.json` and `SHA256SUMS`.
Hedge3 is largest: 8,362,944 geometry bytes and 34,232,578 bytes at the tracked
Mac buffer overlap peak. The peak excludes PVS compiler scratch/intern/source
arrays as well as the other resources listed above; it is not a full retained
load high-water measurement. Every map remains subject to console memory checks.

`tools/allmaps/check-layout.py` extracts the exact source struct declarations
and compiles a small object against the native package's dependencies, then
reads the Vita layout constants from ELF. Prepared staging is 592 bytes on Vita
(608 on Mac); Vertex is 40 bytes on Vita. `summarize.py` attaches these ABI
projections and the explicit human entry-image review. Village geometry, Vita
PVS, staging and drop/freeze counts match the pkg-2 coverage census.

The native Mac package has ELF metadata margin 1,604,132 bytes (minimum 3,348).
Pinned imports, regenerated VELF equality, ZIP CRC and packaged SELF/SFO identity
passed. ELF values confirm the 192 MiB heap and 8 MiB main stack; the game thread
keeps the unchanged explicit 8 MiB stack. No console result is claimed.
