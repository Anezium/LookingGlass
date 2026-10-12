# Milestone 4 progress

Base: e6e2d70, branch vita-renderer-j4. Console packages 3 and 3.1 remain immutable.

## Step 1: normal resume loads once

Menu Continue/Load and explicit --load now build the restored scene before the
fresh-game branch, then install it directly. The bench's prepared scene is reused
as before. Normal resume remains paused; bench resume remains active. Restore
failure keeps the paused fresh-scene/error behavior without retrying restoration.
Gameplay, progression, save data and Restored::build are unchanged.

Host normal --load auto, feature vita_renderer, three rendered frames: exactly
one `Loaded gvillage`, one `[lg/vita-renderer] loaded`, and
`[lg/startup] restored scene reused map=gvillage source=load`.
Evidence: ~/vita-j0/village-retained-world/jalon-4/host/single-load-normal.log.
This exercises the shared startup branch reached by menu Continue/Load; menu
interaction and Vita timing still need the pilot's confirmation.

## Step 2: route coverage

Maps crossed: **gvillage only**. Arrival ends at alice_cat_talk2, the authored
Hallway Blade pickup starts knife_cat, and that scene ends at alice_knife_node.
All are local village nodes (src/village/cinema.rs); the proposed movement ends
before village_route.rs's later Pandemonium exit. The route must reject any map
transition and must not use flight, warp, recovery or debug scene completion.

The feature selects the new renderer for gvillage. Startup now logs the selected
map/backend. Other maps explicitly fall back to legacy; Pandemonium is outside
this delivery. Its cost has not been measured. The old village world-only median
517.1 ms is not a measurement of another map and must not be assigned to it.

Next: renderer loading/cache and unattended route replay. Target package 20:30
CEST; steps 1–3 may be handed off separately if replay is unfinished at 20:00.

## Step 3: loading and cache

The old milestone-2 renderer took 138 s per build on Vita. That total has no
separate phase breakdown; do not invent one. Milestone 3 already logs recipes,
chunks, pipeline/uploads and PVS. Its source-based PVS computes leaf bounds once
per relevant group instead of per group per cluster. Milestone 4 additionally
sorts opaque material order once, instead of sorting every cluster's list.

PVS cache v4 includes the mounted corpus identity and every actual PVS input.
The existing corpus identity is SHA-256 of override-resolved PK3 entry names,
CRC and size metadata, **not raw archive SHA-256**. Visibility/groups are also
hashed from their loaded values. Geometry/chunks are constructed more cheaply,
not persisted. No save fingerprint algorithm or format changes.

Cache reads are bounded to 16 MiB, checked by SHA-256 and validated ranges/counts.
Writes use a private temporary file, sync, close and rename; any failure or
corruption falls back to fresh lists. Paths are under private/renderer-cache,
never player saves. Cache remains off by default pending native SD results;
pvs_cache=true is a runtime option. Assets memoizes its unchanged fingerprint
per read-only mount, avoiding a second archive metadata scan during scene build.

Host proof: source lists match the old compiler for unknown view and all 2,139
clusters; hit loads 8.4 ms; damaged checksum is rejected and rebuilt (42.4 ms).
The cold verification's 1,256 ms includes the deliberately slow old compiler and
is not a cold-build performance result. Native phase timing and SD cost remain
for the pilot; none of these host numbers are Vita measurements.
