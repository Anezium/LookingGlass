# Scaled-target host teardown failure

The older task-1 binary reproduces exit 139 after the scale fixture prints its
120-frame PASS, with `--no-audio`. A temporary signal-handler preload captures
the faulting PC at an unmapped function pointer. Its stack words, symbolized with
`addr2line -f -C`, identify this chain:

```text
std::sys::thread_local::native::eager::destroy<RefCell<Option<scale::Target>>>
core::ptr::drop_glue<Option<scale::Target>>
<miniquad::graphics::gl::GlContext as RenderingBackend>::delete_texture
indirect glDeleteTextures call (unmapped PC)
```

Proof artifacts are local under `private/j8b/scale/baseline-trace.log` and
`private/j8b/segv_trace.c`. The baseline is the already-built task-1 executable;
the regression uses the binary built in this worktree. No debugger, toolchain or
dependency installation is required.

Miniquad revision `6f89180`, `native/linux_x11.rs:515`, destroys the GLX context
and closes the X display before returning from its event loop. Its dynamic
`Module` then calls `dlclose`. Macroquad retains a process-lived global context,
including those now-invalid GL function pointers. At main-thread TLS destruction,
`scale::Target::drop` deletes its host depth image through that context. Its
`RenderPass` field would also issue GL destruction afterward. This explains why
rendering and save checks finish correctly and the unscaled runs exit cleanly.
The captured failure precedes any audio cleanup; no thread join is in its stack.

`scale.rs` makes the last target explicitly process-lived with
`ManuallyDrop<Option<Target>>`. Resize assignment still drops the replaced target
normally while GL is live. The final target is reclaimed with the process rather
than issuing callbacks after backend shutdown. This retains one final target,
its material and attachments until exit; it does not accumulate replaced targets.
Re-entering multiple independent Macroquad contexts in one process remains outside
the framework's existing global-context lifetime contract.

On Vita the same TLS resource wrapper exists, but `Target.depth` is `None` and the
backend has no explicit `vglEnd` on loop exit. The native game thread joins its
parent after returning, so TLS destruction can still happen there. The specific
host fault through an unloaded GLX library cannot occur with statically linked
vitaGL. Avoiding final GL cleanup also removes the shared render-pass teardown
callback; console testing still needs to validate the ordinary running path.
