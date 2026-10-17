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
Native Continue initializes equipment-derived player flags, including the late-game
Turtle Shell breath flag, before the native hash is taken. The checkpoint and its
Route hash are unchanged by this preparation.
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

Mac verification on r3f (2026-10-17): all 39 entrance files restore in separate
native processes with equal stored Route hashes, equal restored/installed native
hashes, consumed requests, captures and byte-identical save sentinels. All 39
registry fingerprints match unchanged r3f; malformed requests, missing/wrong/tampered
checkpoints and direct map/entry starts are checked separately. Simulation code and
Cargo.lock are unchanged. The paused village entrance includes its authored black
cinematic fade; the pilot protocol also requires interactive visibility/control
checks after resuming each representative visit.

The unchanged r3f strict chain and native campaign replay both stop at visit 02
Pandemonium's 640-versus-460 slime-tick assertion with the recorded input family.
Only entrances 01–02 are strict carried states; the other 37 generated entrances
retain upstream's `fixture=true` chapter-loadout label. They are useful test starts,
not evidence of earned campaign completion. The local manifest records each
process, intentional post-entrance stop, state hash and file SHA-256. The full save
sweep also retains r3f's known Cat-trigger fixture failure after 188 writes;
unwritten later registered cases are not claimed passing.

The final source passes 902 all-features and 819 default Rust tests (seven
asset-dependent tests ignored), plus the Python/source-package checks. All 188
produced save payload checksums and serialized future hashes match unchanged r3f
byte-for-byte; the 154 original save cases are included. Source audit also passes
after allowing the ten already-committed r3e/r3f authored presets by exact filename.
The evidence package includes SHA-256s, five representative pilot requests and the
original human r3f SELF/VPK/settings handback kit. No console connection or install
was performed during implementation; the protocol requires existing directories
and uses flat checkpoint uploads, with no MKD.
