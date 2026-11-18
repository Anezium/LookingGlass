# Lean actor programs (2026-11-18)

`actor_lean_shader` (default off) draws actor packets with a shorter fragment program when the
lines it leaves out cannot run for that packet.

Why it exists: on the console the actor program is the longest one (compiled fragment 5,622
bytes, against 2,616 for the world program). A large actor on screen costs GPU time in
proportion to the fragments it covers: the rolling marble of garden3 and the Jabberwock and
Gryphon of the grounds1 cinematic put those maps over the frame bar with a dozen draws.
The two discard-free keys do not help there. The marble and the Jabberwock layers are not in
their certified sets, and the Gryphon already uses the discard-free program.

A packet is admitted when its own uniform payload makes every removed line a no-op:

- no dissolve and no ghost (`FxAppearance.x <= 0`, `.y <= 0.5`): the two screen-door discards;
- a model packet (`FxModel >= 0.5`): the light atlas read;
- all eight dynamic light slots off, with a finite gain: `dynamicLight` adds zero;
- no power tint and no filter fade (`FxAppearance.z <= 0.5`, `.w == 0`);
- `FogMode <= 3.5`: the source-colour fog branch, which holds a second copy of `fogged`;
- `SkyFade <= 0.5`: the sky backdrop read (always zero in this renderer).

Everything else is the full program byte for byte: the texture read, the alpha product, both
cutout lines, vertex lighting, gain and the `fogged` call. Geometry, order, textures, uniform
words, blend and depth state do not change.

Two programs are compiled. `retained-actor-lean` replaces the full program and keeps the cutout
tests, so a packet that rejects fragments rejects the same ones. `retained-actor-lean-opaque`
has no `discard` and only replaces the existing discard-free program, for the packets
`actor_discard_free_shader`, `actor_cutout_discard_free` and the corpse keys already send
there. The key never widens the set of packets drawn without `discard`.

A packet keeps its current program when a dynamic light is on, when it dissolves, ghosts, is
tinted or fades, when its blend is not one of the nine named blends (the lean programs only
prewarm those pipelines), or when a lean program failed to compile.

Host proof lives outside Git, in the task evidence folder. Console speed, heap and picture are
measured by the device slot only.

## Fog-free twins: `actor_fog_free` (2026-11-18)

`actor_fog_free` (default off, needs `actor_lean_shader`) compiles two more programs,
`retained-actor-lean-fog-free` and `retained-actor-lean-opaque-fog-free`: each lean program
without the fog call and the fog block. A lean packet moves to its twin when `fogged` returns
its argument on every fragment: distance and liquid fog are off, and each fog volume is off or
separated from every eye-to-triangle hull by an axis plane. That is the `fx_add_fog_free` test
(`add_program::actor_fog_free`), valid for actors because every retained packet is drawn with an
identity `Model`, so the mesh positions are the `worldPosition` the fog reads. Then `fogLength`
is zero, every fog `mix` has weight zero and the call is a no-op. Nothing else changes: the
cutout tests, lighting, gain, blend, depth state, geometry and order are the lean program's.

Why: in the jlair2 fight every actor draw already runs a lean program, and the device class
ablation still puts the frame's GPU time in the actor class. The map has one fog volume that
no actor touches, yet each actor fragment evaluates the whole fog block.
