# r3j audio follow-up

Base: r3i ff7a4b2. This candidate has no console acceptance yet. The human
build remains the verified original r3f pair. Keep the r3i qualification gaps
explicit: hedge3 audio timing, film-skip starvation classification, mandatory
pickup windows and the natural village-to-Pandemonium transition.

## Changes and interpretation

The block mixer now distinguishes dead, silent and contributing outputs.
Virtual ambience still advances its PCM position. A deselected audible source
still renders its fade-out block, and a returning source its fade-in block.
Paused, empty and zero-gain blocks avoid summing known silence. Source order,
ambience ranking, gain curves, music and speech protection stay the same.

`pcm_flush_silence_blocks` identifies the first unavailable PCM block at an
intentional prefetch epoch change. It is a subset of the existing
`pcm_starved_blocks` total, which remains unchanged. The next missing block is
still counted normally. This is diagnostic attribution, not an audio repair or
a relaxation of a qualification threshold. These independently sampled counters
can straddle a reporter-window boundary; inspect the surrounding windows too.
Use the raw total to evaluate the existing starvation gate.

## Console ownership and preservation

Run only after the current P.T. E1b owner has finished and its restoration
readbacks have been inspected. Never run a second console pilot. Read the r3i
protocol and the final-r3f records in reference-build-r3i/console first.

Use the original vitacompanion 1.07 and the shared locked client
`~/vita-j0/bin/vita-ftp` / `vita_ftp.Session`, with `VITA_FTP_LOG` per run,
one connection and short sessions. No reboot, MKD, VPK installer, promotion,
system configuration or asset transfers. Reuse the existing capture driver,
keepalive and watchdog. Back up the actual starting SELF/config pair and only
the private files this run can change; record original absences. P.T. stays
closed and untouched throughout this Alice pilot.

On network loss, close sessions, log the drop, and wait up to 15 minutes,
checking at most every 30 seconds. Once 1338 and the FTP banner both recover,
verify app/manifest state and resume; repeat an interrupted measurement window.
No FTP or capture inside measured windows. At handoff, restore the original
human pair, saves/settings/profile files and absences with hash readbacks,
release inputs, verify no-sleep/keep-screen and leave Alice closed.

## Focused measurements

1. Verify package hashes, ELF checks and the source revision. Pair the SELF
   with its supplied reference config, profiler off and capture disabled.
2. At the same hedge3 entrance pose, compare r3i and r3j in at least three
   alternating matched pairs. Match load completion, menu dwell, listener pose
   and camera. Settle for 10 seconds and retain 90 seconds per arm with no
   inputs, FTP or captures. Keep actual clocks, device rate, priorities and
   affinity equal to the r3i policy. Retain raw audio/stage logs, producer
   per-window p95/max, gaps, starvation, music underruns and memory peaks.
   Require producer p95 <=8 ms, PCM-starved blocks=0, music underruns=0, no
   allocation failures and heap below 180 MiB. Host speedups do not pass this
   console gate.
3. Check walking, pause/resume, cull-boundary fades and voice/music continuity
   in hedge3 and village. Record any audible comparison that was actually
   listened to; counters alone do not prove listening quality.
4. Repeat opening-film skip with r3i and r3j from the same state, at least
   three times each. Preserve skip/purge/epoch logs and every starvation
   window. Report total starvation and the flush subset separately. A missing
   block later than the flush remains an unresolved underrun; a new counter
   does not turn an old failed gate into a pass.
5. The r3i pickup and natural-transition coverage remains mandatory for full
   qualification. Use a documented saved pose or one bounded traversal.
   Stop a navigation loop after 20 minutes without meaningful progress;
   record location/evidence and the missing coverage. Chapters is not a
   substitute for a natural exit. Do not force progression or alter physics.

This focused pass always restores the original r3f human build. Its report
must distinguish a measured optimization, diagnostic attribution, failed
thresholds, unrun coverage and a fully verified final restoration.
