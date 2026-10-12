# Milestone 3.1: shader lifetime

The milestone-2 retained world releases `Scene.materials` and `bound_materials`.
`render::WORLD_MATERIALS` was a weak-only cache; dynamic effect passes could lose
their last material reference after each frame and call `load_material` again.
Macroquad 0.4.14's `GlPipelineGuarded::drop` clears its slot through
`PipelinesStorage::delete_pipeline`; that function does not delete the backend GL
shader/program. This explains repeated compilation and unbounded program retention.

Each scene now keeps its used effect materials alive. Immutable definitions remain
weakly shared between scenes, so they do not permanently consume Macroquad's 32
slots. Milestone 3's direct actor renderer already owns one shader and caches its
pipelines by blend/depth policy, deleting backend resources when its owner drops.
This fixes frame-by-frame recreation; it does not repair Macroquad's backend
resource deletion during scene destruction or prove long campaign stability.

Host full-frame reference-pose check: 60 warmup + 120 following frames.
A host-only `LOOKING_GLASS_REPRO_M2_LEAK=1` bypass reproduces the old lifetime with
`vita-interop-off`: 5 shaders and 5 pipelines each frame, 600 of each after warmup.
Fixed legacy, bridge, and direct paths create 0 of each after warmup. The direct
path creates one shader and five pipelines in its first frame. The actual shader
alpha/depth/appearance/light fixtures also pass. These are host checks, not Vita
heap measurements.

All LG material creation calls and direct miniquad shader/pipeline creation sites
are counted. Native reports include zero-valued `shader_creations` and
`pipeline_creations` series. Framework startup shaders are outside this scope.
`heap.frame_end_bytes` samples newlib allocation at the next frame boundary;
`heap.delta_bytes` includes profiler allocations and must be read with the series
and existing ten-second memory logs. The profiler now includes dynamically named
`vita_*` spans in its per-frame arrays, with zero backfill for late labels.

`world_viewport_scale` defaults to 1.0; 0.5 draws a 480x272 world viewport on Vita.
It applies only to renderbench with `world_only=true`, equally to legacy and
retained world draws. Full-frame actor/HUD rendering stays at full size. The actual
viewport is recorded in `vita_renderer.view.viewport`. GPU 222 remains a separate
named, GPU-only runtime experiment. No console PVS verification/compiler runs.
