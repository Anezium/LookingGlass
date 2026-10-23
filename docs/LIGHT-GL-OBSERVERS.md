# Light GL observers

`configs/PL-def-r3.json` sets `light_gl_observers` to `false`. This strictly
boolean switch affects only `profiler: "light"`. Set it to `true` in every
candidate arm when comparing against older builds whose light profiler always
installed GL observers. It observes every previously hooked GL name regardless
of the two world cache flags. Either cache also requires observers.

With both caches off and this switch false, light profiling can use direct GL
calls. GL-derived counters and complete deferred-tail CPU measurements are
unavailable in that mode. The acceptance gate uses wall time. This diagnostic
switch does not enable observers under `profiler: "off"`.

Light CSVs include a `gl_observers` column (`direct` or `observed`) on every
row. Direct rows leave complete `frame_cpu_us`, all HUD/present fields, and
each phase's GL-call and deferred-CPU fields empty. Engine counters and direct
pass timers for world/actors/FX remain available. Empty values are unavailable,
never zero. Tools under `tools/light_console/` parse these exports; wall time
alone drives timing acceptance. Legacy CSVs without the marker require an
explicit `--legacy-observers observed|direct` declaration.
