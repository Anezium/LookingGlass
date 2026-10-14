# Live actor preparation accounting

The `actor_prep_profile` field in `private/vita_renderer.json` defaults to false.
It enables exclusive game-thread timers, record-copy/comparison counters and the
corrected deferred-tail boundary. Skin arithmetic and the five-slot GPU ring
are unchanged. No renderer handles are given to workers.

On a native build with `--features vita_renderer`, set
`LOOKING_GLASS_ACTOR_REPLAY=1` and launch the ordinary viewer with
`--load auto --no-audio --frames 180 --save-dir private/saves`.
The switch must be true. The replay uses the normal restoration, simulation,
physics, character, scene and submission path. It bypasses the load pause,
uses neutral input, and alternates exactly `1_f32/30.` and `0.0366_f32` seconds.
It rejects a frame that leaves active gameplay. Save writes and preview readbacks
are disabled in this diagnostic; the supplied primary autosave remains external.

The JSON output is `private/actor-replay/samples.json`. Discard frames 1–60 and
retain all of frames 61–180, including stalls. The output records exclusive
nanoseconds, record bytes copied/visited by instrumented comparisons, logical
skin draws, total game-thread allocation calls, final submission/upload counters,
worker service sum/maximum, dispatch-to-last-numeric-completion makespan, and
collection wait. Scalar/pointer validation and metadata writes are not included
in byte counts. The render tail's Linux hooks remain unavailable, so native CPU
submission timings do not measure the complete framework tail or GPU completion.

The categories are exclusive: nested consume, publication, modifier and packet
spans are removed from their enclosing discovery/gather span. Contact shadow
and atmosphere have a separate category. Actor categories exclude later pickup
and particle submissions. Simulation blocks have independent spans. Whole-game
snapshot serialization and output formatting occur after the CPU/allocation
window. Canonical simulation hashes allow corresponding replay frames to be
compared without HashMap serialization-order differences.

The deferred-tail correction records a pending packet's boundary at `cpu_end`.
Only the suffix after that boundary contributes extra frame CPU time. The whole
packet remains attributed to its render phase. Both measurement arms must use
the same switch setting; the change is an accounting repair, not a claimed speedup.
A synthetic straddling-packet test checks a 10 µs packet with a 6 µs CPU prefix:
its extra CPU contribution is 4 µs. Separate tests check exclusive nesting and
both prescribed dt values.

Milestone evidence and reproduction scripts are stored outside Git at
`/srv/fast/projects/alice/jalons/jalon-23/`. No game data or save payload is added
to the repository. The final milestone report records the phase-1 stop verdict;
subsequent actor/puff work and packaging are conditional on that gate.
