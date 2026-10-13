# Vita startup caches, milestone 7

The j6 console measurements put 75.1 seconds of gvillage's 88.3-second map load
inside 12 shader creation calls. No hardware measurements are available for j7.

## Shader paths and contract

All application Macroquad materials pass through `frame_profile::load_material`:
world/depth variants (`render.rs`), Alice (`character.rs`), weapons, particles and
pickup bursts, loot, camera portals, save-film, and scaled presentation. Direct
Miniquad paths are retained world/full/low/sky, retained actors/FX, and the host
interop diagnostic. Macroquad itself also creates its default and screen-copy
shaders. `__wrap_vglGetProcAddress` intercepts the loader used by every Vita path;
wrapping `glLinkProgram` alone would miss these function-pointer calls.

Each `[lg/shader]` line identifies the name, source hash, build-bound cache key,
vertex/fragment byte lengths, compile/load milliseconds, result, and cumulative
`before` requests / `after` compiler calls / `avoided` compilations. Compilation
means the vertex/fragment pair: vitaGL's postponed mode compiles both internally
at link time, so the pair's compile time cannot be divided reliably by stage.
`[lg/shader-request]` identifies application material call sites on hosts too.

Identical source pairs reuse serialized binaries within the process, including
when persistent caching is off. Each request still gets its own GL program,
uniform storage, material and pipeline. Rendering GLSL and pipeline state are
unchanged. The nine world blend/depth requests share sources with retained
actors/FX; the expected measured-map reduction is 12 -> 3 compilations on a cold
cache and 12 -> 0 on a warm cache (hypotheses pending hardware logs).

`vita_renderer.json` accepts `"shader_cache":"on"` (default) or `"off"`.
Entries live in `ux0:data/alice/private/cache/shaders/`. Keys include exact source
bytes (thus defines), stage boundaries, ABI namespace, and SHA-256 of the linked
vitaGL archive and header. Envelopes validate magic, key, checksum and bounded
size; stage lengths, GXP size/signature, GXM validity/type and matrix parameter
indices are checked before deserialization. Writes use a synced temporary file
and rename. Missing, malformed, rejected or unwritable entries preserve normal
source compilation; successful compilation replaces invalid entries.

Loading builds a temporary program from two CG shader objects and the paired
serialized stage binaries. Failed candidates are deleted before creating the
ordinary GLSL program. Original sources and shaders remain untouched throughout.
Using public stage loading also avoids the pinned `glProgramBinary` implementation
appending default attribute bindings after restoring its saved attribute count.
The backend uses default attribute bindings; no pre-link explicit bindings exist.

## Evidence and limits

The installed `vitaGL.h` declares `glGetProgramBinary`, `glProgramBinary`,
`glShaderBinary`, `vglGetShaderBinary` and `vglSetShaderCachePath`; its default
semantic mode is postponed. The installed archive exports those symbols and
its `glsl_sema_mode` data is 2 (postponed). Its package is
`0.0.0.r1488.g2bdbe89-1`.
[The matching driver source](https://github.com/Rinnegatamante/vitaGL/blob/2bdbe89/source/custom_shaders.c)
shows paired compilation, serialization of matrix indices and GXP bytes, stage
restoration, and GL program lifetimes. The cache uses these supported public APIs,
without relying on the driver's optional unchecked disk cache.

Host tests model the GL contract and cover 12 -> 3 process deduplication, fresh
process disk hits, corruption, size/identity/define mismatches, rejected candidate
cleanup, and unchanged fallback sources. They do not execute GXM or libshacccg.
Actual linker interception, GXP registration, visual equivalence, device storage
behavior, cache-hit latency and the under-15-second target need Vita verification.
The SDK deserializer/patcher does not report every allocation/registration failure
through GL status; resource exhaustion remains a driver limitation.

The largest generated default pair is the world/retained-actor shader (fog,
alpha/material branches and eight dynamic-light terms). Generated source sizes: world/retained actors 411 + 5,622 = 6,033 bytes;
retained world 1,843 + 2,616 = 4,459; Alice 363 + 2,176 = 2,539;
particles 363 + 1,829 = 2,192. Runtime logs also cover generated variants. No branches,
uniforms, precision qualifiers or rendering output were simplified.

## Console test

Back up or remove only `private/cache/shaders/`, leave player saves intact, launch
and select New Game, then exit fully and repeat. Expect cold `cache=compile` then
`cache=memory`; on the second launch expect `cache=disk` and `compile_ms=0`.
Compare `map_load` totals and check the scene, actors, alpha, lighting, fog,
menus, save-film and movie presentation. Test `shader_cache=off` separately and
truncate one shader entry to verify recompilation/rewrite. Invalid configuration
values use the existing whole-config validation/fallback policy.

## Other startup work

`menu_load` markers separate material parsing, layouts, textures, the animated
Alice mirror, and UI/save-film/target creation, with read/decode/upload/shader
subtotals. The old menu timestamp included preceding UI work. Menu loading also
creates the Alice skin shader, so shader caching can improve this phase too.

`"texture_cache":"on"` enables an optional cache under
`ux0:data/alice/private/cache/textures/`; default is `"off"` pending device review.
Miniquad already chooses DXT5 for immutable POT RGBA images. The hook runs after
that choice, hashes the resized pixels, dimensions, compression-quality flag and
SDK build identity, and uses the same envelope/atomic-write rules as shaders.
It saves the driver's **swizzled** compressed allocation returned by the public
`vglGetTexDataPointer`. On a hit, the same null-data DXT5 upload allocates and sets
up driver storage; the cached bytes fill it directly. This avoids compressing
again or swizzling an already swizzled buffer with `glCompressedTexImage2D`.
Dynamic images, fonts, movie frames, render targets, NPOT textures and mip levels
bypass this cache. A missing/invalid entry or null allocation falls back to the
original upload and rewrites the result. Host mocked-upload tests compare exact
bytes and cover second-launch reuse, corruption, changed pixels and allocation
failure. The matching
[driver allocation implementation](https://github.com/Rinnegatamante/vitaGL/blob/2bdbe89/source/utils/gpu_utils.c)
and installed header establish the layout/API; actual GPU coherence and visuals
require hardware. Expected savings are the measured compression portion only,
less hashing and storage overhead; 3 seconds is an upper bound for map textures.
Cold writes can make the first launch slower.

The reported save-store 13,038 ms was **viewer elapsed time**, not a separate
13-second wait. `Store::new` only owns its two arguments; it neither scans saves
nor waits. The call first computes the corpus identity (65 ms in the supplied
hardware log). Logs now separate local `elapsed_ms`, `store_construct_ms`, and
`viewer_elapsed_ms`. Save semantics remain unchanged; no 13-second saving is
claimed.

For texture testing, enable `texture_cache`, remove only its cache directory,
then compare first/second launch `[lg/texture-cache] cache=compress|disk` logs,
menu subtotals and texture appearance. Keep the compressed cache off when
isolating the shader gain. Do not remove the player's save directory.
