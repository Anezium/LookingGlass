# Milestone 8 renderer experiments

All switches are strict fields in `vita_renderer.json`, under
`ux0:data/alice/private/` on Vita or `private/` in the host working directory.
Missing fields use defaults; an invalid file falls back to the complete defaults.
Build with `--features vita_renderer`. Without that feature the original paths remain.

`bone_palette` (default **false**) expands bone rotations and scales once per
skinned draw, including the model rotation. Weight order is retained but float
evaluation changes, so this is a tolerance-tested visual option. Animation,
attachment tags and gameplay retain their original transforms. This switch does
not change actor lighting.

`--skin-bench --data /path/to/base` runs without a window using the real Alice
SKL and sixteen run animation poses. Eight alternating rounds each skin 5,000
draws, including palette setup, non-unit bone scales and negative model scale.
The log gives median ns/vertex, all samples and maximum coordinate error.
Host x86 results are only a relative indication; Cortex-A9 timings require Vita.

`LOOKING_GLASS_FRAME_PROFILE=/path/to/output` now exports host `skinning`,
`frame_characters` and its `characters_*` decomposition, including positions,
normals, shading, layers and submission. Shared helpers outside the character
block contribute only to `skinning`. Native spans retain their existing owner
scopes. Profile medians are per invocation, not GPU timings.

Adapted MIT engine code: NextOs-Ports/Looking-Glass-NextOS 15779e4 (felc18-blip).
No NextOS platform bridge, Android implementation or ETC2 code is used.

`skin_frustum_cull` (default **true**) unions bone-local boxes built from positive
normalized SKL weights after transforming by the current facial/bone pose, bone
scales and signed model scale. Numerical slack covers weighted sums and both
quaternion and matrix paths. Invalid or empty boxes fail open. It skips only
visual skinning/submission. Facial posing, blinking, tags, sounds, footing and
attachments continue; attachments use their own bounds. Frozen-capable rigs and
active electrical/Rage overlays preserve current vertices because their shaders
expand beyond base bounds. Shadows, reflections and portals retain their existing
updates and use the actual pass projection. A rejected ambient refresh invalidates
its visual cache. Route columns add `skinned_draws_total` and `skinned_draws_culled`,
with aligned per-frame raw rows and mean/peak summaries; the native frame profiler
uses the same names. These count model draw invocations, not surface layers.
Adapted from NextOs-Ports/Looking-Glass-NextOS 51b6224 (felc18-blip).

`fast_frustum` (default **true**) uses center/extent homogeneous clip-plane tests
and the no-transform bounds shortcut in both legacy and retained world paths.
The original eight-corner path remains available with false. Uncancelled arithmetic
magnitudes set numerical slack; ambiguous and nonfinite boxes stay visible.
Tests cover 10,000 randomized boxes, reflection/oblique matrices, near-plane and
camera-crossing boxes, wholly behind-camera geometry and identity bounds.
Adapted from NextOs-Ports/Looking-Glass-NextOS 668de14 (felc18-blip), render code only.

All quality levers default **off** (optional numeric fields default **null**):
- `flat_actor_light`: constant 217/255 actor RGB, skipping triangle normal rebuild.
  Fullbright stays 255; alpha/skin positions stay live. Protected expanding-overlay
  scopes keep normals; normal-dependent texture effects are intentionally simplified.
- `max_texture_dimension`: null or 16..512. Immutable/static uploads through the
  LG helper use the existing 2x averaging path; Vita's normal 512 ceiling remains.
  Streaming movies retain their original update dimensions. This includes static
  UI images using that helper, so 128 can blur menus. Texture cache
  keys still include actual pixels/dimensions. GPU render targets are unaffected.
- `draw_distance_cap`: null or 128..30000 world units. Limits the live 3D camera's
  far plane and inherited portal projections. Simulation/audio distances stay live.
- `simple_fog`: removes fragment fog while retaining background colors and the
  world-position varying needed by lights. `simple_dynamic_lights`: removes shader
  dynamic-light contribution while retaining gameplay, selected lights, flares and
  contact shadows. Shader cache keys change with shader source.

`configs/vita_renderer.lean.json` enables all five plus palette skinning and the
conservative switches. It reuses `render_scale:0.5` (480x272 on Vita), preserving
aspect and native HUD presentation. No additional render-scale implementation or
NextOS resolution is introduced. Restart after changing any configuration file.
Inspired by NextOs-Ports/Looking-Glass-NextOS 005adaa, 15779e4 and 0c716a9
(felc18-blip), MIT engine code only; visual gains/quality require console A/B.

`gpu_index_dedupe` and `gpu_uniform_dedupe` (both default **false**) act only on
LG-owned retained actor/FX buffers and programs. Identical u16 indices can stay
resident while changing vertices still upload. Capacity growth replaces buffers
and clears history. Uniforms compare every float's bits, including signed zeros;
they skip only consecutive identical full payloads of the private actor program.
Every finish/view and intervening world draw interrupts the uniform cache. Textures,
bindings, pipeline/depth state and every draw still apply. Existing full FX upload
caching now also compares vertex bits exactly. No generic Rust object bytes or
padding are read. No dependency change or fork patch is needed for this local scope.

`gpu_state_probe` (default **false**) records exact repeated upload/uniform candidates
without enabling skipping. It keeps vertex history and adds overhead; turn it off
for timing. Route rows add four aligned `gpu_probe_*` request/identical columns.
Native frame counts add `vita_fx_index_uploads_skipped` and
`vita_fx_uniform_sets_skipped`; upload/uniform byte totals reflect actual calls.
Adapted/inspired MIT code: NextOs-Ports/Looking-Glass-NextOS d59b16f and 668de14
(felc18-blip). The external Miniquad generic stream cache and Macroquad uniform
batch-breaking change are deliberately not ported: the former requires a fork and
the latter shares queued material state. This narrower cache has explicit owners.

`route_world_wish` (default **false**) is an input-only replay diagnostic. It
recovers a tape's world wish from its reference heading while leaving natural
camera changes intact. It avoids neutral physics ticks introduced by the existing
bounded camera recentering path. No player pose, scene flag, physics or clock is
edited. Normal play is unaffected; the morning automated route presets enable it.
