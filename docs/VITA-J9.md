# Milestone 9 startup clocks

With `--features vita_renderer`, Vita reads `vita_renderer.json` at the start of
the game thread, before archive mounting and window initialization. The validated
configuration is cached once for the process. `clocks` accepts `"max"` (the default
when absent) or `"stock"`; other values reject the config under the existing
whole-file fallback policy.

`max` requests ARM 444, bus 222, GPU 222 and GPU crossbar 166 MHz. The startup log
records setter return codes and actual clocks in that order. `stock` calls no
setters and logs the inherited clocks with `rc=[]`. These power calls are Vita
only; host configuration parsing has no clock side effects.

The GPU experiment runs after the startup policy: `unchanged` calls no setter,
`gpu222` requests 222 MHz and only explicit `gpu111` requests the lower GPU clock.
The diagnostic `profile_mode` `clk444` still requests the same maximum clocks.
The existing lean preset inherits `clocks=max` without changing its JSON or any
rendering/quality settings. Restart after changing configuration.

Milestone 9 packages compare the existing J8 build flags (A-stock, Rust NEON
disabled) with the same build minus `-C target-feature=-neon` (B-neon). Both use
the same clock policy. Console launch, actual clocks, visuals, temperature and
performance remain to be validated on hardware.
