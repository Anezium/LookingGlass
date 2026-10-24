"""Materialize a twelve-launch package locally. Does not import transport."""
from pathlib import Path
import json,shutil,sys
R=Path(__file__).resolve().parents[2];T=R/'tools/world_vao_r7';O=Path(sys.argv[1]).resolve()
for d in ['console','scripts','configs','proof']:(O/d).mkdir(parents=True,exist_ok=True)
for f in (T/'console').glob('*.py'):shutil.copy2(f,O/'console'/f.name)
for n in ['PL-baseline.json','vita-sim-all.json','native-timing-disabled.json','profile_mode-full.txt','profile_mode-fullbench.txt']:shutil.copy2(R/'tools/lever_matrix_r4/configs'/n,O/'configs'/n)
base=json.loads((O/'configs/PL-baseline.json').read_text())
for arm,overrides in [('vao',dict(world_vertex_input=True)),('baseline-census',dict(phase_census=True)),('vao-census',dict(world_vertex_input=True,phase_census=True))]:
 (O/'configs'/f'PL-{arm}.json').write_text(json.dumps(dict(base,**overrides),indent=2)+'\n')
for name in ['log-integrity.py','test-log-integrity.py','verify-elf-margin.py','verify-process-params.py','capture-environment.py']:
 s=(R/'tools/lever_matrix_r4/scripts'/name).read_text().replace('STOP_ALL_REMAINING_R4','STOP_ALL_REMAINING_R7')
 if name=='log-integrity.py':s=s.replace("sequence=protocol['sequence'];","sequence=protocol['sequence']+protocol.get('optional_sequence',[]);").replace("for e in protocol['sequence'])==1","for e in protocol['sequence']+protocol.get('optional_sequence',[]))==1")
 (O/'scripts'/name).write_text(s)
protocol=json.loads((R/'tools/lever_matrix_r4/CONSOLE-PROTOCOL.json').read_text());seq=[];diagnostic=[]
for e in protocol['sequence'][:8]:
 e=dict(e,session='s8-world-vao');e['block']=e['session']+'-'+e['map'];e['label']=e['block']+'-'+e['label'].split('-')[-2]+'-'+e['arm'];e['config_arm']=e['package']='vao' if e['arm']=='B' else 'baseline';e['census']=False;seq.append(e)
for m in ['pandemonium','hedge3']:
 for arm in ['A','B']:
  e=dict(next(e for e in seq if e['map']==m and e['arm']==arm),session='s8-vao-census',config_arm='vao-census' if arm=='B' else 'baseline-census',census=True)
  e['package']=e['config_arm'];e['label']='s8-vao-census-'+m+('-5-A' if arm=='A' else '-6-B');e['block']='s8-vao-census-'+m;diagnostic.append(e)
protocol.update(sessions=['s8-world-vao','s8-vao-census'],sequence=seq,optional_sequence=diagnostic,diagnostic_runs_required=True,admissions=[dict(kind='wall',launches=8,pandemonium_launches=4,estimated_minutes=127.5),dict(kind='census',launches=4,pandemonium_launches=2,estimated_minutes=88.5)],session_cap_minutes=150,session_directory='session-s8-world-vao',restoration_between_admissions_required=True,
 measurement='Eight wall-only off/on ABBA runs, followed by four mandatory census diagnostics outside the wall gate; common SELF',
 roles={'A':'world_vertex_input=false','B':'world_vertex_input=true'},
 config_changes={'baseline':[],'vao':['world_vertex_input'],'baseline-census':['phase_census'],'vao-census':['world_vertex_input','phase_census']},
 scope='r7 Mac-only package; NOT EXECUTED',
 pass_rule='Benefit requires BOTH Hedge3 oriented B-A median deltas strictly below -1.000 ms; BOTH Pandemonium deltas must be <=+1.000 ms. All integrity/audio/heap gates must pass. No trimming, pooling or observer subtraction. Four census diagnostics never enter the wall gate.')
(O/'CONSOLE-PROTOCOL.json').write_text(json.dumps(protocol,indent=2)+'\n')
for arm in protocol['config_changes']:
 for mode in ['full','self']:
  m=json.loads((R/'tools/lever_matrix_r4/manifests'/f'stage-baseline-{mode}.json').read_text())
  for f in m['files']:f['local']=f['local'].replace('@OUTPUT@',str(O)).replace('/packages/baseline/',f'/packages/{arm}/').replace('/PL-baseline.json',f'/PL-{arm}.json')
  (O/f'stage-{arm}-{mode}.json').write_text(json.dumps(m,indent=2)+'\n')
(O/'stage-fullbench-mode-only.json').write_text((R/'tools/lever_matrix_r4/manifests/stage-fullbench-mode-only.json').read_text().replace('@OUTPUT@',str(O)))
for n in ['bind_helpers.py','test_session_e2e.py']:shutil.copy2(T/n,O/'scripts'/n)
for n in ['CONSOLE-BRIEF.md','DESIGN.md','README.md']:
 if (T/n).exists():shutil.copy2(T/n,O/n)
print('Prepared r7: eight wall runs + four required diagnostics; session directory absent.')
