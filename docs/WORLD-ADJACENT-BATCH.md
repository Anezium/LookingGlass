# Exact adjacent world work (experimental)

`world_adjacent_batch` is a boolean, default **false**, requiring
`world_list_submit=true` and `world_shared_buffers=true`. It supports the shipped
`world_gxm=live` path. It adds no geometry uploads or reordering.

Opaque draws join only when actual state, uniform bits, rank and shared buffer
ranges match in the original order, and the union of padded screen bounds remains
disjoint. The resulting GL/GXM command uses the first range's base and the sum of
its members' counts. Host probe builds independently certify that emitted command
immediately before GL; wrong-base/count controls must fail even if occluded.

World alpha retains its original joint order with actors/FX and all original draw
commands. Identical state applications may be skipped only between consecutive
world callbacks. Actors, puffs, frames and merged-water programs are interruption
fences; pipeline transitions force uniform application. No blend approximation,
material-clock change, near cull or game-state change is involved.

Host evidence from 2026-11-14: full Vita tape a2-04 replays exact off/on, with
matching scene images; six scripted maps have exact per-frame pixels and semantic
packets. The real river prefix still submits the same 1007 world commands in both
arms, while skipping 986 redundant state-application/comparison operations at the
sampled frame. Those operations are not a count of removed GL syscalls. Conservative
opaque joining saves only a few commands in other views. This is not a solution
for hundreds of river draws, and no Vita time saving has been established.

Keep off until a clean console A/B establishes useful cost. Normal gameplay should
keep spike logging off. Optional `world_gxm=proof` is a separate correctness run;
its observer frames cannot serve as ordinary gameplay timings.
