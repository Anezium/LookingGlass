# r3k film PCM source candidate

Base: e484ceb0f85741661599612a583debd007be50f8 (r3j). This is an
uncommitted source candidate. No Vita build, package, device contact, install,
listening, timing benchmark, or qualification is implied.

`movie_sound` publishes an Output with an immutable movie role and unique
non-recycled instance ID. Creating a new instance revokes the previous one;
stop/drop irrevocably revokes only its own ID. A movie Sink represents one film
lifetime; append/play after stop does not rearm that ID. Ordinary Sink reuse
keeps its existing generation semantics.

The producer renders every source once, in its existing order, and accumulates
both the normal sum and (only for a movie interval) the ordinary sources in
the same relative order. Both sums cover identical PCM indices and are limited
independently before queue publication. There is no post-limiter subtraction.
The no-movie path does not perform a second sum or limiter pass. It still pays
the larger fixed payload/storage/copy cost; this has no measured Vita timing
claim. Existing r3j silent-source, ranking, fade and phase behavior remains.

A block tags the actual immutable movie ID selected before source rendering.
Only that ID may contribute to its normal premix. Superseded Outputs cannot
add a second film. A replacement published during rendering starts at its own
first pair in the next interval; block mixing admits pending sources once per
interval. Buffered legacy sample mixing defers a different, currently active
movie to the next interval while ordinary sinks retain per-sample admission.
A replacement published after epoch read but before source rendering may start
in the current interval. No old block is retagged with a later control ID.

At the next stereo-pair frontier observing revocation, the callback chooses
the already limited movie-free premix, including a partially consumed block.
An odd stop completes the current L/R pair with its previous choice. PCM
indices keep advancing without repeat/skip in other sounds. The callback
performs only bounded PCM copies, queue pops and atomics; no source rendering,
limiter, allocation, source destructor, wait, lock or I/O was added.

Film quit/skip/natural end now calls the real Sink stop without a common PCM
flush. Its log explicitly says movie revocation and no prefetch flush. Genuine
global `Control::flush` still invalidates the entire old epoch with the same
at-most-eight-pop search. The raw missing-block predicate, increment, reset,
reporter and flush-subset attribution are unchanged. A real empty queue still
returns a zero fallback and increments raw starvation. Valid zero producer PCM
is accepted without starvation. No grace or new prefill was introduced.

Capture/monitor samples describe the producer's normal limited premix. A later
callback revocation can choose the other premix, so capture is not a recording
of the final callback selection. Unbuffered playback retains its existing
Monitor/Iterator route and existing stop granularity; no claim of retroactive
selective removal is made for that mode. The backend's already submitted
samples cannot be retracted. Movie amplitude may jump on revocation and click;
future authorized listening remains necessary.

Host evidence and precise commands, fixture/source hashes, queue/layout costs,
baseline real-PCM reproduction, review and limitations are recorded under
`~/vita-j0/atoz/reference-build-r3k-film/`. The baseline reproduction uses frozen
source with test-only producer barriers, not a copied callback model. Candidate
regressions use the production Mixer, Sink, Monitor/limiter, SPSC, producer step
and Callback with bounded channels and no sleep-based race selection.

The console gates remain raw starvation=0, music underruns=0, producer p95
<=8 ms, heap <180 MiB and no allocation failure. Existing hedge3 producer gains
are baseline evidence only; the payload increase still needs native/package,
stack/heap and controlled Vita timing validation. Third-film comparison,
subjective continuity/cull fades, pickups first/repeat/overlap and the natural
village-to-Pandemonium transition remain mandatory qualification gaps.
The r3j film2 corrupt producer-gap parser field remains unrelated and untouched.
Human r3f SELF 387ccf678251d4267b8ced53e207830358826f8e0642631a0459c6b0b9939990
remains the restored human build; no r3j/r3k human installation is authorized.
The P.T. E1c pilot is the sole console owner. Root owns commit, later native
package and any future authorized console work.
