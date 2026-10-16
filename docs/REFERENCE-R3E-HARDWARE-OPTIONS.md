# Audio hardware follow-ups (Mac inspection, no device measurements)

The installed SDK has `psp2/audiodec.h`, `libSceAudiodec_stub.a`,
`libSceNgsUser_stub.a`, `libSceNgs_stub.a` and `psp2/ngs_internal.h`.
It does **not** have the public `psp2/ngs.h`. No SDK files were changed.
Upstream API references: [Audiodec](https://docs.vitasdk.org/group__SceAudiodecUser.html),
[NGS](https://raw.githubusercontent.com/vitasdk/vita-headers/master/include/psp2/ngs.h).

## Hardware music/speech decoding

An Audiodec adapter would parse MP3 frames/ID3 and MPEG version/channel headers,
initialize the codec library, create per-stream decoders, and feed elementary
stream bytes through `SceAudiodecCtrl`. Its consumed-input and produced-PCM byte
counts drive bounded refill. The installed interface provides 256-byte alignment
macros and 16-bit PCM output; conversion into our f32 stereo blocks and rate
normalization would remain. AT9 requires its codec configuration and superframe
metadata; it is not a drop-in replacement for the user's MP3/WAV assets.

Use a separate bounded decode-ahead worker, never the device callback or game
thread. Start at lower urgency than the producer (e.g. priority 163 versus 161),
measure decode-call wall time and starvation, and prefill before play. The API has
no per-call scheduling-priority parameter: thread policy, synchronous call time,
codec service scheduling and firmware behavior all need console verification.
Keep pause/seek/loop/reset/generation cancellation and the software fallback.

Engineering estimate, not a measured delivery promise: 400–800 lines plus tests,
1–3 working days for an MP3 proof, 3–5 for reliable seeking/looping/film parity.
Medium risk: MP3 bit reservoir, encoder delay, EOF, resource limits and blocking
codec calls. Only worthwhile as the next targeted cut if music/speech decoding
remains a material part of the measured producer budget. WAV effects are already
decoded at load and cannot benefit from replacing the music decoder.

## NGS level-mixer backend

Plan one 48-kHz system with a 512-sample grain, a 32-voice PCM rack (20 village
emitters plus protected music/speech/player slots), and a stereo master/mixer
rack. Allocate system/rack storage using size-query APIs, route voices with
patches and gain matrices, and publish pitch/filter parameters per grain. Stream
music/speech through double/triple buffers whose lifetime lasts until the DSP
has consumed them. Bridge output into the existing bounded callback path.
The public API documents 48 kHz on firmware 3.60, so this also changes the output
clock from 44.1 kHz; movie position, queue duration and audio-device setup must
all change together. `sceNgsSystemUpdate` still has synchronous completion and
callbacks on its calling thread; offload is not evidence of zero CPU/wait cost.

Estimate: 1,500–3,000 lines including FFI, rack/voice ownership, routing, buffer
callbacks, cancellation, streaming and backend parity tests; 1–2 weeks plus
console qualification. High integration risk: incomplete installed public types,
parameter layouts, output extraction, sample clocks, voice eviction, filter and
reflection matching, teardown during skip/load. It could replace most software
voice DSP/resampling/summing work, whereas Audiodec only replaces decode.

Tonight's decision is recorded with the measured stage breakdown in
`REFERENCE-R3E-AUDIO.md`: qualify the smaller software changes first. Neither
hardware option is implemented or claimed to meet a measured Vita budget.
