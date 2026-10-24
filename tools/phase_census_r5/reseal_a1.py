"""Reseal the Mac helper amendment without rebuilding or contacting the console."""
from pathlib import Path
import hashlib,json,subprocess,sys,zipfile
R=Path('/Users/anezium/vita-work/lg-phase-census-r5')
O=Path(sys.argv[1]).resolve()
BRIEF='48857506bc84cb02f322c1fa16603bcd94727cd5435dc96cab39a4fa1b2173c0'
def sha(path):return hashlib.sha256(path.read_bytes()).hexdigest()
def git(*args):return subprocess.check_output(['git',*args],cwd=R,text=True).strip()
assert sha(O/'CONSOLE-BRIEF.md')==BRIEF
assert not git('status','--porcelain'), 'Commit amendment tools before resealing'
head=git('rev-parse','HEAD');branch=git('branch','--show-current')
assert branch=='vita-phase-census-r5-20261024'
assert git('rev-parse','refs/remotes/origin/'+branch)==head, 'Plain-push before resealing'
remote_head=git('ls-remote','--heads','origin',branch)
assert remote_head.split()[0]==head, 'Remote branch must equal amendment commit'
old=json.loads((O/'ARTIFACT-SHA256.json').read_text())
for name,record in old.items():
 if name.startswith(('configs/','packages/')):assert sha(O/name)==record['sha256'],name
for arm in ['baseline','census']:assert sha(O/'packages'/arm/'eboot.bin')=='50ac74a2ced3e4caf4ad239828df2dcbe1007f51be6b465471f00f5d3b918f69'
source=json.loads((O/'proof/SOURCE-IDENTITY.json').read_text())
for name,digest in source['files'].items():assert sha(R/name)==digest,name
for name in ['prepare.py','test_tools.py','test_a1.py','reseal_a1.py']:assert (O/'scripts'/name).read_bytes()==(R/'tools/phase_census_r5'/name).read_bytes()
checks=json.loads((O/'proof/CONSOLE-DRY-RUN.json').read_text());logs=json.loads((O/'proof/LOG-INTEGRITY-FIXTURES.json').read_text())
assert checks['status']==logs['status']=='PASS' and checks['count']>=102 and len(logs['checks'])==12
assert 'All amendment 1 offline checks passed' in (O/'logs/a1-offline-checks.log').read_text()
host_tests=json.loads((O/'proof/HOST-TESTS.json').read_text())
assert len(host_tests)==7 and all(row['exit_code']==0 for row in host_tests)
assert 'PASS native conversion byte equality and VPK contents' in (O/'logs/a1-offline-checks.log').read_text()
assert not list(O.rglob('__pycache__'))
# Keep the existing compact console package membership and use current local bytes.
with zipfile.ZipFile(O/'CONSOLE-PACKAGE.zip','w',compression=zipfile.ZIP_DEFLATED,compresslevel=6) as archive:
 for name in ['CONSOLE-BRIEF.md','DESIGN.md','CONSOLE-PROTOCOL.json','PACKAGE-MATRIX.json']:archive.write(O/name,name)
 for directory in ['configs','console']:
  for path in sorted((O/directory).glob('*')):
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
 assert hashlib.sha256(archive.read('CONSOLE-BRIEF.md')).hexdigest()==BRIEF
zip_hash=sha(O/'CONSOLE-PACKAGE.zip')
remote=dict(status='PASS',head=head,remote=remote_head,plain_push=True,branch=branch,working_tree_clean=True,engine_source_revision=source['revision'],verification='Remote branch readback after successful plain push')
(O/'proof/REMOTE.json').write_text(json.dumps(remote,indent=2)+'\n')
validation=json.loads((O/'proof/FINAL-VALIDATION.json').read_text())
validation.update(status='PASS',head=head,working_tree_clean=True,remote_verified=True,artifact_hashes_verified=True,console_helper_checks=checks['count'],log_integrity_fixture_cases=len(logs['checks']),package_zip_sha256=zip_hash,console_brief_sha256=BRIEF,amendment=1,device_contact=False,driver_contact=False,engine_self_configs_unchanged=True,amendment_offline_checks='logs/a1-offline-checks.log',historical_engine_proofs_retained=True,offline_host_test_selections=len(host_tests),native_local_checks_rerun=True)
(O/'proof/FINAL-VALIDATION.json').write_text(json.dumps(validation,indent=2)+'\n')
# Reports carrying the manifest hash are excluded to avoid self-reference.
excluded={'ARTIFACT-SHA256.json','SOL-REPORT.md','A1-FIX.md'}
manifest={str(path.relative_to(O)):dict(bytes=path.stat().st_size,sha256=sha(path)) for path in sorted(O.rglob('*')) if path.is_file() and path.name not in excluded}
(O/'ARTIFACT-SHA256.json').write_text(json.dumps(manifest,indent=2)+'\n')
for name,record in manifest.items():assert (O/name).stat().st_size==record['bytes'] and sha(O/name)==record['sha256'],name
manifest_hash=sha(O/'ARTIFACT-SHA256.json')
output=(O/'logs/a1-offline-checks.log').read_text().rstrip()
report=f"""# Amendment 1 helper fix

Implemented census export preservation, restoration and stale-export rejection. The baseline records both fullbench census paths in BASELINE.json and PROFILE-BASELINE.json, with original bytes and SHA-256 or explicit absence. Its presence regex accepts census exports. Restoration permits the exact amended export regex and retains both independent readbacks before deletion. B collection rejects a census SHA-256 matching its preserved initial file. The inherited-helper checks allow only the exact amendment in baseline-r3.py and restore-files.py.

Offline fixtures execute the real baseline and restoration helpers with both census exports present: recorded initial absences, preserved originals and inventory cleanup all require PASS_ORIGINAL_STATE_VERIFIED. Unrelated new files and disagreeing cleanup readbacks fail without deletion or a final PASS. The initial-census and earlier-run census checks both assert their specific rejection reason. Clone prepare.py reproduces the amended helpers; clone tests and resealing tools match the package scripts.

Test output (all local; transport blocked in helper and regeneration fixtures):

```text
{output}
```

No engine, SELF or config changes. The common SELF remains `50ac74a2ced3e4caf4ad239828df2dcbe1007f51be6b465471f00f5d3b918f69`. All seven existing host test selections were rerun offline. ELF margin, process parameters, native conversion equality and asset-free VPK contents were rechecked locally. Original host render ABBA evidence is retained; the engine and SELF were not rebuilt. No Vita or shot-driver contact occurred. CONSOLE-BRIEF.md was not edited; its resealed SHA-256 is `{BRIEF}`.

Plain-pushed `{branch}` at `{head}`. No force push or co-author trailer.

- ARTIFACT-SHA256.json SHA-256: `{manifest_hash}`
- CONSOLE-PACKAGE.zip SHA-256: `{zip_hash}`
- Manifest: {len(manifest)} files verified; every ZIP member equals its local source.
- FINAL-VALIDATION.json: PASS, with the amended brief hash recorded.

A1-FIX.md, SOL-REPORT.md and the manifest itself are excluded from the artifact manifest to avoid circular report hashes. The ZIP retains the original console-package member set.
"""
(O/'A1-FIX.md').write_text(report)
print('PASS all artifact manifest hashes and ZIP members')
print('ARTIFACT-SHA256.json SHA-256:',manifest_hash)
print('CONSOLE-PACKAGE.zip SHA-256:',zip_hash)
