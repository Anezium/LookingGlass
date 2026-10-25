# Static collision SAH contract

r10c note: where this r8 text disagrees with `AMENDMENTS-R10.md` or `READING-RULES.md`, those later files win. `AMENDMENTS.md` is left unchanged; the same supersession is recorded in `AMENDMENTS-R10C.md`.

`collision_sah=false` is the default. The option is sampled at World creation.
The original tree and all hulls/plane slices remain immutable and unchanged.
Enabled Worlds additionally own four compact arrays built during map load.
No task can mutate the immutable audio geometry; queries use stack scratch.

Ordering is the exact original Node::build left-before-right DFS, then the
stored order of each leaf. SAH leaf indices contain that unique rank. Accepted
candidates are sorted by rank before the unmodified clipping arithmetic runs.
No closest-hit shortening is permitted. Each clip starts with the current Trace,
including its normal, not a default Trace. The strict raw enter comparison occurs
before max(0); all-solid can set fraction=0 while preserving an earlier normal.

New pruning uses the *same* SweepBounds::overlaps expressions as the old tree,
with union bounds. This is a floating-point monotonicity proof, not an assumption
about real-valued ray intersections or conventional triangle collision:

1. The accelerated domain requires finite ordered geometry bounds, start/end/half (half >= 0),
   end-start, padding, enclosing query bounds and reciprocal components. Every
   other input uses the original traversal, including reciprocal overflow from
   subnormal deltas and delta overflow. Parallel/zero axes are skipped identically.
2. On a nonparallel axis the reciprocal is finite and nonzero. Widening the
   lower bound monotonically decreases `(min-padding-start)`; widening the upper
   bound monotonically increases `(max+padding-start)`. Each rounded subtraction
   is monotone. Multiplication by a positive reciprocal preserves that order;
   a negative reciprocal reverses it and min/max select the reversed endpoints.
   Subtraction overflow can yield infinity; multiplication cannot produce NaN
   because the reciprocal is finite/nonzero. No new slab calculation contains
   NaN. Signed zero changes no numeric <= or > decision.
3. Thus union bounds have a slab interval containing the child's *rounded*
   interval. Per-axis enter max and leave min, including early rejection, are
   monotone. Enclosing AABB overlap has the same property. If the original hull
   predicate accepts, every new ancestor union accepts. This proves no false
   negative relative to the hull predicate, including finite overflow cases.
4. The old ancestor predicates remain authoritative. For each hull we first
   clip a copy of the current Trace. If any of its six output fields changes,
   every old ancestor and leaf must accept before that proposal is committed.
   A query-local direct-mapped cache remembers exact node IDs and their complete
   path acceptance; collisions repeat work. A no-change proposal has no effect,
   so omitting its ancestry validation is observationally identical.
5. Induct over the original ranks: a previously rejected hull has no effect;
   every previously admitted hull is discovered, receives the identical prior
   Trace, and either leaves it unchanged or commits exactly the legacy result.
   All six final fields therefore match as bits, including ties and solid flags.

A 256-candidate overflow discards the partial gathering before any clipping and
runs the original tree. SAH depth is capped at 48; the original balanced tree has
at most 32 levels under the checked 31-bit index limit. Scratch is bounded and
thread-local to the call stack, with no query allocation or persistent cache.

Settled supports, NPC static query reuse, mutable collision, ignored weapon IDs,
acoustic static caching and camera completion retain their original order and
lifetime. Liquids/masks/hull generation are unchanged. This design needs IEEE
monotone arithmetic without unsafe fast-math reassociation, as in the existing
Rust builds. Finite differential tests supplement this structural argument;
hardware conformance and performance are still console experiments.
