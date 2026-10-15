# J23 phase 2: Mac resume validation

Source checkpoint: `7e5e1cff9d2537e9ded6e6058f6640f4ed7b1afa`.
The 15 October Mac-only amendment supersedes the older remote-build instructions.
All evidence is external to Git, under
`~/vita-j0/village-retained-world/jalon-23-phase2/`.

The absent checkpoint target was rebuilt on the Mac with locked dependencies,
four build jobs and an isolated target. Pinned host binary SHA-256:
`f4250a85132659951bf42d4b234395a2e17051a339f32b2642c25f913a91278d`.
Functional runs waited for the coverage track's timing lock to be removed.

Fresh `functional4` results:

- Live off/on: 180/180 simulation and final submission digests match, including
  the cold frame with fixed camera aspect. Private Slot 4 write/read passes at
  frame 90 in both arms; source Autosave bytes remain unchanged.
- Two fresh cells reload the baseline private Slot 4 as Autosave: 180/180
  simulation and final submission digests match again.
- Each arm passes 60 serial/overlap capture comparisons across six real maps,
  both worker counts and the consumer witnesses. All 120 cross-arm images match.
  All 15 renderer reference/steam images match across arms. Actual-data direct
  worker comparisons pass in both arms.
- Retained frames have identical draw counts, uniform bytes, ring capacity and
  buffer growths. Publication-bank capacity stays at 168686 bytes and 51 surface
  slots. Fallback/overflow counters remain zero. Existing source-checkpoint
  tests passed 851 cases, with two inherited asset-dependent cases ignored.
- Final default-feature check and both negative Send/Sync confinement checks
  pass. Changed source passes the source audit; 12 inherited allowlist exceptions
  and Cargo.lock are unchanged.

The upload audit is still open: 85/120 retained frames upload an extra 1280
actor vertex bytes through two otherwise unchanged surfaces. Medians are
194804 bytes off and 195736 bytes on; this difference of medians is not the
per-frame byte delta. No accepted performance pair or phase-2 gate result exists
at this checkpoint. These functional runs are not performance measurements.

The separate console-owner probe report now measures conservative replay,
validation and publication at 6.599/6.895 ms median/p95, above the 1.5 ms
prioritization threshold. This permits a later phase-2 console A/B only after
its host gate and parity pass; it establishes no optimization gain. This track
has not accessed the console. Phase 3 and packaging have not started.

## Surface dependency follow-up

Whole-skeleton changes unnecessarily dirtied two static surfaces. Each CPU surface
now caches the bones actually evaluated by the exact skin kernel, including
zero-weight entries. Single preparation compares only those numeric dependencies
on the game thread, bit-exactly for palette, pose and scale inputs. The worker
kernel is unchanged; no dual-variant output comparison is enabled. Existing UV,
topology, alpha, epoch and held-publication guards still apply.

The new dependency regression covers unrelated/relevant bones, palette and scaled
pose paths, signed zero, zero-weight entries and zero warmed comparison allocations.
The complete feature suite passes 852 tests, with the same two inherited ignored
cases. Final default-feature check and source audit pass.

Pinned follow-up host binary SHA-256:
`32f3bc3a22842b07c500ca744b1704b2834540b8dc22ccb73474806b4a679b60`.
Fresh `functional5` repeats all live, Slot 4, reload, image and actual-data worker
checks above successfully. Upload bytes now match in every one of the 120
retained frames, alongside draws, uniforms, stream capacity and buffer growths.
The preceding upload finding is resolved; its raw evidence remains preserved.
Publication-bank capacity is still constant at 168686 bytes and 51 slots.
This counter excludes shared load-time CPU metadata and allocator overhead.
There is still no accepted timing pair at this functional checkpoint.

## Load-qualified host gate

Three alternating pairs completed on the Mac under the shared timing lock.
Each launch advances 180 ordinary-update frames with prescribed dt and neutral
input, discards exactly 60 warmup frames and retains all 120 remaining frames.
Accepted pair attempts are 1, 1 and 2; pair 3 attempt 1 was rejected in full for
load and preserved. Every observed 1/5/15-minute load average in accepted pairs
is at most 3. No accepted stall is trimmed or replaced.

Preparation and body values below are median/p95 in Mac milliseconds. Preparation
adds exclusive gather, discovery, consume, simple publication, modifiers, packet
publication, shadow/atmosphere and collection per frame before taking quantiles.

- Pair 1: preparation 0.221251/0.372709 off -> 0.106728/0.339458 on,
  reduction 51.761%; body 1.443750/1.924667 -> 1.396187/1.888292. PASS.
- Pair 2: preparation 0.217021/0.377584 -> 0.113000/0.405792,
  reduction 47.931%; body 1.409625/1.790250 -> 1.393542/2.144667. FAIL.
- Pair 3: preparation 0.225230/0.393750 -> 0.101354/0.237625,
  reduction 55.000%; body 1.435813/1.766291 -> 1.346355/1.750416. PASS.

Phase 2 **fails its full gate** because pair 2 worsens preparation and body p95.
The positive paired finish/upload delta is charged conservatively (0, 0 and
0.007912 ms); all three charged median reductions still exceed 30%. Simulation
hashes match 120/120 in every accepted pair. Worker service is not added to
game-thread elapsed. Body excludes the deferred framework/GPU tail; it is not a
Vita wall measurement. Raw timing, load qualification, rejected attempts and all
exclusive distributions are in `timed5/` in the external evidence directory.

The gather tail increases in the rejected gate; collection remains microseconds.
This is unrelated to puff fusion, so independent phase 3 is permitted by the
brief. The actor switch remains default off. No phase-2 console acceptance or
package readiness is claimed.

Independent phase 3 also fails its consistent-three-pair CPU gate; see
`J23-PUFF-FUSION.md` and `J23-FINAL-REPORT.md`. Neither full gate passes, so phase 4
is not entered. Both new switches remain default off; no package exists.
