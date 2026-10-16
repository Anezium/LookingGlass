# Restore and village entry diagnostics

The r3c human session establishes a black Load Save result despite 310 submitted
frames. It does not establish the cause. The saved reference Autosave has no
active village entry/fade. Restoring paused should still draw world and HUD.
No restore semantics or renderer defaults are changed on that assumption.

The additional Chapters -> village evidence (princesslog.txt lines 5542-5994)
selects a fresh initial entry, not an opening RoQ. The front end explicitly sets
opening_requested=false for Chapters. After `Story gvillage: 8 conversations`,
the initial viewer loads Alice, aim pointer and village LevelArt. The log contains
Alice, more texture uploads, then the 720x408 scale target and opaque world depth
marker. It therefore reaches the first world rendering path, rather than stopping
inside film validation. No ten-frame window follows in the supplied interval.
The village entry is an in-engine cinematic and uses the same wall-time skip
action as RoQ, including the film-only handheld Cross alias.

The audio callback runs on CPU1 at priority 128; skin worker zero also uses CPU1
and waits are synchronous. About 69-73 over-budget callbacks per second can
starve lower-priority skin work. This is a source-supported starvation hypothesis,
not proof of where the main thread stopped. Creating music/ambient sinks paused,
gating their updates until an actual viewer yield, and caching filter coefficients
remove that known loading DSP from New Game, restore and Chapters alike.

The 115,768-byte free-arena reading is unused space inside that malloc arena,
not all remaining capacity of the reserved 192 MiB heap. Later readings grow the
arena and retain about 96-97 MiB used. It does not prove an allocation failure.
Do not treat it as an OOM diagnosis.

One-shot `[lg/load-diagnostic]` markers bracket first camera preparation, skin
gather, world submission, worker collection, actor work, upscale and HUD queuing.
They report current framebuffer binding/completeness, viewport, existing/new GL
errors, allocated scale target dimensions, camera pose, actual world draw/triangle
counts and whether HUD draw calls were queued. Queries do not flush, read pixels,
bind targets or repair state. A final `frame-presented` sample follows the actual
frame yield; neither queued draws nor swap completion prove visible pixels.
`entry_after_story` and `entered_resources` split the formerly uninstrumented
resource phase. `[lg/audio-load] scene_presented=true` follows the real frame yield.

Use `configs/PL-r3d-baseline.json` first, then separate `PL-r3d-actor-prepare-off.json`,
`PL-r3d-native-scale.json`, and `PL-r3d-preview-every-frame.json` arms if the image stays black.
Each arm changes exactly its named field. Capture the displayed output and retain
all stage markers; the last completed stage localizes any remaining first-frame
stall. The exact menu interaction, Vita GL state and priority behavior require
console observation. A host successful restore cannot certify the console image.
