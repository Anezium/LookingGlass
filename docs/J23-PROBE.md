# J23 measurable console probe

Based on 86967eaa74b7ff848090d5115a4709b8133e850d. No gameplay or render
submission changes. The light-profiler CSV retains its original 124 columns and
appends `actor_prep_profile_enabled`, `actor_prep_body_ns`, then, in protocol order,
`actor_prep_<category>_<field>` for all 23 exclusive categories. Fields are
`exclusive_ns`, `calls`, `copied_bytes`, `compared_bytes`, `logical_draws`.

A fixed-size snapshot is copied once in `cpu_end`, after the last exclusive span
closes. It stays on that frame's row until next begin-frame completes the wall
interval, before the exclusive state resets. The 60-row ring and snapshot require
no heap allocation during recording. Existing periodic file formatting/export
remains outside recording and its cost stays in wall time. With the probe disabled,
the enabled flag is zero and exclusive fields are unavailable (zero placeholders).
`actor_prep_body_ns` bounds exclusive sums and excludes the deferred framework tail;
`frame_cpu_us` includes only the suffix of that tail after the same CPU boundary.
This tail rule applies identically with the actor probe enabled or disabled.

On Vita use profiler=light with actor_prep_profile=true/false. Defaults stay off.
For host export verification only, set `LOOKING_GLASS_LIGHT_PROFILE=1` and use the
same light config. This opt-in uses the existing neutral-input, unpaused restore
and save-write suppression gates; no replay JSON or fixed dt is required.
Run at least 181 advancing viewer frames to export rows 1-180.
The host opt-in uses the same bounded ring and CSV serializer; it is not console
performance evidence. Validate ordered windows with
`python3 tools/check_actor_prep_csv.py --probe on <window-60.csv> <window-120.csv> <window-180.csv>`.
The on/off arms use one executable and configs differing only in the probe flag.

Host checks: `cargo +stable test --locked --features vita_renderer`,
`cargo +stable check --locked`, `python3 tools/check_gl_confinement.py`.
Snapshot/ring tests check allocation count, field identity, rollover and source
reset. Frozen `--renderer-host-check` captures must match the existing host
reference in both probe modes. No game data belongs in source control.

## Verified host evidence (2026-10-15)

Executable source: `3b850c7c1d064d0df8c080c360c0ac5f64b764cf`.
The feature suite passes 847 tests, zero failures, with two inherited asset cases
ignored. The default-feature check and both ownership compile-fail checks pass.
Both probe flags reproduce all 15 existing Mac J20 final3 reference PNG hashes.
Both export 180 contiguous light CSV frames with 241 columns. Every one of the
115 enabled category fields matches the existing host report for the same frame;
all exclusive sums fit actor_prep_body_ns. The enabled run has 181 distinct
advancing states, with strictly advancing environment clock. Saves are unchanged.
Snapshot and ring recording tests report zero allocation calls.

Native binary SHA-256: `4460029ee57a95be9ed727e0af41b84ed0911915996fcb11ce956dd535c26fe7`.
Evidence: `/Users/anezium/vita-j0/village-retained-world/jalon-23-probe/`, mirrored
to `/srv/fast/projects/alice/jalons/jalon-23-probe/`. See HOST-VERIFICATION.json and
HOST-CHECKS.json. Rejected host attempts are preserved; host-only initialization
and neutral-play setup repairs do not change Vita code paths. These checks make
no console performance claim. Package verification follows in the same evidence.
