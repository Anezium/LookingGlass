# r11b amendments — fixed before console data, console NOT EXECUTED

Read this file together with AMENDMENT-R11.md. CONSOLE-BRIEF.md remains sealed
and unchanged. These amendments supersede its conflicting statements.

1. The native pass rule is no crash, ordered successful operations, all required
   before/released/after samples, sampled uordblks strictly below 170 MiB and
   sampled arena strictly below 179 MiB. Any sample or peak at either limit is
   FAIL_OR_UNKNOWN. Missing, inconsistent, unfinished or mismatched records fail.
   Every transition reports arena, fordblks, arena minus uordblks and vitaGL pool
   free values; pool values are descriptive. The 179 MiB ceiling is below the
   historical r8 arena failure at 187,994,112 bytes (179.285 MiB), with a 299,008
   byte margin. It is an experimental stop threshold, not a guarantee that a large
   contiguous allocation fits. Freed holes and external allocations still matter.
   The review's 172.1 MiB chapter-to-Hedge3 host upper bound becomes 172.26 MiB
   in r11b with surviving common surface recipes preserved for correct rendering.
   This conservative model excludes C/GL/OS memory. The expected native sampled
   used peak is approximately 157–160
   MiB, inferred from host/native steady-state differences, not measured. The
   170 MiB used gate is intentionally stricter than the host model; do not relax
   either gate after seeing console data. A failure is useful evidence.
2. Black screens during native Load, chapter load and Retry are expected during
   the synchronous build. Natural exits retain the original Loading/title image.
   Wait at most 100 seconds after the confirmed operation; no end record by then
   is a hang/FAIL_OR_UNKNOWN. Do not replay the input. A black frame within this
   window alone is not ambiguous UI. The startup bracket now begins after the
   chooser resolves the real destination; title-menu dwell is excluded.
3. The checked native command table contains hashtree, help, launch, nosleep,
   press, quit, reboot, release, screen, version and wait. Wait is native as well
   as accepted by the input sequencer. Reboot is explicitly forbidden. No new
   native commands are authorized by documenting their existence.
4. cache_qual_command.json is not read by this engine and is no longer staged.
   Any original human file remains covered by preservation/restoration.
5. Run A with all three levers OFF, B with all three ON. Each arm still requires
   chapter Hedge3 → native human Auto Load (Pandemonium) → chapter Hedge3, in
   that order. After the return, Retry is optional only if an actual death is
   safely available through existing inspected inputs. No verified safe death
   route is currently available: do not invent movement, damage, cheats or a
   replacement save. Record `operate.py LABEL retry-unavailable` before collect
   when death cannot safely be reached. If an inspected natural death does
   occur, select native Retry with the existing Cross input, inspect/ack, and
   `wait-transition retry-hedge3`; this must be the fourth successful transition
   with the same gates. Never manufacture a Retry completion. The skip is
   explicit in AUDIT.json and does not qualify native Retry memory.
6. Preserve M3, M4B, en0 admission, fixed lifetime/budget, serial FTP and no MKD.
   Restore human SELF 387ccf678251d4267b8ced53e207830358826f8e0642631a0459c6b0b9939990,
   Auto ea4cd221d9f950e613188ccf3ecf2c1eabb1d34106f413125864320d364c8fa9,
   all companions, preferences.json and audio-settings.txt with independent
   readbacks and original absences. The future console owner alone executes
   this protocol. Preparation and offline rehearsal do not contact the console.
