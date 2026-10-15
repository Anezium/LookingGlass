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
