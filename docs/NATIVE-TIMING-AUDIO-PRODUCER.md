# Producer observations

The optional producer journal is a source implementation awaiting independent
review. It supplies bounded observations and structural checks. It grants no
source acceptance, native/device validation, audio qualification, listening,
method readiness or campaign qualification. Phase 2B remains unimplemented.

Enable `native_timing`, `enabled: true` in the existing bounded startup config,
and `audio_producer_capture: true` (default false). `audio_raw_counters: true`
remains a separate opt-in; see [counter observations](NATIVE-TIMING-AUDIO-COUNTERS.md).
The actual viewer binds immediately after `Session::open`, after Audio already
exists. A direct/unbuffered/null audio route cannot supply producer observations.

The first actual `Recorder::begin` requests start with its already-read timestamp.
Producer consumption has its own later timestamp. This is an implemented viewer
path, not a native-age scheduler. Explicit prepare/start/seal APIs support a
future caller; they do not implement Phase 2B. Load/menu/prelude time can exhaust
the journal. Capacity exhaustion rejects rather than trims, overwrites or grows.
The current journal spans the whole viewer session, from the first Recorder begin
through Quit/export, even after the phase1 frame recorder has sealed. At 44.1kHz,
8191 usable block rows cover about 95.1 seconds; the separate usable fence limit
is 127 rows (about 127 seconds with nominal one-second cuts). These are independent
limits. A long session sets LOSS. The strict `check_capture` rejects the entire
capture when producer validation is Invalid, including an otherwise usable
phase1 frame population. This coupling is a current limitation. Changing journal
scheduling or separating verdicts belongs to a future stage.

## Lifetimes and ownership

Feature builds observe mixer identity creation in the actual mixer constructor,
a separate worker ordinal/spawn-request metadata bracket, and actual producer
thread entry/policy. A spawn request is not a created thread or hardware-output
start. The `worker_spawn_request` bracket surrounds early ordinal allocation at
buffered-body entry, before PCM prefill and reporter creation; it does not bracket
the later Builder::spawn call. Binding, attachment and start consumption are
distinct observations.
Counter UUIDs identify counter activation lifetimes, not mixer/worker lifetimes.
No pre-binding producer blocks, reports or raw events are reconstructed.

Checked ordinals/topology use bounded process atomics. There is no registry.
A Metrics object admits one buffered worker; enabled Session construction is
one-shot before its large frame allocations. One global producer capture is
admitted before buffer allocation. A second session/capture claim, overlapping
mixers or a changed topology is explicitly invalid. Replacement cannot restore
a capture by returning the live mixer count to one.

Main owns the Start sender and completion receiver. The existing producer owns
their counterparts. Both channels use the existing SPSC implementation with
capacity one. Neither endpoint is Sync, cloned, stored in a shared OnceLock or
placed in a shared mutable recorder. Only immutable control/identity metadata,
OnceLock publication and primitive atomics are shared. The buffers move once to
the producer and once back. No per-block Arc clone, reserve/growth or extra thread
is used. Raw counter lifetime references in the producer section must match the
separate raw-counter section when it is present.

## Blocks, fences and legacy reports

Every retained row describes a complete mixed producer block: native begin/end
microseconds and the exact bits of the original `total_ms` f64. The original
Instant measurement expression and source/sample body remain. Native bracket
width is not substituted for that duration. IDs are capture-local; prefill
silence is not a mixed producer block. A failed PCM enqueue or later flush does
not remove the mixed block from the diagnostic population.

Periodic native cuts precede the unchanged legacy swaps/loads/emit/reset. The
next begin is exactly the preceding native end, including reset/logging work in
coverage. Whole blocks belong by completion in `(begin,end]`; no splitting,
clipping or proportional durations. Native fence and legacy report IDs are
separate. The actual legacy Window and enqueue/direct-print outcome are retained
under their associated ID, including music fields. Legacy collection brackets
are coarse and do not make its staggered atomic reads simultaneous.

The first native fence is marked FIRST: it can cover only part of the legacy
window. The first legacy N can therefore exceed native N. Subsequent associated
legacy windows must have the same N and exact quantile bits as their native block
population. The final diagnostic partial has FINAL; EMPTY is explicit. There is
no extra legacy partial, counter swap, reset or print at diagnostic seal.

The estimator remains sorted `[n/2]`, `[(n-1)*95/100]`, `[n-1]`. In particular N=2
can have p50 greater than p95. Native quantiles sort a bounded scratch copy with
an allocation-free unstable sort and preserve original block acquisition order.
An empty window is never a passing zero-p95 window. Clock failure/backward values,
zero-length cuts, completion at/before the current begin, arithmetic overflow,
loss, saturation and scratch exhaustion are explicit invalidity.

## Seal and completion

After the viewer, before the existing exit autosave and `audio.finish`, main
requests diagnostic seal. It waits against a separate std Instant deadline of
at most two seconds, not the possibly faulted diagnostic Checked clock. Actual
elapsed wait is reported separately: this deadline is a policy bound, not an OS
scheduling/realtime guarantee. A blocked source can delay safe producer handling.
Timeout is unqualified; main never reads mutable buffers or waits indefinitely.
The return receiver remains owned until teardown. Late publication cannot turn
that already failed snapshot into accepted evidence.

At a safe producer boundary, seal appends the final residual partial and immutable
receipt, then publishes the complete ownership bundle. Only a successful queue
push is followed by final summaries and DELIVERED Release. Main acquires that
terminal before popping and validating the bundle, identity and counts. An empty
queue alone proves nothing. Failed publication retains ownership and publishes
failure; there is no retry. Exact counts distinguish produced/retained/lost rows
from legacy enqueued-or-printed/queue-dropped reports, including a dropped last
report without a subsequent successful one.

Diagnostic closure stops diagnostic writes and clocks; playback continues. A
real WorkerStop is a distinct reason. Loop closure precedes source destructors
and is not OS thread return, decoder stop or physical-output stop. Main records
producer join outcome only after joining. Reporter closure/panic and reporter
join outcome are also distinct. Export usually precedes these later joins; it
cannot claim future stop facts or append them retroactively. Checker output
`thread_returned` is UNKNOWN until a successful join proves return; absence of a
join does not prove that the thread has not returned.

The reporter now acquires producer_done and then drains. Playback stop alone
cannot end it. Producer_done is published after the last legacy emit attempt and
diagnostic close/abort, including unwind and policy rejection. A producer spawn
failure closes and joins the already-created reporter before returning the error.
No new reporter, playback restart, gain/fade/epoch or queue policy is introduced.
This final reporter drain correction also applies to feature-off builds.
The collect wait can delay the existing exit AutoSave by up to two seconds plus
OS scheduling overshoot, before export IO itself. Forced close can lose that save.

## Storage and observer cost

The hard total reservation remains 2,097,152 bytes, including the existing frame,
world, event and raw-counter storage, all journal allocations/headers/slots/control,
bootstrap/static metadata, producer scratch/stack allowance and 65,536 bytes for
export/startup auxiliary work. Legacy 4096 and heap <180MiB guards are unchanged.

Journal capacities are 8192 blocks, 128 fences, 128 legacy rows and 512 scratch
f64s. Saturation at capacity rejects. At normal 44.1kHz stereo cadence, 8191 blocks
are about 95.1 seconds; this is planning, not a scheduling or capture guarantee.
All capacities are checked/reported. The existing producer stack is 1MiB; no new
thread stack is allocated. The reservation conservatively charges a whole 32KiB
host worker-test stack plus measured inline and allocated types. The actual host
worker test requested 16KiB and queried 28,672 bytes. Native stack behavior remains
untested. The 32KiB reserve is an accounting charge, not an additional stack
allocation or a measurement of incremental observer stack usage. Small metadata
remains reserved even for a producer-disabled recorder.
The historical `allocated_storage_bytes` field includes these explicit reserves.

The committed memory runner fills 6143 frames, 127 worlds, 511 events, all raw
slots, 8191 blocks, 127 fences and 127 legacy rows. Actual production exporter
opt0/2 host runs measured 33,340 bytes peak requested export heap plus the entire
28,672-byte export-thread stack and 32 raw-slot bytes: 62,044 <=65,536. Total
reservation with the probe's clock was 2,058,669 <=2,097,152. The unchanged 32KiB
buffer is released before the small manifest is built. Measurements are path
and host dependent; allocator bookkeeping, OS/thread runtime, process RSS, actual
application stack high-water, native ABI/layout/heap/RAM are not qualified.

Feature absent: producer diagnostic state/hooks are absent. Feature present with
producer config false: mixer/worker creation metadata and the small Start channel
remain; there is one empty inbox probe per producer loop and cheap inactive local
hook guards. No block/fence diagnostic clocks, appends, sorts or large journal
allocation occur. Existing raw-counter disabled event costs are separate. There
are no new callback/RingSource/Mixer sample clocks, locks, allocations or logs.
Host tests measure hook wall/thread CPU, zero hot allocations and callback event
counts. These are not full-game overhead estimates or predictions for Vita.
Observer cost remains inside legacy wall/CPU/heap; paired arms must use identical
feature/config state, without numerical comparison to historical disabled runs.

## Export and checker

Typed frozen records stream by reference through the real payload transaction,
hash, byte-count, flush and sync. Only then may a complete manifest be flushed and
synced. Partial/orphan outputs and repeated cumulative exports remain rejected.
Raw AfterViewer reads occur after seal handling and retain their own field
brackets while playback continues. They are not the producer fence.

The strict checker recomputes every fence's whole-block membership, IDs, N and
quantile bits. It checks identity/lifetimes, policy, exact contiguity/endpoints,
counts, loss, saturation, partials and bounded dimensions. It selects ALL fences
with `begin<T && end>S`, including both boundaries, requires gapless W coverage,
each N>0 and each p95<=8.0ms (8000us). There is no pooled or game-frame p95.
`native_audio_data_complete` means structural/unattested producer data only.
With producer capture enabled and raw counters disabled, each raw-zero gate is
UNKNOWN with an explicit absence reason. The manifest's `phase2_unimplemented`
list follows actual sections: counter-only has raw brackets but no producer
fences; producer-only has producer fences but no raw brackets. This is a section
inventory, not raw-zero qualification or Phase2B scheduling. The legacy
`audio_qualification: UNIMPLEMENTED` key describes full qualification; it does
not deny the available structural producer data. Physical output, decoder stop
and route activity remain UNKNOWN. Audio,
method and campaign qualification remain false. Phase1 and counter-only formats
remain explicitly distinct. `python3 -I` loads trusted sibling checker modules
from the checker's own directory; malformed data still fails closed.

Music route activity remains UNKNOWN. A music_thread binding does not prove mix,
music controls, eligible decode-ahead, job admission, a ring or an active voice;
zero can be vacuous. Binding/AfterViewer zero proofs cover a conservative session
superset. Positive surrounding delta is FAIL with positive-inside-W UNKNOWN;
busy/wrapped Requested rows still reject the whole capture. Publication timing
confirms main-thread order, not bounded visibility to another thread.

Phase 2B image tokens/review acknowledgement/inspection chronology, native-age
S=ready+D/T=S+30s and review deadline, paired state/pose tolerances, physical
input/command/save/capture isolation and deployment/readback provenance remain
unimplemented. Manual first-menu Resume/real-begin/later-Quit and bench limitations
remain. Per-frame doors and backend swap bounds remain unavailable. App-created
export directories still need a device preservation protocol; no device folder,
agent MKD, native build or campaign is authorized by this source.
