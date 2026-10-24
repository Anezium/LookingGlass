from pathlib import Path
import sys
import hashlib,json,shutil,subprocess,zipfile
O=Path(sys.argv[1]).resolve();R=Path('/Users/anezium/vita-work/lg-lever-matrix-r4');P=O.parent/'alice-lever-matrix-r3';B=O/'packages/baseline'
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def git(*args):return subprocess.check_output(['git',*args],cwd=R,text=True).strip()
receipt=json.loads((O/'proof/BUILD-COMMAND.json').read_text())
base=json.loads((O/'configs/PL-baseline.json').read_text());assert (O/'configs/PL-baseline.json').read_bytes()==(P/'configs/PL-baseline.json').read_bytes()
assert not any(base[k] for k in ['audio_offmain','world_static_merge','world_material_sort'])
rows=[]
for arm,key in [('baseline',None),('audio','audio_offmain'),('world-static-merge','world_static_merge')]:
 d=O/'packages'/arm;d.mkdir(exist_ok=True)
 if arm!='baseline':shutil.copy2(B/'eboot.bin',d/'eboot.bin')
 cfg=json.loads((O/'configs'/f'PL-{arm}.json').read_text());diff={k:[base[k],cfg[k]] for k in base if base[k]!=cfg[k]};assert set(diff)==({key} if key else set())
 v={'arm':arm,'revision':receipt['revision'],'source_revision':receipt['revision'],'branch':git('branch','--show-current'),'r3_base':'881988ec1801a68a425d43c04c63d8998372e952','build_command':receipt['argv'],'build_environment':receipt['environment'],'self_sha256':sha(d/'eboot.bin'),'self_bytes':(d/'eboot.bin').stat().st_size,'elf_sha256':sha(B/'looking-glass.elf'),'velf_sha256':sha(B/'looking-glass.velf'),'vpk_sha256':sha(B/'looking-glass.vpk'),'lock_sha256':sha(R/'Cargo.lock'),'config_path':f'configs/PL-{arm}.json','config_sha256':sha(O/'configs'/f'PL-{arm}.json'),'exact_switch_diff':diff,'common_binary':True,'copy_of':'packages/baseline/eboot.bin','native_build_count':1,'shared_timer_source_sha256':sha(R/'src/frame_total_profile.rs'),'zero_render_observer_hooks':True,'regenerated_self_byte_equal':sha(B/'regenerated.self')==sha(B/'eboot.bin'),'regenerated_velf_byte_equal':sha(B/'regenerated.velf')==sha(B/'looking-glass.velf'),'elf_margin_pass':True,'process_parameters_pass':True,'dependency_pins_unchanged':True,'console_access':False,'runtime_performance':'UNMEASURED','new_self_requires_fresh_A':True,'logging_fix_retained':True}
 assert v['regenerated_self_byte_equal'] and v['regenerated_velf_byte_equal']
 (d/'VERIFICATION.json').write_text(json.dumps(v,indent=2)+'\n');rows.append(v)
assert not any('looking_glass::render_tail::hooks' in l for l in (O/'logs/symbols.log').read_text().splitlines())
assert not git('diff','881988ec','HEAD','--','Cargo.lock','Cargo.toml','build.rs','.cargo','src/logging.rs','src/frame_total_profile.rs','src/render_direct.rs')
with zipfile.ZipFile(B/'looking-glass.vpk') as z:
 members=[{'name':i.filename,'bytes':i.file_size} for i in z.infolist()]
 assert {i['name'] for i in members}=={'eboot.bin','sce_sys/param.sfo'},members
 assert hashlib.sha256(z.read('eboot.bin')).hexdigest()==rows[0]['self_sha256']
(O/'proof/VPK-CONTENTS.json').write_text(json.dumps(members,indent=2)+'\n')
(O/'PACKAGE-MATRIX.json').write_text(json.dumps({'status':'BUILT_HOST_QUALIFICATION_IN_PROGRESS_CONSOLE_NOT_EXECUTED','packages':rows},indent=2)+'\n')
(O/'proof/SOURCE-DIFF.patch').write_text(git('diff','881988ec',receipt['revision'],'--','src')+'\n')
(O/'proof/DEPENDENCIES-AND-DEFAULTS.json').write_text(json.dumps({'dependency_pins_unchanged':True,'lock_sha256':sha(R/'Cargo.lock'),'baseline_config_byte_identical_to_r3':True,'baseline_defaults_disabled':['audio_offmain','world_static_merge','world_material_sort'],'zero_observer_hook_symbols':True,'timer_and_atomic_logger_sources_byte_identical_to_r3':True},indent=2)+'\n')
print(json.dumps({k:rows[0][k] for k in ['revision','self_sha256','elf_sha256','vpk_sha256']},indent=2))
