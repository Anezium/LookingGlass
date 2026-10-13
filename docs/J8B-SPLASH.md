# Splash allocator ownership: off-console analysis

The existing `startup_splash=false` default from `550dc04` already removes the
splash/main-thread overlap. No second switch or speculative allocator repair is
needed. `startup_splash=true` deliberately restores an SDK path with concurrent
GPU allocation/free; it must remain a legacy diagnostic, not a morning preset.
The exact corruption behind the 2026-10-12 abort remains unproven.

At vitaGL revision `2bdbe89`, [vgl.c](https://github.com/Rinnegatamante/vitaGL/blob/2bdbe89/source/vgl.c#L286)
calls `invoke_splashscreen` midway through initialization, before further blit
programs, scissor storage and texture state. The invocation starts a separate
thread and returns immediately. That thread allocates/decompresses vertices,
indices and an alternate GXM context through the same GPU allocator while the
caller continues initialization. See
[splashscreen.c](https://github.com/Rinnegatamante/vitaGL/blob/2bdbe89/source/splashscreen.c#L6287).

There is a second overlap at first rendering: the main thread signals the splash
to stop and waits on its completion semaphore, but the splash signals completion
**before** freeing its geometry, context and ring buffers. The main thread can
resume shader/resource work while those frees proceed. Merely waiting for the
existing semaphore, adding a sleep, or moving LG asset work does not close both
windows. See [completion/cleanup](https://github.com/Rinnegatamante/vitaGL/blob/2bdbe89/source/splashscreen.c#L6455)
and [scene_reset](https://github.com/Rinnegatamante/vitaGL/blob/2bdbe89/source/gxm.c#L545).

Concrete hypothesis: on a custom-heap archive with allocator locking disabled,
simultaneous main-thread allocation and splash free coalesce/insert the same
free-list nodes, corrupting next/prev links before `heap_blk_insert_free` dereferences
them. This explains an intermittent abort with a clean retry, but does not rule
out a bad free or another overwrite. Thread overlap is established from source;
the free-list corruption mechanism is an inference.

LG's `src/render/vita_renderer.rs:115` wrapper never calls the real invocation
when the switch is false. `build.rs` emits `--wrap=invoke_splashscreen` for native
feature builds. The archive has a separate `vgl.o` call relocation, so GNU linker
wrapping applies. The SDK splash thread does not exist in that configuration.
The opening film is separate: ZIP workers deliver owned bytes; only the viewer
thread uploads textures. The periodic Miniquad memory thread reads memory stats;
it does not allocate/free GPU resources. Suppression addresses splash overlap,
not a general proof that every SDK thread is safe.

The local SDK package is `vitaGL-0.0.0.r1488.g2bdbe89-1`. Its extracted
`mem_utils.o` imports `sceClibMspace*` and has no `heap_blk_insert_free` or
LwMutex symbols. Therefore this local archive uses the mspace branch, not the
custom heap named by the console abort. Absence of LwMutex imports alone does
**not** prove mspace unsafe. The custom branch has conditional locks, removed by
`HAVE_SINGLE_THREADED_GC`; its exact console build flags must be checked.
See [allocator variants](https://github.com/Rinnegatamante/vitaGL/blob/2bdbe89/source/utils/mem_utils.c#L39).
Local archive SHA-256:
`2b7d19510b72fe8e68d32450f247aaf7dc4eedd457ce0850b0dcc351a2ea94f4`.
Header SHA-256:
`c41af733b803f8369a7d0d98c357c334605844c9a040da5e08a586334cc7532c`.
Extracted symbol/relocation evidence remains in `private/j8b/sdk/`.

Morning probe, without changing SDK or Cargo pins:

1. Hash the Mac's actual linked archive/header and inspect its `mem_utils.o` for
   custom-heap and mutex symbols. Record the exact VPK and archive beside results.
2. Disassemble the packaged ELF: the `vglInitWithCustomSizes` call site must reach
   `__wrap_invoke_splashscreen`, not the real entry point. A wrapper symbol existing
   somewhere in the ELF is insufficient. Build with `--features vita_renderer`.
3. Keep `startup_splash=false`; verify `SDK splash suppressed; initialization
   allocator owner=main` and cold-start repeatedly through menu/New Game. Record
   the first/last startup marker and crash PC if an abort recurs.
4. If the suppressed path still aborts, a diagnostic SDK probe should record
   thread ID, phase, heap kind, pointer and operation in a bounded preallocated
   ring at allocator entry/exit. Capture simultaneous calls and validate free-list
   links in the failing heap. Avoid formatting/allocating inside the probe. Only
   after identifying the actual heap/build should a fully joined splash lifecycle
   or allocator lock fix be considered; holding a main lock across the existing
   semaphore wait can deadlock against splash cleanup.
