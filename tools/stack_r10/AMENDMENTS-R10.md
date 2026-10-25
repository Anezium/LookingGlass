# r10 amendments

These replace the r8 lever description. The inherited admission, FTP, restoration and budget procedure still applies, with the names and checks below.

1. Session directories, absent from the package and the ZIP: `session-s10-stack-load-smoke`, then `session-s10-stack`. Do not reuse s7, s8 or s9 names.

2. One common SELF. A is PL-baseline.json. B is PL-stack.json. The only effective wall difference is audio_static_cache, collision_sah and skin_actual_consumers, all false on A and all true on B. Census arms add phase_census=true on both sides and do not change those three switches.

3. Smoke is two B-only launches, Hedge3 then Pandemonium, full 60+120, excluded from the wall comparison. Each B log must contain exactly one COLLISION_SAH_LOAD line: Hedge3 hulls=108424, Pandemonium hulls=8211. Hedge3 must also contain one `footing source=shared_static` line and no legacy footing line. Heap uses the same 180 MiB whole-run stop and 165 MiB Hedge3 plateau.

4. Wall order is Hedge3 ABBA, then Pandemonium ABBA, then Hedge3 census A and census B. Reading rules are in READING-RULES.md and were fixed before any r10 console data. The separate gains listed there are not a prediction.

5. Log integrity delta inside a session is 3 bytes, from protocol field config_record_A_minus_B_bytes. The twelve inherited fixtures still require 1 byte. The fixture runner removes that field before replaying them.

6. Census schema is 3. Cache hit+miss must equal queries only when audio_static_cache is true. When it is false, hit, miss and eviction stay zero even if queries are nonzero. Actor rows and static traversal columns are both required.

7. vitacompanion commands are only those in cmd_definitions.c: hashtree, help, launch, nosleep, press, quit, reboot, release, screen, version, wait. This package does not issue reboot or appstate. The e2e driver allows vita_cmd raw `release all` only.

8. Offline proof uses PYTHONDONTWRITEBYTECODE=1 and the venv interpreter. Do not pass python -I. Full mode requires both session paths absent. --main-admission accepts a completed smoke session at the exact smoke path, rejects any occupied main path, and rehearses ten launches. Output stays outside the package.

9. Restoration identities are unchanged: human SELF 387ccf678251d4267b8ced53e207830358826f8e0642631a0459c6b0b9939990 (37371674 bytes) and save ea4cd221d9f950e613188ccf3ecf2c1eabb1d34106f413125864320d364c8fa9 (181900 bytes, pandemonium$first).

10. Budgets follow the r8 arithmetic. Smoke: 2 launches, 1 Pandemonium, 69 minutes. Main with both census arms: 10 launches, 4 Pandemonium, 145.5 minutes. Cap 150 minutes. Leave more than 30 minutes before driver expiry. en0 preflight, one FTP connection, no MKD, no reboot, no storagemgr.
