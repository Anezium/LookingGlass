"""Final local proof/package verification and artifact inventory. No transport."""
from pathlib import Path
import json,hashlib,subprocess,sys,zipfile
O=Path(sys.argv[1]).resolve();R=Path(__file__).resolve().parents[2]
load=lambda p:json.loads((O/p).read_text());sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
engine=load('proof/SOURCE-IDENTITY.json')['engine_revision'];native=load('packages/baseline/VERIFICATION.json');assert native['source_revision']==engine
assert not subprocess.check_output(['git','diff',engine,'HEAD','--','src','vendor','Cargo.toml','Cargo.lock'],cwd=R),'Engine changed after native build'
a=load('configs/PL-baseline.json');b=load('configs/PL-stack.json');keys=sorted(k for k in a.keys()|b.keys() if a.get(k)!=b.get(k));assert keys==['audio_static_cache','collision_sah','skin_actual_consumers'];assert all(a[k] is False and b[k] is True for k in keys)
assert 'audio_cache_qualification' not in a and 'audio_cache_qualification' not in b
assert sha(O/'packages/baseline/eboot.bin')==sha(O/'packages/stack/eboot.bin')==native['self_sha256']
with zipfile.ZipFile(O/'packages/baseline/looking-glass.vpk') as z:assert set(z.namelist())=={'eboot.bin','sce_sys/param.sfo'} and hashlib.sha256(z.read('eboot.bin')).hexdigest()==native['self_sha256']
assert sha(O/'packages/baseline/looking-glass.vpk')==native['vpk_sha256']
assert sha(R/'Cargo.lock')==native['lock_sha256']
for p in ['proof/HOST-TESTS.json','proof/HOST-CENSUS.json','proof/TRANSITION-EQUALITY.json','proof/FAILED-TRANSITION-EQUALITY.json','proof/SAVE-PAYLOAD-EQUALITY.json','proof/STACK-ORACLE.json','film/proof/FULL-FILM-EQUALITY.json','proof/REPEATED-TRANSITIONS.json','proof/NESTED-TRANSITIONS.json','proof/LOADING-FRAME-EQUALITY.json','proof/STARTUP-CHOOSER.json','proof/R11C-VALIDATION.json','proof/DOUBLE-FAILURE-EQUALITY.json','proof/PORTAL-TRANSITIONS.json','proof/TEXTURE-REGRESSIONS.json','proof/DEPENDENCY-BYTE-EQUALITY.json']:
 assert load(p)['status']=='PASS',p
visual=load('proof/TRANSITION-VISUAL.json');assert visual['status']=='PASS_ORDERED_GPU_PACKETS' and all(r['raster']['exact'] for r in visual['pairs'] if 'raster' in r)
binary_sha=sha(O/'proof/recovery-bin')
assert load('proof/HOST-CENSUS.json')['binary_sha256']==binary_sha
assert load('proof/TRANSITION-EQUALITY.json')['binary_sha256']==binary_sha
assert load('proof/FAILED-TRANSITION-EQUALITY.json')['binary_sha256']==binary_sha
assert visual['binary_sha256']==binary_sha
assert load('proof/STACK-ORACLE.json')['binary_sha256']==binary_sha
assert all(r['binary_sha256']==binary_sha and r['exit_code']==0 for r in load('proof/HOST-STACK-RUNS.json'))
assert all(v['all_equal'] for v in load('proof/STACK-SIMULATION-EQUALITY.json').values())
assert len(load('proof/TRANSITION-EQUALITY.json')['pairs'])==16
assert len(load('proof/FAILED-TRANSITION-EQUALITY.json')['rows'])==58
assert len(load('proof/DOUBLE-FAILURE-EQUALITY.json')['rows'])==10
assert len(load('proof/PORTAL-TRANSITIONS.json')['rows'])==4
assert not (R/'vendor').exists()
assert {(r['action'],r['levers']) for r in load('proof/REPEATED-TRANSITIONS.json')['rows']}=={(a,b) for a in ['retry','native-load','chapter'] for b in [False,True]}
for row in load('proof/REPEATED-TRANSITIONS.json')['rows']:
 assert row['cycles'] >= (30 if row['action']=='chapter' else 10) and row['pass_trend']
for p in ['proof/REPEATED-TRANSITIONS.json','proof/NESTED-TRANSITIONS.json','proof/STARTUP-CHOOSER.json','film/proof/FULL-FILM-EQUALITY.json']:
 assert load(p)['binary_sha256']==binary_sha,p
assert all(r['extra_above_max_endpoints_bytes']<4*1048576 for r in load('proof/HOST-CENSUS.json')['rows'])
for pair in load('film/proof/FULL-FILM-EQUALITY.json')['pairs']:
 row=pair['new']['records'][0]
 assert row['peak_heap_used']-max(row['before']['heap_used'],row['after']['heap_used'])<4*1048576
protocol=load('CONSOLE-PROTOCOL.json');assert protocol['pass_rule']['peak_arena_exclusive_bytes']==179*1048576
assert protocol['pass_rule']['peak_uordblks_exclusive_bytes']==170*1048576
assert not (O/'configs/cache_qual_command.json').exists()
for path in O.glob('stage-*-full.json'):
 assert not any('cache_qual_command' in row['remote'] for row in json.loads(path.read_text())['files'])
assert load('proof/COMMAND-GRAMMAR.json')['native_commands']==['hashtree','help','launch','nosleep','press','quit','reboot','release','screen','version','wait']
assert (O/'CONSOLE-BRIEF.md').read_bytes()==(R/'tools/transition_mem_r11/CONSOLE-BRIEF.md').read_bytes()
for p in ['proof/SESSION-E2E.json','proof/SESSION-E2E-ZIP.json']:
 v=load(p);assert v['status']=='PASS' and v['final_status']=='PASS_ORIGINAL_STATE_VERIFIED' and v['synthetic_offline_only'] and v['serialized_fake_ftp_maximum']==1
 assert v['tested_package_identity_sha256']==sha(O/'PACKAGE-IDENTITY.json')
assert load('proof/SESSION-E2E-ZIP.json')['tested_zip_sha256']==sha(O/'CONSOLE-PACKAGE.zip')
assert all(not f.name.startswith('session-') for f in O.iterdir())
with zipfile.ZipFile(O/'CONSOLE-PACKAGE.zip') as z:assert not any(n.startswith('session-') for n in z.namelist())
assert not (O/'target-vita').exists() and not (O/'tmp').exists()
assert (O/'SOL-REPORT.md').is_file()
result=dict(status='PASS',engine_revision=engine,branch=subprocess.check_output(['git','branch','--show-current'],cwd=R,text=True).strip(),branch_head=subprocess.check_output(['git','rev-parse','HEAD'],cwd=R,text=True).strip(),console_executed=False,vita_contact=False,shot_driver_contact=False,hardware_170_MiB_gate='NOT_EXECUTED',self_sha256=native['self_sha256'],vpk_sha256=native['vpk_sha256'],console_zip_sha256=sha(O/'CONSOLE-PACKAGE.zip'),session_directories_absent=True,task_caches_removed=True)
(O/'proof/FINAL-VERIFICATION.json').write_text(json.dumps(result,indent=2)+'\n')
# The detached seal covers this inventory plus the final r11b report, avoiding
# a report/inventory self-reference. Every other artifact is in this inventory.
files={str(p.relative_to(O)):dict(bytes=p.stat().st_size,sha256=sha(p)) for p in sorted(O.rglob('*')) if p.is_file() and str(p.relative_to(O)) not in ['ARTIFACT-SHA256.json','SOL-REPORT-R11C.md','SEAL-R11C.json']}
(O/'ARTIFACT-SHA256.json').write_text(json.dumps(files,indent=2)+'\n')
print('PASS',len(files),'artifacts; native hardware gate unrun')
