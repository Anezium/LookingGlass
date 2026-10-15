# J24 other probe

Instrumentation only, based on vita-j23-probe 7d416f0 (runtime 3b850c7).
Seven exclusive children split the old other span: world_opaque_prepare,
world_opaque_submit, power_art, world_particles_alpha, effects_first_person,
present_preview and hud_tail. Existing finish/upload/submission children remain
exclusive; other is the residual. No rendering, gameplay or scheduling change.

The light CSV preserves its first 241 J23 columns and appends 35 category fields
(exclusive_ns, calls, copied_bytes, compared_bytes, logical_draws) plus four raw
counters: actor_prep_world_opaque_submit_gl_draw_calls, _gl_uniform_calls,
_gl_bind_calls and actor_prep_present_preview_captures. Total: 280 columns.
Raw GL calls are observed at driver entry points only while opaque submit owns
the thread: glDrawElementsInstanced, sampler/float/matrix uniforms, texture/buffer
binds. Sky is prepare, not loop submission. Counters are neither API requests
nor durations. Preview counts actual grab_screen calls after a valid-size check.
Recording remains fixed-size and allocation-free, with one same-frame snapshot.
The probe-disabled arm bypasses all new clocks and counter state; the game and GL
calls retain their arguments/order. Disabled CSV placeholders remain unavailable.

Mac-only verification and package evidence are under
/Users/anezium/vita-j0/village-retained-world/jalon-24-other/.
PROTOCOL.md retains J23's three alternating normal-play pairs and frames 61-180,
adds the seven categories and reconstructed old other per-frame sum, and reports
raw counts including capture-bearing rows without deleting primary stalls.
No console access or optimization/performance result is claimed.

## Verified Mac handoff (2026-10-15)

Runtime source: `e7053e3a7d42f19864166d01ebf16548b84bfcf5`. Feature tests: 848 passed;
default tests: 782 passed; zero failures and two inherited asset ignores each.
Default check, both ownership compile-fail checks, and allocation-free snapshot/
ring recording checks pass. Both flags reproduce all 15 J23/final3 reference
PNGs byte-for-byte; all 181 fixed-dt simulation-state hashes also match J23.
Both arms export 180 contiguous 280-column CSV frames, retaining the exact first
241 J23 header identities. All 150 exclusive fields and four raw counters match
same-frame host reports. Residual other is below 0.36% of reconstructed J23 other
in every host frame (accounting coverage only). Saves remain unchanged. Host
preview copies stay suppressed by the existing J23 host gate; console occurrence
and cost remain unmeasured.

A-stock was built on the Mac using native ~/vitasdk, nightly-2026-10-11 and
CARGO_BUILD_JOBS=4, with unchanged Cargo.lock and the exact pinned vitaGL archive
`6842d410b2571fe4a31d4fe933dd76c5679330a28e80a5c27b1539a534d91b3b`
and header `5da33f7b11eadce8e8d4174ff63d94ec6e3460e03c193b4c7b722a5afb8b14d7`.
ELF margin: **1558484 bytes**, above the 3348-byte minimum.
Imports, regenerated VELF equality, ZIP CRC and embedded SELF/SFO identity pass.

- VPK SHA-256: `48c768d875a3a1aeb2af1e5907d2302a19effe5e93b2314155eeff495ea5088d`
- SELF SHA-256: `e80b96ccc879bfda012f838fd68af64cc9c44076187ba5bc8a0be489e78b9d5a`

Evidence: /Users/anezium/vita-j0/village-retained-world/jalon-24-other/.
Console protocol: PROTOCOL.md in that directory; three alternating pairs,
frames 61-180, all seven new categories and four raw counters reported per frame.
Configs PL-def (probe on), PL-def-noprobe and vita-sim-all match J23. SHA256SUMS,
HOST-CHECKS.json, HOST-VERIFICATION.json, HOST-CLOSURE.json and A-stock/VERIFICATION.json
record the checks. No console was accessed; no remote server job or optimization
claim was made. Documentation-only handoff commits do not change runtime source.
