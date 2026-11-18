# Extra level cue prefetch (2026-11-18)

`audio_level_cues_extra` (default off) makes `audio_cue_prefetch` also decode each
map's measured first-use clips on the load path, inside the same 6 MiB cue budget.

Why it exists: on the console the first play of some sounds reads and decodes the
WAV on the main thread inside `audio_mix`. The RC19 qualification measured garden3
`quake1.wav` costing one 206 ms frame (frame 809 of `a4-garden3-01`) and
`marble1.wav` one 54 ms frame (668); `rw-20261116-wchess1-01` paid 36.6 ms for
`quit_c.wav` (27) and 28.2 ms for `submerge.wav` (465); the cat's `appear.wav`
cost 67-69 ms in gvillage, tower1/2/3 and wforest. None of these is named by an
authored speaker list, an actor animation or a code table, so every first use
spiked a frame.

The per-map list (`measured_level_cues`, `audio.rs`) is the `runtime:` decode set
of the RC18/RC19 qualification logs (36 map entrances, 2026-11-16/18), in measured
first-use order. A clip is listed only where the map's measured RC19 cue prefetch
leaves `PREFETCH_BYTES` room for it, so the key-on prefetch never displaces a clip
that RC19 prefetched. The clips that do not fit (centipede1's antlion
`anl_painsquirm.wav`, the evil-mushroom set, the mock-turtle and walking-rock
voices, Alice's larger fidgets: every map whose cue prefetch is already at the cap)
and the jabberwock idle loop (12.3 MB decoded, `finale_checkpoint_background`'s
jlair2 window) stay on the ordinary runtime read-and-decode path.

Nothing else changes: same samples, same mix, same game state. With the key off
the candidate list is byte-identical to RC19; with it on the extras decode once
during load through the same bounded `prefetch_level_clips`, and a clip that would
cross the budget is skipped there exactly as before.

Host proof lives outside Git, in the task evidence folder. Console speed and heap
are measured by the device slot only.
