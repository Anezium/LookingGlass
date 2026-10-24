from pathlib import Path
import json,shutil,sys
R=Path(__file__).resolve().parents[2];T=R/'tools/sah_r8';O=Path(sys.argv[1]).resolve()
for d in ['console','scripts','configs','proof']:(O/d).mkdir(parents=True,exist_ok=True)
for f in (T/'console').glob('*.py'):shutil.copy2(f,O/'console'/f.name)
for f in T.glob('*.py'):shutil.copy2(f,O/'scripts'/f.name)
for n in ['vita-sim-all.json','native-timing-disabled.json','profile_mode-full.txt','profile_mode-fullbench.txt']:shutil.copy2(R/'tools/lever_matrix_r4/configs'/n,O/'configs'/n)
base=json.loads((R/'tools/lever_matrix_r4/configs/PL-baseline.json').read_text());base.update(collision_sah=False,audio_static_cache=False,phase_census=False)
for arm,sah,census in [('baseline',False,False),('sah',True,False),('baseline-census',False,True),('sah-census',True,True)]:
 (O/'configs'/f'PL-{arm}.json').write_text(json.dumps(dict(base,collision_sah=sah,phase_census=census),indent=2)+'\n')
for name in ['log-integrity.py','test-log-integrity.py','verify-elf-margin.py','verify-process-params.py','capture-environment.py']:
 source=R/'tools/lever_matrix_r4/scripts'/name;s=source.read_text().replace('STOP_ALL_REMAINING_R4','STOP_ALL_REMAINING_R8')
 if name=='log-integrity.py':s=s.replace("sequence=protocol['sequence'];","sequence=protocol['sequence']+protocol.get('optional_sequence',[])+protocol.get('smoke_sequence',[]);").replace("for e in protocol['sequence'])==1","for e in protocol['sequence']+protocol.get('optional_sequence',[])+protocol.get('smoke_sequence',[]))==1")
 if name=='test-log-integrity.py':
  s=s.replace("protocol = json.loads((OUT / 'CONSOLE-PROTOCOL.json').read_text())","protocol = json.loads((OUT / 'CONSOLE-PROTOCOL.json').read_text())\n    extra=dict(protocol['sequence'][0],label='fixture-isolated-1-A',session='fixture-independent');protocol['sequence'].append(extra)")
  s=s.replace("real_a = a + '/runtime-log.txt'","real_a = 's4-audio-pandemonium-1-A/runtime-log.txt'").replace("real_b = b + '/runtime-log.txt'","real_b = 's4-audio-pandemonium-2-B/runtime-log.txt'")
 (O/'scripts'/name).write_text(s)
shutil.copytree(R/'tools/lever_matrix_r4/proof/log-integrity-fixtures',O/'proof/log-integrity-fixtures',dirs_exist_ok=True)
protocol=json.loads((R/'tools/lever_matrix_r4/CONSOLE-PROTOCOL.json').read_text());old=protocol['sequence'][:8];sequence=[];optional=[];smoke=[]
for name in ['hedge3','pandemonium']:
 for e in [x for x in old if x['map']==name]:
  e=dict(e,session='s9-sah',config_arm='sah' if e['arm']=='B' else 'baseline',package='sah' if e['arm']=='B' else 'baseline',census=False,smoke=False)
  e['block']=e['session']+'-'+name;e['label']=e['block']+'-'+e['label'].split('-')[-2]+'-'+e['arm'];sequence.append(e)
for arm in ['A','B']:
 e=dict(next(x for x in sequence if x['map']=='hedge3' and x['arm']==arm),session='s9-sah-census',census=True,config_arm='baseline-census' if arm=='A' else 'sah-census',package='baseline-census' if arm=='A' else 'sah-census')
 e['block']='s9-sah-census-hedge3';e['label']=e['block']+'-'+('5' if arm=='A' else '6')+'-'+arm;optional.append(e)
for name in ['hedge3','pandemonium']:
 e=dict(next(x for x in sequence if x['map']==name and x['arm']=='B'),session='s9-sah-load-smoke',smoke=True)
 e['block']=e['session']+'-'+name;e['label']=e['block']+'-0-B';smoke.append(e)
protocol.update(sequence=sequence,optional_sequence=optional,smoke_sequence=smoke,sessions=['s9-sah','s9-sah-load-smoke'],session_directory='session-s9-sah',smoke_session_directory='session-s9-sah-load-smoke',roles={'A':'collision_sah=false','B':'collision_sah=true'},config_changes={'baseline':[],'sah':['collision_sah'],'baseline-census':['phase_census'],'sah-census':['collision_sah','phase_census']},scope='Mac-only package; NOT EXECUTED',measurement='Common SELF, acoustic cache off. Two B-only load smokes in a separate restored session, then eight wall arms and two Hedge3 census arms.',pass_rule=(T/'READING-RULES.md').read_text(),heap_budget=dict(expected_hedge3_delta_MiB=[3,5],oriented_delta_max_MiB=8,hedge3_plateau_exclusive_MiB=165,whole_run_stop_MiB=180))
(O/'CONSOLE-PROTOCOL.json').write_text(json.dumps(protocol,indent=2)+'\n')
for arm in ['baseline','sah','baseline-census','sah-census']:
 for mode in ['full','self']:
  m=json.loads((R/'tools/lever_matrix_r4/manifests'/f'stage-baseline-{mode}.json').read_text())
  for f in m['files']:f['local']=f['local'].replace('@OUTPUT@',str(O)).replace('/packages/baseline/',f'/packages/{arm}/').replace('/PL-baseline.json',f'/PL-{arm}.json')
  (O/f'stage-{arm}-{mode}.json').write_text(json.dumps(m,indent=2)+'\n')
(O/'stage-fullbench-mode-only.json').write_text((R/'tools/lever_matrix_r4/manifests/stage-fullbench-mode-only.json').read_text().replace('@OUTPUT@',str(O)))
for name in ['CONTRACT.md','READING-RULES.md','CONSOLE-BRIEF.md','AMENDMENTS.md','MEASUREMENT-LIMITS.md','README.md']:shutil.copy2(T/name,O/name)
print('Prepared package, no session directory or transport action')
