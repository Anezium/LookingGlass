# Partial audio counter candidate

This checkpoint is **PARTIAL_COUNTERS_ONLY_UNQUALIFIED**. PHASE2A is incomplete.
There are no native producer fences, report/block identities, complete worker
lifetime/drain/final-partial evidence or audio-window p95 checks in this change.
The unchanged asynchronous legacy reports cannot supply any of those facts.
Audio, full-method, campaign, native and device qualification remain unavailable.

With `native_timing` and a valid enabled startup recorder config, the additional
optional boolean `audio_raw_counters` (default false) enables these observations.
The actual viewer calls `Audio::bind_native_counters` immediately after
`Session::open`. Audio was already constructed earlier in the viewer. Binding
activates only the available PCM-worker and music-thread routes; an absent route
stays null. Pre-binding events are **not captured or reconstructed**. Publication
is confirmed after `OnceLock` returns, and each field gets a separate initial
snapshot. The music binding is the configured future underrun route; its lazy decoder
thread may not have started. It is not proof of worker activation.
The process music counter persists; a newly created PCM metrics object
has its own counter. These are counter lifetimes, **not completed native audio
lifetime/generation records**. No playback stop/restart is used.

Each existing raw callback starvation or unavailable music-source frame updates a
nonresetting u32 total. Healthy samples do no additional work. When the feature
is compiled out there is no extra event work. Feature present/config false does
one empty `OnceLock::get` only at each existing starvation/underrun event; no new
RMWs, clock, allocation, logging or lock. Once active, each event performs three
additional native u32 primitive RMWs (writer enter, total increment, writer exit)
and one sticky store on wrap. It performs no explicit CAS/retry loop. The legacy
counter increment occurs inside the busy bracket. Existing legacy report resets,
percentile conventions, fields, reporting queue, settings and PCM behavior remain.
In particular intentional flush starvation still increases RAW; no film-branch
code or flush-subset subtraction was imported. Flush subset is explicitly null.

The target's `rustc --print cfg --target armv7-sony-vita-newlibeabihf` reports
`target_has_atomic="32"` (and 64). A compile guard requires 32-bit atomics. This
checks the configured target contract, not a native build, timing bound or device
measurement. Rust may lower primitive RMWs to target exclusive-instruction loops;
there is no claim of a bounded hardware instruction count or wait freedom.

`Recorder::sample_raw_counters(Tag::Requested)` is a functional host-tested API
for future outside-window requests. There is no scheduler or device command
integration here. The actual `Session::export_after_viewer` also takes a separate
`AfterViewer` read while playback may still run; this is **not a final audio
fence or stopped/drained evidence**. Each value load has native pre/post brackets
from the same checked clock domain as viewer observations. The raw value's exact
instant inside that bracket is unknown. PCM and music cuts are staggered.

An in-flight event makes a snapshot busy and unqualified. Wrap is sticky and
busy publication prevents a wrapped zero from preceding its fault undetected.
Clock errors/backwards readings, missing/changed identities, invalid brackets,
reset, loss or saturation reject. Snapshot storage is preallocated for 16 records,
never grows or overwrites, and reports attempts/loss. At cap it is unqualified.
Full JSON evidence streams by reference through the existing 32KiB buffer.

The optional schema extension carries raw bindings, snapshots, per-field lifetime,
width, activation brackets, counts, capacities, sizes and shared allocation bytes.
It is covered by the payload hash and manifest storage receipt. Absence remains
explicit PHASE2A UNIMPLEMENTED. The offline checker validates strict keys/types,
file order, identities, brackets, dimensions and counter monotonicity. It uses only
the prescribed Binding/AfterViewer pair, retaining every Requested snapshot.
For each field, before.post<=S and after.pre>=T with same lifetime/no fault and
delta0 proves zero over a **containing superset** of W. Positive surrounding delta
is FAIL; the exact positive count inside W stays UNKNOWN. Missing coverage is
UNKNOWN. No selection of favorable counters, interpolation or gap repair occurs.
`audio_qualified` and `campaign_qualified` are always false, including raw-zero.
The phase1 gaps list deliberately retains the full raw-bracket/audio-fence stage:
these partial reads do not close the complete audio capture contract.

The new inline capture fields are included in Recorder layout; snapshot capacity
and two shared Raw allocations (including Arc strong/weak headers) are included
in `actual_storage_bytes`. The two added OnceLock slots are charged to the existing
64KiB auxiliary reserve, including config-false feature builds. Compile-time worst
case includes both raw counters and all 16 snapshots within the unchanged 2MiB
cap. No new queue or producer timing vector exists. No heap guard was increased.

Run the maintainable host allocation regression with:

```
python3 -B tools/test_native_timing_export_memory.py --output /absolute/new/owned/path
```

It reuses locked/offline Cargo dependencies selected from compiler-artifact
receipts, runs actual production modules at opt-level0 and2, fills frame/world/
event/counter storage, and checks requested export heap plus the entire measured
macOS probe-thread stack and raw-slot reserve within 64KiB. Output paths affect
heap peak. This is not allocator overhead, OS thread storage, process RSS, native
RAM or the actual application thread's stack proof. Caches and failed outputs are
retained. The small (<16KiB) manifest remains unbuffered. BufWriter Drop can retry
a failed flush and leave parseable orphan records; a missing manifest still
rejects in the checker and CLI. Export still precedes exit autosave.

The concrete remaining audio blocker is worker-owned completion and transfer:
the legacy reporter observes the common stop flag and can exit before the
producer finishes its last enqueue. Its dropped count is visible only through a
later successful legacy report. Neither an empty queue nor this after-viewer
counter snapshot proves final emission/drain. A separate bounded capture path
must implement and test producer fences, completion membership, policy/identity,
request handling, shutdown/final partial and produced/emitted/dropped/stopped
receipts. Those changes are not implemented or accepted by this candidate.

All image/review-ack/action chronology, native-age scheduling/deadline, paired
simulation/state/pose tolerances, physical input/command/save/capture isolation
and deployment/readback provenance remain PHASE2B UNIMPLEMENTED. The manual
menu->Resume->real begin->later Quit phase1 stop and bench unsealable behavior
remain. Per-frame doors and swap bounds remain unavailable. Original raw0,
producer-window p95<=8ms and heap<180MiB gates and historical failures stand.
Observer costs affect legacy wall/CPU/heap; paired arms must share feature/config,
and historical disabled numbers are not numerically comparable. New exact source
requires independent full-context review; this checkpoint grants no acceptance.
