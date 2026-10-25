"""Seal r9 helpers/artifacts locally. Never imports a transport or console driver."""
from pathlib import Path
import argparse,hashlib,json,os,shutil,subprocess,zipfile
p=argparse.ArgumentParser(description=__doc__);p.add_argument('output',type=Path);p.add_argument('--cleanup',action='store_true');p.add_argument('--manifest-only',action='store_true');a=p.parse_args();O=a.output.resolve();R=Path(__file__).resolve().parents[2];T=R/'tools/skin_exact_r9'
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def write(p,v):p.write_text(json.dumps(v,indent=2)+'\n')
protocol=json.loads((O/'CONSOLE-PROTOCOL.json').read_text());assert all(not os.path.lexists(O/n) for n in protocol['session_directories'])
if a.manifest_only:
 verification=json.loads((O/'proof/PACKAGE-VERIFICATION.json').read_text())
 assert sha(O/'CONSOLE-PACKAGE.zip')==verification['zip_sha256']
 with zipfile.ZipFile(O/'CONSOLE-PACKAGE.zip') as z:
  assert z.testzip() is None and set(z.namelist())==set(verification['members'])
  for name,record in verification['members'].items():
   raw=z.read(name);assert len(raw)==record['bytes'] and hashlib.sha256(raw).hexdigest()==record['sha256'] and raw==(O/name).read_bytes()
 write(O/'ARTIFACT-SHA256.json',{str(f.relative_to(O)):dict(bytes=f.stat().st_size,sha256=sha(f)) for f in sorted(O.rglob('*')) if f.is_file() and f.name not in ['ARTIFACT-SHA256.json','SOL-REPORT.md']})
 print('Verified unchanged ZIP and refreshed complete artifact manifest')
 raise SystemExit(0)

for f in T.glob('*.py'):shutil.copy2(f,O/'scripts'/f.name)
head=subprocess.check_output(['git','rev-parse','HEAD'],cwd=R,text=True).strip();build=json.loads((O/'proof/BUILD-COMMAND.json').read_text())
write(O/'PACKAGE-IDENTITY.json',dict(package='alice-skin-exact-r9',engine_source_revision=build['revision'],branch='vita-skin-exact-r9-20261024',branch_head=head))
write(O/'proof/SOURCE-IDENTITY.json',dict(engine_revision=build['revision'],tooling_head=head,files={str(f.relative_to(R)):sha(f) for f in sorted((R/'src').rglob('*.rs'))}))
removed=[]
if a.cleanup:
 for path in [O/'target-vita',O/'tmp',O/'proof/host-bin',O/'proof/r7-host-bin',O/'proof/png-host-bin',O/'packages/baseline/regenerated.velf',O/'packages/baseline/regenerated.self',*list((O/'host').glob('*/private/cache'))]:
  if not path.exists():continue
  size=sum(f.stat().st_size for f in path.rglob('*') if f.is_file()) if path.is_dir() else path.stat().st_size
  removed.append(dict(path=str(path),bytes=size));shutil.rmtree(path) if path.is_dir() else path.unlink()
 for root in [T,O]:
  for path in list(root.rglob('__pycache__')):
   size=sum(f.stat().st_size for f in path.rglob('*') if f.is_file());removed.append(dict(path=str(path),bytes=size));shutil.rmtree(path)
 previous=json.loads((O/'proof/CLEANUP.json').read_text()).get('removed',[]) if (O/'proof/CLEANUP.json').exists() else []
 removed=previous+removed
 write(O/'proof/CLEANUP.json',dict(removed=removed,removed_bytes=sum(v['bytes'] for v in removed),shared_host_target_retained='/Users/anezium/vita-j0/cargo-target-host',df=subprocess.check_output(['df','-h',str(O)],text=True)))
files=[]
for n in ['CONSOLE-BRIEF.md','CONSOLE-PROTOCOL.json','PACKAGE-MATRIX.json','PACKAGE-IDENTITY.json','DESIGN.md','README.md','VISUAL-QUALIFICATION.md','AMENDMENTS.md','AMENDMENTS-AUDIO.md','AMENDMENTS-INHERITED.md']:files.append(O/n)
files.extend(O/'scripts'/n for n in ['bind_helpers.py','test_session_e2e.py','test_protocol.py','test_late_callbacks.py','log-integrity.py','test-log-integrity.py'])
files.append(O/'proof/CHAPTER-REACHABILITY.json')
files.extend(sorted(O.glob('stage-*.json')))
for name in ['configs','console']:
 files.extend(sorted(f for f in (O/name).iterdir() if f.is_file()))
for arm in ['baseline','baseline-census','skin','skin-census']:
 files.extend(O/'packages'/arm/n for n in ['eboot.bin','VERIFICATION.json'])
with zipfile.ZipFile(O/'CONSOLE-PACKAGE.zip','w',zipfile.ZIP_DEFLATED,compresslevel=6) as z:
 for f in files:z.write(f,str(f.relative_to(O)))
with zipfile.ZipFile(O/'CONSOLE-PACKAGE.zip') as z:
 assert z.testzip() is None
 assert not any(any(n in f for n in protocol['session_directories']) for f in z.namelist())
 members={f:dict(bytes=len(z.read(f)),sha256=hashlib.sha256(z.read(f)).hexdigest()) for f in z.namelist()}
 assert all(members[str(f.relative_to(O))]['sha256']==sha(f) for f in files)
write(O/'proof/PACKAGE-VERIFICATION.json',dict(status='PASS',zip_sha256=sha(O/'CONSOLE-PACKAGE.zip'),members=members,session_paths_absent=True,console_executed=False))
# The report is edited after disk cleanup; exclude it to avoid a circular seal.
write(O/'ARTIFACT-SHA256.json',{str(f.relative_to(O)):dict(bytes=f.stat().st_size,sha256=sha(f)) for f in sorted(O.rglob('*')) if f.is_file() and f.name not in ['ARTIFACT-SHA256.json','SOL-REPORT.md']})
print('Sealed',len(members),'console members; session directories absent; no device access')
