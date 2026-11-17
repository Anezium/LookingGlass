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

RC8 additionally enables three keys measured on the Vita on 2026-11-14:
`sim_flight_memo` (route search 206 to 38 ms), `ant_corpse_opaque_shader` and
`ant_corpse_triangle_cull` (the corpse GPU wait is gone). `sim_sweep_any`, the
`diag_skip_*` keys, the spike window and every other opt-in stay off.
`PL-rc7-release.json` keeps the RC7 release bytes for comparison; the RC8
release differs from it only in those three keys.

RC9 additionally enables `water_draw_join`, measured on the Vita on 2026-11-15
(walk2 43.2 to 38.7 ms, fight2 median 37.4 to 33.6 ms; `docs/validation`,
a2-29 session, on 7c089be). The join requires `world_shared_buffers` and
`world_parameter_reuse`, both already on, and stays exclusive with
`water_stage_merge`, which remains off. `PL-rc8-release.json` keeps the RC8
release bytes for comparison; the RC9 release differs from it only in that key.

RC10 additionally enables `actor_consumer_frustum_cull` (the skool1 actor cull:
actor median 53.7 to 38.7 ms, worst 55.4 to 40.1, draw 52.1 to 37.1 on the Vita
on 2026-11-15) and `fx_unlit_exact` (the hatter2 unlit FX path: median 49.7 to
43.2 ms, worst 54.2 to 45.2). Both were reviewed with fixes applied; the cull
requires `skin_actual_consumers`, already on, and does not need
`actor_opaque_shader`, which stays off. `PL-rc9-release.json` keeps the RC9
release bytes for comparison; the RC10 release differs from it only in those
two keys.

RC11 keeps `load_texture_share` off. The merge brings the sampler-keyed shared
upload and the boot loading screen, but enabling the key fails the nine-map
oracle against RC10: `render_fx::register` keeps its material Surface table keyed
by the base skin texture handle, and sharing makes that handle no longer unique
per material, so materials that resolve to one image exchange their stages. With
the key off this head is draw-for-draw and pixel-equal to RC10 on all nine maps.
`loading_screen` was already on; `texture_cache` stays `off` and `pvs_cache`
stays `false`, so both disk caches remain off. `PL-rc10-release.json` keeps the
RC10 release bytes for comparison; the RC11 release keeps every RC10 value.

RC12 turns `load_texture_share` on. The merge brings the material identity fix
(each project mesh owns its material recipe separately from its GPU texture, and
an image-only load-scoped stage cache preserves the old local-base image alias)
and the review-2 upload identities, which keep merge eligibility independent of
the shared handle. On the A6 branch at 6fd8ada that pair is 9/9 pixel-equal with
zero packet count and order differences, both against RC11 and between the key
off and on. `loading_screen` stays on, `texture_cache` stays `off` and
`pvs_cache` stays `false`, so both disk caches remain off; no other value moves.
`PL-rc11-release.json` keeps the RC11 release bytes for comparison; the RC12
release differs from it only in that one key.

RC13 keeps `alpha_model_pvs` off. The merge brings the model alpha PVS cut: a
world alpha group whose recipe carries a model and has no motion and no vertex
deform is tested against the map's own PVS over its placed bounds, and is
skipped when the descent proves no visible non-solid leaf cell can contain it.
The descent fails open outside the compiled world box, on cycles and on unusable
data; the plane support is evaluated in f64 and widened from the term
magnitudes; the placed box carries a separate outward allowance. The key is
default off, and the release writes it explicitly false: in the release oracle
against RC12 it removes visible, alpha-tested `textures/glass/glass_pane1sh.tga`
panes on hatter2 from scripted frame 155 (eleven of the eighteen culled groups
have visible samples; 2,736 pixels differ at frame 165), so it is not a release
default yet. `PL-rc12-release.json` keeps the RC12 release bytes for comparison;
the RC13 release keeps every RC12 value.

RC15 turns on the three potears1 simulation levers (`sim_snark_move_skip`,
`sim_snark_sight_blocked`, `sim_pool_name_match`) and sets `audio_stats_log`
false. The levers are exact work removals measured on the Vita on 2026-11-16 and
checked against the RC14 configuration frame by frame on the host; the periodic
audio statistics are release diagnostics, not gameplay state. Because a fresh
install has no `ux0:data/alice/private/vita_renderer.json`, this compiled input
is what those users run: it must carry the same four RC15 values as the shipped
play config. `PL-rc15-release-play.json` is this fallback plus only the two
measurement keys (`replay_play=a2-04.lgtape`, `log_frame_time_every=10`), and the
unit tests enforce that relationship. `PL-rc13-release-play04.json` keeps the
RC14 package bytes for comparison.

RC16 turns on the two main-thread audio fixes (`audio_loop_resident`,
`audio_cue_prefetch`) and the exact actor shader specialization
(`actor_discard_free_shader`). All three were measured on the Vita on 2026-11-17
and checked exact on the host against the RC15 configuration, frame by frame and
sample by sample. The owner approved the picture-changing
`actor_frontface_cull` on 2026-11-17 after the host screenshots; it is on in both
shipped inputs, and the host oracle records the resulting picture differences per
tape for the release report. Because a fresh install has no
`ux0:data/alice/private/vita_renderer.json`, this compiled input is what those
users run: it carries the same eight RC15/RC16 values as the shipped play config.
`PL-rc16-release-play.json` is this fallback plus only the two measurement keys
(`replay_play=a2-04.lgtape`, `log_frame_time_every=10`), and the unit tests
enforce that relationship and pin the four RC16 keys in both shipped inputs.
`PL-rc15-release-play.json` keeps the RC15 package bytes for comparison.

The 20 renderer keys added after the played RC3 input are explicit neutral
values: `world_adjacent_batch`, `exact_blends`, `sky_cube_finish`, `fog_sky`,
`low_fog`, `sky_cube_clear_only`, `sim_sweep_blocked`, `sky_cube_retain`,
`detail_layers`, `sky_depth`, `sky_cube_capture_only`, `audio_combat_preload` and
`sky_cube` are false; `sky_cube_size=256`, `actor_near_cull=0.0`,
`spike_log_ms=0.0` and `sky_cube_refresh=0`. Water normals remain off. The four
RC8 and RC9 keys are true; `water_draw_join` is the only one added by RC9,
RC10 adds only `actor_consumer_frustum_cull` and `fx_unlit_exact`, both true and
both measured, RC11 changes no measured value, RC12 enables only the shared
upload and RC13 enables no measured value: the model alpha PVS cull stays off.

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
