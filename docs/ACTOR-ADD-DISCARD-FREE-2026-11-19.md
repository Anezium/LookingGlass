# Discard-free additive actor layers (2026-11-19)

`actor_add_discard_free` (default off, needs `actor_lean_shader`) draws an additive actor
packet whose cutout lines cannot run with `retained-actor-lean-opaque` instead of
`retained-actor-lean`. No program is added: both are compiled and prewarmed by
`actor_lean_shader`.

Target: the Jabberwock body glow in jlair2. Each body part draws `skin01` (Alpha, `Cutout=1`)
and then the same triangles again with `skin01g` (`Blend::Add`, ONE/ONE, `Cutout=0`, depth
writes on, LEQUAL). On RC19 that second draw runs the lean program, which still contains the
two cutout `discard` lines although `Cutout=0` makes them dead. On the device the
`actor_max_layers: 1` diagnostic (which removes these draws and changes the picture) took the
breath GPU wait p95 from 45.5 to 33.1 ms.

Selector (`actor_lean::add_discard_free`), read from the packet's own payload:

- not an effect packet, and already on the lean program (`Route::ActorLean`);
- blend `Add` or `AlphaAdd`;
- the payload `Cutout` word is at most 0.5 (NaN fails).

Why the picture cannot change: every clause of the first cutout line needs `Cutout > 0.5`
and every clause of the second `Cutout > 4.5`. With `Cutout <= 0.5` neither line discards, so
the lean program and its discard-free twin, which differ by exactly those two lines, write the
same colour, alpha and depth for every fragment. Geometry, order, textures, uniform words,
blend and depth state are unchanged.

What it may save on the device (assumed, not measured): a program that contains `discard` makes
the GPU wait for the shader before it can resolve depth for those fragments, whether or not the
discard runs. The glow is blended and writes depth, so it still shades every fragment that
passes the depth test; only that discard handling goes.

Counter: `vita_actor_add_discard_free_draws`. Host proof lives outside Git, in the task
evidence folder. Console speed and picture are measured by the device slot only.
