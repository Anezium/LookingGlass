"""Materialize the Mac-only phase census console draft. Never opens transport."""
from pathlib import Path
import json,shutil,sys
R=Path('/Users/anezium/vita-work/lg-phase-census-r5');T=R/'tools/phase_census_r5';OLD=R/'tools/lever_matrix_r4';O=Path(sys.argv[1]).resolve()
O.mkdir(parents=True,exist_ok=True)
for d in ['console','configs','scripts','proof/log-integrity-fixtures']:(O/d).mkdir(parents=True,exist_ok=True)
for f in (T/'console').glob('*.py'):shutil.copy2(f,O/'console'/f.name)
for name in ['PL-baseline.json','vita-sim-all.json','native-timing-disabled.json','profile_mode-full.txt','profile_mode-fullbench.txt']:shutil.copy2(OLD/'configs'/name,O/'configs'/name)
base=json.loads((O/'configs/PL-baseline.json').read_text());assert not base.get('phase_census',False)
c=dict(base,phase_census=True);(O/'configs/PL-census.json').write_text(json.dumps(c,indent=2)+'\n')
for name in ['log-integrity.py','test-log-integrity.py','verify-elf-margin.py','verify-process-params.py','capture-environment.py']:
 shutil.copy2(OLD/'scripts'/name,O/'scripts'/name)
shutil.copytree(OLD/'proof/log-integrity-fixtures',O/'proof/log-integrity-fixtures',dirs_exist_ok=True)
protocol=json.loads((OLD/'CONSOLE-PROTOCOL.json').read_text());seq=[]
for e in protocol['sequence'][:8]:
 e=dict(e);e['session']='s6-phase-census';e['block']='s6-phase-census-'+e['map'];e['label']=e['block']+'-'+e['label'].split('-')[-2]+'-'+e['arm'];e['config_arm']=e['package']='census' if e['arm']=='B' else 'baseline';seq.append(e)
protocol.update(sessions=['s6-phase-census'],sequence=seq,measurement='wall-only ABBA; B additionally exports fixed-storage census',roles={'A':'census disabled','B':'census enabled'},config_changes={'baseline':[],'census':['phase_census']},scope='r5 Mac-only draft, NOT EXECUTED',pass_rule='No optimization verdict. If either oriented B-A median or their mean exceeds 1 ms, label breakdown OBSERVER_PERTURBED; otherwise NO_RESOLVED_OBSERVER_COST. Never subtract observer cost from regions.')
(O/'CONSOLE-PROTOCOL.json').write_text(json.dumps(protocol,indent=2)+'\n')
for arm in ['baseline','census']:
 for mode in ['full','self']:
  m=json.loads((OLD/'manifests'/f'stage-baseline-{mode}.json').read_text());
  for f in m['files']:
   f['local']=f['local'].replace('@OUTPUT@',str(O)).replace('/packages/baseline/',f'/packages/{arm}/').replace('/PL-baseline.json',f'/PL-{arm}.json')
  (O/f'stage-{arm}-{mode}.json').write_text(json.dumps(m,indent=2)+'\n')
m=(OLD/'manifests/stage-fullbench-mode-only.json').read_text().replace('@OUTPUT@',str(O));(O/'stage-fullbench-mode-only.json').write_text(m)
p=O/'scripts/log-integrity.py';p.write_text(p.read_text().replace('STOP_ALL_REMAINING_R4','STOP_ALL_REMAINING_R5'))
p=O/'scripts/test-log-integrity.py';p.write_text(p.read_text().replace('STOP_ALL_REMAINING_R4','STOP_ALL_REMAINING_R5'))
for name in ['census_report.py','test_tools.py','bind_helpers.py']:
 if (T/name).exists():shutil.copy2(T/name,O/('console' if name=='census_report.py' else 'scripts')/name)
print('Prepared draft; no console calls')
# r4 historical log fixture paths are immutable; labels in the new protocol differ.
p=O/'scripts/test-log-integrity.py';s=p.read_text().replace("real_a = a + '/runtime-log.txt'", "real_a = 's4-audio-pandemonium-1-A/runtime-log.txt'").replace("real_b = b + '/runtime-log.txt'", "real_b = 's4-audio-pandemonium-2-B/runtime-log.txt'")
s=s.replace("protocol = json.loads((OUT / 'CONSOLE-PROTOCOL.json').read_text())", "protocol = json.loads((OUT / 'CONSOLE-PROTOCOL.json').read_text())\n    extra=dict(protocol['sequence'][0],label='fixture-isolated-1-A',session='fixture-independent');protocol['sequence'].append(extra)")
p.write_text(s)
for name in ['CONSOLE-BRIEF.md','DESIGN.md']:
 if (T/name).exists():shutil.copy2(T/name,O/name)
