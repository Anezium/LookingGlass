"""Materialize r6 helpers and fixed protocol locally; never imports transport."""
from pathlib import Path
import json,shutil,sys
R=Path(__file__).resolve().parents[2];T=R/'tools/acoustic_cache_r6';OLD=R/'tools/phase_census_r5';O=Path(sys.argv[1]).resolve()
for d in ['console','scripts','configs','proof']:(O/d).mkdir(parents=True,exist_ok=True)
for f in (T/'console').glob('*.py'):shutil.copy2(f,O/'console'/f.name)
for n in ['PL-baseline.json','vita-sim-all.json','native-timing-disabled.json','profile_mode-full.txt','profile_mode-fullbench.txt']:shutil.copy2(R/'tools/lever_matrix_r4/configs'/n,O/'configs'/n)
base=json.loads((O/'configs/PL-baseline.json').read_text())
for arm,c in [('cache',dict(base,audio_static_cache=True)),('cache-census',dict(base,audio_static_cache=True,phase_census=True))]:
 (O/'configs'/f'PL-{arm}.json').write_text(json.dumps(c,indent=2)+'\n')
for name in ['log-integrity.py','test-log-integrity.py','verify-elf-margin.py','verify-process-params.py','capture-environment.py']:
 s=(R/'tools/lever_matrix_r4/scripts'/name).read_text().replace('STOP_ALL_REMAINING_R4','STOP_ALL_REMAINING_R6')
 if name=='log-integrity.py':s=s.replace("sequence=protocol['sequence'];","sequence=protocol['sequence']+protocol.get('optional_sequence',[]);").replace("for e in protocol['sequence'])==1","for e in protocol['sequence']+protocol.get('optional_sequence',[]))==1")
 if name=='test-log-integrity.py':
  s=s.replace("real_a = a + '/runtime-log.txt'", "real_a = 's4-audio-pandemonium-1-A/runtime-log.txt'").replace("real_b = b + '/runtime-log.txt'", "real_b = 's4-audio-pandemonium-2-B/runtime-log.txt'")
  s=s.replace("protocol = json.loads((OUT / 'CONSOLE-PROTOCOL.json').read_text())", "protocol = json.loads((OUT / 'CONSOLE-PROTOCOL.json').read_text())\n    extra=dict(protocol['sequence'][0],label='fixture-isolated-1-A',session='fixture-independent');protocol['sequence'].append(extra)")
 (O/'scripts'/name).write_text(s)
shutil.copytree(R/'tools/lever_matrix_r4/proof/log-integrity-fixtures',O/'proof/log-integrity-fixtures',dirs_exist_ok=True)
protocol=json.loads((R/'tools/lever_matrix_r4/CONSOLE-PROTOCOL.json').read_text());seq=[];optional=[]
for e in protocol['sequence'][:8]:
 e=dict(e,session='s7-acoustic-cache');e['block']=e['session']+'-'+e['map'];e['label']=e['block']+'-'+e['label'].split('-')[-2]+'-'+e['arm'];e['config_arm']=e['package']='cache' if e['arm']=='B' else 'baseline';e['census']=False;seq.append(e)
for m in ['pandemonium','hedge3']:
 e=dict(next(e for e in seq if e['map']==m and e['arm']=='B'),session='s7-cache-census',config_arm='cache-census',package='cache-census',census=True)
 e['label']='s7-cache-census-'+m+'-5-B';e['block']='s7-cache-census-'+m;optional.append(e)
protocol.update(sessions=['s7-acoustic-cache'],sequence=seq,optional_sequence=optional,measurement='Wall-only cache off/on ABBA on a common SELF; census disabled in all eight gate runs',roles={'A':'audio_static_cache=false, phase_census=false','B':'audio_static_cache=true, phase_census=false'},config_changes={'baseline':[],'cache':['audio_static_cache'],'cache-census':['audio_static_cache','phase_census']},scope='r6 Mac-only package; NOT EXECUTED',pass_rule='Benefit requires BOTH Hedge3 oriented B-A median deltas strictly below -1.000 ms; BOTH Pandemonium deltas must be <=+1.000 ms. All integrity/audio/heap gates must pass. No trimming, pooling or observer subtraction. Optional census runs never enter the wall gate.')
(O/'CONSOLE-PROTOCOL.json').write_text(json.dumps(protocol,indent=2)+'\n')
for arm in ['baseline','cache','cache-census']:
 for mode in ['full','self']:
  m=json.loads((R/'tools/lever_matrix_r4/manifests'/f'stage-baseline-{mode}.json').read_text())
  for f in m['files']:f['local']=f['local'].replace('@OUTPUT@',str(O)).replace('/packages/baseline/',f'/packages/{arm}/').replace('/PL-baseline.json',f'/PL-{arm}.json')
  (O/f'stage-{arm}-{mode}.json').write_text(json.dumps(m,indent=2)+'\n')
(O/'stage-fullbench-mode-only.json').write_text((R/'tools/lever_matrix_r4/manifests/stage-fullbench-mode-only.json').read_text().replace('@OUTPUT@',str(O)))
for n in ['bind_helpers.py','test_session_e2e.py','test_protocol.py']:shutil.copy2(T/n,O/'scripts'/n)
for n in ['CONSOLE-BRIEF.md','DESIGN.md','README.md']:shutil.copy2(T/n,O/n)
print('Prepared r6 package locally; session directory not created')
