"""Reseal the r5 Mac-only helper correction after a clean plain push.

No native build, Vita transport, driver invocation or existing-session mutation.
"""
from pathlib import Path
import hashlib,json,os,subprocess,sys,zipfile
R=Path('/Users/anezium/vita-work/lg-phase-census-r5');T=R/'tools/phase_census_r5';O=Path(sys.argv[1]).resolve()
BRIEF='48857506bc84cb02f322c1fa16603bcd94727cd5435dc96cab39a4fa1b2173c0'
ENGINE='62704971576f7e3d30499962388b69bf2ed692d7'
def sha(path):return hashlib.sha256(path.read_bytes()).hexdigest()
def write(path,value):path.write_text(json.dumps(value,indent=2)+'\n')
def git(*args):return subprocess.check_output(['git',*args],cwd=R,text=True).strip()
def check_protected():
 moved={}
 if (O/'MOVE-RECEIPT.json').exists():
  receipt=json.loads((O/'MOVE-RECEIPT.json').read_text())
  assert receipt['status']=='PASS' and receipt['all_hashes_identical']
  for record in receipt['files']:
   assert record['before_sha256']==record['after_sha256']==sha(O/record['to']),record['to']
   assert not (O/record['from']).exists(),record['from']
   moved[record['from']]=record['to']
 for name,digest in json.loads((O/'proof/A1B-PROTECTED-BEFORE.json').read_text()).items():assert sha(O/moved.get(name,name))==digest,name
 assert sha(O/'CONSOLE-BRIEF.md')==BRIEF
 for arm in ['baseline','census']:assert sha(O/'packages'/arm/'eboot.bin')=='50ac74a2ced3e4caf4ad239828df2dcbe1007f51be6b465471f00f5d3b918f69'
 for name,digest in json.loads((O/'proof/SOURCE-IDENTITY.json').read_text())['files'].items():assert sha(R/name)==digest,name
check_protected()
assert sha(O/'proof/A1B-SEALED-ARTIFACT-SHA256.json')=='f65ac33a59dec730edf949707cac08d3a4d54ec4e22de77d82d34b3dbc68dce5'
sealed_manifest=json.loads((O/'proof/A1B-SEALED-ARTIFACT-SHA256.json').read_text())
with zipfile.ZipFile(O/'proof/A1B-SEALED-HELPERS.zip') as archive:
 for name in archive.namelist():
  raw=archive.read(name);assert len(raw)==sealed_manifest[name]['bytes'] and hashlib.sha256(raw).hexdigest()==sealed_manifest[name]['sha256'],name
assert not git('status','--porcelain'),'Commit helper fixes before resealing'
branch=git('branch','--show-current');head=git('rev-parse','HEAD')
assert branch=='vita-phase-census-r5-20261024'
remote=git('ls-remote','--heads','origin',branch);assert remote.split()[0]==head,'Plain-push the helper commit before resealing'
identity=dict(package='alice-phase-census-r5',engine_source_revision=ENGINE,branch=branch,branch_head=head)
write(O/'PACKAGE-IDENTITY.json',identity)
env=dict(os.environ,PYTHONDONTWRITEBYTECODE='1')
def run(name,script,*args,expected=0):
 with (O/'logs'/name).open('w') as log:
  process=subprocess.run([sys.executable,str(T/script),str(O),*map(str,args)],env=env,stdout=log,stderr=subprocess.STDOUT)
 assert process.returncode==expected,(name,process.returncode,(O/'logs'/name).read_text()[-4000:])
 return process.returncode
run('a1b-e2e-before.log','test_session_e2e.py','--sealed-helpers',O/'proof/A1B-SEALED-HELPERS.zip','--output',O/'proof/A1B-E2E-BEFORE.json',expected=1)
before=json.loads((O/'proof/A1B-E2E-BEFORE.json').read_text());assert before['status']=='FAIL' and before['error_type']=='FileNotFoundError' and 'packages/audio/VERIFICATION.json' in before['error']
run('a1b-e2e-after.log','test_session_e2e.py','--output',O/'proof/A1B-E2E-AFTER.json','--evidence-dir',O/'proof/A1B-SYNTHETIC-SESSION')
run('a1b-focused-checks.log','test_tools.py')
# Historical fixture runner is package-owned, and invokes only the host log gate.
with (O/'logs/a1b-log-integrity-checks.log').open('w') as log:
 process=subprocess.run([sys.executable,str(O/'scripts/test-log-integrity.py')],env=env,stdout=log,stderr=subprocess.STDOUT)
assert process.returncode==0
run('a1b-audit-checks.log','test_a1b.py')
after=json.loads((O/'proof/A1B-E2E-AFTER.json').read_text());assert after['status']=='PASS' and after['final_status']=='PASS_ORIGINAL_STATE_VERIFIED' and after['branch_head']==head
checks=json.loads((O/'proof/CONSOLE-DRY-RUN.json').read_text());integrity=json.loads((O/'proof/LOG-INTEGRITY-FIXTURES.json').read_text());audit=json.loads((O/'proof/A1B-HELPER-AUDIT.json').read_text())
assert checks['status']==integrity['status']==audit['status']=='PASS' and len(integrity['checks'])==12
check_protected()
assert not list(O.rglob('__pycache__')) and not list(T.rglob('__pycache__'))
# The compact archive adds the new sealed identity; no proof is confused with a
# live console session. Offline evidence stays outside the operational ZIP.
with zipfile.ZipFile(O/'CONSOLE-PACKAGE.zip','w',zipfile.ZIP_DEFLATED,compresslevel=6) as archive:
 for name in ['CONSOLE-BRIEF.md','DESIGN.md','CONSOLE-PROTOCOL.json','PACKAGE-MATRIX.json','PACKAGE-IDENTITY.json']:archive.write(O/name,name)
 for directory in ['configs','console']:
  for path in sorted((O/directory).iterdir()):
   if path.is_file():archive.write(path,str(path.relative_to(O)))
 for path in sorted(O.glob('stage-*.json')):archive.write(path,path.name)
 for arm in ['baseline','census']:
  for name in ['eboot.bin','VERIFICATION.json']:
   path=O/'packages'/arm/name;archive.write(path,str(path.relative_to(O)))
 for name in ['log-integrity.py','bind_helpers.py']:
  path=O/'scripts'/name;archive.write(path,str(path.relative_to(O)))
with zipfile.ZipFile(O/'CONSOLE-PACKAGE.zip') as archive:
 assert archive.testzip() is None
 for name in archive.namelist():assert archive.read(name)==(O/name).read_bytes(),name
 zip_members=len(archive.namelist())
zip_hash=sha(O/'CONSOLE-PACKAGE.zip')
write(O/'proof/REMOTE.json',dict(status='PASS',head=head,remote=remote,branch=branch,plain_push=True,working_tree_clean=True,engine_source_revision=ENGINE))
validation=json.loads((O/'proof/FINAL-VALIDATION.json').read_text())
validation.update(status='PASS',head=head,engine_build_revision=ENGINE,working_tree_clean=True,remote_verified=True,
 artifact_hashes_verified=True,console_helper_checks=checks['count'],log_integrity_fixture_cases=12,
 package_zip_sha256=zip_hash,console_brief_sha256=BRIEF,amendment='A1B',device_contact=False,driver_contact=False,
 engine_self_configs_unchanged=True,blocked_session_evidence_unchanged=True,console_not_executed=True,
 amendment_offline_checks='logs/a1b-audit-checks.log',e2e_before='proof/A1B-E2E-BEFORE.json',e2e_before_exit_code=1,
 e2e_after='proof/A1B-E2E-AFTER.json',e2e_after_exit_code=0,e2e_maps=2,e2e_launches=8,e2e_restoration='PASS_ORIGINAL_STATE_VERIFIED',
 e2e_synthetic_only=True,helper_audit='proof/A1B-HELPER-AUDIT.json',helpers_audited=audit['helper_count'],
 historical_engine_proofs_retained=True,native_local_checks_rerun=False,
 engine_tests_rerun_for_A1B=False,package_identity='PACKAGE-IDENTITY.json')
write(O/'proof/FINAL-VALIDATION.json',validation)
excluded={'ARTIFACT-SHA256.json','SOL-REPORT.md','A1-FIX.md','A1B-FIX.md'}
manifest={str(path.relative_to(O)):dict(bytes=path.stat().st_size,sha256=sha(path)) for path in sorted(O.rglob('*')) if path.is_file() and path.name not in excluded}
write(O/'ARTIFACT-SHA256.json',manifest)
for name,record in manifest.items():assert (O/name).stat().st_size==record['bytes'] and sha(O/name)==record['sha256'],name
manifest_hash=sha(O/'ARTIFACT-SHA256.json')
rows='\n'.join('| `'+name+'` | '+record['audit']+' |' for name,record in audit['files'].items())
old_output=(O/'logs/a1b-e2e-before.log').read_text().rstrip();new_output=(O/'logs/a1b-e2e-after.log').read_text().rstrip()
report=f'''# A1B r5 console helper correction

Mac-only correction, plain-pushed to `{branch}` at `{head}`. Engine/source revision remains `{ENGINE}`. No Vita or shot-driver contact, engine change, native rebuild, SELF change, config change, or brief edit occurred. Blocked evidence is retained unchanged; MOVE-RECEIPT.json records any archival relocation. The real package session path must be absent before admission.

The preservation main path now resolves exactly baseline/census from the r5 protocol, validates the package engine/source revision, and records the r5 package, both SELF hashes, engine/source revision, branch and sealed tooling HEAD in SESSION.json. PACKAGE-IDENTITY.json separates the tooling HEAD from the unchanged engine revision and is included in the operational ZIP. Current directory inventories are enumerated afresh, including nested save/cache/report directories, without depending on the old r4 inventory file. The shared cache index is copied before staging.

## Every leftover found and fixed

1. preserve.py used baseline/audio/world-static-merge instead of baseline/census, depended on a historical console-r4 inventory path and omitted engine/source revision and branch HEAD. All are fixed.
2. audit-run.py still computed off-main engagement from the r4 audio package name and retained an obsolete audio-B counter requirement. Both r5 arms explicitly require requested=false and active=false.
3. host-log-receiver.py stamped new connections as r4; it now stamps r5. run_health.py's inherited r4 interval comment now describes the shared r5 interval.
4. session.py retained an unreachable r4 fullbench-staging branch and duplicate BENCH-ARMED receipt without the new boundary. These are removed; fullbench uses arm-mode-only.py exclusively. light_csv.py retained the obs02 label exception and unreachable observed-mode counter checks. These are removed; direct-mode and blank legacy fields remain mandatory.
5. compare-abba.py called the census a lever and omitted an explicit A1-A4 drift field. It now identifies a diagnostic and records drift, both oriented deltas and the unchanged strict >1 ms observer rule.
6. strict-audio.py retained late_callbacks as a reported raw field without gating nonzero values, and run_health.py missed the same stop. Both now reject nonzero CPAL late callbacks, including a constant nonzero sequence with zero endpoint delta.
7. make-report.py used whole-run heap maxima for the B-A label. arm-mode-only.py now records an exact BENCH-ARMED log offset; heap_readout.py and audit-run.py produce benchmark, loading/preparation, confirmed post-export and descriptive whole-run windows. Only the amended benchmark window determines CENSUS_HEAP_NOT_CONFIRMED; it is post-session and never stops a valid launch.
8. Census reports lacked amendment 3's additive root partition and conditional frame count/mixed-parent disclosure; amendment 4's children, calls/frame, SELF_NESTED suppression and named nesting annotations; amendment 5's ASSUMED_UNIFORM events/cost/bias bounds and 20% ranking exclusion/follow-up; amendment 6's K=4 room/ambient reading, warmup-eviction lower bound and gameplay/static-share caveats; amendment 7's collision coverage/owner footnote; and amendment 8's >15% residual label, contents and follow-up. All now appear in JSON and Markdown for every B run. Map residual labels are also assembled.
9. prepare.py's partial r4 copy-and-replace scheme and test_tools.py's inherited byte-equality contract preserved the broken preservation helper. Canonical r5 console sources now live in the clone and regeneration verifies every helper and all eight unchanged protocol labels.
10. run_tests.py declared six host selections although the retained proof declared seven. The direct-resolver selection is restored. No engine tests or native checks were rerun for this helper-only correction; their existing proofs remain historical.
11. finalize.py and the A1-only resealer lacked the new required package identity and current full-session validation contract. The finalizer now includes PACKAGE-IDENTITY.json; reseal_a1.py delegates to the helper-only A1B resealer, which checks protected artifacts, remote HEAD, regression results, ZIP bytes and every manifest entry. Hash-bearing fix reports are excluded from the manifest to avoid cycles.

The full audit below covers every Python helper in console/ and scripts/. Historical r4 baseline identities, the brief-mandated four-map preservation/cleanup regex, r4-1 policy names, historical log fixture filenames, pinned human SELF/save fixture inputs and driver-v5 implementation paths are intentional retained contracts. The actual launch protocol remains only s6-phase-census, Pandemonium then Hedge3, A/B/B/A each. Defaults remain eight launches, four Pandemonium handling allowances, 60 warmup + 120 measured frames, 180-row B census, and 127.5 minutes including restoration. Config bytes and effective booleans are unchanged.

## Per-helper audit and diff summary

| Helper | Audit and resulting behavior |
|---|---|
{rows}

## Full-session regression

The same test_session_e2e.py runs against the pre-fix helpers archived in proof/A1B-SEALED-HELPERS.zip (every member verified against the original manifest, SHA-256 f65ac33a59dec730edf949707cac08d3a4d54ec4e22de77d82d34b3dbc68dce5) and against the fixed package. Sockets, DNS, bind, arbitrary subprocesses and system calls are blocked before helper import; vita_ftp is replaced before importing pilot. The transport holds remote bytes in memory, enforces one connection, and stages/restores through a narrow stager fake. A separate driver fake supplies inputs, raw logs, exports and image placeholders. No real FTP transport module or driver is loaded or invoked. Known local human SELF/save bytes retain their actual hashes; these and host census templates are test inputs, not current hardware observations. All fixture receipts are explicitly synthetic.

The test calls real preservation/baseline/save mains, admission and boundary checks, session.stage, one SELF stage, per-arm SELF verification and UI launch/entry/arming/resume/ack mains, session.collect with two readbacks, full log-integrity/entry/audit/strict-audio mains, per-arm rollback, both ABBA comparators, report assembly and session-wide restore-ledgers -> verify-self -> restore-files. It checks eight launch receipts, both maps and all eight exact labels, 16 small stages, benchmark and post-export heaps, all amendment 3-8 tables, conditional root closure, self nesting, bias exclusion, warmup evictions, residual labels and final byte-for-byte memory filesystem equality. Fake B heap deltas exceed +1 MiB and both maps still complete, proving the label does not stop measurements. The final receipt must be PASS_ORIGINAL_STATE_VERIFIED.

Before: exit 1, expected missing r4 package error:

```text
{old_output}
```

After: exit 0:

```text
{new_output}
```

Focused checks: {checks['count']} PASS. Log-integrity fixtures: 12 PASS. Individual helper audits: {audit['helper_count']} PASS. Regeneration, syntax, clone/package equality, unchanged brief/SELF/config/source checks and original blocked-session evidence checks: PASS. Synthetic report tables are retained in proof/A1B-SYNTHETIC-SESSION/RESULTS.md and RESULTS.json, separately from operational session evidence.

The engine emits the frame-total completion record after census export. Post-export memory samples therefore begin at that confirmed boundary; export-internal transients before the record cannot be isolated without another marker. The benchmark window follows the brief's exact BENCH-ARMED-to-measured=120 rule. No sampled maximum is described as a continuous peak.

## Resealed identities

- Unchanged CONSOLE-BRIEF.md SHA-256: `{BRIEF}`.
- Unchanged common SELF SHA-256: `50ac74a2ced3e4caf4ad239828df2dcbe1007f51be6b465471f00f5d3b918f69`.
- ARTIFACT-SHA256.json SHA-256: `{manifest_hash}` ({len(manifest)} entries verified).
- CONSOLE-PACKAGE.zip SHA-256: `{zip_hash}` ({zip_members} members, all equal local bytes).
- proof/FINAL-VALIDATION.json: PASS, records the unchanged brief hash, engine revision, tooling HEAD, offline regression and no device/driver contact.

No force push or co-author trailer. A1B-FIX.md, A1-FIX.md, SOL-REPORT.md and the manifest itself are excluded from the manifest to avoid circular hashes. The ZIP adds the sealed identity and heap-readout helper to the original operational member set.
'''
(O/'A1B-FIX.md').write_text(report,encoding='utf-8')
print('PASS clean branch and remote HEAD:',head)
print('PASS before FAIL / after PASS, all eight labels and original state restoration')
print('PASS protected source, SELF, configs, brief and blocked-session evidence unchanged')
print('ARTIFACT-SHA256.json SHA-256:',manifest_hash)
print('CONSOLE-PACKAGE.zip SHA-256:',zip_hash)
print('PASS manifest entries:',len(manifest),'ZIP members:',zip_members)
