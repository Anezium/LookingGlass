"""A1B complete helper audit, regeneration, and protected-file checks, offline."""
from pathlib import Path
import contextlib,hashlib,io,json,os,runpy,socket,subprocess,sys,tempfile
from unittest.mock import patch
R=Path('/Users/anezium/vita-work/lg-phase-census-r5');T=R/'tools/phase_census_r5';O=Path(sys.argv[1]).resolve()
def sha(path):return hashlib.sha256(path.read_bytes()).hexdigest()
def deny(*args,**kwargs):raise AssertionError('Offline audit attempted networking or process execution')
notes={
 'console/preserve.py':'FIX: r4 package names and inventory dependency; require r5 engine/source revision and sealed branch HEAD.',
 'console/pilot.py':'ADD: shared LOG and CACHE_INDEX paths for exact log boundaries and isolated fixtures. Driver-v5 transport, ownership and recovery contracts retained.',
 'console/session.py':'FIX: use shared LOG path and remove unreachable r4 fullbench-stage branch/duplicate BENCH-ARMED receipt; r5 uses mode-only arming. Actual collection/staging paths executed end to end. Two reads and all stale-export guards retained.',
 'console/ui.py':'FIX: use shared LOG path. Exact s6 labels, Chapter 32, 20/45/55/75-second UI policies, image interlocks retained and main exercised.',
 'console/arm-mode-only.py':'FIX: record exact BENCH-ARMED log byte offset to implement amended heap window; retain four config readbacks.',
 'console/heap_readout.py':'ADD: benchmark, loading/preparation, confirmed post-export and descriptive whole-run heap samples; no continuous peak claim.',
 'console/audit-run.py':'FIX: remove audio-package predicate and obsolete B-offmain requirement; both r5 arms require requested=false active=false. Add actual heap readout.',
 'console/run_health.py':'FIX: r4 comment and missing CPAL late-callback stop; retain >180 MiB and strict producer p95 <8 ms.',
 'console/strict-audio.py':'FIX: nonzero late_callbacks previously reported but not gated. Actual main rejects constant nonzero counts.',
 'console/host-log-receiver.py':'FIX: r4 connection banner becomes r5; listener provenance and unchanged driver-v5 paths retained.',
 'console/light_csv.py':'FIX: remove obs02 historical-label special case and unreachable observed-mode branch; r5 requires direct, blank non-wall fields.',
 'console/compare-abba.py':'FIX: inherited lever field becomes diagnostic; add explicit A1-A4 drift. Observer threshold and continuation rules retained.',
 'console/census_report.py':'FIX: missing amendments 3-8 root, children/self nesting, calls/frame, bias/ranking, acoustics, collision coverage and residual views; validate r5 export shape.',
 'console/make-report.py':'FIX: whole-run heap delta used for qualification; now use BENCH-ARMED to measured=120 and report other windows separately. Publish amendments 3-8 per B and map residual labels.',
 'console/baseline-r3.py':'RETAIN: historical filename and four-map light-export preservation list are restoration coverage, not launch maps. Both r5 census paths recorded; amendment 1 exact regex retained.',
 'console/restore-files.py':'RETAIN: exact brief-mandated four-map light/census cleanup regex and independent readbacks; old files are preserved, never launched.',
 'console/map_identity.py':'RETAIN: pinned real human autosave identity; only Pandemonium/Hedge3 canonical maps and s6 protocol labels admitted.',
 'console/check-session-budget.py':'RETAIN: 8 launches, 4 Pandemonium allowances, 15 + 4.5 + 72 + 6 + 30 = 127.5 min; cap 150, expiry margin strictly >30, fixed 43200-second driver lifetime. Offline admission and every arm boundary exercised.',
 'console/entry-state.py':'RETAIN: runtime log entry observations and unavailable state labels; real main exercised for all labels.',
 'console/ack.py':'RETAIN: pending-image acknowledgement and no input before inspection interlock; real main exercised with fake images.',
 'console/restore-arm.py':'RETAIN: per-arm small ledgers only, never SELF; real main exercised eight times.',
 'console/restore-ledgers.py':'RETAIN: reverse mtime rollback including root SELF, no retry of wedged ledgers; real main exercised.',
 'console/verify-self.py':'RETAIN: two actual byte/hash checks of restored human SELF; real main exercised against memory bytes.',
 'console/verify-human-entry.py':'RETAIN: two reads before any fake write; pinned human SELF size/hash, real main exercised.',
 'console/verify-arm-self.py':'RETAIN: each package common SELF size/hash and fixed remote path; real main exercised eight times.',
 'console/save-proof.py':'RETAIN: fresh session save and all companions, pinned autosave size/hash; real main exercised.',
 'scripts/prepare.py':'FIX: incomplete r4 copy-and-replace helper generation becomes canonical r5 console sources. r4 baseline bytes, manifest templates and immutable log fixtures are intentional build inputs.',
 'scripts/test_tools.py':'FIX: inherited r4 equality checks had required broken preserve.py bytes. Compare every helper with canonical r5; retain and extend focused restoration/collection fixtures.',
 'scripts/test_a1.py':'UPDATE: all clone helper copies and recursive canonical sources checked; original protected identities retained.',
 'scripts/reseal_a1.py':'FIX: old A1-only reseal could omit r5 identity/validation. Compatibility entry delegates to A1B resealer.',
 'scripts/reseal_a1b.py':'ADD: helper-only reseal, protected-file check, remote branch verification, full offline regression, updated ZIP/manifest/validation and report.',
 'scripts/test_a1b.py':'ADD: every console/scripts helper individually accounted for; canonical regeneration, immutable artifacts and source hashes checked.',
 'scripts/test_session_e2e.py':'ADD: identical before/after full-session regression with blocked sockets/processes, memory FTP and driver fake. Historical local SELF/save bytes and host census rows are fixture inputs only.',
 'scripts/finalize.py':'FIX: new package identity must be created and included in ZIP; exclude hash-bearing fix reports from manifest circularity. Native build not executed.',
 'scripts/run_tests.py':'FIX: declared host rerun list had six selections while proof had seven; restore direct-resolver selection. Engine tests not rerun for helper-only change.',
 'scripts/census_report.py':'FIX: synchronize canonical amended census report.',
 'scripts/make_report.py':'FIX: synchronize canonical amended result assembly.',
 'scripts/bind_helpers.py':'RETAIN: fresh session-s6-phase-census only; exact helper hashes and no ownership admission. Old blocked session not reused or edited.',
 'scripts/log-integrity.py':'RETAIN: r4-1 names an unchanged integrity policy, not an r4 session. Failure status already R5; full main exercised on all eight new logs.',
 'scripts/test-log-integrity.py':'RETAIN: s4 fixture filenames and R3-SOURCES.json describe immutable historical regression inputs only. Labels under test come from r5 protocol; 12 cases.',
 'scripts/seal.py':'RETAIN: original native-build verifier; r4 baseline/config and base revision are historical identity assertions. Not executed; packages untouched.',
 'scripts/build_native.py':'RETAIN: original native build tool, no r4 arm/map/session assumptions. Not executed.',
 'scripts/host.py':'RETAIN: two maps, ABBA baseline/census, 180 host frames (60+120). Existing host equality proof retained; no new host run.',
 'scripts/capture-environment.py':'RETAIN: local toolchain capture only, no package/session assumptions. Not executed.',
 'scripts/verify-elf-margin.py':'RETAIN: linked-ELF minimum margin check, no arm/map/session assumptions. Existing sealed proof retained.',
 'scripts/verify-process-params.py':'RETAIN: 192 MiB heap reservation and 8 MiB stack ELF checks; distinct from 180 MiB sampled stop. Existing sealed proof retained.'
}
with contextlib.ExitStack() as stack:
 for obj,name in [(socket.socket,'connect'),(socket.socket,'connect_ex'),(socket.socket,'bind'),(socket,'create_connection'),(socket,'getaddrinfo'),(subprocess,'Popen'),(os,'system')]:stack.enter_context(patch.object(obj,name,deny))
 paths=sorted([* (O/'console').glob('*.py'),* (O/'scripts').glob('*.py')])
 assert {str(p.relative_to(O)) for p in paths}==set(notes), 'Audit must cover EVERY helper, without stale entries'
 for path in [*paths,*T.rglob('*.py')]:compile(path.read_bytes(),str(path),'exec')
 for path in T.glob('*.py'):assert path.read_bytes()==(O/'scripts'/path.name).read_bytes(),path
 for path in (T/'console').glob('*.py'):assert path.read_bytes()==(O/'console'/path.name).read_bytes(),path
 assert (T/'census_report.py').read_bytes()==(T/'console/census_report.py').read_bytes()
 assert (T/'make_report.py').read_bytes()==(T/'console/make-report.py').read_bytes()
 print('PASS syntax, all canonical console helpers and clone/package scripts match')
 with tempfile.TemporaryDirectory(prefix='alice-a1b-regeneration-') as tmp:
  with patch.object(sys,'argv',['prepare.py',tmp]),contextlib.redirect_stdout(io.StringIO()):runpy.run_path(str(T/'prepare.py'),run_name='__main__')
  for path in (O/'console').glob('*.py'):assert path.read_bytes()==(Path(tmp)/'console'/path.name).read_bytes(),path
  assert (Path(tmp)/'CONSOLE-PROTOCOL.json').read_bytes()==(O/'CONSOLE-PROTOCOL.json').read_bytes()
 print('PASS prepare regenerates every r5 helper and the unchanged eight-label protocol')
 protected=json.loads((O/'proof/A1B-PROTECTED-BEFORE.json').read_text())
 for name,digest in protected.items():assert sha(O/name)==digest,name
 assert sha(O/'CONSOLE-BRIEF.md')=='48857506bc84cb02f322c1fa16603bcd94727cd5435dc96cab39a4fa1b2173c0'
 source=json.loads((O/'proof/SOURCE-IDENTITY.json').read_text())
 for name,digest in source['files'].items():assert sha(R/name)==digest,name
 assert not list(T.rglob('__pycache__')) and not list(O.rglob('__pycache__'))
 print('PASS engine, SELF, configs, brief and blocked-session evidence unchanged; no bytecode caches')
 audit=dict(status='PASS',offline_only=True,helper_count=len(paths),files={str(path.relative_to(O)):dict(sha256=sha(path),audit=notes[str(path.relative_to(O))]) for path in paths})
 (O/'proof/A1B-HELPER-AUDIT.json').write_text(json.dumps(audit,indent=2)+'\n')
 print('PASS individual audit of all',len(paths),'console/ and scripts/ helpers')
print('All A1B offline audit checks passed')
