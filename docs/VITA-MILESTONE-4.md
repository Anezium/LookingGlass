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
