# Runtime simulation compromises

Iteration 9 is based on vita-sim-8 at fc9836b19deb271c868da5ffd34f3a0f9cae2d8d.
This change does not integrate the retained-world renderer. All configuration is read once,
after command-line parsing and before asset loading. Restart after changing a file.

## Configuration

Vita: `ux0:data/alice/private/vita_sim.json`. Host: `vita_sim.json` in the parent
of the selected save folder, normally `private/vita_sim.json` next to `private/saves`.
Missing file or `{}` selects all defaults. Unknown fields, duplicate fields, wrong
JSON types, non-object top-level documents, malformed JSON and invalid ranges reject the
whole file. An invalid/unreadable file prints `[lg/vita-sim] error ...; using full defaults`.
There is no partial recovery. Startup prints exactly one effective `[lg/vita-sim] config {...}`
line, and profiling reports put the same object at top-level `vita_sim`.

Defaults:

```json
{"puff_cap":4096,"puff_collide":"all","puff_sweep_every":1,"anim_cull":false,"anim_far_distance":null,"anim_far_every":1,"mover_far_every":1}
```

`puff_cap` accepts 0..4096. All three `*_every` values accept 1..120.
`anim_far_distance` accepts null (disabled) or a finite nonnegative distance in world units.
`puff_collide` accepts only `all`, `moving`, or `none`. `anim_cull` is a boolean.
The defaults leave the old particle integration, actor draw and eager mover paths active.
The additional transient cache/cadence fields are absent from saves.

## Particles

`puff_cap` removes the oldest live births after integration/emission. World puffs are
append-only births with stable retention, so vector order is birth order. RNG and emitter
credit run through the normal emission path; the existing absolute 4096 emission bound
remains. The cap bounds the population after an update; its integration savings begin on
following ticks. Capping zero removes every world puff.

`puff_collide=moving` avoids sweeps below 0.001 units/second, measured after acceleration;
`none` avoids all unconstrained puff sweeps. These are deliberately visual compromises:
moving machinery can enter a stationary puff without killing it. Bubble liquid checks,
axis constraints, emission and lifetime integration remain active.

`puff_sweep_every=N` counts positive per-puff integration ticks. It sweeps from the position
at the start of the accumulated interval to the latest position, not just the last tick.
A world-time pause does not advance that puff's cadence; `no_deadtime` puffs retain their
old exception. A straight accumulated segment does not reconstruct a curved path or old
mover poses. Impact removal may be delayed by N-1 ticks. Full accumulated segments and wall
crossings on skipped ticks are covered by host tests. These levers affect world Steam puffs;
script-attached particle effects reconstructed by `Attached::sample` stay exact.

Safety: `Steam::update_clocks_config` (`src/particles.rs:886`) takes only `&World`, with no
Player, Stats, combat or save reference. Puffs/particle RNG are absent from `Game` and `Level`
(`src/save.rs:96`, `src/save.rs:151`); the runtime restored-level bundle contains Steam
separately (`src/save.rs:567`). Emitters used for gameplay steam volumes are not removed:
only their visual puffs are capped. `puffs_after` is the live population; `puffs_sweeps`,
`puffs_skipped_sweeps` and `puffs_dropped` identify work actually performed/removed.

Hypotheses, not measurements: cap 128 versus roughly 644 live puffs could remove about
80% of per-puff work; every=4 could remove about 75% of eligible sweep calls; `none` removes
all puff sweep calls. These estimates overlap and cannot be added. The supplied ~45 ms
particle span is an older baseline; liquid/constraint/emission work still remains.

## Characters: the safe set is deliberately narrow

Only the ambient branch of `Npcs::draw` (`src/npc.rs:3339`) is eligible. The complete admission
predicate is `src/npc/visual_lod.rs:54`: no Alice model/name, guard, Ant, chess piece, Imp,
Clockwork, resident, guide or active electrical effect; no combat timing, authored or loaded
attachment, prop, effect, own effect or bone scaling. This is a data-checked safe set,
not a guessed allowlist of model names. Any actor whose metadata does not prove eligibility
continues through the exact draw path. The asset-free host cannot enumerate which placed
village actors pass this predicate on a user's PK3s, and no such population is claimed.

Proof of isolation:

- NPC simulation updates `Actor::tick` at `src/npc.rs:2813`; its saved timers/yaw/watch remain
  independent of the local pose sampled in `Npcs::draw` at `src/npc.rs:3377`.
- Combat targets come from the controller cases at `src/npc.rs:2880`; every such case is
  excluded before the LOD branch. Protected-pupil contacts use position and authored AABB
  at `src/npc.rs:2924`, not the drawn skeleton or mesh.
- `Model::draw_pose` builds a temporary global pose (`src/npc.rs:1106`) and feeds it to mesh
  skinning. Attachment points consume it at `src/npc.rs:1153`; all actors with such consumers
  are excluded. Clockwork rig/muzzle pose queries (`src/npc.rs:3861`) are also excluded.
- Alice's separate draw path (`src/viewer.rs:3256`), every Puppet, LevelArt and cinematic
  handoff pose (`src/viewer.rs:84`) are untouched. Alice never enters this cache.
- `Npcs::snapshot` clones only actors and gameplay state (`src/npc.rs:1928`). The per-actor
  mesh cache lives in Npcs outside Actor; restore clears it (`src/npc.rs:1936`).

`anim_cull=true` skips pose sampling, skinning, normals and submission outside the active
view frustum. It uses the existing projection/frustum helper and a cube enclosing every
loaded clip's authored bounds with 128 local units of extra facial/watch padding.

A non-null `anim_far_distance` plus `anim_far_every>1` refreshes far actors' visual pose and
skin only every N eligible draw ticks. Other ticks submit per-actor cached meshes, so models
shared by several actors cannot alias cached vertices. Re-entry from a frustum cull, crossing
back into near range, root movement/rotation, scale or fullbright changes force immediate
refresh. Layer/shader time remains current; only the cached visual pose/blink lags.
`anim_far_distance` alone with every=1 deliberately does no throttling.

Presets `anim-far-distance` (distance 800/every 4) and `anim-far-every` (same distance/every 8)
therefore document their coupled settings explicitly. The second versus the first isolates
cadence; neither is falsely advertised as a distance-only run. All other single-lever presets
change one field. `anim-cull` is an additional independent opt-in so defaults never gain new
frustum culling silently.

Per-frame counters: `characters_lod_exact`, `characters_lod_culled`,
`characters_lod_refreshed`, `characters_lod_reused`, `characters_lod_protected`. They describe
the ambient branch after pre-existing visibility/ownership gates. Controller/LevelArt/Alice
renders are protected by absence of any LOD hook, not counted in this ambient-only series.

Hypothesis: refresh every 4 can avoid 75% of eligible skin/normal/pose work, and frustum
culling also avoids submissions. The supplied entire skinning span was only ~9 ms; the gain
must be weighted by the eligible population, and cached submission/layers still cost time.
GPU/resource correctness of cached meshes and scene-wide visuals still require console review.

## Movers: defer placement, never movement or contacts

`mover_far_every` counts the existing 120 Hz village substeps, with a stable object-index
phase to spread eager rebuilds. Every object's sampled pose/time is still updated normally.
On deferred ticks its current pose is published through an immutable lazy collider. Any
later particle, projectile or body query can build the exact current hull on demand; there
is no stale collider interval and no skipped collision response in a gameplay query.

The margin predicate compares collider AABB to Alice's body AABB expanded by SKIN
(`src/collision.rs:242`). Both the current and candidate AABBs must be more than 256 units
away (`src/village.rs:528`). Missing bounds/non-finite gaps force the exact fallback.
Within that margin, the original eager build, ground probe, rider correction, world
transport obstruction and move-rejection code all run unchanged (`src/village.rs:551`).

Why skipping a far temporary publication cannot affect a player contact:

1. The only old-support probe is the 3-unit downward body trace at `src/village.rs:562`.
   A 256-unit AABB gap (even if distributed over three axes) exceeds this reach. Therefore
   the old object cannot carry Alice and the `riding` predicate cannot be true.
2. The non-riding candidate test is a stationary body overlap (`src/village.rs:631`), also
   outside the certified candidate AABB. It cannot reject this move. No world sweep is
   consumed by this branch when the rider predicate is false.
3. Supported village objects are script-object inline brush models (`src/village.rs:19`).
   Village motion is sampled independently of Player (`src/village.rs:74`); there is no held
   object or carry-by-Alice state in Village/Object or Player (`src/movement.rs:47`).
4. Other moving objects republish current colliders before their own world transport test
   (`src/village.rs:576`); the final list includes every current solid (`src/village.rs:659`).
   The omitted temporary list had no consumer. Fixed colliders and list order remain intact.
5. Lazy `Placed` retains exact origin/quaternion and uses the original geometry placement
   (`src/collision/model_shape.rs:182`). Its first actual query builds/caches that geometry
   (`src/collision/model_shape.rs:191`); eager/lazy equivalence tests predate this change.
   Broad AABBs can be conservative rather than equal, but final hulls and Trace are exact.

Counters: `village_deferred_placements`, `village_eager_placements`,
`village_in_range_fallbacks`, `lazy_hull_builds`, plus span `collider_lazy_build`.
The last two are global on-demand geometry diagnostics, including pre-existing lazy users.
Subtract neither one blindly from mover gains: deferred work can reappear in query owners.

Hypothesis: every=4 avoids up to 75% of far eager placement/probe/publication work, before
on-demand builds. The old ~51 ms placement cost is an upper scaling reference, not a measured
sim8 residual. Actual savings depend on far population and query activity. Combining fewer
puff queries with lazy placement may help; gains cannot be added as independent constants.
No 25 ms simulation claim is made without a new console run.

## Validation and deliberate exclusions

Baseline `cargo test --locked`: 700 passed, zero failures, one pre-existing asset-dependent
ignored test. Defaults keep the sim8 trace/contact suite, and a copied sim8 particle integrator
checks physical puff state, RNG, credit and deaths over the existing 120-frame fixture.
Each particle lever, mover cadences and the all preset exercise the existing 180-step rider,
obstruction, hatch-open and restore sequences. They compare player contacts/transport and
complete format-12 save envelopes/checksums with a fixed test timestamp (`src/save.rs:919`).
Final `cargo check --locked` succeeds; final `cargo test --locked` reports 717 passed,
zero failures and one pre-existing ignored test. Character policy tests compare saved Actor
bytes/positions/targets and invoke those same player/save sequences. Cadence tests use fixed tick counts/dt, never measured frame time.

Not implemented: throttling gameplay clocks or NPC controllers; skipping Alice or cinematic
handoff poses; attachment-bearing/combat-character LOD; stale distant colliders; approximate
player contact normals; reconstructing old moving-world contacts for delayed visual puffs;
integration of another renderer branch. These either violate the gameplay contract or require
proof outside this host-only task. No console installation, connection, input or measurement
is part of this delivery. Game-asset progression/native-render tests remain unexecuted.
