"""Seal native r9 packages and reproduce conversion from the linked ELF."""
from pathlib import Path
import hashlib,json,os,shutil,subprocess,sys,zipfile
R=Path('/Users/anezium/vita-work/lg-skin-exact-r9');O=Path(sys.argv[1]).resolve();B=O/'packages/baseline';SDK=Path('/Users/anezium/vitasdk/bin')
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def git(*args):return subprocess.check_output(['git',*args],cwd=R,text=True).strip()
def run(tag,cmd):
 p=subprocess.run(cmd,capture_output=True,text=True);(O/'logs'/tag).write_text(p.stdout+p.stderr);assert p.returncode==0,(cmd,p.stderr);return p.stdout
receipt=json.loads((O/'proof/BUILD-COMMAND.json').read_text());assert receipt['exit_code']==0
run('elf-margin.log',[sys.executable,str(O/'scripts/verify-elf-margin.py'),str(B/'looking-glass.elf')])
run('process-params.log',[sys.executable,str(O/'scripts/verify-process-params.py'),str(B/'looking-glass.elf')])
run('symbols.log',[str(SDK/'arm-vita-eabi-nm'),'-C',str(B/'looking-glass.elf')])
assert 'looking_glass::render_tail::hooks' not in (O/'logs/symbols.log').read_text()
assert not git('diff','6270497',receipt['revision'],'--','Cargo.toml','Cargo.lock','build.rs','.cargo','src/logging.rs','src/render_direct.rs')
run('regenerate-velf.log',[str(SDK/'vita-elf-create'),str(B/'looking-glass.elf'),str(B/'regenerated.velf')])
run('regenerate-self.log',[str(SDK/'vita-make-fself'),'-s',str(B/'regenerated.velf'),str(B/'regenerated.self')])
assert sha(B/'regenerated.velf')==sha(B/'looking-glass.velf');assert sha(B/'regenerated.self')==sha(B/'eboot.bin')
with zipfile.ZipFile(B/'looking-glass.vpk') as z:
 members=[{'name':i.filename,'bytes':i.file_size} for i in z.infolist()];assert {i['name'] for i in members}=={'eboot.bin','sce_sys/param.sfo'}
 assert hashlib.sha256(z.read('eboot.bin')).hexdigest()==sha(B/'eboot.bin')
(O/'proof/VPK-CONTENTS.json').write_text(json.dumps(members,indent=2)+'\n')
rows=[];baseline=(O/'configs/PL-baseline.json').read_bytes()
assert baseline==Path('/Users/anezium/vita-j0/resume-2026-10-24/alice-phase-census-r5/configs/PL-baseline.json').read_bytes()
a=json.loads(baseline)
for arm in ['baseline','baseline-census','skin','skin-census']:
 d=O/'packages'/arm;d.mkdir(exist_ok=True)
 if arm!='baseline':shutil.copy2(B/'eboot.bin',d/'eboot.bin')
 c=json.loads((O/'configs'/f'PL-{arm}.json').read_text());diff={k:[a.get(k,False),c.get(k,False)] for k in a.keys()|c.keys() if a.get(k,False)!=c.get(k,False)}
 assert diff=={k:[False,True] for k in (['skin_actual_consumers'] if arm=='skin' else ['phase_census'] if arm=='baseline-census' else ['skin_actual_consumers','phase_census'] if arm=='skin-census' else [])}
 v={'arm':arm,'revision':receipt['revision'],'source_revision':receipt['revision'],'branch':git('branch','--show-current'),'r5_base':git('rev-parse','6270497'),'build_command':receipt['argv'],'build_environment':receipt['environment'],'self_sha256':sha(d/'eboot.bin'),'self_bytes':(d/'eboot.bin').stat().st_size,'elf_sha256':sha(B/'looking-glass.elf'),'velf_sha256':sha(B/'looking-glass.velf'),'vpk_sha256':sha(B/'looking-glass.vpk'),'lock_sha256':sha(R/'Cargo.lock'),'config_path':f'configs/PL-{arm}.json','config_sha256':sha(O/'configs'/f'PL-{arm}.json'),'exact_effective_switch_diff':diff,'common_binary':True,'native_build_count':1,'regenerated_self_byte_equal':True,'regenerated_velf_byte_equal':True,'elf_margin_pass':True,'process_parameters_pass':True,'no_new_GL_hooks':True,'zero_render_observer_hooks':True,'console_access':False,'runtime_performance':'UNMEASURED','new_self_requires_fresh_A':True,'logging_fix_retained':True}
 v['native_build_attempts']=1+len(list((O/'proof').glob('BUILD-COMMAND*-failed.json')))
 v['successful_native_builds']=1
 (d/'VERIFICATION.json').write_text(json.dumps(v,indent=2)+'\n');rows.append(v)
(O/'PACKAGE-MATRIX.json').write_text(json.dumps({'status':'BUILT_HOST_QUALIFIED_CONSOLE_NOT_EXECUTED','packages':rows},indent=2)+'\n')
(O/'proof/ENGINE-DIFF.patch').write_text(git('diff','6270497',receipt['revision'],'--','src')+'\n')
(O/'proof/DEPENDENCIES-AND-DEFAULTS.json').write_text(json.dumps({'dependency_build_inputs_unchanged':True,'Cargo_lock_sha256':sha(R/'Cargo.lock'),'baseline_config_byte_identical_to_r5':True,'atomic_logger_source_unchanged':True,'direct_resolver_source_unchanged':True,'no_new_GL_hooks':True,'new_runtime_default':{'skin_actual_consumers':False,'phase_census':False}},indent=2)+'\n')
run('toolchain-capture.log',[sys.executable,str(O/'scripts/capture-environment.py'),str(O)])
print(json.dumps(rows[0],indent=2))
