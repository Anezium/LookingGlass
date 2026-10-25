"""Final offline receipt, native identity and archived-helper validation."""
from pathlib import Path
import argparse,hashlib,json,os,subprocess,zipfile
p=argparse.ArgumentParser();p.add_argument('output',type=Path);a=p.parse_args();O=a.output.resolve();R=Path(__file__).resolve().parents[2]
def read(n):return json.loads((O/n).read_text())
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
protocol=read('CONSOLE-PROTOCOL.json');assert all(not os.path.lexists(O/n) for n in protocol['session_directories'])
assert read('proof/HOST-QUALIFICATION.json')['status']=='PASS'
checks=[]
for mapname in ['pandemonium','hedge3']:
 for wall in [False,True]:
  for sealed in [False,True]:
   name='SESSION-E2E-'+mapname+('-wall-only' if wall else '')+('-sealed' if sealed else '')+'.json'
   x=read('proof/'+name)
   assert x['status']=='PASS' and x['final_status']=='PASS_ORIGINAL_STATE_VERIFIED'
   assert x['launches']==(4 if wall else 6) and x['self_stages']==1 and x['serialized_fake_ftp_maximum']==1
   assert x['real_package_session_path_absent'] and not x['device_contact'] and not x['driver_contact']
   checks.append(dict(receipt=name,status='PASS'))
assert read('proof/LATE-CALLBACK-TESTS.json')['status']=='PASS'
assert '12 offline log-integrity fixtures passed.' in (O/'logs/test-log-integrity.log').read_text()
ids=[read(f'packages/{arm}/VERIFICATION.json') for arm in ['baseline','skin','baseline-census','skin-census']]
assert len({x['self_sha256'] for x in ids})==1
for arm,x in zip(['baseline','skin','baseline-census','skin-census'],ids):assert sha(O/f'packages/{arm}/eboot.bin')==x['self_sha256']
assert sha(O/'packages/baseline/looking-glass.vpk')==ids[0]['vpk_sha256']
with zipfile.ZipFile(O/'packages/baseline/looking-glass.vpk') as z:
 assert set(z.namelist())=={'eboot.bin','sce_sys/param.sfo'} and hashlib.sha256(z.read('eboot.bin')).hexdigest()==ids[0]['self_sha256']
symbols=(O/'logs/symbols.log').read_text()
assert not any(s in symbols for s in ['render_tail::hooks','skin_exact_check','sim_maps::probe::gpu_geometry'])
verification=read('proof/PACKAGE-VERIFICATION.json');assert sha(O/'CONSOLE-PACKAGE.zip')==verification['zip_sha256']
with zipfile.ZipFile(O/'CONSOLE-PACKAGE.zip') as z:
 assert z.testzip() is None
 for name,record in verification['members'].items():
  raw=z.read(name);assert raw==(O/name).read_bytes() and len(raw)==record['bytes'] and hashlib.sha256(raw).hexdigest()==record['sha256']
 assert not any(any(n in f for n in protocol['session_directories']) for f in z.namelist())
engine=read('proof/BUILD-COMMAND.json')['revision']
diff=subprocess.check_output(['git','diff','--name-only',engine,'HEAD','--','src'],cwd=R,text=True).splitlines()
assert diff==['src/npc/skin_exact_check.rs'],diff
(O/'proof/POST-NATIVE-HOST-ONLY.patch').write_bytes(subprocess.check_output(['git','diff',engine,'HEAD','--','src'],cwd=R))
assert '#[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]\npub(crate) mod skin_exact_check;' in (R/'src/npc.rs').read_text()
assert not (O/'target-vita').exists() and not (O/'tmp').exists()
caches=[str(p) for root in [R/'tools/skin_exact_r9',O] for p in root.rglob('*') if p.name=='__pycache__' or (p.is_dir() and p.name=='cache' and p.parent.name=='private')]
assert not caches,caches
asset_suffixes={'.pk3','.bsp','.tik','.ska','.skel','.tga','.jpg','.png','.wav','.mp3','.bin','.vpk','.self','.elf','.velf'}
changed=subprocess.check_output(['git','diff','--name-only','777460d','HEAD'],cwd=R,text=True).splitlines()
assert not any(Path(n).suffix.lower() in asset_suffixes for n in changed)
messages=subprocess.check_output(['git','log','--format=%B','777460d..HEAD'],cwd=R,text=True)
assert 'co-authored-by:' not in messages.lower()
result=dict(status='PASS',offline_main_path_variants=checks,common_self_sha256=ids[0]['self_sha256'],vpk_sha256=ids[0]['vpk_sha256'],package_zip_sha256=verification['zip_sha256'],session_paths_absent=True,post_native_change_is_host_only_png_persistence=True,per_task_caches_remaining=caches,shared_host_target_retained=True,console_executed=False,device_contact=False,driver_contact=False,source_branch=subprocess.check_output(['git','branch','--show-current'],cwd=R,text=True).strip(),source_head=subprocess.check_output(['git','rev-parse','HEAD'],cwd=R,text=True).strip(),df=subprocess.check_output(['df','-h',str(O)],text=True))
(O/'proof/FINAL-VALIDATION.json').write_text(json.dumps(result,indent=2)+'\n');print('PASS final package, eight offline main-path variants, native identity, source scope and cleanup')
