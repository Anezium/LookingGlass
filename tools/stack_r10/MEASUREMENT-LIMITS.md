# Measurement limits

The native toolchain uses IEEE scalar floating point with NEON disabled, as r6.
Compact heap array layouts contain only f32 bounds and u32 fields: SpatialNode
32 bytes, Certificate 28 bytes, Original 8 bytes and index 4 bytes. They contain
no host usize or pointer fields; their 32-bit sizes follow the target ABI.
Box/Vec metadata and recursion scratch are stack storage. The host allocation
ledger encloses only Hierarchy::build, after the legacy map/tree has loaded.
It measures retained allocation requests and a conservative request high water
that counts both realloc buffers before freeing the old one. It also applies
the r3 native `_malloc_r` minimum chunk formula to each request. This is a model
of minimum newlib usage, not allocator fragmentation or a continuous Vita peak.

The original tree and plane storage are retained once; there is no hull/plane
clone. B-A is only the four new hierarchy arrays. The expanded StaticGeometry
header and diagnostic schema exist in both arms of the common binary. Census
storage is allocated only when configured and is excluded from wall arms.

Each map's neutral 180-frame host run uses the real local assets and a silent,
sample-stepped production mixer with deterministic music decode. Entries are
normal host entries, not the console save. Gvillage's cutscene produces zero
player queries in that interval; this absence is reported. NPC static cache
hits bypass traversal and do not enter this miss corpus. The original 83.33%
Hedge3 scoped-hit denominator remains relevant; do not apply SAH miss savings
to all 348 NPC sweeps/frame. Owner attribution uses production owner scopes.

Recorded Trace bits are checked against a separate legacy traversal, then the
SAH path. All five owner rows are retained, including empty populations. The
recorded corpora contain static misses; 180-frame whole simulation, selected
lights, draw ordering and final captures separately check complete gameplay.
Synthetic tests cover moving dynamic/settled/weapon completion and ignored IDs.
Every distinct authored content mask plus 0, player, actor, liquid and all-bits
masks is checked on all four maps. This means every supported mask semantics,
not an enumeration of all 2^32 equivalent integer combinations.

Reported raw host ABBA loop times include owner filtering and counters and ran
while other host/native work was active. They are descriptive, not isolated
kernel benchmarks. Work counters are the reliable host before/after measure.
The console wall gate alone decides whether the exactness overhead pays off.
The r6 remaining Hedge3 audio sweep cost (~2.575 ms/frame) used cache on; r8's
experiment keeps it off to isolate SAH and cannot add hypothetical SAH savings
to r6 savings.
