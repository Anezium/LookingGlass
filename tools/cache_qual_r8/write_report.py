"""Write the final local implementation report from verified receipts."""
from pathlib import Path
import hashlib, json, re, subprocess, sys
R=Path(__file__).resolve().parents[2];O=Path(sys.argv[1]).resolve()
def read(n):return json.loads((O/n).read_text())
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def line(name,text):return next(i for i,s in enumerate((R/name).read_text().splitlines(),1) if text in s)
def ref(name,text):return f'`{name}:{line(name,text)}`'
remote=read('proof/REMOTE.json');assert remote['status']=='PASS'
host=read('proof/HOST-EQUALITY.json');assert host['status']=='PASS'
build=read('proof/BUILD-COMMAND.json');native=read('packages/baseline/VERIFICATION.json')
for name in ['SESSION-E2E-MAIN.json','SESSION-E2E-ZIP.json']:
    e=read('proof/'+name);assert e['status']=='PASS' and e['final_status']=='PASS_ORIGINAL_STATE_VERIFIED' and e['real_package_session_path_absent']
package=read('proof/PACKAGE-VERIFICATION.json');assert package['status']=='PASS'
clean=read('proof/CLEANUP.json');head=remote['head']
margin=re.search(r'gap=(\d+) bytes',(O/'logs/verify-elf-margin.py.log').read_text()).group(1)
refs=[
 (ref('src/render/vita_renderer.rs','pub audio_cache_qualification'),'the recorder defaults false and is omitted from default serialized config; audio_static_cache remains false.'),
 (ref('src/cache_qualification.rs','pub(crate) fn poll'),'increasing-ID start/stop commands are polled only in existing paused/menu frames, including the native modal loop; no new input or simulation path.'),
 (ref('src/cache_qualification.rs','pub(crate) fn frame'),'actual listener position, scene, newlib heap and per-class counters are captured; active gameplay rows are bounded to 8,192 per window.'),
 (ref('src/cache_qualification.rs','pub(crate) fn acoustics'),'prepare_acoustics elapsed time wraps the same kernel in both arms; storage publication, heap sampling and export are outside that timer.'),
 (ref('src/cache_qualification.rs','struct Export'),'JSON is streamed from borrowed rows after stop; no duplicate per-row JSON tree. Final finish evidence is exported after clean exit.'),
 (ref('src/audio/static_cache.rs','fn reset_for'),'resets inspect actual valid occupancy, scene/pin identity and old Arc reference count, clear the table, drop the pin and record heap. Every Audio load/finish retains its reset.'),
 (ref('src/audio/static_cache.rs','if hit &&'),'optional sampled-hit check recomputes only immutable static traversal and compares all Trace bits. It returns the original cached trace and always performs current mutable completion once.'),
 (ref('src/transition_memory.rs','cache_qualification::transition'),'records both scene-drop phases; Audio::load records post-install heap. Disabled instrumentation adds no logging, IO or recorder allocation.'),
 (ref('src/audio/offmain_tests.rs','fn static_cache_qualification_map_output'),'production-path sample equality with instrumentation and every-hit static validation on Hedge3 and Pandemonium.'),
 (ref('tools/cache_qual_r8/console/audit.py','def validate'),'fixed duration/motion, per-class accounting, audio, warm-reset/scene and absolute heap gates; paired window/transition/load heap gate in report(). Hit rates/timings remain descriptive.'),
 (ref('tools/cache_qual_r8/console/session.py','def saves'),'two independent readbacks preserve changed/new saves before baseline restoration. Preparation extends the inherited exhaustive restoration helper to new saves and stage-index byte restoration.'),
 (ref('tools/cache_qual_r8/test_session_e2e.py','def exercise'),'real helper-main preservation-to-restoration rehearsal, with all network/process boundaries denied before imports and only in-memory transport/synthetic driver activity.'),
]
report=f'''# SOL-REPORT: Alice cache qualification r8

Prepared and plain-pushed branch `vita-cache-qual-r8-20261024` at `{head}` to `https://github.com/Anezium/LookingGlass.git`. Remote HEAD independently matches. New clone: `{R}`. Base is r6 HEAD `73e303c49e62338c6cc3b603259eb11ae02bee19`; final engine/build revision `{build['revision']}`. Later tooling-only commits do not change the engine. No force push, game assets, model identifiers or co-author trailers were added.

All work was Mac-only. No Vita, console port, route query, shot driver, real FTP or receiver was contacted/started. Every driver/transport action in the offline tests was a labelled synthetic fixture. **Console qualification is NOT EXECUTED. Cache and recorder remain default-off.**

## Design and handoff

`CONSOLE-PACKAGE.zip` contains a new common SELF and matched cache-off/on configs, the sealed brief, separate amendments, protocol, preservation/qualification/audit/restoration helpers and mandatory offline rehearsal. The exact future session directory is **session-s8-cache-qual**; it is absent from the loose package and ZIP, including symlinks.

Each arm uses normal gameplay, profile_mode=full, and six windows in fixed order: Hedge3 64-second movement; five-second sampled check; Pandemonium 64-second movement; five-second sampled check; same-map Auto reload with five-second observation; chapter change back to Hedge3 with five-second observation. The first Auto load also proves a warm cross-map load. Before each Auto activation, preserve and restore the original human save and all companions. Quit normally, preserving its new autosave for restoration evidence. No death is attempted; native reload satisfies the required alternative.

Motion uses the existing pilot pad mechanism: 16 pairs of unsigned-byte left-stick (64,128) and (192,128), two seconds each, releasing inputs after each pulse. Existing local receipts confirmed the 128 axis center. No console calibration was run. The native audit requires >=60 seconds of active gameplay AND >=60 seconds of measured listener movement, >=64 path units, no gap >1 second and paired entry listener positions within one unit. Commanded wait duration is not proof of movement. Native UI navigation and confirmations require image inspection and acknowledgements.

Existing input-only route replay was inspected but is tied to gvillage cinematics and new_game/save:slot1. Fullbench freezes the listener and only measures 120 frames; it also suppresses normal saves. Native pad gameplay, chapter selection, Load/Save, quick saves, exits and Retry already exist. Generic demo playback suitable for these maps was not found. A new SELF is needed because r6 cannot export moving-window counters or actual cache occupancy/pin resets.

Reading rules are sealed before console data: zero sampled mismatches; zero underruns; all existing audio gates; sampled window/event heap <=180 MiB and paired B-A window/transition/load heap <=+1 MiB; actual empty cache/no pin/new scene at every warm transition/reload. Late callbacks are report-only with raw windows and summed reported counts. Hit rates and prepare_acoustics timings are descriptive. Self-check stride is ZERO during movement/post-load measurements and 64 only in separate check windows. Both B check windows must check actual hits. No automatic promotion.

The protocol retains arm-aware whole-log integrity, benchmark-window heap, en0 admission, serial locked FTP, no MKD, census-export restoration, fresh helper binding, a fixed 120-minute budget including 30 for restoration and more than 30 minutes to driver expiry. All late corrections are separate amendment files; the sealed brief SHA-256 remains `{read('PACKAGE-IDENTITY.json')['sealed_brief_sha256']}`.

## Changes with file:line

'''+ '\n'.join('- '+a+': '+b for a,b in refs)+f'''

## Proofs

Final host selections PASS under r6's stable host recipe and both default and production simulation policies. Each policy compares 2,211,840 scalar audio sample bits: the four r6 maps plus instrumented Hedge3/Pandemonium. All four original map hashes remain exactly r6's. Instrumented checks compare 380 cached hits on Hedge3 and 152 on Pandemonium per policy, with zero mismatches and unchanged output samples. A deliberately corrupted-entry unit fixture proves mismatch detection while retaining the cached return value. Randomized map fixtures compare 10,240 Trace results per map/policy, four World replacements and seven explicit resets; moving mutable geometry changes results as expected. Warmed synthetic cache/kernel allocation tests remain zero. These are host proofs, never Vita timing/continuity forecasts. See `proof/HOST-EQUALITY.json`, `proof/HOST-TESTS.json` and final host logs. Overlapping test selections must not be summed.

Final native build uses exactly r6's recipe: nightly-2026-10-11, VITASDK=/Users/anezium/vitasdk, `cargo vita build vpk --release --offline --locked --features native_timing,vita_renderer,frame_total_only`, RUSTFLAGS=`-C target-feature=-neon --cfg observer_free_B`, CARGO_INCREMENTAL=0, four jobs. Final build took {build['seconds']:.3f} seconds. Cargo.toml/lock, build.rs/.cargo, atomic logger and direct renderer resolver are unchanged from r6. No cargo update. ELF metadata gap {margin} bytes passes; 192 MiB newlib heap and 8 MiB main stack are unchanged. Observer-hook symbol scan passes. Regenerated VELF and SELF match final originals byte-for-byte; a separate clean independent link is not claimed.

ARM disassembly proves 1,245,184 bytes reserved per window, six windows totaling 7,471,104 bytes (7.125 MiB), common to both arms, plus small metadata. JSON export streams borrowed rows to avoid another large tree. Recorder memory is additional to the approximately 31 KiB static cache; requested sizes are not allocator rounding or measured peaks. See `proof/NATIVE-STORAGE.json` and `STORAGE-AMENDMENT.md`.

Both final loose-helper and exact ZIP rehearsals PASS to **PASS_ORIGINAL_STATE_VERIFIED**, with two launches, six windows per arm, one common SELF stage and at most one fake FTP connection. They run real helper mains for admission/binding, preservation, independent SELF/save checks, staging, pilot motion/commands, collection, log integrity, audit/report and exhaustive restoration. Real socket/process boundaries are denied before helper imports. They prove acceptance of late_callbacks=1, rejection of underrun/missing counter/short motion/mismatch/zero sampled hits/uncleared cache/orphan or interleaved log records, and the exact +1 MiB heap boundary. The stage index is deliberately mutated by the fake stager and restored byte-identically. Initial census/export bytes, initial absences, every original save and all new transition/quit saves are restored; new saves are read twice before deletion. These synthetic receipts are not device results. See `proof/SESSION-E2E-MAIN.json`, `proof/SESSION-E2E-ZIP.json`, `proof/LOG-INTEGRITY-FIXTURES.json` and `proof/FINAL-VALIDATION.json`.

The preserved/restored local human fixtures are the real SELF `387ccf678251d4267b8ced53e207830358826f8e0642631a0459c6b0b9939990` (37,371,674 bytes) and Auto `ea4cd221d9f950e613188ccf3ecf2c1eabb1d34106f413125864320d364c8fa9` (181,900 bytes). These private bytes are never committed or shipped. A future session must independently read and verify CURRENT console originals, all companions and inventories; historical fixture receipts do not admit ownership or substitute for fresh preservation.

## Artifact identities

- Common SELF, both arms: **{native['self_sha256']}**, {native['self_bytes']:,} bytes.
- VPK: **{native['vpk_sha256']}**. It contains only eboot.bin and sce_sys/param.sfo.
- ELF: `{native['elf_sha256']}`.
- VELF: `{native['velf_sha256']}`.
- Console ZIP: `{package['zip_sha256']}`. Every member is verified byte-identical to the loose file; no game assets or session directory.

## Risks and remaining console work

No hardware motion, hit rate, transition/reload heap, sampled self-check, audio continuity or listening was measured. The replay is reproducible pad input, not deterministic simulation: frame timing, collisions, enemies and camera smoothing can diverge. The native duration/path gates reject an unsafe, cinematic or blocked entry; do not improvise a replacement route after results. A route adjustment needs a separate amendment and a fresh matched pair. Death/respawn and natural traversal exits are not claimed exercised.

Diagnostic recorder reservations and timing/heap samples add observer overhead, identically enabled in both arms. They do not establish production heap peaks or moving-gameplay benefit with diagnostics off. The disabled recorder adds branches but performs no recorder IO/allocation/sampling/logging and preserves host audio bits/default serialized config; identical binary bytes or hardware timing are not claimed. Scene pointer safety still relies on immutable geometry and current mutable completion, as in r6. A sampled static bit-check is finite coverage, not proof of all rays. Native mallinfo readings are sampled, not continuous peaks. Retain r6's idle evidence separately and complete this hardware qualification before any default-on decision.

Preparation found and corrected an export-memory duplication risk; its initial build/proofs are retained separately and are not the shipped engine. An existing host audio binding test rejected a reused output directory with File exists; the runner now creates a unique fixture directory per invocation, and the final checks pass. The first offline fixture found a macOS /var versus /private/var ledger-key alias; canonicalizing fixture backup paths fixed it. The final real-helper restoration path and ZIP replay pass. Failed preparation receipts/logs are retained; no console data was collected or discarded.

## Cleanup and disk

Deleted {clean['removed_bytes']:,} logical bytes of task-specific native/temporary/Python caches. Final native artifacts, logs and proofs remain. The existing shared host target is retained; other agents' clones/caches were not touched. No per-task target or Python cache remains in this clone/package.

Final df:

```
{clean['df_after'].rstrip()}
```
'''
(O/'SOL-REPORT.md').write_text(report,encoding='utf-8')
print('Wrote',O/'SOL-REPORT.md')
