# Milestone 11: preview orientation and exact retained-pass reductions

Base: `vita-night-j10` at `4f8a8f0`. All three renderer settings default on
and remain in the existing strict `Config` (`deny_unknown_fields`). Restart
after editing `private/vita_renderer.json`.

- `preview_flip: "auto"`: correct the Vita save thumbnail orientation.
  `"on"` requests the same correction explicitly; `"off"` restores J10's
  unconditional encoder flip. Desktop encoding is unchanged in every mode.
- `world_state_cache: true`: reuse actual pipeline, depth mask, complete
  bindings and bit-identical full uniform payloads within one opaque pass.
- `mover_far_lazy: true`: preserve exact mover clocks/poses and defer every
  placement certified safe by both old and next bounds, including the former
  periodic eager tick. This applies even when `mover_far_every` is 1.

## Thumbnail diagnosis

The viewer calls `present_3d` before `Frame::update`. The V correction from
`784c962` has already been applied when a scaled frame reaches the screen;
the grab is not sampling the scaled target directly.

Macroquad 0.4.14 `Texture2D::grab_screen` calls `glCopyTexImage2D`.
The matching VitaGL `2bdbe89` implementation in `source/textures.c:2254`
implements it as `glReadPixels` followed by `glTexImage2D`. Miniquad
`6f89180` `GlTexture::read_pixels` then attaches that copied texture to an
FBO and calls `glReadPixels` again. The pinned `HAVE_UNFLIPPED_FBOS`
row-copy branches in `source/framebuffers.c:761` reverse rows in both reads.
The resulting bytes are already top-down. `Preview::encode` applied a third
vertical flip and inverted the persisted JPEG. Vita now omits that extra
flip in auto/on mode, independently of render scale. Existing persisted
JPEGs are not rewritten; new thumbnails use the corrected orientation.

## Owned world state and accounting

The cache is created after pass/target/viewport setup, any Macroquad flush,
and sky submission, and dies before ending the pass. Alpha callbacks,
actors, new targets, frames and interop cannot retain its state. Pipeline
identity includes the actual low/full shader pipeline object. A pipeline
change forces bindings again for attributes and sampler locations.
Uniform history belongs to each actual shader program, because cull/blend
pipelines share that program. Equality includes signed zero and NaN bits,
checks all 143 floats field by field, and reads no struct padding.

Draw order, draw calls, indices and geometry remain unchanged. When enabled,
existing opaque draw/triangle/requested-byte and world-accounting totals are
published once per pass. `vita_world_uniform_bytes` still counts requested
payloads, including hits. New `vita_world_{pipeline,depth,binding,uniform}_applications`
and `vita_world_uniform_applied_bytes` report actual backend work for A/Bs.
Instrumentation savings must be distinguished from rendering savings.
Disabled mode keeps J10's per-draw applications and accounting.

## Far movers and host validation

The same conservative 256-unit proof checks the old and candidate bounds on
each safe substep. The 1/120-second stepping and exact poses/clocks do not
change. The candidate owns exact on-demand hulls for every collision user;
near placements retain rider/contact/blocking checks and final publication.
The sampled rotation is reused by candidate, eager fallback and carry code.

Six unit regressions cover strict settings/defaults, orientation/JPEG output,
all uniform fields, backend state replay, shader/pipeline ownership and
far/near/hatch/restore/carry/block/query parity. The native
`--village-machinery-check` additionally compares 31 actual asset movers for
240 updates and 44,640 collision pairs with far-lazy on/off. Host world
captures and published counters must match with caching on/off in full and
low quality; retained alpha and save/menu diagnostics cover interop.

Build/package: J9/J10 A-stock, `-C target-feature=-neon`, locked dependencies,
ELF-margin and import verification. Artifacts and evidence live in
`~/vita-j0/village-retained-world/jalon-11/`. Five PL-def presets preserve
all J10 settings and vary only these three new settings.

Vita visual orientation, hardware performance, full-frame image/collision
parity and live profiler hierarchy remain unverified. No device or remote
server contact is needed for this milestone.
