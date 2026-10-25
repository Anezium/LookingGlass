"""Write the final local r11b report after proofs, rehearsal, cleanup and inventory."""
from pathlib import Path
import hashlib,json,re,subprocess,sys
R=Path(__file__).resolve().parents[2];T=Path(__file__).resolve().parent;O=Path(sys.argv[1]).resolve()
load=lambda p:json.loads((O/p).read_text());sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
native=load('packages/baseline/VERIFICATION.json');census=load('proof/HOST-CENSUS.json');repeat=load('proof/REPEATED-TRANSITIONS.json');tests=load('proof/HOST-TESTS.json');validation=load('proof/R11B-VALIDATION.json');cleanup=load('proof/CLEANUP.json')
chapter_bound=next(r['release_newlib32_upper_bound']/1048576 for r in census['rows'] if r['scenario']=='chapter' and r['levers'])
assert validation['status']=='PASS' and load('proof/FINAL-VERIFICATION.json')['status']=='PASS'
def line(file,text):
 return next(n for n,s in enumerate((R/file).read_text().splitlines(),1) if text in s)
def ref(file,text):
 n=line(file,text);return f'[{file}:{n}]({R/file}:{n})'
test_count=sum(int(re.search(r'test result: ok\. (\d+) passed',Path(r['log']).read_text())[1]) for r in tests['runs'])
table=[]
for on in [False,True]:
 for action in ['retry','native-load','chapter']:
  row=next(r for r in repeat['rows'] if r['levers']==on and r['action']==action)
  values=[s['live_rust_request_bytes']/1048576 for s in row['steady']]
  table.append(f"| {'ON' if on else 'OFF'} | {action} | "+', '.join(f'{v:.6f}' for v in values)+f" | {row['later_growth_bytes']:+d} |")
peak_rows=[]
for off in [r for r in census['rows'] if not r['levers']]:
 on=next(r for r in census['rows'] if r['scenario']==off['scenario'] and r['levers'])
 peak_rows.append(f"| {off['scenario']} | {off['release_request_peak']/1048576:.3f} | {on['release_request_peak']/1048576:.3f} | {off['release_newlib32_upper_bound']/1048576:.3f} | {on['release_newlib32_upper_bound']/1048576:.3f} |")
seal_names=['CONSOLE-PACKAGE.zip','ARTIFACT-SHA256.json','CONSOLE-BRIEF.md','AMENDMENT-R11B.md','CONSOLE-PROTOCOL.json','proof/COMMAND-GRAMMAR.json']
seal_rows='\n'.join(f'| {name} | `{sha(O/name)}` |' for name in seal_names)
loose=load('proof/SESSION-E2E.json');zipped=load('proof/SESSION-E2E-ZIP.json')
proof_runs='\n'.join(f"{r['tag']}: PASS (exit {r['exit_code']}, {r['seconds']:.2f}s)" for r in validation['runs'])
report=f'''Alice r11b-mem is implemented and prepared on `vita-transition-mem-r11-20261025`.
Engine revision: `{native['source_revision']}`. Package branch head:
`{subprocess.check_output(['git','rev-parse','HEAD'],cwd=R,text=True).strip()}`.
The final native build, host proofs and both offline rehearsals pass. Console
execution is **NOT RUN**. Neither the Vita (192.168.1.23) nor the shot driver was
contacted. All execution used this Mac; the rehearsal denies real sockets and
process launches before importing console helpers.

The review and original SOL-REPORT.md were read first. CONSOLE-BRIEF.md and the
original SOL-REPORT.md are unchanged. All protocol changes are in the new
AMENDMENT-R11B.md, fixed before any console data. Original r11 JSON receipts and
its old artifact inventory are retained in proof/r11-original.

| Finding | Fix and source |
|---|---|
| B1 | The startup bracket opens after the chooser has resolved options.map. {ref('src/viewer.rs','startup_memory.unwrap_or_else')}. The default skool1 → chooser Hedge3 host fixture emits matching real begin/end records. The fake uses records captured from the Rust formatter, and the old begin-skool1/end-Hedge3 fixture must fail. {ref('tools/transition_mem_r11/log_fixture.py','for label,kind')}; {ref('tools/transition_mem_r11/scripts/test_session_e2e.py','old startup begin skool1')} |
| A1 | One shared policy gates waits, passive health and final audit: uordblks <170 MiB and arena <179 MiB, including audio-memory arena samples outside brackets. Arena, fordblks, arena−uordblks and vitaGL pool free values are reported for every transition. {ref('tools/transition_mem_r11/console/transition_policy.py','USED_LIMIT')}; {ref('tools/transition_mem_r11/console/audit.py','allocator_samples')} |
| A2 | Separate amendment documents expected black loading frames and a 100-second deadline. Natural exits retain Loading/title. Title-menu dwell is outside the startup bracket. {ref('tools/transition_mem_r11/AMENDMENT-R11B.md','Black screens')} |
| A3 | The record is derived from the actual cmd_definitions.c table: hashtree, help, launch, nosleep, press, quit, reboot, release, screen, version, wait. Wait is native and also accepted by the sequencer. Reboot is forbidden. {ref('tools/transition_mem_r11/seal.py','commands=re.findall')} |
| A4 | The unused cache_qual_command.json was deleted from source/output configs and both full staging manifests. Original human files are still preserved/restored. {ref('tools/transition_mem_r11/prepare.py','for local,remote')} |
| E1 | Retirement draws the exact original Loading/title commands on natural exit. Its last submitted frame before synchronous construction is captured and compared, in addition to the initial indicator. {ref('src/viewer.rs','fn draw_transition_loading')}; {ref('src/viewer.rs','released_with(||')}; {ref('tools/transition_mem_r11/loading_proof.py','for on in')} |
| E2 | Repeated runs exposed two real library leaks. Macroquad's material destructor removed only its own slot, leaving backend shader/pipeline entries allocated; the local patch flushes queued draws and deletes both once. Miniquad deleted GPU textures but retained their CPU records forever in a Vec; its existing ResourceManager now reclaims records while IDs remain unique. {ref('vendor/macroquad/src/material.rs','fn drop')}; {ref('vendor/macroquad/src/quad_gl.rs','fn delete_material_pipeline')}; {ref('vendor/miniquad/src/graphics/gl.rs','deleted managed texture')}. Every dependency version is unchanged; Cargo.lock changes only Macroquad/Miniquad to vendored same-version sources (the latter from the exact locked Vita fork). No cargo update. |
| E3 | Recovery reacquires scene resources against the untouched live controllers, without save validation, migration or player settling. Live particles retain compact runtime state. Surviving common surface recipes remain registered; dead weak entries are pruned. Music retains its source/decoder and clock rather than restarting. {ref('src/viewer.rs','macro_rules! rollback_resources')}; {ref('src/particles.rs','pub(crate) struct Runtime')}; {ref('src/render_fx.rs','pub(crate) fn prune_surfaces')}; {ref('src/audio.rs','pub(crate) fn reload_failed_transition')}. All five paths and build stages pass the expanded failure matrix. |
| E4 | A fallible incoming level lookup returns through the load-error result, before teardown. Native destination-build failures emit LoadFailed again. Title Load also handles missing identities as menu errors. {ref('src/viewer.rs','match incoming.level().map')}; {ref('src/viewer.rs','eprintln!("Load failed:')}; {ref('src/viewer/start.rs','Action::Load(slot)')}. The review's Store::decode premise was inaccurate: it already calls game.validate(), at {ref('src/save.rs','        game.validate()?;')}. The integrity-correct malformed-save unit test proves refusal; the viewer hook additionally exercises a malformed in-memory Game. |
| E5 | Confirmed New Game owns conflicting F9/Retry input in both the throttled pending state and its active bracket. The other inputs are discarded. A genuinely superseded diagnostic bracket closes unsuccessfully once; its stale guard cannot close its successor. {ref('src/viewer.rs','if new_campaign_memory.is_some() || pending_menu_newgame')}; {ref('src/transition_memory.rs','A later operation supersedes')}. Eight actual-viewer combinations plus the guard unit test pass. |
| E6 | Title-menu sampling cost is removed by moving the bracket after the chooser. Allocating workers sample only mallinfo; vitaGL pool queries occur only in main-thread boundary snapshots. {ref('src/transition_memory.rs','Allocation observers need only mallinfo')}. Native timing cost during builds/film remains unmeasured under the Mac-only restriction. |
| E7 | The ownership comparison remains an explicit r11b overlap-reference toggle, not an e19f73c executable. The new loading proof separately binds the drawing commands to e19f73c and checks the construction frame. Error paths have their own failure proofs. No broader original-binary parity is claimed. |

Repeated-transition census, MiB of live Rust requests: the first value is initial
Hedge3; the following values are the steady sample after each operation/cycle.
Retry ×5 and native Load ×5 use actual viewer operations on Hedge3. Chapter
Hedge3 → Pandemonium → Hedge3 runs ten round-trips, exceeding the required three.
Loads resume and every operation gets six seconds of deterministic real silent-
mixer/game clocks before the steady sample; Hedge3's authored music fade is five
seconds. The old short-run 3.3–4.5 MiB residue included bounded retiring music;
the separate repeated shader/pipeline and texture metadata leaks are fixed at
their destructors. Host timing/creation histories are disabled only in the
allocation-census mode ({ref('src/frame_profile.rs','fn retain_host_history')});
per-frame work counters and the ordinary parity suites remain enabled. A heap
size histogram in heap-diagnostic records the allocation trace that identified
the growing 56-byte texture-metadata vector. Its diagnostic binary is not used
for the final census or equality proofs.

| Levers | Operation | Initial; cycle steady samples (MiB) | Last cycle delta (bytes) |
|---|---|---|---:|
{chr(10).join(table)}

The last-cycle deltas are shown explicitly above. Warm-up/capacity changes are
visible in the full sequences;
these are not a claim of zero allocation drift for arbitrary run lengths.
The large offset does not recur per transition in these settled runs. The late
audio cache/clip/retiring/effect/emitter counts are also recorded and stable;
retiring music is zero at these samples. The late-cycle assertion allows 16 KiB
for small host bookkeeping, rather than treating MiB growth as acceptable.
All per-transition before/released/after and peak records, and all steady samples,
are embedded in HOST-CENSUS.json under repeated_transitions.

| Path | Requests OFF MiB | Requests ON MiB | newlib32 upper OFF MiB | newlib32 upper ON MiB |
|---|---:|---:|---:|---:|
{chr(10).join(peak_rows)}

The largest ordinary request excess over max(before,after) is
{census['max_extra_above_max_endpoints_bytes']/1048576:.3f} MiB; every row is below
4 MiB. The model is max(16,(request+11)&~7), retaining larger host pointer-bearing
allocations as a conservative upper bound. It excludes C/GL/OS allocations,
native fragmentation and exact target-ABI sizing. It is not native RAM data.
The all-ON chapter → Hedge3 upper bound is approximately {chapter_bound:.3f} MiB (the review's
172.1 MiB bound grows slightly when live common rendering recipes are preserved).
Expected native sampled used peak remains approximately **157–160 MiB**, an
inference from retained host/native evidence, not a measurement. The fixed
<170 MiB used gate is deliberately stricter than the host upper bound; a model
overestimate is not permission to relax it after data. Arena <179 MiB stops
299,008 bytes below r8's observed 187,994,112-byte (179.285 MiB) arena failure.
This experimental ceiling does not guarantee a contiguous request can fit.

The final-binary TRANSITION-VISUAL proof has 16 exact state/packet comparisons
and 14 exact destination PNG pairs. LOADING-FRAME-EQUALITY checks initial and
last-submitted loading frames for OFF/ON, with unchanged original drawing
commands; the submitted frame was also inspected visually. The startup proof
starts without --map and follows the chooser's actual chapter-result branch,
with a host-only injected choice. The 58 failure pairs cover chapter and natural
exit at level/scene/NPC/steam/art, Retry/quick-load/native-load at prepare/scene/
logic/NPC/Alice/steam, and invalid-level native Load, in both arms. The reference
keeps original outgoing resources on a failed build; reconstruction must match
its eight Game states, ordered packets and final PNG exactly. Music decoder
position is equal across recovery. These are real host builder faults, not
hardware faults or an exhaustive proof over all maps and assets.

The existing three-lever 180-frame simulation suites, acoustic-cache work and
GPU packet/pixel oracle pass. All {test_count} selected release tests pass,
including four-map SAH, sample-exact audio cache, randomized acoustic work,
save formats, recovery, campaign, skin overlap and the LoadFailed generation test.
Zero-test filters are rejected. Four full opening-film runs finish all 3,731
frames with muted real Mac callback clocks; New Game state and outgoing Auto
payloads remain exact. Final validation output:

```text
{proof_runs}
```

The r10c native invocation is unchanged:

```text
cargo vita build vpk --release --offline --locked --features native_timing,vita_renderer,frame_total_only
VITASDK=/Users/anezium/vitasdk
RUSTUP_TOOLCHAIN=nightly-2026-10-11
RUSTFLAGS=-C target-feature=-neon --cfg observer_free_B
CARGO_NET_OFFLINE=true
```

The final BUILD-COMMAND receipt is for {native['source_revision']}. ELF/process
parameter checks, regenerated VELF/SELF equality and the minimal two-member VPK
(eboot.bin and sce_sys/param.sfo only) pass. The final linked garbage_collector
disassembly again masks ring indices with #3 (four slots); retirement uses six
native frames. Both arms carry the identical SELF, with all three levers OFF in
A and ON in B. Native outputs:

```text
SELF SHA-256  {native['self_sha256']}
VPK SHA-256   {native['vpk_sha256']}
ELF SHA-256   {native['elf_sha256']}
VELF SHA-256  {native['velf_sha256']}
Cargo.lock   {native['lock_sha256']}
```

The package requires Hedge3 → native human Auto Load to Pandemonium → chapter
Hedge3. No verified safe death route exists among the retained inspected inputs;
the package records Retry as unavailable unless an inspected natural death is
safely available. A genuine Retry must be the fourth operation and pass the same
gates. No extra movement, damage command or replacement save is invented.
The offline fake checks the optional fourth-operation audit too. The main loose
and ZIP rehearsals both return PASS_ORIGINAL_STATE_VERIFIED, without python -I,
with {len(loose['negative_fixtures'])} and {len(zipped['negative_fixtures'])}
negative fixtures respectively. These include the old startup mismatch, arena
ceiling at every boundary/peak and outside transitions, heap limits, missing
records, OOM, M3 and M4B failures. Serial fake FTP maximum is one; session
directories remain absent. Synthetic results do not qualify console memory.

M3, M4B double closure/HOME inspection, en0 admission, fixed budget/lifetime,
serial FTP, no MKD, changed-file preservation and independent readbacks are
carried forward. Both rehearsals restore byte-for-byte human SELF
387ccf678251d4267b8ced53e207830358826f8e0642631a0459c6b0b9939990,
Auto ea4cd221d9f950e613188ccf3ecf2c1eabb1d34106f413125864320d364c8fa9,
companions, preferences.json and audio-settings.txt, as well as original absences,
SFO/configs/exports and the host cache index. No actual human console state was
touched by this task. Native crash freedom, arena headroom, VRAM reclamation,
audible continuity and sampling overhead remain for the sole console owner.

Seal hashes:

| Artifact | SHA-256 |
|---|---|
{seal_rows}

SEAL-R11B.json is a detached seal over ARTIFACT-SHA256.json, this report and the
console ZIP. The inventory excludes itself, this report and that detached seal
to avoid a hash cycle; the detached seal covers the report separately. All other
local artifacts are inventoried. The console ZIP excludes sessions and game
assets. No Alice game assets entered the repository; the vendored dependency
contains its licensed built-in UI resources, not Alice data. No prohibited
trailers, model identifiers, force push or docs/RCHESS1.md changes were added.

Task-owned native target/tmp, exploratory binaries and Python caches were
removed. The shared host target is retained. Cleanup removed
{cleanup['removed_bytes']/1048576:.1f} MiB cumulatively. Final df:

```text
{cleanup['df_after'].strip()}
```
'''
(O/'SOL-REPORT-R11B.md').write_text(report,encoding='utf-8')
seal=dict(schema=1,console_executed=False,files={name:dict(bytes=(O/name).stat().st_size,sha256=sha(O/name)) for name in ['ARTIFACT-SHA256.json','SOL-REPORT-R11B.md','CONSOLE-PACKAGE.zip']})
(O/'SEAL-R11B.json').write_text(json.dumps(seal,indent=2)+'\n',encoding='utf-8')
print('Wrote SOL-REPORT-R11B.md and detached SEAL-R11B.json')
