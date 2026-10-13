# Scaled world depth diagnostic

Base: `4643b2b7170037e8a8865a16868834efe943ccf7` (J15). No device access.
`depth_sentinel` and `scaled_depth_fix` are strict Boolean renderer settings,
require a restart, and default to false. The fix is experimental because static
control flow does not prove an actual GXM scene break or lost depth contents.

## Frame trace

References to dependencies below use Miniquad `6f89180`, Macroquad 0.4.14 and
vitaGL `2bdbe89`. The packaged SDK archive/header identities match J15. Driver
source is inspected locally; it is not copied or linked into this change.

1. Macroquad starts the frame on the native target and clears it. The viewer's
   `prepare_3d` (`src/viewer.rs:3278`) then clears depth to one on the scaled
   720x408 target at scale 0.75, including colour, or clears just depth on the
   native target at scale 1.0. `scale.rs:129-143` forces the write mask before
   clear. Target allocation is once per size; desktop explicitly attaches
   Depth32, Vita uses the driver's implicit FBO depth surface. Target setup and
   creation of the upscale material happen before world submission.
2. `set_3d_camera` selects that same target for queued Macroquad geometry
   (`scale.rs:145-172`). `begin_fx_view` flushes previously queued geometry before
   direct world draws (`render_fx/retained.rs:31-37,54-55`). That is a Macroquad
   batch flush, not GL `glFlush` or `glFinish`. Camera portals are prepared before
   this boundary (`viewer.rs:3374`); their renders/readbacks precede the world.
3. Retained world `begin_3d_pass(Nothing)` rebinds the selected target, viewport
   and scissor, then draws sky and opaque groups (`vita_renderer.rs:1047` onward).
   Sky disables depth testing; opaque groups apply LEQUAL/test on/mask one.
   Neither Nothing nor a texture/buffer bind clears depth.
4. The candidate boundary is `vita_renderer.rs:1202` -> `end_world_pass` at
   `:337-341`: Miniquad `graphics/gl.rs:1729` binds the default FBO. Its end-pass
   also unbinds vertex/index buffers. Scale 1.0 stays on the default target.
   With `scaled_depth_fix=true`, a scaled direct retained world view leaves its
   target bound here. It adds no draw, copy, allocation, depth clear or flush.
5. Characters/Steam enqueue retained packets. World-alpha preparation returns
   without drawing or switching target (`vita_renderer.rs:1010` onward).
   `finish_world` performs its Macroquad batch flush (`retained.rs:817`), then
   prepares/uploads FX buffers; its `begin_3d_pass(Nothing)` at `:919` selects the
   same target before opaque actors and sorted world-alpha/FX callbacks.
   Each pipeline reapplies its authored LEQUAL/EQUAL and actual write mask.
   Queued shadow geometry uses the scaled camera too. World callbacks do not
   switch targets. No production readback, blit, GL finish/flush or clear was
   found between world opaque and the first world FX draw.
6. FX finish restores mask one and binds default (`retained.rs:957-962`). Only
   afterwards does the viewer intentionally clear depth for the weapon layer
   (`viewer.rs:3559-3564`). Upscale, save preview readbacks and HUD follow all
   world/weapon draws (`viewer.rs:3577` onward). Upscale is a normal colour quad,
   not a depth copy. Single-sample targets have no resolve blit. Swap ends the
   frame; neither swap nor the thumbnail belongs to the world-to-FX interval.

Miniquad begin-pass (`graphics/gl.rs:1656-1697`) binds an FBO and resets viewport/
scissor; Nothing never clears. vitaGL bind (`framebuffers.c:307-319`) changes
pointers only. Real GXM scene boundaries occur on a subsequent draw/clear with
another active FBO (`gxm.c:577-587`, `draw.c:172`, `misc.c:651`), readback
(`framebuffers.c:687-688`), blit (`:825`), GL finish/flush (`gxm.c:970-992`), query
result waits (`buffers.c:179-180,215-216`) or swap (`gxm.c:810,939`). No query waits
are used here. Merely unbinding and rebinding without one of those operations
is not evidence of a scene break. Texture uploads normally leave scene ownership
alone; replacing a render-target texture can dirty/reallocate its framebuffer
and depth (`textures.c:1822-1835`). The reused scaled target is not uploaded or
replaced between world and FX. GPU buffer uploads do not end a scene.

Thus the target-unbind boundary is identified, but an actual depth-loss boundary
is **not proved**. The switch removes exposure to operations on the temporarily
bound default target; it cannot guarantee depth retention after an independent
readback/flush or partial rendering inside the driver. Do not enable it globally
on the strength of a desktop GL pass. No SDK rebuild, material EQUAL changes,
weapon-depth-clear changes, or particle simulation changes are included.

## Colour sentinel

An opaque blue near-plane panel is drawn at the production world-pass exit.
After all world/FX submissions, three panel cells test a wholly rear additive
red sprite, a front additive green sprite, and two front alpha layers drawn
near-then-far. Expected RGB is blue, cyan, and approximately (128,64,64).
The last cell requires both alpha layers to survive without depth writes.
The panel occupies NDC x [-0.96,-0.36], y [-0.96,-0.76], intentionally covers
part of the picture, and repeats every frame with a direct opaque world pass.
Depths are inside the clip range, away from coplanarity and cell edges.

Only three RGBA pixels are read, after all world/FX draws and before any weapon
clear or upscale. Each frame logs raw colours, errors, rear/front visibility,
read-only alpha verdict, scale and fix state. The first actual effect packet
logs FBO, viewport, test/mask/function and query errors after its own pipeline
and mask have been applied. The sentinel does not require depth readback or
unsupported attachment queries. A read error is a failed diagnostic, never a
successful occlusion result. Readback itself can restart a scene, so sentinel
runs are diagnostic and cannot measure ordinary rendering performance.

## Host and device interpretation

`--vita-scale-check` adds 120 frames with the actual world shader drawing opaque
geometry in a separate world pass, the shared production exit helper, rear
additive FX, and a visible front alpha sprite. The existing moving actor/FX,
upscale and native HUD checks remain. Run in separate processes at 0.75 and 1.0
with fix off/on; enable sentinel at 0.75 for its positive checks and deliberate
mid-boundary depth-clear negative control. The negative control must fail the
sentinel and subsequent ordinary frames must recover.

Desktop success proves shader/state/target correctness on host GL only. Prior
console scale isolation is measured; this change's Vita occlusion and GXM scene
preservation remain unmeasured. Expected fix cost: approximately 0 ms (one fewer
FBO/buffer unbind sequence), with no numerical Vita timing claim. Sentinel cost
is four small transparent draws plus the opaque panel and synchronous readback;
its GPU stall time is unknown. Console comparisons follow the other sessions.
