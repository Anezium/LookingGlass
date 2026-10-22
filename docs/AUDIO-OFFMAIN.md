# Optional acoustic preparation on the PCM producer

`audio_offmain` defaults to `false` and requires `audio_pcm_worker=true`.
It changes acoustic static collision preparation, not the PCM mixer/DSP,
resampler, decoder, source order, output format or buffer cushion.

The viewer's `frame_audio_mix` label covers `Audio::prepare` (pending clip
resolution/decoding, room and obstruction queries, acoustic controls) and
`Audio::update_clocks` (music fades/ducking, pause/mute, speaker scheduling,
spatial pan and sink gain). PCM mixing already runs on the audio producer.

When enabled, the first acoustic preparation after each level load makes an
immutable copy of the static hulls and BVH. A single reusable packet holds up
to 256 point queries and their results. The existing producer executes at most
one static query per turn, only when its configured PCM cushion is full. It
checks PCM demand again before the next query. No game World, lazy collider,
asset archive, sink, voice schedule or renderer state crosses that boundary.

Main completes each static result against this frame's settled supports,
dynamic colliders and weapon obstacles, in the original clipping order. Room,
obstruction smoothing, random events and controls keep their existing update
order. Identical query endpoints reuse exact static results; live geometry is
still checked every frame. There is no stale-frame control approximation.

Main waits for this frame's static results, bounded to 100 ms. Late packets,
producer shutdown and packet overflow use the original complete query path.
A late packet is reclaimed before reuse; level changes invalidate its geometry.
The wait can consume any CPU-time benefit on changing views. Stationary query
reuse is the primary expected wall-time benefit; console measurements decide.

Queues, packet arrays and result storage are allocated once at mixer creation.
Static geometry is allocated once per scene on its first preparation. Steady
submission, trace service, completion and cache reuse add no allocations.
Pending request storage now retains its existing capacity. This does not claim
that unrelated voice creation or runtime clip loading is allocation-free.

`[lg/audio-offmain]` reports activation and session totals (`submitted`, `reused`,
`fallback`, `static_traces`, `trace_max_us`, `wait_max_us`). Standard PCM
starvation, rejected-sink, producer p95/gap and music-underrun telemetry remains
unchanged. Static work is outside the PCM mix-duration bracket; inspect both
mix p95 and trace/gap timing. Accept only producer mix p95 below 8 ms, zero
PCM starvation/music underruns, no fallback during measured gameplay, and
adequate heap/stack headroom. Keep the switch off until console validation.

Host tests compare every f32 output bit at controlled control/sample boundaries
for synthetic moving-listener/mover/pause/mute/submersion and user-data maps.
They test actual production mixer samples and producer job service, without a
hardware audio device. They establish algorithmic equality; they do not model
Vita scheduling or promise identical realtime callback interleavings.
