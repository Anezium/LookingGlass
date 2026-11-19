# Mover shape reuse (2026-11-19)

`mover_shape_reuse` (default off) stops rebuilding mover colliders whose inputs did not change.

Why it exists: on the console the movers bucket of the simulation split is 9-14 ms on the wchess1
window and a flat 13.8 ms on the hatter2 window (`rw-20261116-wchess1-01.lgtape` 1486..1966,
`fp-hatter2-01.lgtape` 556..1036, RC20 device logs). The host replay census of those windows
(probe counters `mover_*`, one binary at the branch head) splits the bucket:

- wchess1: the Pale Realm rebuild guard at `levels/wchess1/motion.rs` already builds only moved
  objects. Measured 15 collider builds per frame (2 clock hands, 5.5 lesson spikes on average),
  every one with changed inputs, 0 unchanged. The remaining movers cost is genuine motion plus a
  redundant second `rebuild` pass; there is no unchanged-rebuild group to reuse.
- hatter2: `levels/hatter2/scene.rs` rebuilds the whole shape list whenever any brush pose moved,
  and the clock hands and the test pendulum move every frame, so the early-out never fires.
  Measured 32 colliders rebuilt per frame, 31 of them with inputs identical to the previous frame
  (cages, chains, knobs, clock-door cams, doors, drawbridge, exit platforms); one pendulum moves.
  That group owns about 92 percent of the hatter2 movers bucket on the host.

The reuse: each brush keeps the last collider built for it, together with the pose tuple it was
built from (`(model, translation, rotation)`). A brush reuses its cached collider when its pose
tuple equals the recorded one; anything else rebuilds and replaces the entry. `brush_collider` is
a pure function of the map, the brush record and that pose, so equal inputs build a bit-identical
collider and the reuse is exact. A brush excluded this frame keeps its stale entry: an entry can
only be reused for its recorded pose, and the same inputs build the same collider. A save restore
changes poses; unchanged brushes reuse, changed brushes rebuild. Collision, interaction, mover
replay and NPC transcripts see the same colliders in the same order either way.

The clock hands themselves stay out of the shape list as before; the pendulum and every other
genuinely moving brush still rebuilds every frame it moves.

Host proof lives outside Git, in the task evidence folder. Console speed, heap and picture are
measured by the device slot only.
