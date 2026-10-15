# Reference build r2 validation and handoff

Runtime `90a2b823e5faa1a978bb8734e8aa114c1b99e6db` on `vita-reference`, from r1 `37d6f58` (pkg-2 `86792fe`).
Controls `a8bb3fa`; cache/audio pin `999da0c`; inventory confirmation `f8d536d`;
final runtime `90a2b82` loads non-level effects on demand.
CPAL `ca89c9b2055e9bd48a77914362f4859c8f774c89` is pushed on `vita-reference-r2` from the isolated `cpal-ref` clone.
Only CPAL's Cargo.lock source changed; miniquad, renderer, build.rs and main entry
remain at pkg-2. No game data, package or save was committed.

Cross jumps/climbs/swims up and confirms menus; Square uses; Triangle retries or
skips; Circle descends/backs out; R/L attack; D-pad left/right cycle toys, Up
hints, Down toggles inventory; Select chapters; Start menus/pause. Opening
inventory cannot cycle on that edge; chooser Down stays in the chooser, and
Cross cannot let its duplicate pointer event overwrite the selected chapter or
inventory toy.
The analog radial deadzone is 0.24; remapped magnitude below 0.65 walks and
at/above 0.65 runs. Returning from modal/load/focus boundaries requires neutral.
Physical prompts include the HUD, chapter chooser, settings and movie paths.
Custom preferences survive; back them up before installing the supplied r2 preset.

Vita CPAL requests BGM stereo at 44100 Hz, 512 frames (11.610 ms), priority 128,
CPU1 mask 0x20000, 1048576-byte callback stack and explicit 32768 stereo volume.
It verifies effective priority/affinity/stack with sceKernelGetThreadInfo before
opening playback. CPU0 is excluded; skin workers remain 159 on CPU1/CPU2.
The mixer/device rates match, removing the 44100-to-48000 conversion. Music and
voices still decode incrementally from compressed sources in the callback.
Software late-callback and over-budget callback estimates log nonzero windows
once per second with wall-clock and elapsed timestamps; these are not hardware
underrun counters. Vita audibility, scheduling and stack sufficiency remain
console gates, not host claims.

Eager global preloading was rejected after 29 unrelated clips hit the cap in
the village (attempts/eager-cap). Level emitters/core player sounds now preload;
other effects resolve on actual requests through the existing bounded cache,
using transient playback when full. Queued effects use the existing transient playback path at the retention cap.

Retained mono f32 cache cap: 6291456 samples / 25165824 bytes (24 MiB), down from
32000000 samples / 128000000 bytes. Host village peak cache: 2014022
samples / 8056088 bytes (7.683 MiB); no preload cache-limit
failure. Growth and level-load logs include cache counts and Vita mallinfo used,
free-arena and arena bytes. Host mallinfo is explicitly unavailable. The cap
excludes compressed music/voice bytes, decoder buffers, transient playback and
active loops and retiring sinks; transition peaks and non-village fit need console observation.

Host suites: 848 feature / 784 default passed, zero failed, two inherited ignored
asset cases each; the exact source hashes match the runtime commit. Two CPAL
algorithm tests pass (extracted actual deadline/PCM code, no hardware simulation).
All 15 J20 PNGs are byte-identical to pkg-2 with audio disabled. Preview
persistence/menu, weapon inputs, 241 advancing village frames without periodic
preview capture, and the every-frame capture witness pass. The real host output
test passes its measured playback intervals; its app-only capture is local evidence.

Native Mac package uses nightly-2026-10-11, ~/vitasdk, four Cargo jobs, -neon and
exact J24 vitaGL archive/header pins. 192 MiB newlib, 8 MiB main and 8 MiB game
stacks are retained. ELF margin 1479600 >=3348 bytes; process parameters,
imports, regenerated VELF equality, ZIP CRC and embedded SELF/SFO identities pass.
VPK SHA-256: `0af86c240448fdc73ef61ae2005f46150deee6967274011be998357d6c4b9e67`.
SELF SHA-256: `05a0e55d1117f4f0ef32e207b056f55536a7e71aeb871469f84aabf84422571b`.

Evidence: `~/vita-j0/atoz/reference-build-r2/`; r1 was not changed. `PROTOCOL.md`
adds thumbnail shot-driver steps, worker_probe readback, heap/cache and menu/
village loads versus pkg-2's 49.7 s, timestamped human listening, explicit input
regressions and the silent-console stop rule. No console or remote-server work
was performed. Physical reachability, audio quality/CPU/heap/latency, save/Continue,
three timing launches versus 36.63 ms, five-minute soak, movies and progression
remain unaccepted until the owner's console protocol is completed.
