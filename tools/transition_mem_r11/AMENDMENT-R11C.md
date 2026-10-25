# r11c amendment, fixed before console data

The original CONSOLE-BRIEF.md, SOL-REPORT.md, AMENDMENT-R11.md and
AMENDMENT-R11B.md remain historical sealed instructions. This amendment applies
to the new r11c SELF and pinned dependency sources. Console execution is NOT RUN.

1. **C1. Arena headroom label.** Pass/fail remains uordblks <170 MiB and arena
   <179 MiB, including samples outside transition brackets. For each arm report
   `arena_headroom_bytes = 187994112 - max(peak_heap_arena over every transition,
   every heap_arena= sample)`. Always report this number. A passing arm with
   headroom <4,299,008 bytes **or** peak arena >=175 MiB is labelled
   `PASS_LOW_ARENA_HEADROOM`. That says the run did not crash; it does not show
   margin and must not be cited as merge evidence for release-first.
   The review's two boundaries are not equivalent: 175 MiB leaves 4,493,312
   bytes against its 187,994,112-byte reference. This amendment conservatively
   uses the earlier 175 MiB label boundary and retains the stated byte condition;
   pass/fail is unchanged.
2. **C2. Stay on the sealed maps.** Never leave the Hedge3 -> Pandemonium ->
   Hedge3 sequence, including to look for a death route. The r11c build fixes
   render-target double deletion; that does not extend this sealed experiment
   to additional maps. `key not found` or `panicked` anywhere is FAIL: stop and
   do not retry. Retry remains optional only after an inspected natural death
   within the sealed maps and the existing safe-input rules.
3. **C3. What a PASS means.** PASS measures sampled release-first memory for
   this exact SELF on the console. This build pins the Anezium macroquad and
   miniquad forks by revision; it contains no vendored dependencies. Its SELF
   identity differs from r11b. Host proofs, including extended cycle trends and
   forced recovery failures, do not qualify native fragmentation, VRAM cleanup,
   audible continuity or Retry memory. Release qualification still requires
   the sole console owner's hardware evidence; a low-headroom PASS provides
   no merge evidence for release-first.

B1's chooser-derived begin/end identities, A1's shared arena gate, A2's expected
loading presentation and 100-second deadline, A3's native command grammar,
A4's staging exclusions, M3/M4B closure inspections, serial FTP, no MKD, fixed
budget and exact human SELF/Auto/settings restoration all remain mandatory.
