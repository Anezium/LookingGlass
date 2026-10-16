# r3e software mix

The r3d producer missed its 11.61 ms grain despite an inexpensive callback.
The host probe `audio::tests::mix_profile` uses the authored 20 village emitters,
the restored village listener pose, level music and deterministic footsteps.
It executes the same 512-frame blocks for 20.9 simulated seconds, discarding the
first 90 blocks; no device, simulation advancement or wall-clock decoder pacing
is substituted for actual source/DSP work. Exact instantaneous console transient
identities are unavailable; the observed live-sink peak was 26. Calibration is a
workload estimate, not proof of Vita cost or preemption behavior.

The new path separates bounded stream decode from rational-phase resampling,
fetches effects from a mono 44.1-kHz cache, performs stereo spatial/filter DSP in
tight loops, sums voice blocks, and retains the existing limiter/monitor. Effect
decode and effect resampling happen at load, not on the producer. The cache keeps
the existing 24 MiB limit after resampling. Stream decode uses bounded scratch;
no whole-music PCM allocation or decoder thread is introduced.

`audio_block_mix`, `audio_preresample`, `audio_block_parameters` all default off.
`audio_ambient_voices=0` and `audio_cull_gain=0` disable ambience limiting.
The explicit r3e candidate enables all three switches, keeps the loudest eight
ambient sinks above gain 0.001, and always retains music, speech, player and
other gameplay effect sinks. Distance/obstruction already feed sink gain.
Virtual ambience advances its play position, with its old reflection tail reset;
reactivation can change the ambience sound and requires listening qualification.
The cap is an ambience cap, not a total gameplay-effect cap.

`audio_producer_priority=161` and `audio_producer_affinity=262144` keep the r3d
placement. Priority is restricted to 160–191, below skin workers at 159. Affinity
selects CPU0/1/2 explicitly. Requested/effective Vita policy is logged; failure
stops the worker and fails console qualification. Host policy stays normal.
`audio_cushion_blocks` defaults to two and accepts one through four; candidate
four adds 23.22 ms prefetch relative to r3d (46.44 ms total). Generation flushes
remain bounded to four queue slots; loading audio pause and film skip survive.

Every producer second logs per-stage p50/p95/max in milliseconds per block.
`conversion` is the residual limiter/monitor, sample delivery, timing bookkeeping
and unclassified overhead after exclusive source/DSP/summing/control intervals;
it does not include CPAL's device-format conversion, which runs in the callback.
Stage percentiles are individual distributions and must not be added as pooled
percentiles. Disabled legacy paths retain total-mix telemetry; differential host
source probes attribute their nested PCM/filter/resampler costs without inserting
a clock into every sample. The no-preresample arm labels its nested legacy
source work as effect source/decode, including embedded DSP; compare total costs.

The reproducible measurements, load-gate logs, suite results, package hashes and
full report are under `~/vita-j0/atoz/reference-build-r3e/`. Hardware alternatives
and engineering estimates are in `REFERENCE-R3E-HARDWARE-OPTIONS.md`.
Console procedure: `REFERENCE-R3E-PROTOCOL.md`. No console gate is certified here.
