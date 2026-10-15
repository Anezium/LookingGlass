# J23 phase 3: final-storage puff vertices

Phase 2 failed its full host gate on the accepted second pair's preparation/body
p95. The independent non-puff-failure allowance permits this phase. The actor
switch stays off for puff acceptance; no phase-2 console acceptance is implied.

`world_puff_fusion` is strictly boolean and defaults off. Its numeric workers
produce only owned corner positions and quantized colour. Registration copies
that plain numeric value; UV/normal/colour Vertex records are constructed directly
in the final bounded material-batch arrays. Texture handles stay on the game
thread. Legacy arithmetic, joint alpha ties, world/actor/multi-pass barriers,
first-fit material policy, 60000/180000 limits, publication ownership and the
existing two finish phases/five-slot GPU ring are preserved. No worker receives
render handles, and no worker output-comparison loop is added.

The producer still snapshots numeric recipes, evaluates corner arithmetic and
copies numeric descriptors. This removes intermediate Vertex representations;
it does not claim to remove all numeric copies or all puff arithmetic.

Verification on the Mac (`jalon-23-phase2/` external evidence):

- 855 feature tests pass, zero fail, two inherited asset-dependent tests remain
  ignored. Final default-feature and changed-source audits pass; Cargo.lock and
  the 12 inherited allowlist exceptions are unchanged.
- Independent real-puff arithmetic comparisons include changing age/roll,
  degenerate camera directions and bit-exact sort centers. Numeric production
  and final packing have zero warmed allocation in their targeted tests.
- Membership/material changes, equal-depth ties, world/actor/EQUAL barriers,
  vertex and index boundaries, held old publications and unchanged revisions
  pass against the legacy path and inherited independent-merger checks.
- Fresh live and Slot 4 reload off/on pairs each match 180/180 simulation and
  final submission digests. Both private frame-90 write/read witnesses pass.
  Source Autosave remains byte-identical.
- All 120 cross-arm matrix PNGs and 15 renderer PNGs match. Both arms pass their
  real-data worker checks. The new default-off path also matches all 135 prior
  phase-2 control PNGs. Frozen baseline: all 120 frames upload zero bytes in both
  puff arms.
- All 120 retained live frames match final draws, uniforms, upload bytes, ring
  capacity, buffer growth and final puff Vertex-write bytes. Intermediate puff
  Vertex payload writes fall from 40960 to zero bytes/frame. Numeric output and
  descriptor payloads each write 6656 bytes/frame; final Vertex writes remain
  20480 bytes/frame. These are logical payload counters, not measured physical
  memcpy traffic or a bandwidth-derived CPU saving.
- Warm producer output capacity is 39680 bytes off versus 12896 bytes on.
  Final batch-storage counters match frame by frame. These counters exclude
  recipes, enum/vector metadata, allocator overhead and full/peak heap usage.

Pinned host binary SHA-256:
`4b4c788464b2144a9de696bdc3c2de92abb4f464376c5c6eb89fb5a933fc76a2`.

The 20% CPU gate is still pending at this functional checkpoint. Timing uses
three alternating ordinary-update pairs with actor preparation off, identical
instrumentation, 60 warmup plus all 120 retained frames per arm and the shared
Mac timing lock/load qualification. The per-frame sum includes coarse world-puff
production, full joint sorting and world-puff batch planning/packing/publication;
shared ordering/barrier work is conservatively charged in full in both arms.
Upload/submission and total body are reported separately. No per-vertex clocks,
save witness or parity digest runs in timing. No package exists yet.
