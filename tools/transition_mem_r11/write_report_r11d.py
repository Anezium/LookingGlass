"""Write the final r11d report and detached seal from verified local evidence."""
from pathlib import Path
import hashlib, json, subprocess, sys

R = Path(__file__).resolve().parents[2]
O = Path(sys.argv[1]).resolve()
load = lambda name: json.loads((O/name).read_text())
sha = lambda path: hashlib.sha256(path.read_bytes()).hexdigest()
n = load('packages/baseline/VERIFICATION.json')
attribution = load('proof/BASELINE-ATTRIBUTION.json')
layout = load('proof/FUTURE-LAYOUT.json')
final = load('proof/BASELINE-FINAL.json')
validation = load('proof/R11D-VALIDATION.json')
repeat = load('proof/REPEATED-TRANSITIONS.json')
census = load('proof/HOST-CENSUS.json')
persistent = load('proof/PERSISTENT-FAILURE.json')
cleanup = load('proof/CLEANUP-R11D.json')
assert all(v['status']=='PASS' for v in [attribution,layout,final,validation,repeat,census,persistent])
assert load('proof/FINAL-VERIFICATION.json')['status']=='PASS'
head = subprocess.check_output(['git','rev-parse','HEAD'],cwd=R,text=True).strip()
baseline_table = '\n'.join(f"| {r['map']} | {'ON' if r['levers'] else 'OFF'} | {r['live']['r11b']:,} | {r['live']['r11c']:,} | {r['live']['r11d']:,} | {r['r11d_minus_r11b']:+,} |" for r in attribution['rows'])
final_table = '\n'.join(f"| {r['map']} | {'ON' if r['levers'] else 'OFF'} | {r['r11d']:,} | {r['difference']:+,} |" for r in final['rows'])
trend_table = '\n'.join(f"| {'ON' if r['levers'] else 'OFF'} | {r['action']} | {r['cycles']} | {r['post_warmup_range_bytes']:,} | {r['slope_bytes_per_cycle']:.3f} | {r['last_ten_slope_bytes_per_cycle']:.3f} | PASS |" for r in repeat['rows'])
census_table = '\n'.join(f"| {r['scenario']} | {'ON' if r['levers'] else 'OFF'} | {r['before']['heap_used']:,} | {r['released']['heap_used'] if r['released'] else 'n/a'} | {r['release_request_peak']:,} | {r['release_newlib32_upper_bound']:,} | {r['extra_above_max_endpoints_bytes']:,} |" for r in census['rows'])
run_table = '\n'.join(f"| {r['tag']} | {r['exit_code']} | {r['seconds']:.2f} |" for r in validation['runs'])
hash_names = ['CONSOLE-PACKAGE.zip','ARTIFACT-SHA256.json','AMENDMENT-R11D.md','CONSOLE-PROTOCOL.json','PACKAGE-IDENTITY.json']
hash_table = '\n'.join(f'| {name} | `{sha(O/name)}` |' for name in hash_names)
samples = '\n'.join(f"{'ON' if r['levers'] else 'OFF'} {r['action']}: " + ', '.join(str(s['live_rust_request_bytes']) for s in r['steady']) for r in repeat['rows'])
report = f'''# Alice r11d transition memory

R3-M1 and R3-M2 are resolved on `vita-transition-mem-r11d-20261025`,
from `7fdf87b327be7747c0965c8faf962fb821b5d4b8`.
Engine source: `{n['source_revision']}`. Final tooling/source branch head:
`{head}`. The branch is plain-pushed; no force push is used.

Console execution is **NOT RUN**. All work ran on this Mac. Neither the Vita
nor the shot driver was contacted. The parked console's next run remains r11c
SELF 78e81848e6797272b0d7b981b479b18601c98e3360456ead22b3bf31ea1078d9,
unchanged. `proof/R11C-READ-ONLY.json` fingerprints every original r11c file;
`proof/PACKAGE-LESSONS-R11D.json` verifies that directory remains byte-identical.
REVIEW-R3.md and SOL-REPORT-R11C.md were read in full first.

## R3-M1: exact cause and source fix

At `7fdf87b:src/viewer.rs:1189` and `:1223`, the outgoing-recovery loop and
retry-screen awaits were expanded inside `run`. Their suspend states increased
`run` from **752,928 to 1,144,544 bytes** (+391,616). Boxing only the rebuilt
tuple in r11c did not remove these suspend states. The launch at
`src/main.rs:1559`, through the pinned macroquad `src/lib.rs:906`, stores the
future in an outer async frame whose capture and await state duplicate this
layout increase. The frame-one allocation histogram proves that exactly one
live allocation moved from **1,506,576 to 2,289,808 bytes** (+783,232).
Every other live size bin is identical. No atlas or backend table allocation
changed in this comparison; the fork pins are unchanged by r11d.

The compiler's diagnostic `-Zprint-type-sizes` independently proves all three
layouts in FUTURE-LAYOUT.json and baseline/*/type-layout.log. The macroquad wrapper
stores both its `.future` capture and `.__awaitee`: 753,280 bytes each in r11b/r11d,
1,144,896 each in r11c, plus a 16-byte state/alignment cost. The largest `run`
variant changes from Suspend23 (752,927 bytes) to Suspend31 (1,144,543), with
803,456 bytes of coroutine field-layout padding; r11d returns to 752,928 bytes
total (Suspend24 is 752,919). This is reserved async frame storage, not a
frame-one cache or map-resource leak. RUSTC_BOOTSTRAP was used only to print
diagnostic compiler layouts in isolated archives; shipping build flags stay unchanged.

The published +783,234-byte comparison also used output directory names
`alice-transition-mem-r11` and `alice-transition-mem-r11c`. The extra character
is present in two live PathBuf allocations: `Options.save_dir` and the cloned
`Store.directory` (`src/viewer.rs:716`, `src/save.rs:228`).
`proof/PATH-LENGTH-CONTROL.json` repeats the same binary with one extra save-path
character: two 108-byte requests become two 109-byte requests, exactly +2 bytes.
Thus the reported difference is fully accounted for: **783,232 + 2 = 783,234**.

The fix is the separate async `rebuild_outgoing` at `src/viewer.rs:93`, called
only after failure through `Box::pin` at `src/viewer.rs:1252`. Recovery retains
the same compact Game checkpoint and untouched controllers, particle/audio
clocks, retry/quit choices and one automatic transient retry. Partial builders
drop their locals before the next resource-retirement await. Successful
resources install together. Normal play reserves no recovery suspend state.

`baseline_proof.py` builds isolated source archives and measures the run-future
type without constructing another instance. Its fixed atomic size histogram
records every allocation/free size, and the weighted bins equal the live census.
The full r11d histogram equals r11b in all four cases, with **zero** differing bins:

| Map | Levers | r11b live B | r11c live B | r11d live B | r11d - r11b B |
|---|---|---:|---:|---:|---:|
{baseline_table}

The run future is **752,928 bytes again** in all four r11d cases.
`BASELINE-ATTRIBUTION.json` retains every before/after size bin. A second pass
using the final uninstrumented immutable proof executable independently confirms:

| Map | Levers | Final r11d live B | Difference from r11b B |
|---|---|---:|---:|
{final_table}

These are matched frame-one fixtures with an empty transition plan; their small
absolute differences from the earlier REPEATED fixture reflect fixture storage.
The full production census and repeated fixtures were separately rerun below.

Diagnostic process notes: macOS's lazily initialized Mutex recursed through the
allocation recorder; fixed atomic storage corrected the diagnostic. A provisional
validation found a diagnostic executable in the shared Cargo output and was
rejected before census/E2. DIAGNOSTIC-CACHE-REJECTED.json preserves its receipt.
The production executable was rebuilt, its diagnostic marker rejected explicitly,
and all checks rerun. Future diagnostic builds use an owned target directory.
Neither incident was a gameplay failure or a change to E2 duration/thresholds.
PERSISTENT-CAPTURE-FIXTURE-FAILURE.json also retains the original test's mistaken
fixed host resolution and after-swap readback. Readback now precedes presentation,
and both text bands are checked on the actual host framebuffer dimensions.

## R3-M2: persistent failure and clean Quit

`persistent_proof.py` forces destination failure plus
`LOOKING_GLASS_TRANSITION_FAIL_RECOVERY=always`, for chapter, natural exit,
Retry, quick Load and native Auto Load, with all levers OFF and ON (10 cases).
After two failed rebuilds the real backdrop/toasts are drawn and captured.
Three rendered screen frames precede each scripted input edge through
`Input::ui`. Enter while the fault is still permanent attempts reconstruction,
fails again, and returns to the same screen. A second Enter after clearing the
fault rebuilds successfully. Eight Game states, ordered GPU packets and final
pixels match the retained-outgoing reference exactly in every case.

A separate permanently failing run uses Escape from the screen and exits 0.
It deliberately omits `--frames`, exercising ordinary writable-save mode.
The whole save tree, companions and unrelated bytes retain exact hashes, sizes
and modification times, with no new files. The failed transition closes once
with matching begin/end kind/from/to and success=false, through the Transition
drop guard (`src/transition_memory.rs:124`). No destination state is installed,
and the normal end-of-game Auto writer is bypassed. Cursor capture is explicitly
released at `src/viewer.rs:1255`. No save validation/migration is added to recovery.
The existing destination-build-stage matrix separately tests Npcs failure after
Scene::load succeeds. Persistent recovery faults are injected before its builder;
partial outgoing-rebuild failure remains code-inspected rather than separately
fault-injected.

Screens are retained under `persistent/*/screen-*.png`; PERSISTENT-FAILURE.json
records all save identities, screen events and failed completion brackets.
These are host-scripted keys, not hardware input evidence. Reading the exact
pinned Vita backend `src/native/vita/input.rs:77-78` confirms Cross -> Enter and
Circle -> Backspace. `src/input.rs:455` accepts Backspace as Escape under Vita
controls; `Preferences::vita_emulation` is always true on Vita. No hardware test
is claimed.

## Correct fork interpretation

LG camera portals, menu mirrors and the scaled renderer all use sample_count=1
(the default is also 1). Macroquad supplies a separate resolve Texture2D, so
round 2's alleged LG portal double-delete crash was unreachable. Both fixes stay:

- Miniquad aa9e0ea defensively removes the managed record before GL deletion and
  returns on an absent record, avoiding deletion of a possibly recycled GL name.
  Reuse of a retired ID still fails loudly; IDs remain monotonic.
- Macroquad 90f0fe4 correctly assigns one owner for sample_count=0 attachments.
  This path is currently unused by LG. The tests deliberately exercise synthetic
  zero-sample and repeated-delete cases; their r11b failures do not prove an LG
  crash. Portal-transition tests provide independent final-build regression proof.

The fork pins, material destructor, weak-atlas cleanup and backend capacity
reclamation remain unchanged. Source/licence byte equality, exact isolated fork
unit tests and the synthetic GL regressions all pass again. Historical SOL report
copies in this new output have a prominent correction; their sealed r11c originals
are untouched. The report generator and test descriptions are corrected too.

## Full regression results and fixed E2 schedule

E2-FIXED-PLAN.json was written before any r11d E2 data. Every operation uses
**30 cycles per arm**: 30 Retry, 30 native Load, 60 chapter operations making
30 round trips. Operations are 136 frames apart, with samples at offset 126 and
dt=0.05. The gate skips exactly three cycles, then requires range <=16 KiB and
absolute slope <=1,024 B/cycle over all remaining cycles and over the last ten.
No run length or threshold was enlarged after failure.

| Levers | Operation | Cycles | Post-warm-up range B | Slope B/cycle | Last-ten slope | Gate |
|---|---|---:|---:|---:|---:|---|
{trend_table}

The full census, 16 transition parity pairs, exact TRANSITION-VISUAL pixels and
packets, E1 loading presentation, E2 trend, 58 E3 stage-failure pairs, 10 E3 double
failures, persistent failure, E4 invalid-level refusal, E5 nested/New Game input
precedence, startup chooser, four portal/control transitions, 180-frame Hedge3
and Pandemonium lever parity, save payload parity, and all four 3,731-frame
opening-film runs pass. CLEAN-HOST-BUILD.json binds the proof executable to a
clean committed source rebuild. R11D-VALIDATION.json records the full run list:

| Check | Exit | Seconds |
|---|---:|---:|
{run_table}

Full release-first census (live Rust requests; newlib32 is a conservative chunk
upper bound, not a native measurement):

| Scenario | Levers | Before B | Released B | Peak B | newlib32 peak B | Peak above endpoints B |
|---|---|---:|---:|---:|---:|---:|
{census_table}

Raw fixed E2 populations (initial frame-one sample, then 30 completed cycles):

```text
{samples}
```

Host request counts exclude C/GL/OS allocations, vitaGL pools, native allocator
fragmentation and physical RAM. They qualify no console memory or frame-time claim.

## New native package and offline seals

The new SELF uses the unchanged r10c recipe:

```text
cargo vita build vpk --release --offline --locked --features native_timing,vita_renderer,frame_total_only
VITASDK=/Users/anezium/vitasdk
RUSTUP_TOOLCHAIN=nightly-2026-10-11
RUSTFLAGS=-C target-feature=-neon --cfg observer_free_B
CARGO_NET_OFFLINE=true
```

ELF margin/process parameters, regenerated VELF/SELF equality, linked four-slot
vitaGL collector and six-frame retirement all pass. The VPK contains exactly
eboot.bin and sce_sys/param.sfo. Both arms contain the same SELF:

```text
SELF {n['self_sha256']}
VPK  {n['vpk_sha256']}
ELF  {n['elf_sha256']}
VELF {n['velf_sha256']}
LOCK {n['lock_sha256']}
HOST {final['binary_sha256']}
```

This separate r11d package uses r11c's unchanged console helpers/configs/protocol
except the branch/package identity in preservation. AMENDMENT-R11D.md selects the
new package and carries C1-C3/D1-D5 forward, including the corrected fork wording,
first Hedge3 baseline comparison, opportunistic existing frame-time median/p95,
and the limited meaning of PASS. All OFF and all ON retain exactly:
Hedge3 -> native human Auto Load (Pandemonium) -> chapter Hedge3.
The C1 union headroom label and all A1-A4/B1/M3/M4B, fixed budget, serial-FTP,
no-MKD, independent readbacks and exact human restoration rules survive.
The offline formatter fixture is regenerated from the final engine's real records
and copied into the sealed package; synthetic native allocator substitutions
remain confined to the offline transport boundary.

No verified safe death route is available using the existing inspected inputs
inside the sealed maps. Retry memory is **UNAVAILABLE_NO_VERIFIED_SAFE_DEATH_ROUTE**.
The package keeps the existing optional inspected-natural-death Retry protocol;
it adds no invented movement, damage, cheat or replacement save. Hardware Retry,
portal maps, frame time, native fragmentation/VRAM and audio continuity remain
unqualified.

Loose and ZIP e2e rehearsals ran **without python -I** and both finish
PASS_ORIGINAL_STATE_VERIFIED. They block real sockets/processes before importing
helpers, exercise real helper mains with serial fake FTP maximum 1, and prove
exact human SELF/Auto/settings/companions/original-absence/cache-index restoration.
Real package session directories are absent. ZIP members equal loose bytes and
contain no session, Python cache or game assets. PACKAGE-LESSONS-R11D.json verifies
the original r11c directory, all C/D amendments and the preserved protocol.

| Artifact | SHA-256 |
|---|---|
{hash_table}

SEAL-R11D.json covers this report, ARTIFACT-SHA256.json and CONSOLE-PACKAGE.zip
without a hash cycle. The inventory covers every other file. verify_seal.py
recomputes the entire inventory and detached seal. No console run is represented
by either offline PASS.

No game assets, model identifiers, Co-authored-by trailers or docs/RCHESS1.md
changes entered any commit. Task-owned native targets, temporary source archives,
diagnostic executables and Python caches were deleted; the shared host target
and immutable final proof executables remain. Removed bytes: {cleanup['removed_bytes']:,}.
Final df:

```text
{cleanup['df_after'].rstrip()}
```
'''
(O/'SOL-REPORT-R11D.md').write_text(report)
files = {name:dict(bytes=(O/name).stat().st_size,sha256=sha(O/name)) for name in
         ['ARTIFACT-SHA256.json','SOL-REPORT-R11D.md','CONSOLE-PACKAGE.zip']}
(O/'SEAL-R11D.json').write_text(json.dumps(dict(schema=1,console_executed=False,files=files),indent=2)+'\n')
print('Wrote SOL-REPORT-R11D.md and SEAL-R11D.json')
