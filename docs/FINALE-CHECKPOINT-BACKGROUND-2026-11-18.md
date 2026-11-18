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
- One worker writes the finale slot first and the auto slot second. An error in the
  first write ends the job. Each slot uses the existing validation, atomic write and
  backup rotation.
- The viewer holds the request pending until both writes succeed. On success it
  acknowledges the controllers, restores the requesting frame's post-save snapshot
  as the death retry point, and shows “Checkpoint saved”. On failure the request
  stays set and the viewer reports the save error.
- The writer runs at requested Vita priority 191 with inherited affinity and logs the
  kernel readback once at startup. The viewer uses the foreground path if the thread
  fails to spawn. Vita timing and audio contention require device measurements.
- Death retry, load, menu save listing, foreground saves, New Game, and exit wait for
  the pending job and apply its completion first. The writer also drains and joins
  on drop, including early exits.
- Replay discard mode still builds both envelopes and logs their slot, map and byte
  count. A host diagnostic can enable writes with `LOOKING_GLASS_REPLAY_WRITES=1`
  and an explicit, fresh `--save-dir` distinct from the default save directory. The
  directory's parent must exist so the replay can reserve the destination exclusively.

With the key off, the viewer writes on the calling frame. Every shipped config defaults
to this path. Host proof lives in the task evidence folder outside Git.
