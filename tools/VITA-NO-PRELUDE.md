# Optional intrinsic prelude removal

The release source pin remains SceShaccCgExt `d3c83efceedeca3be863d0c1f19bb145daf1685f`.
`vita_shacccg_no_prelude.py` exports tracked files from that revision, excludes
`include/sce_intrinsics.h`, and applies `patches/SceShaccCgExt-no-prelude.patch`.
The patch contains no copy of the excluded text. Modified files carry dated
change notices. `SceShaccCgExt-release.json` pins the patch and release archive.

The patch removes the prelude load, its C include and its CMake entry. It retains
standard-library loading (unless `nostdlib`), optional caller source, the extension
flag, pragma handling, unload handling and secondary-register-limit support.
The loader returns false on a retained load failure and true after all required
loads succeed. It never substitutes an empty prelude.

The application adds `shader_cache_namespace`, an empty string by default.
Nonempty diagnostic values are limited to 64 ASCII letters, digits, hyphens and
underscores. They select a child of the existing shader-cache path; the game
creates it. Empty keeps the original cache location and key computation.

`vita_no_prelude_driver.py` is root's console comparison driver. It requires an
external READY.json package, existing root-owned rc1kit backup/log session, and
matching SELF/config/tape hashes. It starts no receiver or terminal. Each tape
and library gets a separate fresh cache namespace. It verifies cold successful
shader links, GETs every logged cache key, validates envelopes and compares
payload hashes. It restores with `--keep-saves`, so no FTP deletion is needed.
Optional existing gvillage/hatter1 tapes extend the mandatory a2-04 replay.

Host evidence and the private root-only console package are recorded in
`/Users/anezium/vita-j0/plan-2026-11-12/a6-lgpl/NO-PRELUDE-REPORT.md` on the
release host. Console qualification is a separate root-owned step. Runtime
coverage from replays does not enumerate every configuration-dependent shader
variant. Other licence-audit findings are outside this patch's scope.
