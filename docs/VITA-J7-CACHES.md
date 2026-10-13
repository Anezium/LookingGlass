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
