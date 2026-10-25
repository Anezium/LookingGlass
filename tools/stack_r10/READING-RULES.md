# Rules fixed before r10 console data

One common SELF. Wall arm A has audio_static_cache, collision_sah and skin_actual_consumers all false. Wall arm B has all three true. phase_census is false on every wall arm. A B-only load smoke on Hedge3, then Pandemonium, uses the B config and must pass before any wall launch. Then Hedge3 ABBA, Pandemonium ABBA, then one Hedge3 census run per arm. Census runs are a separate logging session. They are not wall arms.

Both Hedge3 oriented median deltas, B2-A1 and B3-A4, must be strictly below -1.000 ms. Both Pandemonium deltas must be at or under +1.000 ms. No pooled arms, trimming, observer correction, or census timing in the wall gate.

Separate idle gains, each lever alone, are descriptive only. Do not add them and do not treat the sum as the wall result. Audio and SAH overlap: the r8 SAH gain was measured with the acoustic cache off, so part of that gain is static audio work the cache also removes.

- Hedge3 audio, r6: -4.7 ms
- Hedge3 SAH, r8: -4.3 ms
- Pandemonium SAH, r8: -1.6 ms
- Pandemonium skin, r7: -4.0 ms

The stacked wall measurement is its own number. Census on Hedge3 may report static traversal and skin serial or worker counts. Those counts are diagnostic.

Expected Hedge3 retained B-A is about +3.23 MiB: 3.196 MiB of SAH storage plus about 0.030 MiB of static audio cache. Idle Hedge3 skin banks are not expected to add another full bank table. The planning range is +3.0 to +6.5 MiB if skin banks do land. The gate is stricter than the hope and looser than the plan: both oriented benchmark heap deltas must be at or under +8 MiB, every Hedge3 benchmark plateau must be under 165 MiB, and any whole-run newlib uordblks sample above 180 MiB stops the session, including load and shutdown. A loading sample and a benchmark plateau are reported separately. The process heap stays 192 MiB with an 8 MiB main stack. A smoke cannot prove a continuous peak.

Sharing the main world's static geometry removes the second Hedge3 SAH build and the second hull tree. The old 161 MiB load sample was that throwaway world. The new load peak is unmeasured until the smoke. Do not carry forward a 170 MiB two-build figure.

late_callbacks is report-only. Benchmark underrun_estimates, starvation, dropped reports and rejected sinks stop. Producer p95 must be strictly under 8 ms. Music underrun_frames stop. A crash or OOM stops.

Log integrity compares config-record length inside one session. Wall A minus wall B is 3 bytes, one byte for each of the three booleans. Census A minus census B is also 3 bytes, because phase_census is true on both census arms. The inherited fixtures still expect 1 byte because the fixture runner drops config_record_A_minus_B_bytes before replaying them.

This package must not be executed by the implementation worker. Preserve arm-aware log integrity, benchmark-window heap, complete census restoration, fresh session vacancy, the offline main-path test, en0 preflight, serial locked FTP, no MKD, and restoration of human SELF 387ccf678251d4267b8ced53e207830358826f8e0642631a0459c6b0b9939990 and save ea4cd221d9f950e613188ccf3ecf2c1eabb1d34106f413125864320d364c8fa9.
