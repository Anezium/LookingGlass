# r14 host/native diagnostic pass

No rendering, simulation, sorting, upload-cache or ownership policy changes.
All additions use the existing bounded census switch or Vita-only backend hooks.
The allocator observer and its atomic owner-thread guard are unchanged.

`ux0:data/alice/private/vita_renderer.json` accepts `log_frame_time_every`:
omitted = 10 (historical default), positive u32 = that many frames, 0 or null =
off. It is read at startup, so edit the staged file and relaunch for an A/B;
no rebuild is needed. This controls the existing synchronous Miniquad report
only. Raw presentation-to-presentation wall capture is unchanged.

Miniquad is pinned to the exact diagnostic fork revision in Cargo.toml/lock.
The report callback measures formatting, stderr output and report bookkeeping
after swap, outside viewer spans. It rejects an overlapping viewer span.
All times remain elapsed CPU wall time, including preemption/API waits.

Schema 4 retains the original 30 regions and appends 21 diagnostic regions.
Parent masks are u64. Actor leaves require actor_gather; direct and fallback
leaves remain children of the original consumer bucket. NPC visibility restore
and overlay work uses the fallback leaf, followed by attachment traversal.
Rejection counters identify the terminal first failed prepare predicate; retry
attempts that eventually succeed are not failures. Vertex/index bank capacity
and held slots are distinguished. These counts describe prepare calls, not
unique actors or vertices. Existing serial-reason bit 128 remains unchanged.

Finish merge/pack includes puff building, prepared-geometry observation, merges
and synchronous Worker::pair service/wait. Upload CPU is an inclusive parent;
its exclusive value removes uniform preparation and buffer API calls. Upload
requests/hits count passes with nonempty indices, including shared-publication
hits; bytes count only actual vertex/index buffer_update payloads. Buffer
allocation capacity is not payload traffic. Sort, pass setup/end and recycle
have their own leaves; uncategorized finish work stays in its parent exclusive.

World leaves use the exact existing Totals boundaries. They accumulate locally
and publish once per pass, avoiding per-draw TLS/RefCell stack operations.
Vita backend counters count actual draw commands, cache-miss glBindTexture,
sampler glUniform1i, and changed vertex-attribute pointer/layout commands during
the owned opaque pass. Layout changes count attributes, not unique VBOs.
World alpha remains in its original separate census region.

Coarse roots cover input, story/weapons, particles, camera/scene, render setup,
first person and framework tail. They end before async yields. Work between
yield/resume outside swap/report remains residual; no GPU time is inferred.

The existing JSON names are retained, with a companion `.csv` using the same
nanosecond partition and all added leaves/counters. Both exports occur after
the window. `added_brackets` is the sum of calls in appended regions for each
frame: world contributes 3 per prepared draw; actor, finish, root and report
counts vary with the path and report cadence. The synthetic all-leaf frame
has exactly 21 added brackets. No measured Vita bracket count or observer-cost
claim is made by host-only validation. Fixed storage remains bounded; it grows
with the wider region arrays and counter columns, not the draw count.

The strict offline validator is `tools/stack_r10/console/census_report.py` and
accepts historical schema 3 plus exact schema 4 names, leaf parents and counts.
Run its new `test_r14_diagnostics.py` fixture and the historical closure fixture.
The Rust host suite exports 180 synthetic rows and pairs every timing/count
column against CSV while enforcing exclusive + residual == wall exactly.
