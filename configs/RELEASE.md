# Compiled release inputs

`PL-release.json` and `vita-sim/vita-sim-release.json` are the fully explicit
inputs compiled into the production readers. They are selected only when the
corresponding private file is absent (`NotFound`) on Vita. No file is created.
Host missing-file defaults, supplied full/partial files, and invalid/read-error
recovery retain their existing behavior. These inputs do not change serde's
`Config::default()`.

The renderer input preserves the immutable played RC3 settings, with
`log_frame_time_every` changed from 10 to 0. Profiler, frame/worker/GPU probes,
spike capture, replay recording/playback/automatic start and the newer visual or
performance opt-ins remain off. `replay_check_every=30` is inert without replay.
`audio_log_thread=true` preserves the proven asynchronous reporting route; it is
not an audio logging enable. Music threading and all live audio values are kept.

The 17 renderer keys added after the played RC3 input are explicit neutral
values: `world_adjacent_batch`, `exact_blends`, `sky_cube_finish`, `fog_sky`,
`low_fog`, `sky_cube_clear_only`, `sim_sweep_blocked`, `sky_cube_retain`,
`detail_layers`, `sky_depth`, `sky_cube_capture_only`, `audio_combat_preload` and
`sky_cube` are false; `sky_cube_size=256`, `actor_near_cull=0.0`,
`spike_log_ms=0.0` and `sky_cube_refresh=0`. Water normals remain off.

`PL-played-rc3.json` is a test fixture, copied byte-for-byte from the played input
`rc3/configs/PL-rc3.json` (SHA-256
`61f48046a65b578d345a7eeab5704e0f5cd3a7c05dfd361ee4c3576b947a429f`). It is not
a runtime fallback. Tests pin its bytes and every parsed value: the sole delta
from that played renderer state is the deliberate frame-report shutdown.

The simulation input is byte-identical to `vita-sim/vita-sim-all.json`, the
seven-key played owner input (SHA-256
`fa64b0c5b79c52d8e028f7fc9d0d3dd2eec57b7641dfc8f76b34212bdcb3334b`).
Tests also require the release key sets to match every field in the production
configuration types, so adding a field requires an explicit release decision.

These are configuration selection guarantees checked on the host. Fresh-install
startup and play on Vita still require separate hardware qualification.
