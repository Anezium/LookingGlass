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
