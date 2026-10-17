# r3f audio: clean voice-cap transitions, cache memory, producer stalls

r3f keeps the r3e block mix and fixes the r3e review findings, then adds four
console-driven changes after r3e's step 0 showed the producer missing its
cushion about once a second (67 starved blocks in 64 of 96 windows) although
no stage cost ever exceeded one 11.61 ms block.

## Review fixes (always on in the block path)

- **Voice cap.** Each block ramps its applied gain linearly from the previous
  block's value. A voice the loudest-eight cap drops renders one more block while
  it fades out; a returning voice fades in from silence over its first block, so
  the filter and reflection state cleared while it was virtual starts under the
  fade. Held voices rank 1.25x louder (about 2 dB), so two emitters of
  near-equal gain around 8th place no longer swap every block. The same ramp
  smooths every per-block volume change.
- **Pan and reflections.** Effect pan and the block-parameter wet gain move
  linearly across the block instead of stepping at its start. Constant
  controls produce exactly the r3e samples.
- **Cache memory.** Load-time pre-resampling doubled every 22.05 kHz clip.
  With it on, funhouse, hatter1, hedge2 and hedge3 overflowed the 24 MiB level
  cache during preload (90 clips rejected, two hedge3 emitters lost). Clips at
  22.05 or 11.025 kHz (all 1,896 WAVs in the archives are 22.05 or 44.1 kHz) now
  stay native in the cache; the block effect interpolates them with a shift and
  a fixed weight table that reproduce the load-time resampler bit for bit. Only
  other rates are resampled at load. Preload cache bytes now equal the
  non-resampled arm on all 36 maps (largest hedge3, 15.1 MB of 24 MiB).
  `audio::tests::all_maps_preload_budget` (ignored, needs game data) reports it.
- **Speech.** Map speakers playing clips under `sound/character/`, where all
  vocal content lives, are no longer ranked as ambience and always render.

## Console-driven switches (default off)

| Switch | Effect |
|---|---|
| `audio_log_thread` | The producer packs each one-second window into a fixed-size record and pushes it through a lock-free SPSC queue to an `audio-report` thread at priority 191. The producer never formats or prints; a full queue drops and counts the record. |
| `audio_cushion_blocks` 1..8 | Eight blocks prefetch 92.88 ms: +46.44 ms over the four-block candidate, +69.66 ms over r3d. Film timing subtracts queued samples. |
| `audio_music_thread`, `audio_music_priority` (170) | Music sources go to an `audio-music` thread, less urgent than the producer and on its CPU, which decodes 1024-frame chunks into a 0.74 s PCM ring per stream. The producer copies decoded samples; an empty ring yields counted silent frames, never a late block. |

Always on: every window reports the longest interval between producer loop
wakeups, the 1 ms sleep overshoot, wakeups more than one block apart, and the
time the producer spent printing the previous window (0 on the reporter path).
Large gaps with print time point at logging; large gaps without it point at
preemption.

Configs: `PL-r3f-candidate.json` (log thread + music thread, four blocks),
`PL-r3f-cushion-8.json`, `PL-r3f-r3e-control.json` (r3e baseline settings on the
r3f binary). Console procedure and listening checklist:
`REFERENCE-R3F-PROTOCOL.md`. Host evidence is under
`~/vita-j0/atoz/reference-build-r3f/`. No console gate is certified here.
