# Gameplay qualification of the exact static acoustic cache

Prepared on the Mac only. Console execution is pending. The cache stays default-off.
Base engine is r6, branch vita-acoustic-cache-r6-20261024, at 73e303c49e62338c6cc3b603259eb11ae02bee19.
The prior idle benefit is accepted context; this package does not repeat or extrapolate that wall gate.

## Existing mechanisms inspected

- `src/route_replay/mod.rs` accepts only new_game or save:slot1, cinematic gates entry/knife_cat, and the viewer requires village cinema state. It cannot replay Hedge3 or Pandemonium without changing the route subsystem.
- `src/frame_profile.rs` fullbench freezes the listener, accepts 60 warmup plus 120 measurement frames and suppresses saving. It cannot qualify moving gameplay, a native save load or normal autosaving.
- `src/viewer.rs` supports regular pad movement, native Load/Save and chapter menus, console map/restart/load commands, quick F5/F9 saves, natural exits and Retry. No generic original demo playback or general map-independent deterministic benchmark input was found.
- All native save restoration goes through `apply_loaded!`, scene replacement and `Audio::load`. Chapter and natural exits also call `Audio::load`; `transition_memory::replace` samples before and after dropping the old scene. Cache identity is additionally checked every frame/query.
- Normal quit writes Auto and companions; transitions/checkpoints can write other saves. Whole save-directory inventories and all initial bytes must be restored, including removal only after two retained readbacks of any new save.

## Choice

Use normal gameplay, `profile_mode=full`, with a fixed pilot sequence: left-stick x=64 for 2 seconds, then x=192 for 2 seconds, y=128, repeated 16 times. The existing driver uses unsigned 0..255 axes centered on 128, as confirmed from retained local pad command receipts. This is 64 seconds of commanded input, with no look, cheats or new simulation/input code. Each pulse releases all inputs. The owner inspects the starting and final images. The native rows must independently prove at least 60 seconds of active gameplay AND 60 seconds of moving listener samples, with at least 64 units of listener path and no gap over one second. Host wait duration is not a qualification proof. A blocked/cinematic/unsafe entry fails; do not change the sequence after viewing results. A route change needs a separate amendment and a new matched pair.

Frame rate and collisions make pad replay reproducible rather than deterministic. Fresh identical entry/save state, same pad commands, matched first listener pose within one unit, path/displacement and duration receipts make that limitation reviewable. Describe path divergence in the human report; do not claim sample identity for the live runs.

One new common SELF is required: r6 does not export moving-window counters or reset occupancy. The added `audio_cache_qualification` boolean defaults false and is omitted from serialized config when false, preserving r6 default config record bytes. Disabled mode performs no recorder initialization, file IO, heap sampling, logging, cache inspection or self-check recomputation. It adds disabled branches, so binary identity and identical execution timing are not claimed. Audio sample bits and simulation/input behavior are preserved and checked on the host.

## Recorder and self-check

Opt-in commands start/stop labelled windows with increasing IDs. The command file is polled only during existing paused/menu frames, including the native modal menu. No command-file IO or JSON export runs during gameplay measurement. Native acknowledgements prevent blind retries. Each window reserves 8,192 frame rows before resume; overflow rejects the run. A launch accepts at most 16 windows. Each row contains monotonic time, exact scene identity, actual listener coordinates, prepare_acoustics elapsed ns, sampled newlib heap, per-class queries and hit/miss/eviction counts, check count and mismatch count. A bypassed cache counts each executed static query as a miss; eviction and hit remain zero.

Timing encloses the same acoustic kernel in both arms, including lookup/miss and current mutable completion; it excludes heap sampling, row publication and export. Overall audio health includes observer overhead. Diagnostic timings are descriptive, never substituted into the r6 idle wall gate. Rows are retained until clean quit, so all windows reserve the same capacity in both arms; report this recorder allocation separately from the approximately 31 KiB cache.

Self-check is off by default and `check_stride=0` during movement and post-load heap windows. Separate five-second stationary check windows set stride 64, recomputing the immutable static sub-result on the first and every 64th cached hit. Compare fraction bits, all normal bits, start_solid and all_solid. Always return the original cached trace and run current mutable completion once. Any mismatch rejects qualification; zero sampled hits checked is UNKNOWN, never a pass. Encoding occurs after stop, and final finish/reset evidence is exported after quit.

Reset records count valid entries before/after clearing, old immutable scene identity, old Arc reference count, pin presence after reset and heap after releasing the pin. Load records sample heap after resource/audio installation. Existing scene replacements gain before/after event samples. For every warm replacement/load, the audit requires zero valid entries, no pin, the old scene matching the previous window, a different new scene and release of the last outgoing static-geometry reference. The table has a single pinned scene, so zero occupancy proves no old-scene key survives. This is direct state inspection, not an inferred hit-rate change. Sampled heap at every phase and subsequent active frames is paired A/B.

## Fixed reading rules

Zero sampled self-check mismatches, zero underruns, existing audio health gates passing, sampled heap <=180 MiB and every paired B-A window/transition/load sample maximum <=+1 MiB. Native resets must be observed at every scene replacement and reload; at least one warm cross-map load and one warm same-map native save reload are required. Hit rates, evictions, listener paths and prepare_acoustics mean/median/p95/max are descriptive. late_callbacks remain report-only and are exported per reporter window and as the sum of reported counts. Loading before the first window and shutdown after the last are separate descriptive populations; all intervening windows, menus, exports, transitions and reloads are health-gated together. No observer subtraction, discarded frames, retries to pass or automatic promotion.

Death is optional and deliberately excluded from this sealed protocol. The two reproducible native Auto loads satisfy the death-or-reload requirement without inventing a safe death route. If later requested, a death trial requires a separate amendment, preserved retry/save identity and its own matched cache-off/on sequence. Nothing in this package sets the cache default true.
