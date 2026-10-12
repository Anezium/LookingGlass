# Off-console diagnostic builds

Vita3K checks functional behavior and heap growth. Emulator timings are never Vita performance measurements. The `vita3k_diagnostics` Cargo feature is off by default; its code and linker wrappers are Vita-only. A diagnostic build is not the console binary.

In an isolated checkout with the retained renderer, build without updating the lockfile:

```sh
export VITASDK="$HOME/vitasdk"
export PATH="$HOME/.cargo/bin:$VITASDK/bin:$PATH"
export CARGO_TARGET_DIR="$HOME/vita-j0/vita3k-check/build-target"
export RUSTFLAGS='-C target-feature=-neon'
cargo +nightly-2026-10-11 vita build vpk --release --locked \
  --features vita_renderer,vita3k_diagnostics
```

No assets, firmware, shader compiler backups, binaries, or captures belong in Git. Use an isolated Vita3K filesystem and only the Alice title/data. Never run this diagnostic on the console.

At startup, diagnostic builds read `ux0:data/alice/private/vita3k_diagnostics.json`. Missing controls preserve the requested renderer workload. Independent optional boolean controls are `actors`, `objects`, `effects`, `hud`, `fx_queue`, and `depth_flush`. They can enable a subsystem over `world_only=true`, or disable it in a full scene. `depth_flush=false` deliberately removes the read-only depth-mask/flush pair and is only for failure isolation. Actors include skins and level art; objects include power pickups; effects include particles, aiming, pickup sprites and flares. HUD includes text and menus. `fx_queue` controls the legacy queue begin/finish pair.

`trace=true` prints paired entry/exit markers in the first four gameplay frames. `trace_gpu=true` additionally traces standard and precomputed GXM draws. Shader compile/link calls print their entry/exit and newlib heap bytes; counters are printed per sampled frame and every ten seconds. The miniquad shader/pipeline counters are call-boundary proxies verified against the pinned backend: one vertex shader creation per `new_shader`, and one `position` attribute query per `new_pipeline`. All LG layouts include `position`; these proxies must be rechecked if the backend changes.

`soak_seconds` (nonnegative integer) keeps drawing after writing the 120-frame profile, then prints benchmark completion. The saved profile remains the original 120 measured frames. This allows a sustained heap/progress check without changing the normal benchmark. The default is zero in the binary; the external diagnostic harness requests five seconds for capture, or 180 seconds for a leak check.

The private marker `ux0:data/alice/private/vita3k_disable_mask` suppresses only mask triangles inside vitaGL's scissor update, forwarding its state restoration. It disables scissor clipping because Vita3K's Vulkan mask is unimplemented. This is an emulator diagnostic workaround, not renderer correctness or a shipping fix.

The external harness and evidence live at `~/vita-j0/vita3k-check/`; read its README for exact run commands and result limits.
