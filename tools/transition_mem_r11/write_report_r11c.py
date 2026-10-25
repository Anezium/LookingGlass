"""Write the final r11c report and detached seal after all evidence is final."""
from pathlib import Path
import hashlib,json,re,subprocess,sys
R=Path(__file__).resolve().parents[2];O=Path(sys.argv[1]).resolve()
load=lambda p:json.loads((O/p).read_text());sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
n=load('packages/baseline/VERIFICATION.json');repeat=load('proof/REPEATED-TRANSITIONS.json');v=load('proof/R11C-VALIDATION.json');cleanup=load('proof/CLEANUP-R11C.json');deps=load('proof/DEPENDENCY-BYTE-EQUALITY.json')
assert load('proof/FINAL-VERIFICATION.json')['status']=='PASS' and v['status']=='PASS'
head=subprocess.check_output(['git','rev-parse','HEAD'],cwd=R,text=True).strip()
def loc(root,file,text):
 p=Path(root)/file;line=next(i for i,s in enumerate(p.read_text().splitlines(),1) if text in s)
 return f'[{file}:{line}]({p}:{line})'
mini=Path('/Users/anezium/vita-work/miniquad-texture-r11c');macro=Path('/Users/anezium/vita-work/macroquad-material-r11c')
trend=[];samples=[]
for row in repeat['rows']:
 arm='ON' if row['levers'] else 'OFF'
 trend.append(f"| {arm} | {row['action']} | {row['cycles']} | {row['post_warmup_range_bytes']} | {row['slope_bytes_per_cycle']:.3f} | {row['last_ten_slope_bytes_per_cycle']:.3f} | PASS |")
 samples.append(f"{arm} {row['action']}: "+', '.join(str(s['live_rust_request_bytes']) for s in row['steady']))
forks='\n'.join(f"- [{f['name']} fork]({f['url']}/tree/{f['revision']}), `{f['revision']}`; {len(f['files'])} source/licence/build files compared." for f in deps['forks'])
tests=load('proof/HOST-TESTS.json');count=sum(int(re.search(r'test result: ok\. (\d+) passed',Path(r['log']).read_text())[1]) for r in tests['runs'])
seal_names=['CONSOLE-PACKAGE.zip','ARTIFACT-SHA256.json','AMENDMENT-R11C.md','CONSOLE-PROTOCOL.json']
seals='\n'.join(f'| {name} | `{sha(O/name)}` |' for name in seal_names)
report=f'''Alice r11c transition-memory implementation is complete on
`vita-transition-mem-r11c-20261025`. Engine source: `{n['source_revision']}`.
Package/tooling branch head: `{head}`. All branches were plain-pushed.

Console execution is **NOT RUN**. Neither the parked Vita nor the shot driver
was contacted. All tests and builds ran on this Mac. The offline rehearsal
blocks real sockets and process launches before importing console helpers.
Hardware qualification in REVIEW-R2 M3/M5/M6 remains outside this Mac-only task.

The requested REVIEW-R2.md, REVIEW.md and SOL-REPORT-R11B.md were read first.
The original r11/r11b output directory is unchanged. This package is its separate
r11c successor; the sealed original brief, report and earlier amendments retain
their bytes. AMENDMENT-R11C.md records C1/C2/C3 before console data.

| Fix | Source |
|---|---|
| Idempotent managed texture deletion removes metadata before deleting the GL name and returns on an absent record. IDs remain monotonic. | {loc(mini,'src/graphics/gl.rs','Retire managed metadata first')} |
| Non-resolved render-pass color attachments have one deletion owner. Their Texture2D handle is unmanaged; independently resolved textures remain managed. | {loc(macro,'src/texture.rs','Without a resolve target')} |
| The reviewed material destructor flushes queued draws, then releases backend pipeline and shader once. | {loc(macro,'src/material.rs','Flush queued draws')}; {loc(macro,'src/quad_gl.rs','fn delete_material_pipeline')} |
| Frame cleanup removes dead weak atlas candidates even when no atlas is ever built. | {loc(macro,'src/texture.rs','fn collect_unbatched')}; {loc(macro,'src/lib.rs','texture_batcher.garbage_collect')} |
| Excess backend table capacity is reclaimed at frame commit after a large live-set reduction; ordinary churn does not trigger rehashing. | {loc(mini,'src/graphics/gl.rs','fn reclaim_retired_capacity')} |
| Failed outgoing reconstruction builds into locals, retries once and preserves untouched live controllers/checkpoint. Persistent failure waits in a retry/quit screen without writing saves. | {loc(R,'src/viewer.rs','Build into locals')}; {loc(R,'src/transition_memory.rs','pub(crate) fn recovery_fault')} |

The old scene cannot keep drawing if its released resources cannot be reacquired
from persistently unavailable storage. That physical limit is explicit: the
viewer stays alive with its compact checkpoint until the user retries or quits.
A transient destination failure plus first outgoing-build failure now restores
the exact old running state. All five paths, both lever settings, match the
retained-outgoing reference for eight Game states, ordered packets and final PNG:
10 double-failure pairs in DOUBLE-FAILURE-EQUALITY.json, plus all 58 earlier
failure pairs. Audio decoder clocks remain equal. No save validation/migration
is introduced into live recovery.
The staged recovery result is boxed so a retry-screen await does not reserve
additional full Scene/Npcs tuples in the live viewer's async frame.

Forks and byte identity:

{forks}

Miniquad starts at `6f891804784bf4a5b39313ee6499344cb0d7e8b6`; the unrelated four
VAO commits are excluded. Macroquad starts at
`2c60f754b0c0e053f4c5b272fee696fb45e5aca8`. The cached published 0.4.14 crate
hashes to `{deps['package_sha256']}`; its VCS metadata names that commit and every
published src file equals that git tree. The reviewed material changes were
copied byte-for-byte in commit 041bf02 before the additional r11c fixes.

DEPENDENCY-BYTE-EQUALITY.json records SHA-256 for every compared file and proves
the Cargo-consumed checkout equals the clean fork. All reviewed source bytes are
equal except the explicit r11c addenda: miniquad gl.rs deletion guard/capacity
reclamation/test, and macroquad texture.rs/lib.rs ownership/weak-candidate
cleanup/test. DEPENDENCY-PATCHES are superseded by the two complete r11c diff
files. No other source difference is silently accepted. Cargo.toml uses git+full
rev for both patches. Macroquad's macro helper uses the published registry
0.1.8 dependency, matching the crate rather than acquiring a third git package.
LOCK-SOURCE-COMPARISON.json proves only the source field of macroquad/miniquad
changed against aec5590; all other packages and every version are unchanged.
No cargo update was used. vendor/ is deleted. Licences and notices are retained.

The real-GL regressions fail on reviewed r11b: render-target deferred cleanup
and a repeated delete after render-pass drop. Ownership-only and guard-only
variants each pass independently. The final dependency unit tests cover 10,000
texture reloads, monotonic IDs, 30 large table retirements and 10,000 weak atlas
candidate retirements. Isolated tests use exact fork library bytes with only
example dev-dependencies/disabled optional subsystems omitted from their temporary
manifests; the project lock stays untouched. Full engine builds use --locked.

Live outgoing portal captures and actual chapter transitions pass on hatter1,
keep and fortress1. Skool2 is also transitioned as a no-portal corpus control.
PORTAL-TRANSITIONS.json records captures and results. Host portal tests stage
authored brush transforms; they do not alter gameplay state or native code.

E2 diagnosis and extended trend:

The 32/64 KiB doublings were an unbounded macroquad Vec of dead weak pending-
atlas candidates, not allocator bins. The 30-cycle diagnostic also reproduced
128/256 KiB doublings at cycles 14/29. Frame cleanup now removes those candidates
at the owning source. The 541,104-byte cycle-8 step decomposes into 540,672 bytes
of buffer-table replacement (540,680 -> 1,081,352), 384 bytes of bounded console
ring capacity and 48 bytes of new console text. Backtraces name miniquad's
ResourceManager<Buffer> through retained::Gpu::upload. A separate 53,760-byte
step is pipeline-table replacement (53,768 -> 107,528). Backend live populations
stay fixed per map (Hedge3 7,788 buffers/251 pipelines, Pandemonium 4,362/252 in
the diagnostic); table tombstones and retained bucket capacity caused the late
steps. Reclaiming excess capacity after retirement makes these tables repeatable.
The residual 16+32 bytes per chapter cycle are the two console messages; the
console truncates at 512 characters and retains only 128 lines. Its bound test
passes; this is bounded history, not a scene leak. SMALL-ALLOCATION-ATTRIBUTION
and the raw diagnostic logs retain stack and histogram evidence. Diagnostic
instrumentation/binaries are separate from final census binaries and removed
after attribution.

The gate was fixed before the final run: skip exactly three operation cycles,
then require max-minus-min <=16 KiB over **all** remaining cycles, absolute linear
slope <=1,024 bytes/cycle over that population, and the same slope bound over its
last ten. Retry/Load run 20 cycles each, beyond the requested ten, to distinguish
a late bounded 6.4 KiB warm-up step from a continuing slope. Synthetic late-step and
cumulative-growth fixtures that pass the old last-delta check must fail this gate.
Each operation gets six deterministic seconds for authored audio fades to settle.

| Levers | Operation | Cycles | Full post-warm-up range B | Full slope B/cycle | Last-ten slope B/cycle | Gate |
|---|---|---:|---:|---:|---:|---|
{chr(10).join(trend)}

Raw live Rust request bytes: initial sample, then every completed cycle:

```text
{chr(10).join(samples)}
```

These are Rust-request counts on the host, excluding C/GL/OS allocations and
native fragmentation. They do not qualify native arena or VRAM behaviour.
The census, 16 transition parity pairs, TRANSITION-VISUAL packet/pixel equality,
E1 loading frames, E3 matrix, E4 invalid-level refusal and LoadFailed event,
E5 New Game/input precedence, startup chooser/B1, 180-frame lever suites,
save payload equality and all four 3,731-frame opening films pass on the same
immutable final executable. {count} selected engine tests and three dependency
tests pass. CLEAN-HOST-BUILD proves a clean committed-source rebuild equals
the proof binary; R11C-VALIDATION lists all checks and timings.
One provisional earlier build/run differed at two adjacent blue-channel values
by 1/255 despite equal state/packets. Six independent controls were pixel-equal;
RASTER-DIAGNOSTIC retains that investigation. The final full failure proof uses
strict byte equality without any raster tolerance.

Native rebuild uses the unchanged r10c recipe:

```text
cargo vita build vpk --release --offline --locked --features native_timing,vita_renderer,frame_total_only
VITASDK=/Users/anezium/vitasdk
RUSTUP_TOOLCHAIN=nightly-2026-10-11
RUSTFLAGS=-C target-feature=-neon --cfg observer_free_B
CARGO_NET_OFFLINE=true
```

Both arms contain the same SELF. A is all OFF, B all ON. ELF/process-parameter
checks, regenerated VELF/SELF byte equality and minimal two-member VPK checks
pass. The final linked vitaGL garbage collector has four ring slots, and the
engine still retires through six native frames. Native outputs:

```text
SELF {n['self_sha256']}
VPK  {n['vpk_sha256']}
ELF  {n['elf_sha256']}
VELF {n['velf_sha256']}
LOCK {n['lock_sha256']}
```

C1 reports arena headroom for passing and failed audits; passing arms below
4,299,008 bytes or at peak arena >=175 MiB carry PASS_LOW_ARENA_HEADROOM.
Those review boundaries are not equivalent (175 MiB leaves 4,493,312 bytes);
the amendment conservatively honours both before data. The 170/179 MiB A1 gates remain
unchanged. C2 stays on Hedge3 -> Pandemonium -> Hedge3, including death-route
search restrictions. C3 limits PASS to this exact SELF and sampled hardware
evidence. No host result qualifies Retry on the console or replaces the pending
native fragmentation/VRAM/audio/frame-time checks. The package carries every
r11b B1/A1-A4/M3/M4B preservation, closure, budget, serial-FTP and restoration rule.

Loose and ZIP rehearsals, without python -I, both end in
PASS_ORIGINAL_STATE_VERIFIED. They verify exact restoration of human SELF
387ccf678251d4267b8ced53e207830358826f8e0642631a0459c6b0b9939990 and human Auto
ea4cd221d9f950e613188ccf3ecf2c1eabb1d34106f413125864320d364c8fa9, companions,
settings, original absences, exports and cache index. Fake FTP maximum is one.
Session directories and Python caches are absent. No game assets, prohibited
trailers, model identifiers or docs/RCHESS1.md edits entered any commit.

| Artifact | SHA-256 |
|---|---|
{seals}

SEAL-R11C.json covers this report, the artifact inventory and console ZIP without
a hash cycle. The inventory covers all other local files, including historical
r11/r11b documents. The original package directory was not resealed or changed.
Task-owned native targets, temporary sources, exploratory binaries and Python
caches were removed; the shared host target is retained. Removed this task:
{cleanup['removed_bytes']/1048576:.1f} MiB. Final df:

```text
{cleanup['df_after'].strip()}
```
'''
(O/'SOL-REPORT-R11C.md').write_text(report)
files={name:dict(bytes=(O/name).stat().st_size,sha256=sha(O/name)) for name in ['ARTIFACT-SHA256.json','SOL-REPORT-R11C.md','CONSOLE-PACKAGE.zip']}
(O/'SEAL-R11C.json').write_text(json.dumps(dict(schema=1,console_executed=False,files=files),indent=2)+'\n')
print('Wrote SOL-REPORT-R11C.md and SEAL-R11C.json')
