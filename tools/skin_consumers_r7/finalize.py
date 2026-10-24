"""Seal the local handoff and remove only task-specific build/Python caches."""
from pathlib import Path
import argparse,hashlib,json,os,shutil,subprocess,sys,zipfile
R=Path(__file__).resolve().parents[2];T=R/'tools/skin_consumers_r7';parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('package',type=Path);parser.add_argument('--manifest-only',action='store_true');args=parser.parse_args();O=args.package.resolve()
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def write(p,v):p.write_text(json.dumps(v,indent=2)+'\n')
assert all(not os.path.lexists(O/n) for n in ['session-s8-skin-consumers-pandemonium','session-s8-skin-consumers-hedge3'])
assert sha(O/'CONSOLE-BRIEF.md')=='7ccda75258e62b78945eb7e6aecf417180d9ce09c997862b11c91079249ab7ac', 'Protected brief changed'
amendment_hashes={n:sha(O/n) for n in ['AMENDMENTS.md', 'AMENDMENTS-2.md']}
if args.manifest_only:
 verification=json.loads((O/'proof/PACKAGE-VERIFICATION.json').read_text())
 assert verification['amendments']==amendment_hashes
 with zipfile.ZipFile(O/'CONSOLE-PACKAGE.zip') as z:
  assert z.testzip() is None
  assert set(z.namelist())==set(verification['members'])
  for name,record in verification['members'].items():
   data=z.read(name);assert len(data)==record['bytes'] and hashlib.sha256(data).hexdigest()==record['sha256'] and data==(O/name).read_bytes(),name
 manifest={str(f.relative_to(O)):dict(bytes=f.stat().st_size,sha256=sha(f)) for f in sorted(O.rglob('*')) if f.is_file() and f.name not in ['ARTIFACT-SHA256.json','SOL-REPORT.md','LATECB-FIX.md','RESEAL-AMENDMENTS.md']}
 write(O/'ARTIFACT-SHA256.json',manifest)
 print('Verified ZIP and refreshed manifest:',len(manifest),'files')
 sys.exit(0)
for f in T.glob('*.py'):shutil.copy2(f,O/'scripts'/f.name)
for n in ['README.md','DESIGN.md']:shutil.copy2(T/n,O/n)
receipt=json.loads((O/'proof/BUILD-COMMAND.json').read_text());head=subprocess.check_output(['git','rev-parse','HEAD'],cwd=R,text=True).strip()
write(O/'PACKAGE-IDENTITY.json',dict(package='alice-skin-consumers-r7',engine_source_revision=receipt['revision'],branch='vita-skin-consumers-r7-20261024',branch_head=head))
write(O/'proof/SOURCE-IDENTITY.json',dict(engine_revision=receipt['revision'],tooling_head=head,base_engine='62704971576f7e3d30499962388b69bf2ed692d7',files={str(f.relative_to(R)):sha(f) for f in sorted((R/'src').rglob('*.rs'))},host_binary_sha256=json.loads((O/'proof/HOST-MAPS.json').read_text())['binary_sha256']))
before=subprocess.check_output(['df','-h',str(O)],text=True);removed=[]
for p in [O/'target-vita',O/'tmp',O/'packages/baseline/regenerated.velf',O/'packages/baseline/regenerated.self',O/'proof/host-bin']:
 if not p.exists():continue
 size=sum(f.stat().st_size for f in p.rglob('*') if f.is_file()) if p.is_dir() else p.stat().st_size
 removed.append(dict(path=str(p),bytes=size));shutil.rmtree(p) if p.is_dir() else p.unlink()
for cache in list((O/'host').glob('*/private/cache')):
 if cache.is_dir():
  size=sum(f.stat().st_size for f in cache.rglob('*') if f.is_file());removed.append(dict(path=str(cache),bytes=size));shutil.rmtree(cache)
for root in [T,O]:
 for p in list(root.rglob('__pycache__')):
  if 'blocked-attempts' in p.relative_to(root).parts:continue
  size=sum(f.stat().st_size for f in p.rglob('*') if f.is_file());removed.append(dict(path=str(p),bytes=size));shutil.rmtree(p)
previous=json.loads((O/'proof/CLEANUP.json').read_text()) if (O/'proof/CLEANUP.json').exists() else {}
removed=previous.get('removed',[])+removed
write(O/'proof/CLEANUP.json',dict(removed=removed,removed_bytes=sum(v['bytes'] for v in removed),shared_host_target_retained='/Users/anezium/vita-j0/cargo-target-host',df_before=previous.get('df_before',before),df_after=subprocess.check_output(['df','-h',str(O)],text=True)))
with zipfile.ZipFile(O/'CONSOLE-PACKAGE.zip','w',compression=zipfile.ZIP_DEFLATED,compresslevel=6) as z:
 for n in ['CONSOLE-BRIEF.md','DESIGN.md','CONSOLE-PROTOCOL.json','PACKAGE-MATRIX.json','PACKAGE-IDENTITY.json']+list(amendment_hashes):z.write(O/n,n)
 for d in ['configs','console']:
  for f in sorted((O/d).glob('*')):
   if f.is_file():z.write(f,str(f.relative_to(O)))
 for f in sorted(O.glob('stage-*.json')):z.write(f,f.name)
 for arm in ['baseline','baseline-census','skin','skin-census']:
  for n in ['eboot.bin','VERIFICATION.json']:
   f=O/'packages'/arm/n;z.write(f,str(f.relative_to(O)))
 for n in ['log-integrity.py','bind_helpers.py','test_session_e2e.py','test_protocol.py','test_late_callbacks.py']:
  f=O/'scripts'/n;z.write(f,str(f.relative_to(O)))
with zipfile.ZipFile(O/'CONSOLE-PACKAGE.zip') as z:
 assert not any(part.startswith('session-') for name in z.namelist() for part in Path(name).parts)
 members={i.filename:dict(bytes=i.file_size,sha256=hashlib.sha256(z.read(i)).hexdigest()) for i in z.infolist()}
write(O/'proof/PACKAGE-VERIFICATION.json',dict(status='PASS',amendments=amendment_hashes,report_exclusions=['LATECB-FIX.md','RESEAL-AMENDMENTS.md'],blocked_attempts_excluded_from_zip=True,session_directory_absent=True,zip_has_no_session_directory=True,common_self=len({sha(O/'packages'/a/'eboot.bin') for a in ['baseline','baseline-census','skin','skin-census']})==1,members=members))
manifest={str(f.relative_to(O)):dict(bytes=f.stat().st_size,sha256=sha(f)) for f in sorted(O.rglob('*')) if f.is_file() and f.name not in ['ARTIFACT-SHA256.json','SOL-REPORT.md','LATECB-FIX.md','RESEAL-AMENDMENTS.md']}
write(O/'ARTIFACT-SHA256.json',manifest)
print('Sealed',len(manifest),'files; removed',sum(v['bytes'] for v in removed),'bytes')
print(subprocess.check_output(['df','-h',str(O)],text=True))
