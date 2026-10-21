# Native timing observations — phase 1 candidate

This is a source checkpoint awaiting independent review. It is **not a usable
campaign method or a source acceptance**. Audio producer fences/raw counter
brackets, image tokens, separate review acknowledgement, automatic native-age
acquisition, paired state equivalence and full qualification remain phase 2.
The original raw-zero, producer-window p95 <=8ms and heap <180MiB gates remain.

## Enablement and identity

The `native_timing` Cargo feature is off by default. Every viewer call is compiled
out without it. With it, a missing `private/native-timing-config.json` disables the
recorder. A present config must be <=4096 bytes, have no unknown keys, and match:

```json
{
  "enabled": true,
  "source_sha256": null,
  "build_sha256": null,
  "game_config_sha256": null,
  "startup_save_sha256": null
}
```

The optional values are 64 hexadecimal characters. They are deployment-supplied,
**unverified associations**, never invented identities. Null means UNKNOWN. The
config's own bytes are SHA256 hashed. Reload/retry/natural-world save associations
are UNKNOWN; the startup association is retained only for StartSave. This phase
cannot attest that a supplied hash names the running source, ELF/config or save.
No source/build/config/save acceptance follows from a structural population check.

A new random 128-bit run ID comes from host `/dev/urandom` or Vita
`sceKernelGetRandomNumber` (SceLibRng import in `SceLibKernel_stub`).
The local SDK rng.h and archive symbol table confirm that archive name; this
source has not been natively linked or run. Failure leaves an explicitly invalid UNKNOWN
identity. Capture identity is run ID plus a checked export-attempt ordinal. No
joins across launches, copied captures or clock domains are permitted.

## Clock, generations and observations

`native_timing::clock::now_us` is the only adapter for instrumentation. Future
thread users must call this adapter through their own `Checked` stream; they may
not create another Instant origin or mix offsets with raw ticks.

On Vita it calls `clock_gettime(4)` with the SDK ABI: unsigned 32-bit clock ID,
two signed 32-bit longs in timespec. SDK newlib.h sets _WANT_USE_LONG_TIME_T;
sys/config.h sets _USE_LONG_TIME_T. The existing (unchanged) ELF disassembly routes
ID 4 to `sceKernelGetProcessTimeWide`, microsecond resolution. That is process
lifetime time, not UTC or a statement about suspend behavior. The wrapper itself
has not been natively built or device-tested. There is no Linux clock constant,
SystemTime fallback, successful-call assumption or silent clock reset. Return
failure, invalid/negative timespec, conversion overflow or backwards readings
latch a fault and make future samples null. u64 counter overflow also latches a
fault. A 32-bit time_t rollover is invalid/backwards, never a new epoch.

On host all streams use one `OnceLock<Instant>` origin. Samples are integer us
relative to that shared host session. They are not raw Vita process ticks. The
adapter uses checked duration/conversion, with no UTC fallback. Equal samples
are permitted at microsecond resolution; nonpositive whole viewer intervals are
rejected by the checker.

Ready events occur after live world resources/state/audio are installed, before
that world's first simulation: fresh start, start-save, every restore/load,
death retry (including same map), chapter/console entry and natural/cached return.
Prepared/failed worlds do not increment generation. The prior pending interval
is explicitly invalidated BEFORE live state mutation; it is retained until its
next actual viewer begin and cannot qualify across generations. World metadata
contains map/entry/reason, ready timestamp, exact environment/pickup f32 bases,
physics tick baseline and numerical eye xyz/yaw/pitch f32 bits. No state is updated
for observation. First playable is an observed complete noncinematic viewer
frame, not a load-start marker. Status events carry the actual pause/cinema/etc.
mask; skip and nested movie/modal/transition paths carry separate events.

The global viewer sequence increments at every main viewer begin, independently
of accepted Light/Full IDs, including begins after acquisition stops. Every
**acquired** completed interval is [actual begin, next actual main viewer begin).
Rejected, inactive, modal/skip/transition and incomplete simulation intervals stay
visible. Nested loops are explicitly discontinuous intervals/events; their inner
backend frames are not mislabeled main viewer frames. Legacy measured IDs are
read from the actual Vita end_frame path; they remain map/mode-local identifiers,
not an unambiguous join to a legacy file. Host legacy IDs are unavailable/null.
The operation epoch observes lifecycle/status/load/skip/modal changes. It does
not attest every physical input, external command, save or capture request; quiet
input/command isolation still needs phase 2 evidence.
Neither legacy wall/cpu/swap semantics nor legacy accepted/reset/capacity semantics
are changed. When the recorder is enabled its own hook cost falls inside legacy
wall intervals, its end() cost inside the legacy CPU interval, and its ~1.7MB
allocation inside measured heap use. Enabled and disabled runs, or historical
runs without the feature, are therefore not numerically comparable; paired arms
must share the same feature and config state.
Recovery/restart (R, Home, A/Enter while dead without saves, console restart) resets the live state in
place without a new generation; it is recorded as a `Recovery` operation.
The new CPU bounds are separate observations around viewer CPU work;
backend swap bounds are explicitly unavailable, never estimated by subtraction.

Simulation observations preserve elapsed, capped dt, world_dt, environment and
pickup before/after f32 bits, fixed ticks and numerical pose bits. Macroquad's
elapsed comes from a different realtime-derived clock; dt remains capped at0.05.
Powers/world, pickup, environment and fixed physics clocks retain their own
semantics and saved/cached bases. Native residence age does not equal simulation
age. No interpolation, input/camera/physics/scheduler change or forced dt is used.

## Acquisition bound, memory and export

Phase 1 records from the first main viewer begin. On the first nested escape
menu entry it requests acquisition stop. The next real main viewer begin closes
the pending modal interval and seals acquisition; it starts no new pending row.
The explicit stop timestamp and global sequence are exported. Later viewer
begins still advance the global sequence and world/milestone metadata remains
bounded; no later intervals are represented as contiguous acquired frames.
If the viewer exits before that real begin (including Quit directly in the
menu), pending remains unresolved and the checker rejects. There is no synthetic
extra frame, export-time finalization, gameplay export or fabricated boundary.
A Quit chosen in that FIRST menu always exits before the sealing begin, so the
sealed sequence is: menu, Resume, at least one viewer begin, then Quit later.
Bench modes disable the Start menu and stop after 120 frames, so their captures
can never seal and are always unqualified.
This conservative manual acquisition is not the phase 2 native-age scheduler.
A first-pause capture may saturate before a desired window; it then fails rather
than trimming or expanding. After-stop event/world loss also disqualifies.

Buffers reserve 6144 Frame, 512 Event and 128 World slots once, never overwrite,
never grow in hot methods, and track exact completed/stored/lost counts. This is
an independent buffer; the legacy 4096 capture and heap guard are unchanged.
On the tested aarch64 host: Frame264B, World320B, Event56B, Native Recorder664B; reserved
storage1,692,312B plus64KiB auxiliary reserve =1,757,848B <=2,097,152B. Compile-time
and runtime checks enforce the cap on each target using actual sizes/capacities.
Auxiliary reserve covers the <=4097B startup read, bounded parsed config, bounded
manifest/maps/paths/hash/serializer, a fixed 32KiB export buffer and stack scratch.
The buffer sits inside the hash writer; its accepted bytes determine the receipt.
It is flushed and the underlying File synced before a complete manifest can be
written, then released before manifest construction. Payload serialization streams
references without cloning datasets or a full encoded payload buffer. No new
dependency. This allocation budget is not RAM/heap qualification. Native layout,
RAM and runtime overhead remain unmeasured. Hot begin/cpu/sim/end/milestone methods
have no formatting, allocation, file IO, printing or locks. The host OnceLock is
initialized on ready, outside frame timing. Exhaustion never repairs itself.

Only after the main viewer loop exits does Session export. `records.json` streams
frames/worlds/events and any unresolved pending row. The writer creates a unique
capture directory and create_new files, flushes the 32KiB buffer and syncs its
underlying file, then writes,
flushes and syncs `manifest.json` with actual byte hash/length, counts, capacities,
sizes, faults, identities and phase 2 gaps. `complete` means the IO transaction
completed, not that acquisition/provenance/qualification succeeded. Directory
sync/power-loss durability is not claimed. No manifest or malformed manifest after
failure is unqualified. Repeated snapshots consume different ordinals and are
explicitly cumulative/unqualified; a failed first attempt cannot be concealed by
a second. Existing filenames are never overwritten. Offline hash verification is
mandatory; a writer hash alone is not device readback proof.
Export precedes the existing automatic exit save. Device export delay is UNKNOWN;
closing the app before export finishes can also prevent that later autosave.
Vita newlib fsync routes to sceIoSyncByFd (local SDK fact); this export's native
sync behavior, directory/power-loss durability and suspend behavior are UNTESTED.
The app creates `private/native-timing/` and per-run directories only at exit.
Their future device inventory/preservation protocol is unresolved; this source
checkpoint authorizes no device action or new console folders.

## Offline checker and tests

```sh
cargo test --locked --offline
ALICE_TIMING_TEST_OUTPUT=/absolute/new/owned/output cargo test --locked --offline --features native_timing -- --nocapture
ALICE_TIMING_TEST_OUTPUT=/absolute/new/owned/output python3 -m unittest discover -s tools -p test_native_timing_check.py -v
ALICE_TIMING_TEST_OUTPUT=/absolute/new/owned/output python3 tools/test_native_timing_hooks.py
python3 tools/native_timing_check.py /path/to/runhex-0000000000000001 --generation 1 --age-offset-us 20000000
```

Use a new test output directory each run; tests retain failure evidence. Host
unit test exports use a marked fake clock/run ID and are never native evidence.
The extraction harness compiles actual viewer macros/hook text against the actual
recorder, with small game-dependency stubs; it is not an asset/rendered playthrough.
Separate in-crate tests compare actual FixedClock/Player/powers and exact state
bytes with/without observations. The test tools import no campaign/network helper.
The extraction tool accepts ALICE_TIMING_HOOK_RUN for a new evidence subdirectory
and ALICE_TIMING_HOOK_MUTANT=retry-ready/pause-reason/dt-cap/chapter-order for
compiled semantic negative witnesses. Exact per-site static contracts are checked
separately; the chapter-order mutant bypasses only that static assertion so its
actual extracted order must compile and fail before the stub live mutation.
A standalone `src/native_timing/export_memory_probe.rs` probe, compiled with the
same locked HOST dependencies, exercises all bounded datasets and present hash
metadata. It measures requested export heap and runs export on a bounded host
thread stack; on macOS the actual pthread stack allocation is queried and the
heap peak plus that entire stack must fit the 64KiB auxiliary reserve. The tested
host used 33,316B peak export heap and 28,672B actual stack, 61,988B total. The
buffer is released before constructing manifest metadata. Dataset serialization
has fixed schema depth and does not clone data. Startup config scratch is a
separate lifetime (<=4097B read and four <=64B hash strings). This is a host
requested-allocation bound; allocator bookkeeping, OS thread runtime and native
stack/heap/RAM remain unqualified.
The checker tests require the actual Rust-generated fixtures, not a Python model.
Injected exports identify their clock as `host-injected-fake-us`. The production
checker rejects that domain by default; `--allow-test-clock` is required for these
HOST TEST fixtures and the result explicitly says `test_clock:true`. This option
never makes test data native evidence.

The checker rejects malformed/missing/extra fields, booleans as numbers, duplicate
JSON keys/rows, reorder/gaps, missing/invalid epochs, clock faults/backwards,
loss/saturation, budget inconsistency, unresolved pending, unsealed acquisition,
partial transaction, altered hash, wrong identities and cumulative repeats. It
requires full left/right guards around settle and the window. It computes exact
S=world_ready+offset, T=S+30,000,000 and selects all WHOLE rows with b<T and e>S.
Rejected/discontinuous/generation-changing intervals or milestones in settle/W
reject. Whole durations may exceed30s; left/right overhangs and boundary IDs are
explicit. CPU/door values are never clipped or prorated. N<120 is explicitly SMALL
without extension. Pair/simulation-age equality stays UNKNOWN. Successful output
is only `source_population_contract`, always `campaign_qualified:false`, with all
phase 2 gaps. The result includes the actual declared `clock_id` and
`provenance: "UNATTESTED; self-consistent hash only"`. A capture can be relabeled
and rehashed by its author, including a fake clock relabeled as a native clock;
this checker cannot detect that forgery or attest the declared domain/device.
Deployment/readback provenance is still phase 2. It cannot join legacy door chunks, attest provenance, authorize a
campaign, waive known audio failures or establish image-review chronology.

Freeze exact sources/checks, then obtain a **new independent delegated
review with the full original brief/research/findings/responses/unresolved gaps**.
Only root can advance later stages. No native build/device campaign follows from
this checkpoint; no old native is relabeled.
