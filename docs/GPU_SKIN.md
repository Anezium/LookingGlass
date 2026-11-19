# GPU skinning (default off)

Two opt-in keys move actor skinning from the CPU to the vertex program:

- `gpu_skin_actors` admits every skinned NPC and Alice surface that fits the stream below.
- `gpu_skin_ants` is the earlier prototype scope: the army-ant soldier and corporal only.

With both keys off nothing changes: no shader is compiled, no limit is queried, and the
CPU skinning, layer preparation and packets are the RC20 ones. `gpu_skin_capture` writes
six diagnostic PNGs on the a2-04 tape and is independent of the skin keys.

## What runs on the GPU

Each load-time surface is converted once into an immutable vertex stream: five
bone-local position/weight lanes, five palette indices, the stored SKB normal and the
UV (120 bytes a vertex). A surface with more than five influences on any vertex or more
than 60 bones keeps CPU skinning; the refusal is remembered for the surface's lifetime.

Per actor draw the CPU builds one palette of two vec4 per bone, shared by the draw's
surfaces and layers: the bone rotation quaternion with the model rotation folded in, and
the translation with the bone and model scale. A bone is computed the first time a
surface references it. The model translation is added once, as `Transform::point` does
on the CPU. The vertex program uses 2 x 60 + 4 (Projection) + 2 = 126 vertex uniform
vectors; the Vita reports 128. The fragment uniforms are a separate budget. Pose
evaluation, tags, attachments, sound and gameplay stay on the CPU and are unchanged.

## Uniform upload

Each surface's stream indexes its own bones in load order, so a draw uploads only the
`2 x bones` vectors of `Bones` the surface references; the program reads no other entry.
With `uniform_field_cache` each skin program keeps its own field cache: the base payload
goes per changed field, `Bones` and `SkinOrigin` only when the occurrence changes (the
layers of one surface share it), `SkinEye` when it changes. With `fx_submit_state_cache`
the pipeline and bindings are reapplied only after another draw, a world callback or a
pass boundary. A skin draw does not clear the CPU programs' uniform caches: GL keeps
uniform values per program. With `uniform_field_cache` off the whole table goes every
draw. The counters `gpu_skin_uniform_upload_bytes`, `gpu_skin_pipeline_sets` and
`gpu_skin_binding_sets` report what was sent.

Admitted layers are plain textures and environment reflections without tcMods, vector
UVs or dot alpha. An environment layer computes the CPU reflection coordinates
(`Stage::view_uv`) in the vertex program from the skinned normal and the eye position.
Everything else keeps the CPU path: translucent or fading actors, dead ants with the
corpse triangle cull, `actor_opaque_shader`, flat lighting, `packet_min_pixels`, mutable
mesh consumers, changed topology or UVs, transparent surfaces and unsupported overlap
phases.

## Fragment programs

A GPU packet keeps the fragment program its CPU twin would get: the full actor program,
the discard-free program, or one of the two lean programs (`actor_lean_shader`). The
selectors read the same uniform payload and the same surface layers; the placeholder
geometry carries the CPU vertex alpha (255) the selectors check. Only the vertex program
differs. If one specialised variant fails to compile, its packets use the full program.

## Culling and order

GPU packets carry placeholder vertices, so the packet frustum cull tests a conservative
box instead: every skinned vertex is a convex combination of its bones' transformed
offsets, each inside a sphere around the bone origin. Surfaces whose weights are negative
or do not sum to one get no box and are kept. GPU packets are merge barriers; they keep
the opaque group order.

## Picture difference

The CPU path rebuilds area-weighted normals from the deformed triangles. The GPU path
blends the stored SKB normal through the bone rotations. Lighting and environment
reflections therefore differ slightly on every skinned actor. This is an approximation,
not a rounding tolerance, and needs the owner's visual acceptance. Host evidence (close-ups,
per-tape pixel counts) is outside Git under
`/Users/anezium/vita-j0/plan-2026-11-12/v11-gpuskin/`.

## Host proof hooks (never on the Vita)

- `LOOKING_GLASS_GPU_SKIN_CENSUS`: per actor family and reason, the surfaces and vertices
  that go GPU or stay on the CPU, one `GPU_SKIN_CENSUS` line per probe frame.
- `LOOKING_GLASS_PIXEL_SAVE` / `LOOKING_GLASS_PIXEL_REFERENCE` / `LOOKING_GLASS_PIXEL_KEEP`:
  save every probe frame of a reference arm, then print a `PIXEL_DIFF` line per frame of a
  candidate arm and write upright close-ups (reference, candidate, heat).
- `LOOKING_GLASS_SIM_MAPS_TAPE_TIME`: the tape-time probe step and tape input for cross
  replays (off by default, as in the RC oracles).
