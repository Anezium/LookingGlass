"""Local-only package handoff, cache cleanup and evidence manifest."""
from pathlib import Path
import hashlib,json,os,re,shutil,subprocess,sys,zipfile
R=Path('/Users/anezium/vita-work/lg-phase-census-r5');T=R/'tools/phase_census_r5';O=Path(sys.argv[1]).resolve()
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
# Keep reproducibility code outside the repository alongside the sealed artifacts.
for f in T.glob('*.py'):shutil.copy2(f,O/'scripts'/f.name)
for n in ['README.md','DESIGN.md','CONSOLE-BRIEF.md']:shutil.copy2(T/n,O/n)
source={'revision':json.loads((O/'proof/BUILD-COMMAND.json').read_text())['revision'],'tooling_head_at_seal':subprocess.check_output(['git','rev-parse','HEAD'],cwd=R,text=True).strip(),'r4_base':'81f3d1eb1fe4303ff955afee491ef10a7ceecb2a','files':{str(f.relative_to(R)):sha(f) for f in sorted((R/'src').rglob('*.rs'))},'host_binary_sha256':json.loads((O/'proof/HOST-RENDER.json').read_text())['binary_sha256'],'host_build_log':'logs/host-sealed-build.log'}
(O/'proof/SOURCE-IDENTITY.json').write_text(json.dumps(source,indent=2)+'\n')
s=(O/'proof/native-census-tls.s').read_text();assert re.search(r'movw\s+r0, #2600',s) and re.search(r'movt\s+r0, #5',s) and '<__wrap_malloc>' in s
(O/'proof/NATIVE-CENSUS-STORAGE.json').write_text(json.dumps({'tls_allocation_request_bytes':(5<<16)+2600,'scope':'B-only main-thread TLS initialized before gameplay; allocation request from sealed ARM ELF, not live newlib usage','evidence':'proof/native-census-tls.s','elf_sha256':sha(O/'packages/baseline/looking-glass.elf'),'host_State_bytes':330416},indent=2)+'\n')
before=subprocess.check_output(['df','-h',str(O)],text=True)
prior=json.loads((O/'proof/CLEANUP.json').read_text()) if (O/'proof/CLEANUP.json').exists() else {}
removed=list(prior.get('removed',[]))
before=prior.get('df_before',before)
for p in [O/'target-vita',O/'tmp',O/'packages/baseline/regenerated.velf',O/'packages/baseline/regenerated.self',O/'proof/host-bin']:
 if not p.exists():continue
 size=sum(f.stat().st_size for f in p.rglob('*') if f.is_file()) if p.is_dir() else p.stat().st_size
 removed.append({'path':str(p),'bytes':size})
 if p.is_dir():shutil.rmtree(p)
 else:p.unlink()
for root in [R/'tools/phase_census_r5',O]:
 for p in list(root.rglob('__pycache__')):
  size=sum(f.stat().st_size for f in p.rglob('*') if f.is_file());removed.append({'path':str(p),'bytes':size});shutil.rmtree(p)
after=subprocess.check_output(['df','-h',str(O)],text=True)
(O/'proof/CLEANUP.json').write_text(json.dumps({'removed':removed,'removed_bytes':sum(r['bytes'] for r in removed),'shared_host_target_retained':'/Users/anezium/vita-j0/cargo-target-host','df_before':before,'df_after':after},indent=2)+'\n')
# A compact console handoff includes SELF/configs/helpers only. Native build files stay local.
with zipfile.ZipFile(O/'CONSOLE-PACKAGE.zip','w',compression=zipfile.ZIP_DEFLATED,compresslevel=6) as z:
 for n in ['CONSOLE-BRIEF.md','DESIGN.md','CONSOLE-PROTOCOL.json','PACKAGE-MATRIX.json']:
  z.write(O/n,n)
 for d in ['configs','console']:
  for f in sorted((O/d).glob('*')):
   if f.is_file():z.write(f,str(f.relative_to(O)))
 for f in sorted(O.glob('stage-*.json')):z.write(f,f.name)
 for arm in ['baseline','census']:
  for n in ['eboot.bin','VERIFICATION.json']:
   f=O/'packages'/arm/n;z.write(f,str(f.relative_to(O)))
 for n in ['log-integrity.py','bind_helpers.py']:
  f=O/'scripts'/n;z.write(f,str(f.relative_to(O)))
manifest={str(f.relative_to(O)):{'bytes':f.stat().st_size,'sha256':sha(f)} for f in sorted(O.rglob('*')) if f.is_file() and f.name not in ['ARTIFACT-SHA256.json','SOL-REPORT.md']}
(O/'ARTIFACT-SHA256.json').write_text(json.dumps(manifest,indent=2)+'\n')
print('Removed',sum(r['bytes'] for r in removed),'bytes; sealed',len(manifest),'artifacts')
print(after)
