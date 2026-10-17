# Console campaign checkpoint starts

The console test build reads `ux0:data/alice/private/vita_start.json` once at
startup. With no file, startup is unchanged. A request is removed before JSON
validation or restoration; invalid requests are logged and fail the launch.
All save-store writes, including automatic, manual, finale and completion writes,
are blocked for the entire test session. Existing player saves are never used as
a fallback if a requested checkpoint is missing or fails its hash check.

Examples (upload the selected JSON as `vita_start.json`):

```json
{"campaign_from": 6, "frames": 1, "capture": "checkpoint-skool.png"}
```

```json
{"map": "skool1", "entry": "skool1_start2", "frames": 120}
```

`campaign_from` accepts visit numbers 1–39 or upstream `map$entry` selectors.
It is exclusive with `map`/`entry`. Checkpoint filenames use the upstream
`NN-map-first.json` / `NN-map-return.json` naming and are placed **directly in
existing `private/`**, alongside the request: no console directory creation is
needed. They contain the upstream `campaign-chain-visit` envelope, not a native
save. Only entrance checkpoints (route tick zero) can use this native bridge.
Absent frame limits allow interactive play; checkpoint restores initially pause,
so use P/click (the mapped Vita button) to resume. Fixed-frame captures record
presented frames, not deterministic simulation ticks. Capture names must be simple
`.png` basenames, require a positive frame limit and must not already exist.

The log records the consumed request's SHA-256, selected visit and a full
`checkpoint_restored ... state_hash=` from `Route::resume` + `Route::state_hash`
before simulation. A mismatching hash refuses startup. The existing native
save-chain bridge then supplies Alice and the native cast from local assets;
`native_restored ... native_state_hash=` hashes the actual restored game objects.
These two hashes describe different representations and must each be compared
with its corresponding host value. A headless route hash alone does not certify
native actors, rendering or subsequent gameplay.

For Mac verification, `--vita-start /absolute/private/vita_start.json` exercises
the same consumption, restoration and save-write protection. The host never reads
a request implicitly. `tools/generate_campaign_checkpoints.py` runs an unchanged
binary in isolated working directories, first from strict New Game state and then
with explicitly labeled chapter fixtures for visits that strict play cannot reach.
Its manifest distinguishes proven carried checkpoints from staged fixtures.
Game data, checkpoints, packages, logs and local paths belong in private evidence,
not the source repository. See the evidence `PROTOCOL.md` for the pilot sequence,
per-visit expected hashes and restoration of the human's r3f package.
