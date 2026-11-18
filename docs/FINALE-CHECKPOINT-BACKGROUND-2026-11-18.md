# Background finale checkpoint writer (2026-11-18)

`finale_checkpoint_background` (default off) moves the boss-fight checkpoint's serialization
and file writes off the requesting frame, without changing the files it produces.

Why it exists: when the jlair2 fight starts, one frame costs 484.55 ms on the console, and
381.89 ms of it is the finale checkpoint (`finish_intro` sets the checkpoint flag,
`jabberwock.rs`; the viewer writes it inside the hud bracket). The write serializes the whole
game twice, 116,816 bytes each: once for the separate `finale-checkpoint` store and once for
the automatic slot. A qualification replay discards the files, so that cost is pure CPU; a
player's card also pays two atomic writes and their backup rotation.

What the key does, with it on:

- Every envelope input is read at the frame boundary where the two writes run today: the game
  snapshot and the preview JPEG arrive as arguments, and the two envelope clocks are read in
  the submit, one per store, mirroring the two reads the writes make today. The files cannot
  depend on when the writer gets to them; only the wall-clock `saved_at` differs from the
  frame path by its own nature, exactly as between any two runs of the same tape.
- Today the two envelopes are not byte-identical (their `saved_at` reads straddle a whole
  serialization), so each is serialized separately as today, on the worker, with its own
  captured timestamp.
- Serialization and the file writes run on one low-priority writer thread (the audio
  producer's pattern: `std::thread::Builder`, a fixed stack, `thread_policy(191, 0)` at
  entry — lowest application priority on any core, so it may run late, never early). The
  atomic write, the backup rotation and every save validation are the same code, unchanged.
- A later save request for the same store waits for a pending job, or supersedes it in place
  while it is still queued; the exit path flushes pending writes before the final autosave
  and quit, and the store's drop flushes again as the backstop.
- `discard_writes` still logs both `save not written` lines, with the same slot, map and byte
  count; the lines come from the same code, off the frame.

What the key does not change: with it off (the default, and every shipped config) no writer
thread exists and every write stays on the calling frame, byte for byte RC19. Ordinary saves,
quick saves and the exit autosave never use the writer; a background job's failure is
reported on a later frame's tail instead of the requesting frame.

Host proof lives outside Git, in the task evidence folder. Console speed is measured by the
device slot only.
