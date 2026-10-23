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
